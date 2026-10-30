# mandate-bench

> **Status: work in progress (micro1 Eval Hackathon, Oct 30 – Nov 9, 2026).** Every section marked TODO must be filled in from real runs before submission. Do not put any claim here that `results/` and `transcripts/` don't back up.

**mandate-bench** tests whether a frontier model, acting as a **buyer agent**, stays inside the **mandate** its user gave it (budget including all fees, delivery deadline, compatibility, return policy) when it picks among offers from several **scripted seller agents**. Each task has exactly one defensible outcome: either buy the right offer at the right quantity and total, or buy nothing and ask the user because no offer fits the mandate. Seller offers contain the traps a careful human buyer catches: hidden fees, expired offers, near-miss compatibility, prompt injection in seller text, pack-vs-unit pricing, ship date vs. delivery date, final-sale terms, and "nothing qualifies". A deterministic, code-based grader scores the end state (the order ledger) and a JSON final answer. No LLM judge is used.

*"Harness" here can mean two things. The **eval harness** is the `mandate-bench` Rust binary (`cargo run --release -- run ...`), which runs tasks, collects transcripts and grades them. The **buyer-agent loop** is the small tool-calling loop that the model under test drives.*

All tasks are synthetic, priced in **USD**, with fictional sellers and addresses.

## Short description
TODO (3–4 sentences: domain, gap, number of tasks, models tested, headline finding). Paste the same text into the HackerRank description field.

## Video walkthrough
TODO: unlisted YouTube link (≤5 min, must open without login and play with audio). Recorded last, after results are final. Use the same link as in the HackerRank Q2 field.

## Domain and expertise
TODO: agentic commerce / agent infrastructure, i.e. delegated purchasing under a user mandate (compare AP2 Intent Mandates). Author: Özgür Eryılmaz, MIS undergraduate, who builds agent infrastructure (see docs/PRIOR_WORK.md and the prior work section).

## What it tests and why it matters
TODO: the capability being measured, who relies on it (people delegating purchases to agents; teams building buyer agents on ACP/AP2/UCP-style rails), and what a failure costs (overspending, items that can't be returned, missed deadlines, purchases a seller manipulated the agent into).

## Task overview
TODO: a table with one row per task: trap, why it's hard, expected outcome. Use docs/PLAN.md §4 as the source.

## Grading
TODO: components, pass rule, partial credit, ±0.01 tolerance, and grader validation (oracle = 1.0 on every task; every mutant answer fails).

## Setup and run
TODO: verify these from a clean clone before submitting.

Requirements: Rust via [rustup](https://rustup.rs). The toolchain version is pinned in `rust-toolchain.toml` and installs automatically on the first `cargo` command. Dependencies are pinned by `Cargo.toml` + the committed `Cargo.lock` (this repo's equivalent of a requirements file). No Python or Docker needed.

```bash
cp .env.example .env        # then set the API key(s) for the models you want to run
cargo run --release -- oracle                       # reference solutions, no API calls, expect 1.0
cargo test --workspace                              # grader self-tests
cargo run --release -- run --config configs/full.toml   # full run
```
API keys are read from environment variables (`.env` is loaded automatically; see `.env.example` for the names). You only need the keys for the models listed in the config you run.

Approximate runtime: TODO. Approximate cost: TODO.

## Models tested and results
TODO: exact model IDs as returned by the API, run date (Istanbul time and UTC), settings (temperature, reasoning effort, max turns, trials per task), and a table with pass rate, pass^k, spread and cost per run.

| Model (ID, settings) | Trials/task | Pass rate | pass^k | Cost/full run |
|---|---|---|---|---|
| TODO | 5 | | | |

## Failure analysis
TODO: per-task notes (which models failed, how, and a link to a transcript), at least one near-miss task, the main failure mode and the hot take.

## Prior work and sources
TODO: finalize from docs/PRIOR_WORK.md. Must disclose: Magentic Marketplace (closest prior environment), τ-bench (end-state grading, pass^k), AgentDojo (indirect injection), AP2 (the mandate concept), ACP/UCP (offer schema inspiration), plus anything that existed before Oct 30, 2026. All data is synthetic.

## AI use
TODO: fill in from docs/AI_USE.md.

## Repository layout
A Cargo workspace (`crates/`), task data (`tasks/`, `reference/`), JSON schemas (`schemas/`), run configs (`configs/`), and committed results (`results/`, `transcripts/`). Details in AGENTS.md.
