use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, FixedOffset, Utc};
use mb_agent::{EpisodeOutcome, run_episode};
use mb_core::{MAX_TOOL_CALLS, Task, load_task};
use mb_grader::{grade, load_reference, validate_reference};
use mb_llm::{
    Client, CompletionBackend, ModelConfig, RequestRecord, RunConfig, TokenUsage, ToolChoice,
    ToolDefinition, estimate_cost, load_config,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use tokio::task::JoinSet;

struct Artifacts {
    run_dir: PathBuf,
    transcript_dir: PathBuf,
    metadata: Value,
    scores: Vec<Value>,
    records: Vec<RequestRecord>,
    costs: Vec<Option<Decimal>>,
}

impl Artifacts {
    fn create(
        root: &Path,
        out: Option<&Path>,
        kind: &str,
        config: &RunConfig,
        models: &[ModelConfig],
        tasks: &[Task],
    ) -> Result<Self> {
        let start = Utc::now();
        let run_id = format!(
            "{kind}-{}-{}",
            start.format("%Y%m%dT%H%M%S%.9fZ"),
            std::process::id()
        );
        let parent = out.map_or_else(|| root.join("runs"), |path| resolve(root, path));
        fs::create_dir_all(&parent)?;
        let run_dir = parent.join(&run_id);
        fs::create_dir(&run_dir).context("creating an exclusive run directory")?;
        let transcript_dir = if parent.file_name().is_some_and(|name| name == "results") {
            let parent = root.join("transcripts");
            fs::create_dir_all(&parent)?;
            let directory = parent.join(&run_id);
            fs::create_dir(&directory)?;
            directory
        } else {
            let directory = run_dir.join("transcripts");
            fs::create_dir(&directory)?;
            directory
        };
        fs::create_dir(run_dir.join("outputs"))?;
        let resolved_providers = models
            .iter()
            .map(|model| {
                Ok((
                    model.provider.clone(),
                    config.providers[&model.provider].resolved(&model.provider)?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let source = if resolved_providers
            .values()
            .any(|provider| provider.base_url.starts_with("http://"))
        {
            "local_mock"
        } else {
            "provider_api"
        };
        let metadata = json!({
            "schema_version": 1, "run_id": run_id, "kind": kind,
            "source": source, "state": "running",
            "started_at_utc": start.to_rfc3339(), "started_at_istanbul": istanbul(start),
            "ended_at_utc": null, "ended_at_istanbul": null,
            "git_commit": command_output(root, "git", &["rev-parse", "HEAD"]),
            "git_dirty": command_output(root, "git", &["status", "--porcelain"]).map(|status| !status.is_empty()),
            "rustc_version": command_output(root, "rustc", &["--version"]),
            "models": models, "providers": resolved_providers,
            "settings_note": "Requested settings; omitted settings use provider defaults. Effective settings are not inferred.",
            "tasks": tasks.iter().map(|task| json!({"task_id": task.task_id,"task_version":task.task_version})).collect::<Vec<_>>(),
            "trials_per_task": if kind == "smoke" { 1 } else { config.trials },
            "max_turns": config.max_turns, "max_tool_calls": MAX_TOOL_CALLS,
            "cost_is_estimate": true,
            "raw_output_policy": "Provider payloads are retained after credential redaction before first write. Request headers and HTTP error bodies are not recorded.",
            "cost_note": "Configured prices; cached input is within input and reasoning within output. Missing usage/price keeps cost unknown. Failed attempts may have unreported billable usage.",
        });
        let artifacts = Self {
            run_dir,
            transcript_dir,
            metadata,
            scores: Vec::new(),
            records: Vec::new(),
            costs: Vec::new(),
        };
        artifacts.flush("running")?;
        Ok(artifacts)
    }

    fn save(
        &mut self,
        id: &str,
        transcript: &Value,
        raw: &Value,
        row: Value,
        records: Vec<RequestRecord>,
        cost: Option<Decimal>,
    ) -> Result<()> {
        write_new(&self.transcript_dir.join(format!("{id}.json")), transcript)?;
        write_new(
            &self.run_dir.join("outputs").join(format!("{id}.json")),
            raw,
        )?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.run_dir.join("scores.jsonl"))?;
        serde_json::to_writer(&mut file, &row)?;
        writeln!(file)?;
        file.sync_data()?;
        self.scores.push(row);
        self.records.extend(records);
        self.costs.push(cost);
        self.flush("running")
    }

    fn flush(&self, state: &str) -> Result<()> {
        let mut metadata = self.metadata.clone();
        metadata["state"] = json!(state);
        if state != "running" {
            let end = Utc::now();
            metadata["ended_at_utc"] = json!(end.to_rfc3339());
            metadata["ended_at_istanbul"] = json!(istanbul(end));
        }
        metadata["requests"] = json!(self.records);
        metadata["token_usage"] = json!(sum_usage(&self.records));
        metadata["retries"] = json!(
            self.records
                .iter()
                .map(|record| u64::from(record.retries))
                .sum::<u64>()
        );
        metadata["http_attempts"] = json!(
            self.records
                .iter()
                .map(|record| record.attempts.len())
                .sum::<usize>()
        );
        metadata["completed_items"] = json!(self.scores.len());
        metadata["api_error_count"] = json!(
            self.scores
                .iter()
                .filter(|row| row["status"] == "api_error")
                .count()
        );
        metadata["pass_count"] = json!(
            self.scores
                .iter()
                .filter(|row| row["status"] == "pass")
                .count()
        );
        metadata["cost_estimate_usd"] = json!(sum_costs(&self.costs));
        metadata["known_cost_estimate_usd"] =
            json!(self.costs.iter().flatten().copied().sum::<Decimal>());
        let temporary = self.run_dir.join("run_meta.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(&metadata)?)?;
        fs::rename(temporary, self.run_dir.join("run_meta.json"))?;
        Ok(())
    }
}

fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    }
}

fn command_output(root: &Path, program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn istanbul(time: DateTime<Utc>) -> String {
    time.with_timezone(&FixedOffset::east_opt(3 * 3600).expect("valid offset"))
        .to_rfc3339()
}

fn write_new(path: &Path, value: &Value) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("creating immutable output {}", path.display()))?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.sync_data()?;
    Ok(())
}

fn configuration(
    root: &Path,
    config: &Path,
    filter: Option<&str>,
) -> Result<(RunConfig, Vec<ModelConfig>)> {
    let dotenv = root.join(".env");
    if dotenv.is_file() {
        dotenvy::from_path(dotenv)
            .map_err(|_| anyhow::anyhow!("cannot parse local .env; check assignment syntax"))?;
    }
    let config = load_config(&resolve(root, config))?;
    let models: Vec<_> = config
        .models
        .iter()
        .filter(|model| filter.is_none_or(|id| model.id == id))
        .cloned()
        .collect();
    ensure!(
        !models.is_empty(),
        "no configured model matches the selector"
    );
    for model in &models {
        config.providers[&model.provider].resolved(&model.provider)?;
    }
    Ok((config, models))
}

fn preflight_keys(config: &RunConfig, models: &[ModelConfig], dry_run: bool) -> Result<()> {
    let providers: BTreeSet<_> = models.iter().map(|model| &model.provider).collect();
    let mut missing = Vec::new();
    for provider in providers {
        let key = &config.providers[provider].key_env;
        let present = std::env::var(key).is_ok_and(|value| !value.trim().is_empty());
        if dry_run {
            println!(
                "{provider}: {key} {}",
                if present { "present" } else { "missing" }
            );
        }
        if !present {
            missing.push(key.clone());
        }
    }
    ensure!(
        missing.is_empty(),
        "missing API key environment variables: {} (configure locally; do not paste keys into chat)",
        missing.join(", ")
    );
    Ok(())
}

fn clients(config: &RunConfig, models: &[ModelConfig]) -> Result<BTreeMap<String, Arc<Client>>> {
    let providers: BTreeSet<_> = models.iter().map(|model| model.provider.clone()).collect();
    providers
        .into_iter()
        .map(|name| {
            let client = Client::from_env(&name, &config.providers[&name])?;
            Ok((name, Arc::new(client)))
        })
        .collect()
}

fn sum_costs(costs: &[Option<Decimal>]) -> Option<Decimal> {
    costs
        .iter()
        .try_fold(Decimal::ZERO, |sum, cost| sum.checked_add((*cost)?))
}

fn sum_usage(records: &[RequestRecord]) -> TokenUsage {
    fn sum(records: &[RequestRecord], field: impl Fn(&TokenUsage) -> Option<u64>) -> Option<u64> {
        if records.is_empty() {
            return None;
        }
        records
            .iter()
            .try_fold(0_u64, |sum, record| sum.checked_add(field(&record.usage)?))
    }
    TokenUsage {
        input: sum(records, |usage| usage.input),
        cached_input: sum(records, |usage| usage.cached_input),
        output: sum(records, |usage| usage.output),
        reasoning: sum(records, |usage| usage.reasoning),
    }
}

fn request_cost(model: &ModelConfig, record: &RequestRecord) -> Option<Decimal> {
    // Usage on failed attempts is not reliably reported by providers.
    if record.retries > 0 {
        return None;
    }
    estimate_cost(model.price.as_ref(), &record.usage)
}

pub async fn smoke(
    root: &Path,
    path: &Path,
    selector: Option<&str>,
    out: Option<&Path>,
    dry_run: bool,
) -> Result<()> {
    let (config, models) = configuration(root, path, selector)?;
    preflight_keys(&config, &models, dry_run)?;
    if dry_run {
        println!("smoke preflight passed; no API requests sent");
        return Ok(());
    }
    let clients = clients(&config, &models)?;
    let mut artifacts = Artifacts::create(root, out, "smoke", &config, &models, &[])?;
    let tools = vec![ToolDefinition {
        name: "smoke_echo".into(),
        description: "Confirm function-calling transport support.".into(),
        parameters: json!({"type":"object","properties":{"value":{"type":"string","enum":["ready"]}},"required":["value"],"additionalProperties":false}),
    }];
    let history = vec![
        json!({"role":"user","content":"Call smoke_echo with value ready. This is a transport check; do not answer with text."}),
    ];
    let mut failures = 0;
    for (index, model) in models.iter().enumerate() {
        let id = format!("model-{index}");
        let choice = if model.provider == "deepseek"
            && model
                .thinking
                .unwrap_or(model.reasoning_effort.as_deref() != Some("none"))
        {
            ToolChoice::Auto
        } else {
            ToolChoice::Required("smoke_echo".into())
        };
        let result = clients[&model.provider]
            .complete(model, &history, &tools, choice)
            .await;
        let (status, transcript, raw, record) = match result {
            Ok(reply) => {
                let supported = reply.calls.len() == 1
                    && reply.calls[0].name == "smoke_echo"
                    && serde_json::from_str::<Value>(&reply.calls[0].arguments)
                        .is_ok_and(|args| args == json!({"value":"ready"}))
                    && !matches!(
                        reply.finish_reason.as_deref(),
                        Some("length" | "incomplete" | "max_output_tokens" | "content_filter")
                    );
                let status = if supported {
                    "tool_call_supported"
                } else {
                    "no_tool_call"
                };
                let transcript =
                    json!({"source":"smoke", "initial_messages": history, "reply": reply});
                (status, transcript, reply.raw, reply.record)
            }
            Err(error) => {
                let transcript =
                    json!({"source":"smoke", "initial_messages": history,"api_error": error});
                (
                    "api_error",
                    transcript,
                    error.raw.unwrap_or(Value::Null),
                    *error.record,
                )
            }
        };
        let cost = request_cost(model, &record);
        let row = json!({"item_id":id,"status":status,"provider":model.provider,"requested_model":model.id,"model":record.model,"wire_api":record.api,"usage":record.usage,"cost_estimate_usd":cost});
        artifacts.save(&id, &transcript, &raw, row, vec![record], cost)?;
        println!("{}: {status}", model.id);
        failures += usize::from(status != "tool_call_supported");
    }
    artifacts.flush(if failures == 0 { "completed" } else { "failed" })?;
    println!(
        "smoke artifacts: {} (not benchmark results)",
        artifacts.run_dir.display()
    );
    ensure!(
        failures == 0,
        "{failures} smoke checks failed; inspect recorded diagnostics before another attempt"
    );
    Ok(())
}

pub async fn run(
    root: &Path,
    path: &Path,
    selector: Option<&str>,
    task_filter: Option<&str>,
    out: Option<&Path>,
    dry_run: bool,
) -> Result<()> {
    let (config, models) = configuration(root, path, selector)?;
    let mut tasks = Vec::new();
    for id in config
        .tasks
        .iter()
        .filter(|id| task_filter.is_none_or(|wanted| *id == wanted))
    {
        let task = load_task(&root.join("tasks").join(id).join("task.json"))?;
        ensure!(
            task.task_id == *id,
            "task file ID does not match configured task ID"
        );
        let reference = load_reference(&root.join("reference").join(format!("{id}.json")))?;
        validate_reference(&task, &reference)?;
        tasks.push((task, reference));
    }
    ensure!(!tasks.is_empty(), "no configured task matches the selector");
    preflight_keys(&config, &models, dry_run)?;
    if dry_run {
        println!(
            "run preflight passed: {} models × {} tasks × {} trials; no API requests sent",
            models.len(),
            tasks.len(),
            config.trials
        );
        return Ok(());
    }
    let clients = clients(&config, &models)?;
    let visible_tasks = tasks
        .iter()
        .map(|(task, _)| task.clone())
        .collect::<Vec<_>>();
    let mut artifacts = Artifacts::create(root, out, "run", &config, &models, &visible_tasks)?;
    let mut jobs = JoinSet::new();
    for (model_index, model) in models.iter().enumerate() {
        for (task, reference) in &tasks {
            for trial in 1..=config.trials {
                let (model, task, reference) = (model.clone(), task.clone(), reference.clone());
                let client = clients[&model.provider].clone();
                let max_turns = config.max_turns;
                jobs.spawn(async move {
                    let episode =
                        run_episode(client.as_ref(), &model, task.clone(), max_turns).await;
                    (model_index, model, task, reference, trial, episode)
                });
            }
        }
    }
    while let Some(result) = jobs.join_next().await {
        let (model_index, model, task, reference, trial, episode) = match result {
            Ok(result) => result,
            Err(_) => {
                jobs.abort_all();
                artifacts.flush("incomplete")?;
                bail!(
                    "trial worker failed; partial artifacts retained, no trials automatically rerun"
                );
            }
        };
        let id = format!("model-{model_index}-{}-trial-{trial}", task.task_id);
        let mut records = episode
            .transcript
            .replies
            .iter()
            .map(|reply| reply.record.clone())
            .collect::<Vec<_>>();
        if let Some(error) = &episode.transcript.api_error {
            records.push((*error.record).clone());
        }
        let cost = sum_costs(
            &records
                .iter()
                .map(|record| request_cost(&model, record))
                .collect::<Vec<_>>(),
        );
        let outcome_value = serde_json::to_value(episode.outcome)?;
        let (status, grading) = match episode.outcome {
            EpisodeOutcome::ApiError => ("api_error", None),
            outcome => {
                let mut grading = grade(
                    &task,
                    &reference,
                    &episode.environment,
                    &episode.transcript.final_output,
                );
                if !matches!(outcome, EpisodeOutcome::Complete) {
                    grading.pass = false;
                    grading.score = 0.0;
                    grading.failure_label = Some(
                        if matches!(outcome, EpisodeOutcome::InvalidToolCall) {
                            "schema_error"
                        } else {
                            "no_decision"
                        }
                        .into(),
                    );
                    grading.components[if matches!(outcome, EpisodeOutcome::InvalidToolCall) {
                        0
                    } else {
                        1
                    }] = false;
                }
                (if grading.pass { "pass" } else { "fail" }, Some(grading))
            }
        };
        let returned_models = records
            .iter()
            .filter_map(|record| record.model.clone())
            .collect::<BTreeSet<_>>();
        let label = grading
            .as_ref()
            .and_then(|grade| grade.failure_label.clone())
            .or_else(|| (status == "api_error").then(|| "api_error".to_owned()));
        let row = json!({"trial_id":id,"task_id":task.task_id,"task_version":task.task_version,"trial":trial,"provider":model.provider,"requested_model":model.id,"returned_models":returned_models,"status":status,"episode_outcome":outcome_value,"grade":grading,"failure_label":label,"usage":sum_usage(&records),"cost_estimate_usd":cost,"cost_is_estimate":true});
        let mut raw_outputs = episode
            .transcript
            .replies
            .iter()
            .map(|reply| reply.raw.clone())
            .collect::<Vec<_>>();
        if let Some(raw) = episode
            .transcript
            .api_error
            .as_ref()
            .and_then(|error| error.raw.as_ref())
        {
            raw_outputs.push(raw.clone());
        }
        let raw = json!(raw_outputs);
        artifacts.save(
            &id,
            &serde_json::to_value(&episode.transcript)?,
            &raw,
            row,
            records,
            cost,
        )?;
        println!("{} {} trial {trial}: {status}", model.id, task.task_id);
    }
    let errors = artifacts
        .scores
        .iter()
        .filter(|row| row["status"] == "api_error")
        .count();
    artifacts.flush(if errors == 0 {
        "completed"
    } else {
        "completed_with_api_errors"
    })?;
    println!("run artifacts: {}", artifacts.run_dir.display());
    ensure!(
        errors == 0,
        "{errors} trials ended in api_error; retained separately and not rerun"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_costs_are_not_silently_zero_and_timezones_are_explicit() {
        assert_eq!(sum_costs(&[Some(Decimal::ONE), None]), None);
        assert_eq!(
            sum_costs(&[Some(Decimal::ONE), Some(Decimal::ONE)]),
            Some(Decimal::from(2))
        );
        assert_eq!(
            istanbul("2026-11-02T00:00:00Z".parse().unwrap()),
            "2026-11-02T03:00:00+03:00"
        );
    }
}
