use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use mb_agent::replay;
use mb_core::load_task;
use mb_grader::{grade, load_reference, validate_reference};

mod runner;

#[derive(Parser)]
#[command(version, about = "Deterministic mandate-adherence evaluation")]
struct Cli {
    /// Repository root containing tasks/ and reference/.
    #[arg(long, default_value = ".", global = true)]
    root: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Replay hidden reference tool calls through fresh environments; no API calls.
    Oracle {
        /// Restrict replay to a single task ID.
        #[arg(long)]
        task: Option<String>,
        /// Optional scratch directory for offline scores and replay transcripts.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// One forced tool-calling request per selected model; not an evaluation trial.
    Smoke {
        #[arg(long, default_value = "configs/smoke.toml")]
        config: PathBuf,
        #[arg(long)]
        model: Option<String>,
        /// Output parent directory; a new, exclusive run directory is created inside it.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Validate configuration and key presence without sending requests.
        #[arg(long)]
        dry_run: bool,
    },
    /// Run the selected configured tasks, using a fresh environment for every trial.
    Run {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        task: Option<String>,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Oracle { task, out } => oracle(&cli.root, task.as_deref(), out.as_deref()),
        Command::Smoke {
            config,
            model,
            out,
            dry_run,
        } => {
            runner::smoke(
                &cli.root,
                &config,
                model.as_deref(),
                out.as_deref(),
                dry_run,
            )
            .await
        }
        Command::Run {
            config,
            model,
            task,
            out,
            dry_run,
        } => {
            runner::run(
                &cli.root,
                &config,
                model.as_deref(),
                task.as_deref(),
                out.as_deref(),
                dry_run,
            )
            .await
        }
    }
}

fn oracle(root: &Path, filter: Option<&str>, out: Option<&Path>) -> Result<()> {
    let mut paths = fs::read_dir(root.join("tasks"))?
        .map(|entry| Ok(entry?.path().join("task.json")))
        .collect::<Result<Vec<_>>>()?;
    paths.retain(|path| path.is_file());
    paths.sort();
    let mut scores = Vec::new();
    for path in paths {
        let task = load_task(&path).with_context(|| path.display().to_string())?;
        if filter.is_some_and(|wanted| wanted != task.task_id) {
            continue;
        }
        let task_id = task.task_id.clone();
        let reference = load_reference(&root.join("reference").join(format!("{task_id}.json")))?;
        validate_reference(&task, &reference)?;
        let final_output = serde_json::to_string(&reference.expected)?;
        let episode = replay(task.clone(), &reference.oracle_calls, final_output)?;
        let result = grade(
            &task,
            &reference,
            &episode.environment,
            &episode.transcript.final_output,
        );
        println!(
            "{task_id}: {:.1}{}",
            result.score,
            result
                .failure_label
                .as_ref()
                .map(|label| format!(" ({label})"))
                .unwrap_or_default()
        );
        if let Some(directory) = out {
            fs::create_dir_all(directory)?;
            let destination = directory.join(format!("{task_id}.oracle.json"));
            ensure!(
                !destination.exists(),
                "refusing to overwrite {}",
                destination.display()
            );
            fs::write(destination, serde_json::to_vec_pretty(&episode.transcript)?)?;
        }
        scores.push(
            serde_json::json!({"task_id": task_id, "source": "offline_oracle", "grade": result}),
        );
    }
    ensure!(!scores.is_empty(), "no matching task fixtures found");
    let passed = scores
        .iter()
        .filter(|score| score["grade"]["pass"] == true)
        .count();
    println!(
        "offline oracle: {passed}/{} passed (not model evaluation results)",
        scores.len()
    );
    if let Some(directory) = out {
        let destination = directory.join("scores.json");
        ensure!(
            !destination.exists(),
            "refusing to overwrite {}",
            destination.display()
        );
        fs::write(destination, serde_json::to_vec_pretty(&scores)?)?;
    }
    ensure!(passed == scores.len(), "oracle validation failed");
    Ok(())
}
