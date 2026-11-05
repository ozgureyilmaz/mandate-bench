use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, FixedOffset, Utc};
use mb_agent::{EpisodeOutcome, LiveEpisode, run_episode};
use mb_core::{MAX_TOOL_CALLS, Task, load_task};
use mb_grader::{
    Reference, evaluate_metrics_v2, grade, grade_authorized_outcome, load_reference,
    validate_reference,
};
use mb_llm::{
    BudgetGuard, Client, CompletionBackend, ModelConfig, ModelReply, RequestRecord, RunConfig,
    TokenUsage, ToolChoice, ToolDefinition, estimate_cost, load_config, tool_result,
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
    models: Vec<ModelConfig>,
    budgets: PaidBudgets,
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
            "metrics_policy_version": 2,
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
            "raw_output_policy": "Model payloads are redacted before first write. Meta HTTP errors retain only sanitized diagnostic fields. Request headers are omitted.",
            "cost_note": "Configured prices; cached input is within input and reasoning within output. Missing usage/price keeps cost unknown. Failed attempts may have unreported billable usage.",
        });
        let artifacts = Self {
            run_dir,
            transcript_dir,
            metadata,
            scores: Vec::new(),
            records: Vec::new(),
            costs: Vec::new(),
            models: models.to_vec(),
            budgets: BTreeMap::new(),
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
        if !self.budgets.is_empty() {
            let snapshots = self
                .budgets
                .iter()
                .map(|(provider, budget)| Ok((provider, budget.snapshot()?)))
                .collect::<Result<BTreeMap<_, _>>>()?;
            metadata["paid_budgets"] = serde_json::to_value(snapshots)?;
        }
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
        metadata["known_cost_estimate_usd"] = json!(
            self.records
                .iter()
                .filter_map(|record| {
                    self.models
                        .iter()
                        .find(|model| {
                            model.provider == record.provider && model.id == record.requested_model
                        })
                        .and_then(|model| request_cost(model, record))
                })
                .sum::<Decimal>()
        );
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
        let provider = config.providers[&model.provider].resolved(&model.provider)?;
        // Literal loopback is reserved for the existing offline transport fixtures.
        if provider.base_url.starts_with("http://") {
            continue;
        }
        match model.provider.as_str() {
            "openai" => {
                ensure!(
                    model.id == "gpt-6-luna",
                    "only gpt-6-luna is approved for paid evaluation"
                );
                ensure!(
                    provider.base_url.trim_end_matches('/') == "https://api.openai.com/v1",
                    "paid evaluation requires the official OpenAI endpoint"
                );
                ensure!(
                    config.budget.is_some(),
                    "paid evaluation requires the cumulative budget guard"
                );
                ensure!(
                    config
                        .budget
                        .as_ref()
                        .is_some_and(|budget| budget.ceiling_usd == Decimal::ONE
                            && budget.initial_charge_usd == Decimal::ONE),
                    "current paid ceiling is USD 1, fully reserved until historical unknown billing is reconciled"
                );
                ensure!(
                    provider.max_concurrency == 1 && provider.max_retries == 0,
                    "paid evaluation requires serial dispatch and no provider retries"
                );
            }
            "meta" => {
                ensure!(
                    model.id == "muse-spark-1.1",
                    "only muse-spark-1.1 is approved for Meta evaluation"
                );
                ensure!(
                    provider.base_url.trim_end_matches('/') == "https://api.meta.ai/v1",
                    "Muse evaluation requires the official Meta endpoint"
                );
                ensure!(
                    config
                        .muse_budget
                        .as_ref()
                        .is_some_and(|budget| budget.ceiling_usd == Decimal::from(15)
                            && budget.initial_charge_usd == Decimal::ZERO),
                    "Muse evaluation requires its separate USD 15 budget with zero historical project carry-in"
                );
                ensure!(
                    provider.max_concurrency == 1 && provider.max_retries == 0,
                    "Muse evaluation requires serial dispatch and no provider retries"
                );
            }
            "nvidia" => {
                bail!("live NVIDIA NIM is disabled for this submission");
            }
            _ => bail!("provider is outside the current owner-approved model scope"),
        }
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
            if provider == "openai" && config.budget.is_some() {
                println!(
                    "paid budget: USD 1 ceiling is reserved for unresolved historical usage; live paid calls remain blocked"
                );
            }
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

fn tool_eligibility_path(root: &Path, provider: &str) -> Option<(PathBuf, &'static str)> {
    match provider {
        "nvidia" => Some((
            root.join("runs/nim-tool-eligibility.json"),
            "https://integrate.api.nvidia.com/v1",
        )),
        "meta" => Some((
            root.join("runs/muse-tool-eligibility.json"),
            "https://api.meta.ai/v1",
        )),
        _ => None,
    }
}

fn record_tool_eligibility(
    root: &Path,
    model: &ModelConfig,
    status: &str,
    run_dir: &Path,
) -> Result<()> {
    let Some((path, base_url)) = tool_eligibility_path(root, &model.provider) else {
        return Ok(());
    };
    let mut checks: BTreeMap<String, Value> = if path.exists() {
        serde_json::from_slice(&fs::read(&path)?)
            .context("invalid tool eligibility record; refusing to replace it")?
    } else {
        BTreeMap::new()
    };
    checks.insert(
        model.id.clone(),
        json!({
            "status":status,"checked_at_utc":Utc::now().to_rfc3339(),
            "source":"provider_api","base_url":base_url,
            "reasoning_effort":model.reasoning_effort,
            "run_id":run_dir.file_name().and_then(|name|name.to_str()),
            "git_commit":command_output(root,"git",&["rev-parse","HEAD"]),
        }),
    );
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(&checks)?)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn require_tool_eligibility(root: &Path, config: &RunConfig, models: &[ModelConfig]) -> Result<()> {
    for model in models {
        let Some((path, base_url)) = tool_eligibility_path(root, &model.provider) else {
            continue;
        };
        if config.providers[&model.provider]
            .resolved(&model.provider)?
            .base_url
            .starts_with("http://")
        {
            continue;
        }
        let checks: BTreeMap<String, Value> =
            serde_json::from_slice(&fs::read(&path).with_context(|| {
                format!("{} requires a passing live tool smoke before run", model.id)
            })?)?;
        ensure!(
            checks
                .get(&model.id)
                .is_some_and(|check| check["status"] == "tool_call_supported"
                    && check["source"] == "provider_api"
                    && check["base_url"] == base_url
                    && (model.provider != "meta"
                        || check["reasoning_effort"] == json!(model.reasoning_effort))),
            "model {} has no passing tool-calling smoke for these settings",
            model.id
        );
        if model.provider == "meta" {
            let signoff = fs::read_to_string(root.join("docs/SIGNOFF.md"))
                .context("Muse scored runs require owner review in docs/SIGNOFF.md")?;
            let status = fs::read_to_string(root.join("docs/STATUS.md"))
                .context("Muse scored runs require owner task sign-off in docs/STATUS.md")?;
            check_muse_owner_signoff(&signoff, &status)?;
        }
    }
    Ok(())
}

const MUSE_OWNER_SIGNOFF_ITEMS: [&str; 2] =
    ["Task and reference sign-off", "Grader and metrics sign-off"];

const MUSE_OWNER_SIGNED_TASKS: [&str; 9] = [
    "t00_control",
    "t01_hidden_fees",
    "t02_expired_offer",
    "t03_compatibility",
    "t04_seller_injection",
    "t05_bundle_vs_unit",
    "t06_ship_vs_delivery",
    "t07_final_sale",
    "t08_no_valid_offer",
];

fn normalized_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn checked_box(value: &str) -> bool {
    matches!(value.trim(), "☑" | "✅")
}

fn markdown_cells(line: &str) -> Option<Vec<String>> {
    let line = line.trim();
    line.contains('|').then(|| {
        line.trim_matches('|')
            .split('|')
            .map(|cell| cell.trim().to_owned())
            .collect()
    })
}

fn check_muse_owner_signoff(signoff: &str, status: &str) -> Result<()> {
    let mut approvals: BTreeMap<&str, Vec<bool>> = BTreeMap::new();
    for line in signoff.lines() {
        let Some(item) = line.trim().strip_prefix('-').map(str::trim) else {
            continue;
        };
        let Some((mark, label)) = item.strip_prefix('[').and_then(|item| item.split_once(']'))
        else {
            continue;
        };
        let label = normalized_whitespace(label);
        for required in MUSE_OWNER_SIGNOFF_ITEMS {
            if label == required {
                approvals
                    .entry(required)
                    .or_default()
                    .push(mark.trim().eq_ignore_ascii_case("x"));
            }
        }
    }
    for required in MUSE_OWNER_SIGNOFF_ITEMS {
        ensure!(
            approvals
                .get(required)
                .is_some_and(|items| items.len() == 1 && items[0]),
            "Muse scored runs require exactly one checked '{required}' box in docs/SIGNOFF.md"
        );
    }

    const EXPECTED_HEADER: [&str; 5] = [
        "Task",
        "Built",
        "Oracle 1.0",
        "Mutants fail",
        "ozgur sign-off",
    ];
    let lines = status.lines().collect::<Vec<_>>();
    let headers = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            let cells = markdown_cells(line)?;
            (cells.len() == EXPECTED_HEADER.len()
                && cells
                    .iter()
                    .zip(EXPECTED_HEADER)
                    .all(|(cell, expected)| cell.as_str() == expected))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    ensure!(
        headers.len() == 1,
        "Muse scored runs require exactly one task sign-off table in docs/STATUS.md"
    );

    let header = headers[0];
    let mut rows: BTreeMap<&str, Vec<bool>> = BTreeMap::new();
    for line in &lines[header + 1..] {
        if line.trim().is_empty() {
            break;
        }
        let Some(cells) = markdown_cells(line) else {
            break;
        };
        if cells.iter().all(|cell| {
            !cell.is_empty() && cell.chars().all(|character| matches!(character, '-' | ':'))
        }) {
            continue;
        }
        let Some(task_cell) = cells.first().map(String::as_str) else {
            continue;
        };
        let Some(task_id) = MUSE_OWNER_SIGNED_TASKS
            .iter()
            .copied()
            .find(|task_id| *task_id == task_cell)
        else {
            continue;
        };
        ensure!(
            cells.len() == EXPECTED_HEADER.len(),
            "Muse task sign-off row for {task_id} has an unexpected number of columns in docs/STATUS.md"
        );
        rows.entry(task_id)
            .or_default()
            .push(checked_box(&cells[4]));
    }

    for task_id in MUSE_OWNER_SIGNED_TASKS {
        ensure!(
            rows.get(task_id).is_some_and(|cells| cells.len() == 1),
            "Muse scored runs require exactly one '{task_id}' row in docs/STATUS.md"
        );
        ensure!(
            rows[task_id][0],
            "Muse scored runs require the '{task_id}' owner sign-off cell checked in docs/STATUS.md"
        );
    }
    Ok(())
}

type Clients = BTreeMap<String, Arc<Client>>;
type PaidBudgets = BTreeMap<String, Arc<BudgetGuard>>;
fn clients(
    root: &Path,
    config: &RunConfig,
    models: &[ModelConfig],
) -> Result<(Clients, PaidBudgets)> {
    let providers: BTreeSet<_> = models.iter().map(|model| model.provider.clone()).collect();
    let mut budgets = PaidBudgets::new();
    for name in &providers {
        let resolved = config.providers[name].resolved(name)?;
        if resolved.base_url.starts_with("http://") {
            continue;
        }
        let (file, budget) = match name.as_str() {
            "openai" => ("paid-budget-ledger.json", &config.budget),
            "meta" => ("muse-spend-ledger.json", &config.muse_budget),
            _ => continue,
        };
        let directory = root.join("runs");
        fs::create_dir_all(&directory)?;
        budgets.insert(
            name.clone(),
            BudgetGuard::open_for_provider(
                directory.join(file),
                name,
                budget.as_ref().context("paid budget is missing")?,
            )?,
        );
    }
    let clients = providers
        .into_iter()
        .map(|name| {
            let mut client = Client::from_env(&name, &config.providers[&name])?;
            if let Some(budget) = budgets.get(&name) {
                client = client.with_budget(budget.clone());
            }
            Ok((name, Arc::new(client)))
        })
        .collect::<Result<_>>()?;
    Ok((clients, budgets))
}

fn admit_paid_batch(
    budgets: &PaidBudgets,
    models: &[ModelConfig],
    trials_per_model: Option<u64>,
    max_turns: usize,
) -> Result<BTreeMap<String, Decimal>> {
    let mut estimates = BTreeMap::new();
    for model in models {
        let Some(budget) = budgets.get(&model.provider) else {
            continue;
        };
        let reservation = budget.projected_request_cost(model)?;
        let estimate = if let Some(trials) = trials_per_model {
            // Planning assumption for repeated text/tool history. The durable
            // reservation still uses the full context bound on every request.
            let output = u64::from(model.max_output_tokens)
                .checked_mul(u64::try_from(max_turns)?)
                .context("projected output token count overflow")?;
            let per_trial = budget.projected_usage_cost(100_000, output)?;
            per_trial
                .checked_mul(Decimal::from(trials))
                .context("projected batch cost overflow")?
                .max(reservation)
        } else {
            reservation * Decimal::from(if model.provider == "meta" { 2 } else { 1 })
        };
        budget.ensure_batch_fits(estimate).with_context(|| {
            format!(
                "{} batch requires USD {estimate} of available budget",
                model.provider
            )
        })?;
        estimates.insert(model.provider.clone(), estimate);
    }
    Ok(estimates)
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

const MUSE_SMOKE_ECHO: &str = "smoke-roundtrip-confirmed";

fn smoke_is_complete(reply: &ModelReply) -> bool {
    !matches!(
        reply.finish_reason.as_deref(),
        Some("length" | "incomplete" | "max_output_tokens" | "content_filter")
    )
}

fn smoke_called_echo(reply: &ModelReply) -> bool {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct EchoArgs {
        value: String,
    }

    smoke_is_complete(reply)
        && reply.calls.len() == 1
        && reply.calls[0].name == "smoke_echo"
        && serde_json::from_str::<EchoArgs>(&reply.calls[0].arguments)
            .is_ok_and(|args| args.value == "ready")
}

async fn finish_muse_smoke(
    client: &dyn CompletionBackend,
    model: &ModelConfig,
    history: &[Value],
    tools: &[ToolDefinition],
    reply: ModelReply,
) -> (&'static str, Value, Value, Vec<RequestRecord>) {
    let output = tool_result(
        client.api(),
        &reply.calls[0],
        &json!({"value": MUSE_SMOKE_ECHO}),
    );
    let mut continuation = history.to_vec();
    continuation.extend(reply.history.iter().cloned());
    continuation.push(output.clone());
    continuation.push(json!({"role":"user","content":"Reply with exactly the value in the tool result. Do not make any further tool calls."}));
    match client
        .complete(model, &continuation, tools, ToolChoice::Auto)
        .await
    {
        Ok(next) => {
            let passed = next.finish_reason.as_deref() == Some("completed")
                && next.calls.is_empty()
                && next.text.trim() == MUSE_SMOKE_ECHO
                && next.record.model.as_deref() == Some(model.id.as_str());
            let status = if passed {
                "tool_call_supported"
            } else {
                "invalid_tool_round_trip"
            };
            let transcript = json!({"source":"smoke","initial_messages":history,"reply":reply,"tool_output":output,"continuation_input":continuation,"continuation_reply":next});
            (
                status,
                transcript,
                json!([reply.raw, next.raw]),
                vec![reply.record, next.record],
            )
        }
        Err(error) => {
            let transcript = json!({"source":"smoke","initial_messages":history,"reply":reply,"tool_output":output,"continuation_input":continuation,"api_error":error});
            (
                "api_error",
                transcript,
                json!([reply.raw, error.raw]),
                vec![reply.record, *error.record],
            )
        }
    }
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
    let (clients, budgets) = clients(root, &config, &models)?;
    let estimates = admit_paid_batch(&budgets, &models, None, config.max_turns)?;
    let mut artifacts = Artifacts::create(root, out, "smoke", &config, &models, &[])?;
    artifacts.budgets = budgets;
    artifacts.metadata["projected_paid_usd"] = json!(estimates);
    artifacts.flush("running")?;
    let tools = vec![ToolDefinition {
        name: "smoke_echo".into(),
        description: "Confirm function-calling transport support.".into(),
        parameters: json!({"type":"object","properties":{"value":{"type":"string","enum":["ready"]}},"required":["value"],"additionalProperties":false}),
    }];
    let mut failures = 0;
    for (index, model) in models.iter().enumerate() {
        let id = format!("model-{index}");
        let instruction = if model.provider == "meta" {
            "You must call smoke_echo exactly once with value ready. Do not answer in plain text before calling the tool."
        } else {
            "Call smoke_echo with value ready. This is a transport check; do not answer with text."
        };
        let history = [json!({"role":"user","content":instruction})];
        let choice = if matches!(model.provider.as_str(), "meta" | "nvidia")
            || (model.provider == "deepseek"
                && model
                    .thinking
                    .unwrap_or(model.reasoning_effort.as_deref() != Some("none")))
        {
            ToolChoice::Auto
        } else {
            ToolChoice::Required("smoke_echo".into())
        };
        let result = clients[&model.provider]
            .complete(model, &history, &tools, choice)
            .await;
        let (status, transcript, raw, records) = match result {
            Ok(reply)
                if model.provider == "meta"
                    && smoke_called_echo(&reply)
                    && reply.record.model.as_deref() == Some(model.id.as_str()) =>
            {
                finish_muse_smoke(
                    clients[&model.provider].as_ref(),
                    model,
                    &history,
                    &tools,
                    reply,
                )
                .await
            }
            Ok(reply) => {
                let status = if model.provider == "meta"
                    && reply.record.model.as_deref() != Some(model.id.as_str())
                {
                    "unexpected_model"
                } else if smoke_called_echo(&reply) {
                    "tool_call_supported"
                } else {
                    "no_tool_call"
                };
                let transcript =
                    json!({"source":"smoke", "initial_messages": history, "reply": reply});
                (status, transcript, reply.raw, vec![reply.record])
            }
            Err(error) => {
                let transcript =
                    json!({"source":"smoke", "initial_messages": history,"api_error": error});
                (
                    "api_error",
                    transcript,
                    error.raw.unwrap_or(Value::Null),
                    vec![*error.record],
                )
            }
        };
        let cost = sum_costs(
            &records
                .iter()
                .map(|record| request_cost(model, record))
                .collect::<Vec<_>>(),
        );
        let record = records.last().expect("probe records at least one request");
        let row = json!({"item_id":id,"status":status,"provider":model.provider,"requested_model":model.id,"model":record.model,"wire_api":record.api,"usage":sum_usage(&records),"cost_estimate_usd":cost});
        artifacts.save(&id, &transcript, &raw, row, records, cost)?;
        if artifacts.metadata["source"] == "provider_api" {
            record_tool_eligibility(root, model, status, &artifacts.run_dir)?;
        }
        println!("{}: {status}", model.id);
        failures += usize::from(status != "tool_call_supported");
        if status == "api_error" {
            artifacts.metadata["unstarted_items"] = json!(models.len() - index - 1);
            artifacts.metadata["halted_providers"] = json!([model.provider]);
            break;
        }
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

struct TrialJob {
    model_index: usize,
    model: ModelConfig,
    task: Task,
    reference: Reference,
    trial: u32,
}

fn start_trial(
    jobs: &mut JoinSet<(TrialJob, LiveEpisode)>,
    job: TrialJob,
    client: Arc<Client>,
    max_turns: usize,
) {
    jobs.spawn(async move {
        let episode = run_episode(client.as_ref(), &job.model, job.task.clone(), max_turns).await;
        (job, episode)
    });
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
    require_tool_eligibility(root, &config, &models)?;
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
    let (clients, budgets) = clients(root, &config, &models)?;
    let trials = u64::try_from(tasks.len())?
        .checked_mul(u64::from(config.trials))
        .context("batch trial count overflow")?;
    let estimates = admit_paid_batch(&budgets, &models, Some(trials), config.max_turns)?;
    let visible_tasks = tasks
        .iter()
        .map(|(task, _)| task.clone())
        .collect::<Vec<_>>();
    let mut artifacts = Artifacts::create(root, out, "run", &config, &models, &visible_tasks)?;
    artifacts.budgets = budgets;
    artifacts.metadata["projected_paid_usd"] = json!(estimates);
    artifacts.flush("running")?;
    let mut jobs = JoinSet::new();
    let mut pending: BTreeMap<String, VecDeque<TrialJob>> = BTreeMap::new();
    for (model_index, model) in models.iter().enumerate() {
        for (task, reference) in &tasks {
            for trial in 1..=config.trials {
                pending
                    .entry(model.provider.clone())
                    .or_default()
                    .push_back(TrialJob {
                        model_index,
                        model: model.clone(),
                        task: task.clone(),
                        reference: reference.clone(),
                        trial,
                    });
            }
        }
    }
    for (provider, queue) in &mut pending {
        for _ in 0..config.providers[provider].max_concurrency {
            if let Some(job) = queue.pop_front() {
                start_trial(&mut jobs, job, clients[provider].clone(), config.max_turns);
            }
        }
    }
    let mut halted_providers = BTreeSet::new();
    while let Some(result) = jobs.join_next().await {
        let (job, episode) = match result {
            Ok(result) => result,
            Err(_) => {
                jobs.abort_all();
                artifacts.flush("incomplete")?;
                bail!(
                    "trial worker failed; partial artifacts retained, no trials automatically rerun"
                );
            }
        };
        let TrialJob {
            model_index,
            model,
            task,
            reference,
            trial,
        } = job;
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
        let mut metrics = if matches!(episode.outcome, EpisodeOutcome::ApiError) {
            None
        } else {
            Some(evaluate_metrics_v2(
                &task,
                &reference,
                &episode.environment,
                &episode.transcript.final_output,
            ))
        };
        if !matches!(episode.outcome, EpisodeOutcome::Complete)
            && let Some(metrics) = &mut metrics
        {
            metrics.outcome_pass = false;
            metrics.strict_pass = false;
        }
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
                (
                    if metrics.as_ref().is_some_and(|metrics| metrics.outcome_pass) {
                        "pass"
                    } else {
                        "fail"
                    },
                    Some(grading),
                )
            }
        };
        let returned_models = records
            .iter()
            .filter_map(|record| record.model.clone())
            .collect::<BTreeSet<_>>();
        let label = match episode.outcome {
            EpisodeOutcome::ApiError => Some("api_error".to_owned()),
            EpisodeOutcome::InvalidToolCall => Some("tool_protocol_failure".to_owned()),
            EpisodeOutcome::NoDecision => Some("no_decision".to_owned()),
            EpisodeOutcome::Complete if status != "pass" => {
                grade_authorized_outcome(
                    &task,
                    &reference,
                    &episode.environment,
                    &episode.transcript.final_output,
                )
                .failure_label
            }
            _ => None,
        };
        let row = json!({"trial_id":id,"task_id":task.task_id,"task_version":task.task_version,"trial":trial,"provider":model.provider,"requested_model":model.id,"returned_models":returned_models,"status":status,"episode_outcome":outcome_value,"grade":grading,"failure_label":label,"usage":sum_usage(&records),"cost_estimate_usd":cost,"cost_is_estimate":true,"metrics_policy_version":2,"outcome_pass":metrics.as_ref().map(|metrics|metrics.outcome_pass),"strict_pass":metrics.as_ref().map(|metrics|metrics.strict_pass),"format_compliant":metrics.as_ref().map(|metrics|metrics.format_compliant),"final_answer_present":!episode.transcript.final_output.trim().is_empty(),"tool_protocol_failure":matches!(episode.outcome, EpisodeOutcome::InvalidToolCall)});
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
        if status == "api_error" {
            halted_providers.insert(model.provider.clone());
        }
        if !halted_providers.contains(&model.provider)
            && let Some(job) = pending
                .get_mut(&model.provider)
                .and_then(VecDeque::pop_front)
        {
            start_trial(
                &mut jobs,
                job,
                clients[&model.provider].clone(),
                config.max_turns,
            );
        }
    }
    let errors = artifacts
        .scores
        .iter()
        .filter(|row| row["status"] == "api_error")
        .count();
    let unstarted = pending.values().map(VecDeque::len).sum::<usize>();
    artifacts.metadata["unstarted_trials"] = json!(unstarted);
    artifacts.metadata["halted_providers"] = json!(halted_providers);
    if unstarted > 0 {
        artifacts.flush("incomplete")?;
        println!("partial run artifacts: {}", artifacts.run_dir.display());
        bail!(
            "provider API error stopped dispatch; {unstarted} trials were not started, prior outputs retained"
        );
    }
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

    #[test]
    fn muse_batch_admission_checks_remaining_spend_before_dispatch() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = load_config(&repo.join("configs/muse-diagnostic.toml")).unwrap();
        let root =
            std::env::temp_dir().join(format!("mandate-muse-admission-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let ledger = BudgetGuard::open_for_provider(
            root.join("ledger.json"),
            "meta",
            config.muse_budget.as_ref().unwrap(),
        )
        .unwrap();
        let budgets = BTreeMap::from([("meta".to_owned(), ledger.clone())]);
        let estimates = admit_paid_batch(&budgets, &config.models, Some(12), 20).unwrap();
        assert_eq!(estimates["meta"], Decimal::new(358896, 5));
        assert!(admit_paid_batch(&budgets, &config.models, Some(51), 20).is_err());
        let smoke_estimate = admit_paid_batch(&budgets, &config.models, None, 2).unwrap();
        assert_eq!(smoke_estimate["meta"], Decimal::new(2638848, 6));
        assert_eq!(ledger.snapshot().unwrap().reservation_count, 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn nim_admission_requires_a_passing_check_and_a_later_failure_revokes_it() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = load_config(&repo.join("configs/historical/nim-smoke.toml")).unwrap();
        let models = &config.models[..1];
        let root =
            std::env::temp_dir().join(format!("mandate-nim-admission-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("runs")).unwrap();
        assert!(require_tool_eligibility(&root, &config, models).is_err());
        for status in ["no_tool_call", "tool_call_supported", "api_error"] {
            record_tool_eligibility(
                &root,
                &models[0],
                status,
                &root.join("runs/synthetic-test-only"),
            )
            .unwrap();
            assert_eq!(
                require_tool_eligibility(&root, &config, models).is_ok(),
                status == "tool_call_supported"
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
}
