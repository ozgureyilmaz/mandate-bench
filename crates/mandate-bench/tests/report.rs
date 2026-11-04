use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use serde_json::{Value, json};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    run_dir: PathBuf,
}

impl Fixture {
    fn new(parent: &str, rows: &[Value], kind: &str, source: &str) -> Self {
        let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let run_id = format!("run-test.{id}");
        let root =
            std::env::temp_dir().join(format!("mandate-report-test-{}-{id}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let run_dir = root.join(parent).join(&run_id);
        fs::create_dir_all(run_dir.join("outputs")).unwrap();
        let transcript_dir = if parent == "results" {
            root.join("transcripts").join(&run_id)
        } else {
            run_dir.join("transcripts")
        };
        fs::create_dir_all(&transcript_dir).unwrap();

        for row in rows {
            let trial_id = row["trial_id"].as_str().unwrap();
            fs::write(
                run_dir.join("outputs").join(format!("{trial_id}.json")),
                b"{}",
            )
            .unwrap();
            fs::write(transcript_dir.join(format!("{trial_id}.json")), b"{}").unwrap();
        }
        let scores = rows
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(run_dir.join("scores.jsonl"), format!("{scores}\n")).unwrap();

        let api_errors = rows
            .iter()
            .filter(|row| row["status"] == "api_error")
            .count();
        let passes = rows.iter().filter(|row| row["status"] == "pass").count();
        let state = if api_errors == 0 {
            "completed"
        } else {
            "completed_with_api_errors"
        };
        let metadata = json!({
            "run_id": run_id,
            "kind": kind,
            "source": source,
            "state": state,
            "models": [{
                "id": "requested-luna",
                "provider": "openai",
                "price": {
                    "input_per_million": 1,
                    "cached_input_per_million": 0.5,
                    "output_per_million": 2
                }
            }],
            "tasks": [
                {"task_id":"t00_control", "task_version":1},
                {"task_id":"t01_sample_trap", "task_version":1}
            ],
            "trials_per_task": 3,
            "completed_items": rows.len(),
            "api_error_count": api_errors,
            "pass_count": passes,
            "known_cost_estimate_usd": 0,
            "requests": [
                {
                    "provider":"openai",
                    "requested_model":"requested-luna",
                    "model":"runtime-luna-actual-id",
                    "http_status":200,
                    "retries":0,
                    "usage":{"input":1000,"cached_input":100,"output":100,"reasoning":25}
                },
                {
                    "provider":"openai",
                    "requested_model":"requested-luna",
                    "model":"runtime-luna-actual-id",
                    "http_status":200,
                    "retries":1,
                    "usage":{"input":1000000,"cached_input":0,"output":1000000,"reasoning":500000}
                }
            ]
        });
        fs::write(
            run_dir.join("run_meta.json"),
            serde_json::to_vec_pretty(&metadata).unwrap(),
        )
        .unwrap();

        Self { root, run_dir }
    }

    fn relative_run(&self) -> &Path {
        self.run_dir.strip_prefix(&self.root).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn score_row(task_id: &str, trial: u32, status: &str) -> Value {
    let grade = match status {
        "pass" => json!({"pass":true,"score":1.0}),
        "fail" => json!({"pass":false,"score":0.0}),
        "api_error" => Value::Null,
        other => panic!("unsupported test status {other}"),
    };
    json!({
        "trial_id": format!("model-0-{task_id}-trial-{trial}"),
        "task_id": task_id,
        "task_version": 1,
        "trial": trial,
        "provider": "openai",
        "requested_model": "requested-luna",
        "returned_models": if status == "api_error" { json!([]) } else { json!(["runtime-luna-actual-id"]) },
        "status": status,
        "grade": grade,
        "failure_label": if status == "api_error" { json!("api_error") } else { Value::Null },
        "cost_estimate_usd": Value::Null
    })
}

fn mixed_rows() -> Vec<Value> {
    vec![
        score_row("t00_control", 1, "pass"),
        score_row("t00_control", 2, "fail"),
        score_row("t00_control", 3, "api_error"),
        score_row("t01_sample_trap", 1, "pass"),
        score_row("t01_sample_trap", 2, "pass"),
        score_row("t01_sample_trap", 3, "fail"),
    ]
}

fn invoke(fixture: &Fixture, allow_scratch: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mandate-bench"));
    command
        .arg("--root")
        .arg(&fixture.root)
        .arg("report")
        .arg(fixture.relative_run());
    if allow_scratch {
        command.arg("--allow-scratch");
    }
    command.output().unwrap()
}

fn report_value(fixture: &Fixture) -> Value {
    serde_json::from_slice(&fs::read(fixture.run_dir.join("report.json")).unwrap()).unwrap()
}

#[test]
fn mixed_results_report_correct_counts_k_metrics_and_known_request_cost() {
    let rows = mixed_rows();
    let fixture = Fixture::new("results", &rows, "run", "provider_api");
    let scores_before = fs::read(fixture.run_dir.join("scores.jsonl")).unwrap();
    let metadata_before = fs::read(fixture.run_dir.join("run_meta.json")).unwrap();
    let trial_id = rows[0]["trial_id"].as_str().unwrap();
    let output_path = fixture
        .run_dir
        .join("outputs")
        .join(format!("{trial_id}.json"));
    let transcript_path = fixture
        .root
        .join("transcripts")
        .join(fixture.run_dir.file_name().unwrap())
        .join(format!("{trial_id}.json"));
    let raw_before = fs::read(&output_path).unwrap();
    let transcript_before = fs::read(&transcript_path).unwrap();

    let output = invoke(&fixture, false);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fixture.run_dir.join("summary.csv").is_file());
    assert_eq!(
        scores_before,
        fs::read(fixture.run_dir.join("scores.jsonl")).unwrap()
    );
    assert_eq!(
        metadata_before,
        fs::read(fixture.run_dir.join("run_meta.json")).unwrap()
    );
    assert_eq!(raw_before, fs::read(&output_path).unwrap());
    assert_eq!(transcript_before, fs::read(&transcript_path).unwrap());
    let summary_before = fs::read(fixture.run_dir.join("summary.csv")).unwrap();
    assert!(invoke(&fixture, false).status.success());
    assert_eq!(
        summary_before,
        fs::read(fixture.run_dir.join("summary.csv")).unwrap()
    );

    let report = report_value(&fixture);
    let model = &report["models"][0];
    assert_eq!(model["requested_model"], "requested-luna");
    assert_eq!(
        model["returned_model_ids_observed"],
        json!(["runtime-luna-actual-id"])
    );
    assert_eq!(model["overall"]["n"], 5);
    assert_eq!(model["overall"]["c"], 3);
    assert_eq!(model["overall"]["api_errors"], 1);
    assert!((model["overall"]["pass_rate"].as_f64().unwrap() - 0.6).abs() < 1e-12);
    assert_eq!(model["control"]["n"], 2);
    assert_eq!(model["traps"]["n"], 3);

    // Per-task averages: pass^1 = ((1/2) + (2/3))/2 = 7/12.
    assert!((model["pass_power"]["1"]["value"].as_f64().unwrap() - 7.0 / 12.0).abs() < 1e-12);
    assert!((model["pass_at"]["1"]["value"].as_f64().unwrap() - 7.0 / 12.0).abs() < 1e-12);
    assert_eq!(model["pass_power"]["3"]["eligible_task_count"], 1);
    assert_eq!(model["pass_power"]["3"]["value"], 0.0);
    assert_eq!(model["pass_at"]["3"]["value"], 1.0);
    assert_eq!(model["pass_power"]["5"]["value"], Value::Null);
    assert_eq!(model["pass_power"]["5"]["eligible_task_count"], 0);
    assert_eq!(model["tasks"][0]["pass_power"]["3"], Value::Null);
    assert_eq!(model["tasks"][0]["pass_at"]["3"], Value::Null);
    assert!(model["wilson_95"]["low"].as_f64().unwrap() < 0.6);
    assert!(model["wilson_95"]["high"].as_f64().unwrap() > 0.6);

    // Cached tokens are inside input, reasoning is already inside output.
    assert!(
        (report["costs"]["known_successful_request_subtotal_usd"]
            .as_f64()
            .unwrap()
            - 0.00115)
            .abs()
            < 1e-15
    );
    assert_eq!(report["costs"]["cost_estimate_usd"], Value::Null);
    assert!((report["costs"]["known_cost_subtotal_usd"].as_f64().unwrap() - 0.00115).abs() < 1e-15);
    assert_eq!(model["costs"]["priced_successful_requests"], 1);
    assert_eq!(
        model["costs"]["known_subtotal_source"],
        "run_meta_requests_and_score_rows"
    );
    let csv = fs::read_to_string(fixture.run_dir.join("summary.csv")).unwrap();
    assert!(csv.contains("benchmark_results"));
    assert!(csv.contains("runtime-luna-actual-id"));
}

#[test]
fn all_api_errors_have_null_rates_and_preserve_known_success_costs() {
    let rows = [
        score_row("t00_control", 1, "api_error"),
        score_row("t00_control", 2, "api_error"),
        score_row("t00_control", 3, "api_error"),
        score_row("t01_sample_trap", 1, "api_error"),
        score_row("t01_sample_trap", 2, "api_error"),
        score_row("t01_sample_trap", 3, "api_error"),
    ];
    let fixture = Fixture::new("runs", &rows, "run", "provider_api");
    assert!(!invoke(&fixture, false).status.success());
    let output = invoke(&fixture, true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("SCRATCH DIAGNOSTIC REPORT"));

    let report = report_value(&fixture);
    let model = &report["models"][0];
    assert_eq!(report["report_scope"], "diagnostic_scratch");
    assert_eq!(model["overall"]["n"], 0);
    assert_eq!(model["overall"]["c"], 0);
    assert_eq!(model["overall"]["api_errors"], 6);
    assert_eq!(model["overall"]["pass_rate"], Value::Null);
    assert_eq!(model["wilson_95"], Value::Null);
    assert_eq!(model["pass_power"]["1"]["value"], Value::Null);
    assert_eq!(model["pass_power"]["1"]["eligible_task_count"], 0);
    assert_eq!(report["costs"]["cost_estimate_usd"], Value::Null);
    assert!(
        (report["costs"]["known_successful_request_subtotal_usd"]
            .as_f64()
            .unwrap()
            - 0.00115)
            .abs()
            < 1e-15
    );
    assert_eq!(report["costs"]["unknown_cost_trials"], 6);
}

#[test]
fn missing_and_duplicate_trials_are_rejected() {
    let mut rows = mixed_rows();
    rows.pop();
    let missing = Fixture::new("results", &rows, "run", "provider_api");
    assert!(!invoke(&missing, false).status.success());

    let mut rows = mixed_rows();
    rows[1] = rows[0].clone();
    let duplicate = Fixture::new("results", &rows, "run", "provider_api");
    let output = invoke(&duplicate, false);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("duplicate"));
}

#[test]
fn smoke_and_local_mock_artifacts_are_never_reportable() {
    let rows = mixed_rows();
    let smoke = Fixture::new("runs", &rows, "smoke", "provider_api");
    assert!(!invoke(&smoke, true).status.success());

    let local_mock = Fixture::new("results", &rows, "run", "local_mock");
    assert!(!invoke(&local_mock, true).status.success());
}

#[test]
fn report_requires_raw_output_and_transcript_files_with_safe_ids() {
    let rows = mixed_rows();
    let missing = Fixture::new("results", &rows, "run", "provider_api");
    let trial_id = rows[0]["trial_id"].as_str().unwrap();
    fs::remove_file(
        missing
            .run_dir
            .join("outputs")
            .join(format!("{trial_id}.json")),
    )
    .unwrap();
    assert!(!invoke(&missing, false).status.success());

    let missing_transcript = Fixture::new("results", &rows, "run", "provider_api");
    let transcript = missing_transcript
        .root
        .join("transcripts")
        .join(missing_transcript.run_dir.file_name().unwrap())
        .join(format!("{trial_id}.json"));
    fs::remove_file(transcript).unwrap();
    assert!(!invoke(&missing_transcript, false).status.success());

    let mut unsafe_rows = mixed_rows();
    unsafe_rows[0]["trial_id"] = json!("../outside");
    let unsafe_id = Fixture::new("results", &unsafe_rows, "run", "provider_api");
    assert!(!invoke(&unsafe_id, false).status.success());
}
