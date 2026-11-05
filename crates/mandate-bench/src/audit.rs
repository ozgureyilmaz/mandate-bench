//! Immutable, offline comparison of strict v1 and proposed outcome v2 policy.
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::Command,
};

use anyhow::{Context, Result, ensure};
use mb_agent::replay;
use mb_core::{ToolCall, load_task};
use mb_grader::{Grade, grade, grade_authorized_outcome, load_reference, validate_reference};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Default, Serialize)]
struct Counts {
    graded: usize,
    strict_passes: usize,
    outcome_passes: usize,
    strict_schema_passes: usize,
    api_errors: usize,
}

pub fn generate(root: &Path, source: &Path, allow_scratch: bool) -> Result<()> {
    let directory = crate::report::validate_audit_source(root, source, allow_scratch)?;
    let root = fs::canonicalize(root)?;
    let meta: Value = serde_json::from_slice(&fs::read(directory.join("run_meta.json"))?)?;
    let model_commit = meta["git_commit"]
        .as_str()
        .context("audit requires a recorded model-run source commit")?;
    ensure!(
        model_commit.len() == 40 && model_commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid recorded source commit"
    );
    ensure!(
        meta["git_dirty"] == false,
        "audit requires a clean recorded model-run source snapshot"
    );
    let scores_path = directory.join("scores.jsonl");
    let source_rows = fs::read_to_string(&scores_path)?;
    let scratch = directory.parent() == Some(root.join("runs").as_path());
    let transcripts = if scratch {
        directory.join("transcripts")
    } else {
        root.join("transcripts")
            .join(directory.file_name().context("missing run ID")?)
    };
    let mut overall = Counts::default();
    let mut per_task: BTreeMap<String, Counts> = BTreeMap::new();
    let mut rows = Vec::new();
    for line in source_rows.lines() {
        let row: Value = serde_json::from_str(line)?;
        let id = row["trial_id"].as_str().context("missing trial ID")?;
        let task_id = row["task_id"].as_str().context("missing task ID")?;
        let counts = per_task.entry(task_id.into()).or_default();
        if row["status"] == "api_error" {
            overall.api_errors += 1;
            counts.api_errors += 1;
            rows.push(json!({"trial_id":id,"task_id":task_id,"status":"api_error","strict_v1":null,"outcome_v2":null}));
            continue;
        }
        let task = load_task(&root.join("tasks").join(task_id).join("task.json"))?;
        ensure!(
            Command::new("git")
                .args([
                    "diff",
                    "--quiet",
                    model_commit,
                    "--",
                    &format!("tasks/{task_id}/task.json"),
                    &format!("reference/{task_id}.json")
                ])
                .current_dir(&root)
                .status()?
                .success(),
            "task/reference differs from the model-run source; use the matching snapshot"
        );
        ensure!(
            Some(u64::from(task.task_version)) == row["task_version"].as_u64(),
            "task version differs from frozen trial; use matching source"
        );
        let reference = load_reference(&root.join("reference").join(format!("{task_id}.json")))?;
        validate_reference(&task, &reference)?;
        let transcript: Value =
            serde_json::from_slice(&fs::read(transcripts.join(format!("{id}.json")))?)?;
        let calls = transcript["tool_calls"]
            .as_array()
            .context("missing tool ledger")?
            .iter()
            .map(|record| serde_json::from_value::<ToolCall>(record["call"].clone()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let final_output = transcript["final_output"]
            .as_str()
            .context("missing raw final output")?;
        let episode = replay(task.clone(), &calls, final_output.to_owned())?;
        let stored: Grade = serde_json::from_value(row["grade"].clone())?;
        let (strict, outcome) = if row["episode_outcome"] == "complete" {
            let strict = grade(&task, &reference, &episode.environment, final_output);
            ensure!(
                strict == stored,
                "strict replay differs from original grade for {id}; stop before auditing"
            );
            let outcome =
                grade_authorized_outcome(&task, &reference, &episode.environment, final_output);
            (strict, outcome)
        } else {
            ensure!(!stored.pass, "incomplete episode cannot pass outcome audit");
            (stored.clone(), stored)
        };
        for counter in [&mut overall, counts] {
            counter.graded += 1;
            counter.strict_passes += usize::from(strict.pass);
            counter.outcome_passes += usize::from(outcome.pass);
            counter.strict_schema_passes += usize::from(strict.components[0]);
        }
        rows.push(json!({"trial_id":id,"task_id":task_id,"status":row["status"],"strict_v1":strict,"outcome_v2":outcome}));
    }
    let git = |args: &[&str]| -> Option<String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&root)
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let score_path = scores_path.to_str().context("non-UTF8 score path")?;
    let audit = json!({
        "policy":"authorized_outcome_v2_proposed", "scope":"offline_policy_comparison_not_new_model_trials",
        "original_run_id":meta["run_id"], "model_run_git_commit":meta["git_commit"],
        "audit_git_commit":git(&["rev-parse","HEAD"]),
        "audit_git_dirty":git(&["status","--porcelain"]).map(|status|!status.is_empty()),
        "source_scores_git_blob":git(&["hash-object",score_path]),
        "strict_policy_unchanged":true,
        "outcome_policy_notes":["Ignore explanatory reason metadata only in a working grading copy","Only a no-order/no-total ASK_USER may inherit USD when currency is absent","BUY and explicit wrong currency, financial values, decision, ledger, quantity, consistency and episode caps remain strict","Duplicate keys and additional answer properties remain invalid"],
        "overall":overall,"per_task":per_task,"trials":rows,
    });
    let destination = directory.join("outcome-audit-v2.json");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .context("creating immutable policy audit; existing audit files are never overwritten")?;
    serde_json::to_writer_pretty(&mut file, &audit)?;
    file.flush()?;
    file.sync_all()?;
    println!(
        "Offline policy audit: {} strict / {} outcome passes out of {} graded; {} API errors",
        overall.strict_passes, overall.outcome_passes, overall.graded, overall.api_errors
    );
    println!(
        "Original scores/raw outputs unchanged. Proposed policy only: {}",
        destination.display()
    );
    Ok(())
}
