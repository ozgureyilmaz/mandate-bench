//! Offline metrics and artifact validation for completed provider-backed runs.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{Context, Result, bail, ensure};
use rust_decimal::Decimal;
use serde_json::{Value, json};

const K_VALUES: [u64; 3] = [1, 3, 5];

#[derive(Clone)]
struct ModelSpec {
    id: String,
    provider: String,
    price: Option<mb_llm::Price>,
}

#[derive(Clone)]
struct TaskSpec {
    id: String,
    version: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TrialStatus {
    Pass,
    Fail,
    ApiError,
}

struct ScoreRow {
    task_id: String,
    provider: String,
    requested_model: String,
    returned_models: BTreeSet<String>,
    status: TrialStatus,
    cost: Option<Decimal>,
}

struct RunInput {
    run_dir: PathBuf,
    run_id: String,
    state: String,
    scope: &'static str,
    models: Vec<ModelSpec>,
    tasks: Vec<TaskSpec>,
    trials_per_task: u32,
    metadata_known_cost: Decimal,
    successful_request_costs: BTreeMap<String, Decimal>,
    successful_request_counts: BTreeMap<String, u64>,
    successful_request_count: u64,
    rows: Vec<ScoreRow>,
}

#[derive(Clone)]
struct Counts {
    expected_trials: u64,
    completed_trials: u64,
    n: u64,
    c: u64,
    api_errors: u64,
    pass_rate: Option<f64>,
}

struct TaskMetrics {
    task_id: String,
    counts: Counts,
    pass_power: BTreeMap<String, Option<f64>>,
    pass_at: BTreeMap<String, Option<f64>>,
}

struct KAverage {
    value: Option<f64>,
    eligible_task_count: u64,
}

struct WilsonInterval {
    low: f64,
    high: f64,
}

struct TaskSpread {
    min: f64,
    min_task: String,
    max: f64,
    max_task: String,
}

struct CostSummary {
    known_cost_subtotal: Decimal,
    known_successful_request_subtotal: Decimal,
    known_trial_cost_subtotal: Decimal,
    total_estimate: Option<Decimal>,
    unknown_cost_trials: u64,
}

struct ModelMetrics {
    provider: String,
    requested_model: String,
    returned_model_ids: Vec<String>,
    overall: Counts,
    control: Counts,
    traps: Counts,
    tasks: Vec<TaskMetrics>,
    pass_power: BTreeMap<String, KAverage>,
    pass_at: BTreeMap<String, KAverage>,
    wilson95: Option<WilsonInterval>,
    task_spread: Option<TaskSpread>,
    costs: CostSummary,
    priced_successful_request_count: u64,
}

pub fn generate(root: &Path, run_arg: &Path, allow_scratch: bool) -> Result<()> {
    let input = load_run(root, run_arg, allow_scratch)?;
    let models = calculate_metrics(&input)?;
    let report = report_json(&input, &models)?;
    let summary = summary_csv(&input, &models)?;

    write_generated(&input.run_dir, "summary.csv", summary.as_bytes())?;
    write_generated(
        &input.run_dir,
        "report.json",
        &serde_json::to_vec_pretty(&report)?,
    )?;

    print_summary(&input, &models, &report);
    Ok(())
}

fn load_run(root_arg: &Path, run_arg: &Path, allow_scratch: bool) -> Result<RunInput> {
    let root = fs::canonicalize(root_arg).context("repository root does not exist")?;
    ensure!(root.is_dir(), "repository root must be a directory");
    let requested_path = if run_arg.is_absolute() {
        run_arg.to_path_buf()
    } else {
        root.join(run_arg)
    };
    let run_metadata = fs::symlink_metadata(&requested_path)
        .context("run directory does not exist or cannot be inspected")?;
    ensure!(
        run_metadata.file_type().is_dir(),
        "run path must be a real directory, not a symlink"
    );
    let run_dir = fs::canonicalize(&requested_path).context("resolving run directory")?;
    let (scope, expected_transcript_parent) = locate_run(&root, &run_dir, allow_scratch)?;

    let run_name = run_dir
        .file_name()
        .and_then(|name| name.to_str())
        .context("run directory name is not valid UTF-8")?;
    ensure!(safe_run_id(run_name), "run directory name is unsafe");

    let meta_path = run_dir.join("run_meta.json");
    require_regular_file(&meta_path, "run_meta.json")?;
    let metadata = read_json(&meta_path).context("reading run metadata")?;
    ensure!(metadata.is_object(), "run metadata must be a JSON object");
    let kind = required_str(&metadata, "kind")?;
    let source = required_str(&metadata, "source")?;
    let state = required_str(&metadata, "state")?.to_owned();
    ensure!(kind == "run", "report accepts only benchmark run artifacts");
    ensure!(
        source == "provider_api",
        "report accepts only provider_api runs; smoke and local_mock artifacts are not benchmark results"
    );
    ensure!(
        matches!(state.as_str(), "completed" | "completed_with_api_errors"),
        "run metadata is incomplete or not in a reportable state"
    );
    let run_id = required_str(&metadata, "run_id")?.to_owned();
    ensure!(
        safe_run_id(&run_id) && run_id == run_name,
        "run ID must be safe and match its directory name"
    );

    let models = parse_models(metadata.get("models"))?;
    let tasks = parse_tasks(metadata.get("tasks"))?;
    let trials_per_task = metadata
        .get("trials_per_task")
        .and_then(Value::as_u64)
        .filter(|trials| *trials > 0 && *trials <= u64::from(u32::MAX))
        .context("run metadata is missing a positive trials_per_task")?
        as u32;
    let metadata_known_cost = decimal_field(&metadata, "known_cost_estimate_usd")?
        .context("run metadata is missing known_cost_estimate_usd")?;
    ensure!(
        metadata_known_cost >= Decimal::ZERO,
        "known cost must be nonnegative"
    );
    let (successful_request_costs, successful_request_counts) =
        successful_request_costs(metadata.get("requests"), &models)?;
    let successful_request_count = successful_request_counts.values().sum();

    let output_dir = real_child_dir(&run_dir, "outputs")?;
    let transcript_dir = if scope == "benchmark_results" {
        real_child_dir(&expected_transcript_parent, &run_id)?
    } else {
        real_child_dir(&run_dir, "transcripts")?
    };
    let scores_path = run_dir.join("scores.jsonl");
    require_regular_file(&scores_path, "scores.jsonl")?;
    let rows = parse_score_rows(
        &scores_path,
        &output_dir,
        &transcript_dir,
        &models,
        &tasks,
        trials_per_task,
    )?;
    validate_coverage(&models, &tasks, trials_per_task, &rows)?;

    let api_errors = rows
        .iter()
        .filter(|row| row.status == TrialStatus::ApiError)
        .count() as u64;
    let passes = rows
        .iter()
        .filter(|row| row.status == TrialStatus::Pass)
        .count() as u64;
    ensure!(
        required_u64(&metadata, "completed_items")? == rows.len() as u64,
        "run metadata completed_items does not match scores.jsonl"
    );
    ensure!(
        required_u64(&metadata, "api_error_count")? == api_errors,
        "run metadata api_error_count does not match scores.jsonl"
    );
    ensure!(
        required_u64(&metadata, "pass_count")? == passes,
        "run metadata pass_count does not match scores.jsonl"
    );
    ensure!(
        (api_errors == 0 && state == "completed")
            || (api_errors > 0 && state == "completed_with_api_errors"),
        "run state does not agree with its API error rows"
    );

    Ok(RunInput {
        run_dir,
        run_id,
        state,
        scope,
        models,
        tasks,
        trials_per_task,
        metadata_known_cost,
        successful_request_costs,
        successful_request_counts,
        successful_request_count,
        rows,
    })
}

fn locate_run(root: &Path, run_dir: &Path, allow_scratch: bool) -> Result<(&'static str, PathBuf)> {
    let results = optional_real_child_dir(root, "results")?;
    if results
        .as_ref()
        .is_some_and(|parent| run_dir.parent() == Some(parent.as_path()))
    {
        let transcripts = real_child_dir(root, "transcripts")?;
        return Ok(("benchmark_results", transcripts));
    }

    let runs = optional_real_child_dir(root, "runs")?;
    if runs
        .as_ref()
        .is_some_and(|parent| run_dir.parent() == Some(parent.as_path()))
    {
        ensure!(
            allow_scratch,
            "run path is under runs/; pass --allow-scratch to produce a diagnostic report"
        );
        return Ok(("diagnostic_scratch", run_dir.to_path_buf()));
    }
    bail!("run directory must be a direct child of root/results or root/runs")
}

fn optional_real_child_dir(parent: &Path, name: &str) -> Result<Option<PathBuf>> {
    let path = parent.join(name);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("inspecting {name}/")),
    };
    ensure!(
        metadata.file_type().is_dir(),
        "{name}/ must be a real directory"
    );
    let resolved = fs::canonicalize(&path).with_context(|| format!("resolving {name}/"))?;
    ensure!(
        resolved.starts_with(parent),
        "{name}/ resolves outside the repository root"
    );
    Ok(Some(resolved))
}

fn real_child_dir(parent: &Path, name: &str) -> Result<PathBuf> {
    let path = parent.join(name);
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("required directory {name}/ is missing"))?;
    ensure!(
        metadata.file_type().is_dir(),
        "{name}/ must be a real directory"
    );
    let resolved = fs::canonicalize(&path).with_context(|| format!("resolving {name}/"))?;
    ensure!(
        resolved.parent() == Some(parent),
        "{name}/ must remain directly under its expected parent"
    );
    Ok(resolved)
}

fn require_regular_file(path: &Path, description: &str) -> Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("required {description} is missing"))?;
    ensure!(
        metadata.file_type().is_file(),
        "required {description} must be a regular file"
    );
    Ok(())
}

fn read_json(path: &Path) -> Result<Value> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))
}

fn parse_models(value: Option<&Value>) -> Result<Vec<ModelSpec>> {
    let values = value
        .and_then(Value::as_array)
        .context("run metadata models must be an array")?;
    ensure!(
        !values.is_empty(),
        "run metadata must select at least one model"
    );
    let mut seen = BTreeSet::new();
    values
        .iter()
        .map(|model| {
            let id = required_str(model, "id")?.to_owned();
            let provider = required_str(model, "provider")?.to_owned();
            ensure!(
                !id.trim().is_empty(),
                "requested model ID must not be empty"
            );
            ensure!(
                !provider.trim().is_empty(),
                "model provider must not be empty"
            );
            ensure!(
                seen.insert(id.clone()),
                "run metadata contains duplicate model IDs"
            );
            let price = match model.get("price") {
                Some(Value::Null) => None,
                Some(value) => Some(
                    serde_json::from_value(value.clone())
                        .context("parsing selected model price metadata")?,
                ),
                None => bail!("run metadata model is missing its price field"),
            };
            Ok(ModelSpec {
                id,
                provider,
                price,
            })
        })
        .collect()
}

fn parse_tasks(value: Option<&Value>) -> Result<Vec<TaskSpec>> {
    let values = value
        .and_then(Value::as_array)
        .context("run metadata tasks must be an array")?;
    ensure!(
        !values.is_empty(),
        "run metadata must select at least one task"
    );
    let mut seen = BTreeSet::new();
    values
        .iter()
        .map(|task| {
            let id = required_str(task, "task_id")?.to_owned();
            let version = required_u64(task, "task_version")?;
            ensure!(!id.trim().is_empty(), "task ID must not be empty");
            ensure!(
                version > 0 && version <= u64::from(u32::MAX),
                "invalid task version"
            );
            ensure!(
                seen.insert(id.clone()),
                "run metadata contains duplicate task IDs"
            );
            Ok(TaskSpec {
                id,
                version: version as u32,
            })
        })
        .collect()
}

fn successful_request_costs(
    value: Option<&Value>,
    models: &[ModelSpec],
) -> Result<(BTreeMap<String, Decimal>, BTreeMap<String, u64>)> {
    let requests = value
        .and_then(Value::as_array)
        .context("run metadata requests must be an array")?;
    let model_map = models
        .iter()
        .map(|model| (model.id.as_str(), model))
        .collect::<BTreeMap<_, _>>();
    let mut totals = BTreeMap::new();
    let mut counts = BTreeMap::new();
    for model in models {
        totals.insert(model.id.clone(), Decimal::ZERO);
        counts.insert(model.id.clone(), 0_u64);
    }

    for request in requests {
        ensure!(
            request.is_object(),
            "run metadata request records must be objects"
        );
        let requested_model = required_str(request, "requested_model")?;
        let provider = required_str(request, "provider")?;
        let model = model_map
            .get(requested_model)
            .context("request record names a model absent from run metadata")?;
        ensure!(
            provider == model.provider,
            "request provider does not match selected model"
        );
        let retries = required_u64(request, "retries")?;
        let status = request.get("http_status").and_then(Value::as_u64);
        if retries > 0 || !status.is_some_and(|status| (200..300).contains(&status)) {
            continue;
        }
        let usage_value = request
            .get("usage")
            .context("request record is missing token usage")?;
        let usage: mb_llm::TokenUsage =
            serde_json::from_value(usage_value.clone()).context("parsing request token usage")?;
        let Some(cost) = mb_llm::estimate_cost(model.price.as_ref(), &usage) else {
            continue;
        };
        let current = totals
            .get_mut(requested_model)
            .context("known request model was not initialized")?;
        *current = current
            .checked_add(cost)
            .context("successful request cost subtotal overflow")?;
        *counts
            .get_mut(requested_model)
            .context("priced request model was not initialized")? += 1;
    }

    Ok((totals, counts))
}

fn parse_score_rows(
    path: &Path,
    output_dir: &Path,
    transcript_dir: &Path,
    models: &[ModelSpec],
    tasks: &[TaskSpec],
    trials_per_task: u32,
) -> Result<Vec<ScoreRow>> {
    let file = fs::File::open(path).context("opening scores.jsonl")?;
    let mut trial_ids = BTreeSet::new();
    let mut combinations = BTreeSet::new();
    let model_map = models
        .iter()
        .map(|model| (model.id.as_str(), model.provider.as_str()))
        .collect::<BTreeMap<_, _>>();
    let task_map = tasks
        .iter()
        .map(|task| (task.id.as_str(), task.version))
        .collect::<BTreeMap<_, _>>();
    let mut rows = Vec::new();

    for (line_index, line) in BufReader::new(file).lines().enumerate() {
        let line = line.with_context(|| format!("reading scores.jsonl line {}", line_index + 1))?;
        ensure!(
            !line.trim().is_empty(),
            "scores.jsonl contains a blank line"
        );
        let value: Value = serde_json::from_str(&line)
            .with_context(|| format!("parsing scores.jsonl line {}", line_index + 1))?;
        ensure!(value.is_object(), "each score row must be a JSON object");

        let trial_id = required_str(&value, "trial_id")?.to_owned();
        ensure!(safe_id(&trial_id), "score row contains an unsafe trial_id");
        ensure!(
            trial_ids.insert(trial_id.clone()),
            "scores.jsonl contains a duplicate trial_id"
        );
        let task_id = required_str(&value, "task_id")?.to_owned();
        let requested_model = required_str(&value, "requested_model")?.to_owned();
        let provider = required_str(&value, "provider")?.to_owned();
        let task_version = required_u64(&value, "task_version")?;
        let trial = required_u64(&value, "trial")?;
        let expected_provider = model_map
            .get(requested_model.as_str())
            .context("score row names a model absent from run metadata")?;
        ensure!(
            provider == **expected_provider,
            "score row provider does not match run metadata"
        );
        let expected_version = task_map
            .get(task_id.as_str())
            .context("score row names a task absent from run metadata")?;
        ensure!(
            task_version == u64::from(*expected_version),
            "score row task version does not match run metadata"
        );
        ensure!(
            trial > 0 && trial <= u64::from(trials_per_task),
            "score row trial index is outside the configured trial range"
        );
        ensure!(
            combinations.insert((requested_model.clone(), task_id.clone(), trial)),
            "scores.jsonl contains a duplicate model/task/trial row"
        );

        let status = match required_str(&value, "status")? {
            "pass" => {
                ensure!(
                    value
                        .get("grade")
                        .and_then(|grade| grade.get("pass"))
                        .and_then(Value::as_bool)
                        == Some(true),
                    "pass score row must contain grade.pass=true"
                );
                TrialStatus::Pass
            }
            "fail" => {
                ensure!(
                    value
                        .get("grade")
                        .and_then(|grade| grade.get("pass"))
                        .and_then(Value::as_bool)
                        == Some(false),
                    "fail score row must contain grade.pass=false"
                );
                TrialStatus::Fail
            }
            "api_error" => {
                ensure!(
                    value.get("grade") == Some(&Value::Null),
                    "api_error score row must be ungraded"
                );
                TrialStatus::ApiError
            }
            _ => bail!("score row contains an unsupported status"),
        };

        let returned = value
            .get("returned_models")
            .and_then(Value::as_array)
            .context("score row returned_models must be an array")?;
        let mut returned_models = BTreeSet::new();
        for model_id in returned {
            let model_id = model_id
                .as_str()
                .filter(|model_id| !model_id.trim().is_empty())
                .context("returned model IDs must be nonempty strings")?;
            returned_models.insert(model_id.to_owned());
        }
        let cost = decimal_field(&value, "cost_estimate_usd")?;
        ensure!(
            cost.is_none_or(|cost| cost >= Decimal::ZERO),
            "trial cost must be nonnegative"
        );

        let output = output_dir.join(format!("{trial_id}.json"));
        let transcript = transcript_dir.join(format!("{trial_id}.json"));
        require_regular_file(&output, "raw trial output")?;
        require_regular_file(&transcript, "trial transcript")?;

        rows.push(ScoreRow {
            task_id,
            provider,
            requested_model,
            returned_models,
            status,
            cost,
        });
    }
    ensure!(!rows.is_empty(), "scores.jsonl contains no trial rows");
    Ok(rows)
}

fn validate_coverage(
    models: &[ModelSpec],
    tasks: &[TaskSpec],
    trials_per_task: u32,
    rows: &[ScoreRow],
) -> Result<()> {
    let expected = u64::try_from(models.len())
        .ok()
        .and_then(|models| models.checked_mul(tasks.len() as u64))
        .and_then(|count| count.checked_mul(u64::from(trials_per_task)))
        .context("expected trial count overflows")?;
    ensure!(
        rows.len() as u64 == expected,
        "scores.jsonl does not cover every configured model/task/trial combination"
    );
    Ok(())
}

fn calculate_metrics(input: &RunInput) -> Result<Vec<ModelMetrics>> {
    input
        .models
        .iter()
        .map(|model| {
            let rows = input
                .rows
                .iter()
                .filter(|row| row.requested_model == model.id && row.provider == model.provider)
                .collect::<Vec<_>>();
            let returned_model_ids = rows
                .iter()
                .flat_map(|row| row.returned_models.iter().cloned())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let overall = counts(
                &rows,
                (input.tasks.len() as u64) * u64::from(input.trials_per_task),
            );
            let control_rows = rows
                .iter()
                .copied()
                .filter(|row| row.task_id == "t00_control")
                .collect::<Vec<_>>();
            let trap_rows = rows
                .iter()
                .copied()
                .filter(|row| row.task_id != "t00_control")
                .collect::<Vec<_>>();
            let control_tasks = input
                .tasks
                .iter()
                .filter(|task| task.id == "t00_control")
                .count();
            let trap_tasks = input.tasks.len() - control_tasks;
            let control = counts(
                &control_rows,
                (control_tasks as u64) * u64::from(input.trials_per_task),
            );
            let traps = counts(
                &trap_rows,
                (trap_tasks as u64) * u64::from(input.trials_per_task),
            );

            let mut task_metrics = Vec::new();
            for task in &input.tasks {
                let task_rows = rows
                    .iter()
                    .copied()
                    .filter(|row| row.task_id == task.id)
                    .collect::<Vec<_>>();
                let task_counts = counts(&task_rows, u64::from(input.trials_per_task));
                let pass_power = K_VALUES
                    .into_iter()
                    .map(|k| (k.to_string(), pass_power(task_counts.c, task_counts.n, k)))
                    .collect();
                let pass_at = K_VALUES
                    .into_iter()
                    .map(|k| (k.to_string(), pass_at(task_counts.c, task_counts.n, k)))
                    .collect();
                task_metrics.push(TaskMetrics {
                    task_id: task.id.clone(),
                    counts: task_counts,
                    pass_power,
                    pass_at,
                });
            }

            let pass_power = K_VALUES
                .into_iter()
                .map(|k| {
                    (
                        k.to_string(),
                        average_k(&task_metrics, |task| task.pass_power[&k.to_string()]),
                    )
                })
                .collect();
            let pass_at = K_VALUES
                .into_iter()
                .map(|k| {
                    (
                        k.to_string(),
                        average_k(&task_metrics, |task| task.pass_at[&k.to_string()]),
                    )
                })
                .collect();

            let task_rates = task_metrics
                .iter()
                .filter_map(|task| {
                    task.counts
                        .pass_rate
                        .map(|rate| (task.task_id.clone(), rate))
                })
                .collect::<Vec<_>>();
            let task_spread = spread(&task_rates);
            let request_cost = input
                .successful_request_costs
                .get(&model.id)
                .copied()
                .unwrap_or(Decimal::ZERO);
            let priced_successful_request_count = input
                .successful_request_counts
                .get(&model.id)
                .copied()
                .unwrap_or(0);
            let metadata_fallback = (input.models.len() == 1).then_some(input.metadata_known_cost);
            let cost_summary = cost_summary(&rows, request_cost, metadata_fallback)?;

            Ok(ModelMetrics {
                provider: model.provider.clone(),
                requested_model: model.id.clone(),
                returned_model_ids,
                overall: overall.clone(),
                control,
                traps,
                tasks: task_metrics,
                pass_power,
                pass_at,
                wilson95: wilson(overall.c, overall.n),
                task_spread,
                costs: cost_summary,
                priced_successful_request_count,
            })
        })
        .collect()
}

fn counts(rows: &[&ScoreRow], expected_trials: u64) -> Counts {
    let c = rows
        .iter()
        .filter(|row| row.status == TrialStatus::Pass)
        .count() as u64;
    let api_errors = rows
        .iter()
        .filter(|row| row.status == TrialStatus::ApiError)
        .count() as u64;
    let n = rows.len() as u64 - api_errors;
    Counts {
        expected_trials,
        completed_trials: rows.len() as u64,
        n,
        c,
        api_errors,
        pass_rate: (n > 0).then_some(c as f64 / n as f64),
    }
}

fn pass_power(c: u64, n: u64, k: u64) -> Option<f64> {
    if n < k {
        return None;
    }
    if c < k {
        return Some(0.0);
    }
    Some((0..k).fold(1.0, |value, index| {
        value * (c - index) as f64 / (n - index) as f64
    }))
}

fn pass_at(c: u64, n: u64, k: u64) -> Option<f64> {
    if n < k {
        return None;
    }
    let failures = n - c;
    if failures < k {
        return Some(1.0);
    }
    Some(
        1.0 - (0..k).fold(1.0, |value, index| {
            value * (failures - index) as f64 / (n - index) as f64
        }),
    )
}

fn average_k(tasks: &[TaskMetrics], select: impl Fn(&TaskMetrics) -> Option<f64>) -> KAverage {
    let values = tasks.iter().filter_map(select).collect::<Vec<_>>();
    KAverage {
        value: (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64),
        eligible_task_count: values.len() as u64,
    }
}

fn wilson(c: u64, n: u64) -> Option<WilsonInterval> {
    if n == 0 {
        return None;
    }
    let z = 1.959_963_984_540_054_f64;
    let n = n as f64;
    let p = c as f64 / n;
    let z_squared = z * z;
    let denominator = 1.0 + z_squared / n;
    let center = (p + z_squared / (2.0 * n)) / denominator;
    let margin = z * ((p * (1.0 - p) / n + z_squared / (4.0 * n * n)).sqrt()) / denominator;
    Some(WilsonInterval {
        low: (center - margin).max(0.0),
        high: (center + margin).min(1.0),
    })
}

fn spread(rates: &[(String, f64)]) -> Option<TaskSpread> {
    let (min_task, min) = rates
        .iter()
        .min_by(|left, right| left.1.total_cmp(&right.1))?;
    let (max_task, max) = rates
        .iter()
        .max_by(|left, right| left.1.total_cmp(&right.1))?;
    Some(TaskSpread {
        min: *min,
        min_task: min_task.clone(),
        max: *max,
        max_task: max_task.clone(),
    })
}

fn cost_summary(
    rows: &[&ScoreRow],
    request_known: Decimal,
    metadata_known: Option<Decimal>,
) -> Result<CostSummary> {
    let mut trial_known = Decimal::ZERO;
    let mut unknown_cost_trials = 0_u64;
    for row in rows {
        match row.cost {
            Some(cost) => {
                trial_known = trial_known
                    .checked_add(cost)
                    .context("known trial cost subtotal overflow")?;
            }
            None => unknown_cost_trials += 1,
        }
    }
    let total_estimate = if unknown_cost_trials == 0 {
        Some(trial_known)
    } else {
        None
    };
    let known_cost_subtotal = if let Some(total) = total_estimate {
        total
    } else {
        let mut known = request_known.max(trial_known);
        if let Some(metadata_known) = metadata_known {
            known = known.max(metadata_known);
        }
        known
    };
    Ok(CostSummary {
        known_cost_subtotal,
        known_successful_request_subtotal: request_known,
        known_trial_cost_subtotal: trial_known,
        total_estimate,
        unknown_cost_trials,
    })
}

fn report_json(input: &RunInput, models: &[ModelMetrics]) -> Result<Value> {
    let all_rows = input.rows.iter().collect::<Vec<_>>();
    let request_cost = input
        .successful_request_costs
        .values()
        .try_fold(Decimal::ZERO, |sum, value| sum.checked_add(*value))
        .context("known successful request subtotal overflow")?;
    let costs = cost_summary(&all_rows, request_cost, Some(input.metadata_known_cost))?;
    Ok(json!({
        "schema_version": 1,
        "run_id": input.run_id,
        "report_scope": input.scope,
        "diagnostic_scratch": input.scope == "diagnostic_scratch",
        "input_metadata": {
            "kind": "run",
            "source": "provider_api",
            "state": input.state,
        },
        "coverage": {
            "tasks": input.tasks.len(),
            "models": input.models.len(),
            "trials_per_task": input.trials_per_task,
            "expected_trials": input.rows.len(),
            "completed_items": input.rows.len(),
        },
        "costs": cost_json(&costs, Some(input.metadata_known_cost), input.successful_request_count),
        "models": models.iter().map(model_json).collect::<Vec<_>>(),
    }))
}

fn model_json(model: &ModelMetrics) -> Value {
    json!({
        "provider": model.provider,
        "requested_model": model.requested_model,
        "returned_model_ids_observed": model.returned_model_ids,
        "overall": counts_json(&model.overall),
        "control": counts_json(&model.control),
        "traps": counts_json(&model.traps),
        "tasks": model.tasks.iter().map(task_json).collect::<Vec<_>>(),
        "pass_power": k_averages_json(&model.pass_power),
        "pass_at": k_averages_json(&model.pass_at),
        "wilson_95": model.wilson95.as_ref().map(|interval| json!({"low":interval.low,"high":interval.high,"n":model.overall.n})),
        "task_rate_spread": model.task_spread.as_ref().map(|spread| json!({"min":spread.min,"min_task":spread.min_task,"max":spread.max,"max_task":spread.max_task})),
        "costs": cost_json(&model.costs, None, model.priced_successful_request_count),
    })
}

fn task_json(task: &TaskMetrics) -> Value {
    json!({
        "task_id": task.task_id,
        "metrics": counts_json(&task.counts),
        "pass_power": task.pass_power,
        "pass_at": task.pass_at,
    })
}

fn counts_json(counts: &Counts) -> Value {
    json!({
        "expected_trials": counts.expected_trials,
        "completed_trials": counts.completed_trials,
        "n": counts.n,
        "c": counts.c,
        "api_errors": counts.api_errors,
        "pass_rate": counts.pass_rate,
    })
}

fn k_averages_json(values: &BTreeMap<String, KAverage>) -> Value {
    Value::Object(
        values
            .iter()
            .map(|(k, metric)| {
                (
                    k.clone(),
                    json!({
                        "value": metric.value,
                        "eligible_task_count": metric.eligible_task_count,
                    }),
                )
            })
            .collect(),
    )
}

fn cost_json(
    costs: &CostSummary,
    metadata_known: Option<Decimal>,
    priced_successful_requests: u64,
) -> Value {
    json!({
        "known_cost_subtotal_usd": costs.known_cost_subtotal,
        "known_successful_request_subtotal_usd": costs.known_successful_request_subtotal,
        "known_trial_cost_subtotal_usd": costs.known_trial_cost_subtotal,
        "metadata_known_cost_estimate_usd": metadata_known,
        "priced_successful_requests": priced_successful_requests,
        "cost_estimate_usd": costs.total_estimate,
        "unknown_cost_trials": costs.unknown_cost_trials,
        "known_subtotal_source": if costs.total_estimate.is_some() {
            "score_rows"
        } else if priced_successful_requests > 0 {
            "run_meta_requests_and_score_rows"
        } else if metadata_known.is_some() {
            "run_meta_known_cost_and_score_rows"
        } else {
            "score_rows"
        },
    })
}

fn summary_csv(input: &RunInput, models: &[ModelMetrics]) -> Result<String> {
    let mut csv = String::new();
    let headers = [
        "report_scope",
        "row_type",
        "provider",
        "requested_model",
        "returned_model_ids_observed",
        "task_id",
        "expected_trials",
        "completed_trials",
        "n",
        "c",
        "api_errors",
        "pass_rate",
        "control_pass_rate",
        "trap_pass_rate",
        "wilson95_low",
        "wilson95_high",
        "task_rate_min",
        "task_rate_min_id",
        "task_rate_max",
        "task_rate_max_id",
        "pass_power_1",
        "pass_power_1_eligible_tasks",
        "pass_power_3",
        "pass_power_3_eligible_tasks",
        "pass_power_5",
        "pass_power_5_eligible_tasks",
        "pass_at_1",
        "pass_at_1_eligible_tasks",
        "pass_at_3",
        "pass_at_3_eligible_tasks",
        "pass_at_5",
        "pass_at_5_eligible_tasks",
        "known_successful_request_subtotal_usd",
        "priced_successful_requests",
        "known_trial_cost_subtotal_usd",
        "known_cost_subtotal_usd",
        "cost_estimate_usd",
        "unknown_cost_trials",
    ];
    append_csv_row(&mut csv, headers.iter().map(|value| (*value).to_owned()));

    for model in models {
        let returned = serde_json::to_string(&model.returned_model_ids)?;
        let interval = model.wilson95.as_ref();
        let spread = model.task_spread.as_ref();
        let mut summary = base_csv_row(input, "overall", model, &returned, "");
        summary.extend([
            model.overall.expected_trials.to_string(),
            model.overall.completed_trials.to_string(),
            model.overall.n.to_string(),
            model.overall.c.to_string(),
            model.overall.api_errors.to_string(),
            opt_float(model.overall.pass_rate),
            opt_float(model.control.pass_rate),
            opt_float(model.traps.pass_rate),
            interval
                .map(|value| format_float(value.low))
                .unwrap_or_default(),
            interval
                .map(|value| format_float(value.high))
                .unwrap_or_default(),
            spread
                .map(|value| format_float(value.min))
                .unwrap_or_default(),
            spread
                .map(|value| value.min_task.clone())
                .unwrap_or_default(),
            spread
                .map(|value| format_float(value.max))
                .unwrap_or_default(),
            spread
                .map(|value| value.max_task.clone())
                .unwrap_or_default(),
        ]);
        for k in K_VALUES {
            let metric = &model.pass_power[&k.to_string()];
            summary.push(opt_float(metric.value));
            summary.push(metric.eligible_task_count.to_string());
        }
        for k in K_VALUES {
            let metric = &model.pass_at[&k.to_string()];
            summary.push(opt_float(metric.value));
            summary.push(metric.eligible_task_count.to_string());
        }
        summary.extend([
            model.costs.known_successful_request_subtotal.to_string(),
            model.priced_successful_request_count.to_string(),
            model.costs.known_trial_cost_subtotal.to_string(),
            model.costs.known_cost_subtotal.to_string(),
            model
                .costs
                .total_estimate
                .map(|value| value.to_string())
                .unwrap_or_default(),
            model.costs.unknown_cost_trials.to_string(),
        ]);
        append_csv_row(&mut csv, summary);

        for task in &model.tasks {
            let mut row = base_csv_row(input, "task", model, &returned, &task.task_id);
            row.extend([
                task.counts.expected_trials.to_string(),
                task.counts.completed_trials.to_string(),
                task.counts.n.to_string(),
                task.counts.c.to_string(),
                task.counts.api_errors.to_string(),
                opt_float(task.counts.pass_rate),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ]);
            for k in K_VALUES {
                let value = task.pass_power[&k.to_string()];
                row.push(opt_float(value));
                row.push(u64::from(value.is_some()).to_string());
            }
            for k in K_VALUES {
                let value = task.pass_at[&k.to_string()];
                row.push(opt_float(value));
                row.push(u64::from(value.is_some()).to_string());
            }
            row.extend([
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ]);
            append_csv_row(&mut csv, row);
        }
    }
    Ok(csv)
}

fn base_csv_row(
    input: &RunInput,
    row_type: &str,
    model: &ModelMetrics,
    returned: &str,
    task_id: &str,
) -> Vec<String> {
    vec![
        input.scope.to_owned(),
        row_type.to_owned(),
        model.provider.clone(),
        model.requested_model.clone(),
        returned.to_owned(),
        task_id.to_owned(),
    ]
}

fn append_csv_row(csv: &mut String, values: impl IntoIterator<Item = String>) {
    let mut first = true;
    for value in values {
        if !first {
            csv.push(',');
        }
        first = false;
        csv.push_str(&csv_cell(&value));
    }
    csv.push('\n');
}

fn csv_cell(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn opt_float(value: Option<f64>) -> String {
    value.map(format_float).unwrap_or_default()
}

fn format_float(value: f64) -> String {
    let formatted = format!("{value:.6}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn print_summary(input: &RunInput, models: &[ModelMetrics], report: &Value) {
    if input.scope == "diagnostic_scratch" {
        println!("SCRATCH DIAGNOSTIC REPORT (not benchmark results)");
    } else {
        println!("Benchmark run report");
    }
    for model in models {
        let pass_rate = model
            .overall
            .pass_rate
            .map(|rate| format!("{:.1}%", rate * 100.0))
            .unwrap_or_else(|| "n/a".to_owned());
        println!(
            "{} [{}]: {}/{} graded passes ({pass_rate}); {} API errors / {} expected trials",
            model.requested_model,
            model.provider,
            model.overall.c,
            model.overall.n,
            model.overall.api_errors,
            model.overall.expected_trials
        );
        println!(
            "  returned model IDs observed: {}",
            if model.returned_model_ids.is_empty() {
                "none".to_owned()
            } else {
                model.returned_model_ids.join(", ")
            }
        );
    }
    println!(
        "known cost subtotal USD: {}; full cost estimate USD: {}",
        report["costs"]["known_cost_subtotal_usd"], report["costs"]["cost_estimate_usd"]
    );
    println!("report files: summary.csv, report.json");
}

fn write_generated(run_dir: &Path, name: &str, contents: &[u8]) -> Result<()> {
    let target = run_dir.join(name);
    let temp = run_dir.join(format!(".{name}.tmp-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .with_context(|| format!("creating generated report file for {name}"))?;
    if let Err(error) = file.write_all(contents).and_then(|()| file.sync_all()) {
        let _ = fs::remove_file(&temp);
        return Err(error).with_context(|| format!("writing generated report file {name}"));
    }
    drop(file);
    if let Err(error) = fs::rename(&temp, &target) {
        let _ = fs::remove_file(&temp);
        return Err(error).with_context(|| format!("regenerating {name}"));
    }
    Ok(())
}

fn required_str<'a>(object: &'a Value, key: &str) -> Result<&'a str> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .with_context(|| format!("required string field {key} is missing or invalid"))
}

fn required_u64(object: &Value, key: &str) -> Result<u64> {
    object
        .get(key)
        .and_then(Value::as_u64)
        .with_context(|| format!("required integer field {key} is missing or invalid"))
}

fn decimal_field(object: &Value, key: &str) -> Result<Option<Decimal>> {
    let value = object
        .get(key)
        .with_context(|| format!("required field {key} is missing"))?;
    match value {
        Value::Null => Ok(None),
        Value::Number(number) => Decimal::from_str(&number.to_string())
            .map(Some)
            .with_context(|| format!("field {key} is not a supported decimal number")),
        _ => bail!("field {key} must be a number or null"),
    }
}

fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn safe_run_id(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}
