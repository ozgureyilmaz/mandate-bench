# AGENTS.md: instructions for coding agents

This file is the single source of truth for any agent working in this repo. Read it, plus `docs/PLAN.md` and `docs/STATUS.md`, before changing anything.

## Active owner order (Nov 8, 2026)
Use requested native Luna/xhigh workers for bounded disjoint tasks.
M1 is merged; PR #2 on `feat/m4-hardening` is ready for its authorized merge to `main` after required checks. Create the ZIP only after that merge and the package checks, with tree and Git-history secret scans before packaging. The owner explicitly accepts a package with honestly disclosed partial model evidence: nine designed tasks; the frozen Luna v1 pilot on t00/t01/t04/t08 at 9/12 strict; and Muse Spark 1.1 on t01/t02/t05/t06 v2 × 3 trials, scoring 12/12 in outcome, strict and raw format, with zero API errors and zero missing decisions. The former nine-task × five-trial target is no longer a package blocker. Preserve all source evidence and sign-off marks.

Do not make provider calls, run another paid batch, publish a video, or upload to HackerRank under this order. OpenAI remains fully held for unresolved historical exposure; no further OpenAI calls are authorized. Muse's separate USD 15 ledger has USD 14.51772725 remaining, but any further paid run requires a new owner order. NIM candidates remain archived and untested; DeepSeek remains outside scope. The corrected Muse tool round trip, audited HTTP 400 hold release, and approved diagnostic are recorded in `docs/STATUS.md`, `docs/DECISIONS.md` and `docs/MUSE_DIAGNOSTIC_V1.md`. Required PR checks, merge, package checks and ZIP creation are pending until their actual evidence is recorded. No HackerRank upload is authorized.


## Goal
Ship **mandate-bench**, a small code-graded eval for the micro1 Eval Hackathon (final upload deadline **Nov 9, 2026 08:15 Istanbul / 05:15 GMT**). A buyer agent (the model under test) picks among offers from scripted seller agents and must stay inside the user's mandate (budget including all fees, deadline, compatibility, return policy). The correct outcome is either the right offer, quantity and total, or no order plus `ask_user`. There are 8 trap tasks and 1 control task. Everything is priced in **USD** with generic, global (fictional) addresses. Details are in `docs/PLAN.md`.

## Roles
- **ozgur** (owner) decides everything that is visible from outside: submission, video, model list, budget, and what is written about prior work.
- **Implementation agent** does main development (the Rust workspace: env, runner, agent loop, grader, tasks).
- **Cursor cloud agents** take bounded code tasks (one task folder, one grader test, one fix) on their own branches.
- **Grok Bot** tracks status and keeps `docs/` in sync with the code. It does not write eval logic unless asked.
- Every agent updates `docs/STATUS.md` when it finishes a unit of work, and `docs/DECISIONS.md` when it makes a design choice.

## Language and toolchain
- **Rust** for the eval harness, the buyer-agent loop and the grader (decided Oct 30; see DECISIONS.md). Tasks and references stay **JSON data files**; no task logic lives in code.
- Toolchain pinned in `rust-toolchain.toml` at the repo root. Pin `channel = "1.99.1"` (current stable on Oct 30, 2026, per static.rust-lang.org) with `components = ["rustfmt", "clippy"]`. Change the pin only with a DECISIONS.md entry.
- **`Cargo.lock` is committed.** Together with `Cargo.toml` it is the "requirements file or equivalent" the hackathon asks for. Don't add a `requirements.txt`; nothing in this repo needs Python.
- Core crates: `tokio` (runtime), `reqwest` (HTTP, rustls TLS, `json` feature), `serde` + `serde_json`, `clap` (derive), `dotenvy` (loads `.env`). Latest stable on crates.io on Oct 30: tokio 1.54.1, reqwest 0.13.5, serde 1.0.229, serde_json 1.0.154, clap 4.6.7, dotenvy 0.15.7. Suggested extras, only if they remove real code: `rust_decimal` (exact money arithmetic, half-up rounding), `chrono` (frozen clock, business-day math), `anyhow`/`thiserror`, `toml` (configs). Pin via `Cargo.lock`; prefer `default-features = false` where it trims the tree.
- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` must be clean before merging.

## Layout (target; build to this)
```
README.md                # submission README (required sections; TODOs until real results exist)
AGENTS.md                # this file
Cargo.toml               # [workspace] members = ["crates/*"], shared [workspace.dependencies]
Cargo.lock               # committed
rust-toolchain.toml      # pinned toolchain
.env.example             # key NAMES only
configs/                 # model-scoped configs; current diagnostic command is listed below
schemas/                 # mandate.schema.json, offer.schema.json, final_answer.schema.json
prompts/system_v1.md     # fixed buyer system prompt (versioned)
crates/
  mb-core/               # types (serde), total formula, delivery-date math, task/reference loading
  mb-env/                # scripted sellers, tool layer, order ledger, frozen clock
  mb-llm/                # ONE OpenAI-compatible client: per-provider base_url, retries, rate limiting
  mb-agent/              # buyer-agent tool-calling loop (drives mb-env through mb-llm)
  mb-grader/             # deterministic grader + self-tests (tests/: oracle, mutants, leak check)
  mandate-bench/         # the binary (clap CLI): run, oracle, smoke, report
tasks/
  t00_control/ ... t08_no_valid_offer/
    task.json            # VISIBLE to the model: mandate, user request, sellers, offers, scripted Q&A
reference/
  t00_control.json ...   # HIDDEN: expected decision, order, total, distractor_labels, oracle_calls
results/<run_id>/        # COMMITTED: raw model outputs, scores.jsonl, summary.csv, run_meta.json
transcripts/<run_id>/    # COMMITTED: full message logs, one file per trial
runs/                    # GITIGNORED scratch: debug runs, smoke tests, partial runs
docs/                    # HACKATHON, PLAN, ROADMAP, PRIOR_WORK, LINKS, STATUS, DECISIONS, AI_USE
```
Crate names are a suggestion; keep the split between core types, env, LLM client, agent loop, grader and CLI. The oracle is **data**: `reference/<task>.json` → `oracle_calls` is the scripted list of correct tool calls, replayed by `mandate-bench oracle`.

## Commands (keep in sync with the code)
Implemented: all nine task/reference pairs plus `oracle`, `smoke`, `run`, and offline `report`. The Luna control and pilot are frozen historical diagnostics; do not make new OpenAI requests. Only the Muse four-task diagnostic is authorized in the current work order, gated by the owner sign-offs above. A final benchmark run is outside this work order. `report results/<run_id>` generates metrics; `--allow-scratch` labels diagnostic previews and never promotes data. Full-run config remains planned.

```bash
cargo build --release
cargo run --release -- oracle                              # replays reference oracle_calls; must score 1.0 on every task, no network
cargo test --workspace                                     # grader self-tests (oracle + mutants + schema + leak check)
cargo run --release -- smoke --config configs/muse-smoke.toml --dry-run  # preflight only; no provider request
cargo run --release -- run --config configs/muse-diagnostic.toml         # only after all owner sign-off gates are met
cargo run --release -- report results/<run_id>             # tables for the README
```
`run` writes to `runs/<run_id>/` by default. Only a run ozgur wants to report gets promoted to `results/` and `transcripts/` (`--out results` or a move), so scratch and debug runs never get committed by accident.

## Provider implementation and historical profiles (agent-facing only; do NOT pitch providers in the README)
The active evaluation scope is Muse Spark 1.1 only, after the owner sign-off gates above. The README says only which models were tested and how to supply keys. The profiles below preserve implementation and historical context; they do not authorize extra models or live checks. NIM candidates are retired from the active submission and require no key or smoke.

| Provider | Base URL (default) | Key env var | Wire API for tool calls | Limits (verified Oct 30, 2026 unless marked) |
|---|---|---|---|---|
| OpenAI (historical; no further calls authorized) | `https://api.openai.com/v1` | `OPENAI_API_KEY` (historical profile) | **Responses API** (`/v1/responses`). GPT-6 models: "Tool calling requires the Responses API" (gpt-6.1-sol model page); gpt-6-luna allows Chat Completions function calling only with `reasoning_effort: none`. | Per-tier RPM/TPM (e.g. gpt-6.1-sol Build tier 5,000 RPM / 1M TPM). Account tier was unknown; all current OpenAI capacity is held for unresolved historical exposure. |
| Meta Model API (paid) | `https://api.meta.ai/v1` | `MODEL_API_KEY` | Responses for muse-spark-1.1 at medium reasoning | Separate USD 15 project ceiling, 6 RPM, one concurrent request and zero retries in the configs. Corrected live smoke passed. |
| DeepSeek (historical; outside current scope) | `https://api.deepseek.com` | `DEEPSEEK_API_KEY` (historical profile) | Chat Completions (OpenAI format); Responses API also listed as supported | Concurrency limit per account: 2,500 for `deepseek-flash`, 500 for `deepseek-v4-pro`; 429 when exceeded. Peak/off-peak pricing (see PLAN §8). |
| NVIDIA NIM API catalog (historical only) | `https://integrate.api.nvidia.com/v1` | `NVIDIA_API_KEY` (archived profile; not required) | Chat Completions (historical implementation profile) | Three candidates are archived and untested; no active eligibility or live check is required. Archived smoke config: `configs/historical/nim-smoke.toml`. |

Rules for the client (`mb-llm`):
- **One client, one code path per wire API.** A provider entry in the config sets `base_url`, `key_env`, `api = "responses" | "chat_completions"`, and its limits. Optional URL overrides include `META_BASE_URL`. Paid live evaluation requires the official provider endpoint. No OpenRouter, no other gateways.
- **Concurrency and rate limits are config, not code**, per provider: `max_concurrency` (semaphore), `rpm` (token bucket; NVIDIA's limit is shared across all NVIDIA models on one key, so the bucket is per provider, not per model), optional `tpm`.
- **Backoff for retry-enabled providers:** retry the *same* request on 429, 408, 5xx, timeouts and connection errors. Honour `Retry-After` and OpenAI's `x-ratelimit-reset-requests` / `x-ratelimit-reset-tokens` when present; otherwise exponential backoff with full jitter (base 2 s, cap 60 s), `max_retries` default 6. Other 4xx (400/401/403/404) fail fast: they are harness or config bugs. A trial whose request still fails is recorded as `api_error` (PLAN §7), never silently re-run.
- Never log request headers. Never write keys or `Authorization` values to `runs/`, `results/` or `transcripts/`.
- Record per request: provider, model ID **as returned by the API**, wire API, HTTP status, retries, token usage (input / cached input / output / reasoning) and timestamps. Cost is computed from the price table in the config and noted as an estimate.
- Historical starting config from Oct 30 (reference only; it does not authorize calls to these providers):

```toml
[providers.openai]
base_url = "https://api.openai.com/v1"
key_env = "OPENAI_API_KEY"
api = "responses"
max_concurrency = 2
rpm = 30            # conservative: ozgur's tier is unknown and tight
max_retries = 6

[providers.deepseek]
base_url = "https://api.deepseek.com"
key_env = "DEEPSEEK_API_KEY"
api = "chat_completions"
max_concurrency = 8
max_retries = 6

[providers.nvidia]
base_url = "https://integrate.api.nvidia.com/v1"
key_env = "NVIDIA_API_KEY"
api = "chat_completions"
max_concurrency = 2
rpm = 30            # below the ~40 RPM free-tier default, shared by every NVIDIA model
max_retries = 6
```

Historical implementation checks: DeepSeek thinking mode is the default and its reasoning must be replayed in multi-turn tool calls. The planned live checks for the three NIM candidates were not run because the key was missing. They remain untested, are now archived, and are outside the active submission; do not request a key or run a NIM smoke.

## Conventions
- Run everything with fixed inputs: sellers are **scripted JSON**, not LLMs, and each task has a fixed `now` clock (ISO 8601 with an explicit offset). The model under test is the only stochastic part.
- Money: JSON numbers in **USD** (the mandate currency). Other currencies appear only where a task tests FX, with the rate given in the mandate. Use exact decimal arithmetic, round half-up to 2 decimals only at the end. The total formula lives in one function in `mb-core`, used by both the oracle and the grader.
- Every trial starts from a fresh environment (new ledger, no state shared between trials).
- `run_meta.json` records the model ID, provider, wire API, temperature/reasoning settings, max turns, seed (if any), git commit, `rustc --version`, run start/end (UTC and Istanbul), token usage, retries, api_error count and cost estimate.
- File names: `tNN_snake_case`. Task IDs are stable once results exist. If a task changes, bump `task_version` in `task.json`.
- Small commits on topic branches (`feat/...`, `fix/...`). Never force-push `main`.

## Hard rules (non-negotiable)
1. **No keys or secrets** in the repo, transcripts, logs or zip. `.env` and every `.env.*` except `.env.example` are gitignored; `.env.example` contains names only. Before every push and before any zip, scan the tree **and the git history** for `sk-`, `nvapi-`, `ghp_`/`gho_`, `AKIA`, `-----BEGIN`, `Bearer `, `api_key=`/`token=` with values. Don't commit anything that isn't needed (see `.gitignore`; `runs/` is scratch).
2. **Synthetic data only.** Sellers, products, prices, addresses and people are invented. No scraped listings, no real personal data, no third-party material without rights.
3. **The grader must accept the reference solution and reject wrong ones.** The oracle scores 1.0 on all 9 tasks. No-op, "buy cheapest list price", "buy every offer", "right offer with wrong total", "ask_user on a buy task" and "buy on the no-valid-offer task" all fail. `cargo test` enforces this.
4. **Reference answers are never visible to the model under test.** `reference/` is never loaded into prompts, tool outputs or the env. A test asserts that no expected value or rationale string appears in any prompt or tool response.
5. **Disclose prior work.** Anything that existed before Oct 30, 2026 (ozgur's harnessrouter repo, any reused code, inspirations such as Magentic Marketplace, τ-bench, AgentDojo, AP2/ACP/UCP) is listed in `docs/PRIOR_WORK.md` and the README.
6. **Honest results.** Report only numbers that come from files in `results/`. Never edit raw outputs. Report failed and errored trials (API errors count as a separate column, not silently retried away). Don't drop or tune tasks just because a model failed them. Every task change after the pilot gets a reason in `docs/DECISIONS.md`.
7. **Respect external-release boundaries.** The owner has authorized merging PR #2 after required checks and creating a ZIP after the merge and package checks. This does not authorize a HackerRank upload or video publication. Never commit or paste the HackerRank test login/invite link (it embeds a password). The GitHub repo is **public**: treat every push as publishing.

## Definition of done
- [ ] 9 tasks (8 trap + 1 control) in `tasks/`, each with a hidden reference and `oracle_calls`.
- [ ] `cargo run --release -- oracle` → 1.0 on all tasks; `cargo test --workspace` green (mutants fail, schema checks, leak check).
- [ ] Historical full-benchmark target (not a blocker for the owner-approved partial-evidence package): at least 1 frontier model (target 5–7 models) × 5 trials × 9 tasks, with raw outputs and transcripts saved for every model.
- [ ] Failing transcripts read by a human. At least one failure per task is quoted in the README, or the README says plainly that the task saw no failures.
- [ ] README: every required section filled, including versions, run dates, settings, runtime and cost.
- [ ] Clean-room rerun from a fresh clone following only the README (Rust toolchain via rustup + keys in `.env`).
- [ ] Zip < 50 MB, README.md at root, no `target/`, no videos, no keys, not password-protected.
- [ ] Self-scored against the rubric (`docs/HACKATHON.md`), and ozgur has personally reviewed every task, answer and rule.
- [ ] Confirm the owner-provided walkthrough is at most five minutes, accessible without login and plays with audio. The Drive video stays outside the ZIP.
