# AGENTS.md: instructions for coding agents

This file is the single source of truth for any agent working in this repo. Read it, plus `docs/PLAN.md` and `docs/STATUS.md`, before changing anything.

## Active owner order (Nov 4–5, 2026)
M1 is merged; current bounded work is on `feat/m4-hardening` / PR #2. Push each completed step after tree/history secret scans; do not force-push main or merge this PR without further instruction. Use requested native Luna/xhigh workers for bounded disjoint tasks. OpenAI evaluation remains restricted to gpt-6-luna with a cumulative USD 1 ceiling. The Nov 5 addendum authorizes Muse Spark 1.1 through Meta Model API with a separate cumulative USD 15 ceiling. Each paid provider requires a durable pre-request reservation, reconciliation, and a stop on the first provider error. Unresolved historical exposure holds all USD 1 of OpenAI capacity. The corrected Muse tool round trip passed. An audited adjustment released the original HTTP 400 validation hold under the Nov 5 owner policy. The scored diagnostic requires owner task and grader sign-off. Three free NIM candidates require a passing tool smoke; the owner explicitly left their missing-key live checks pending. DeepSeek remains outside the authorized model list. Prospective policy 2 reports outcome, strict outcome-plus-raw-format and independent raw format; preserve original v1 scores. Task-schema contract v2 permits at most 8 offers; t01/t02/t05/t06 are versioned hardening fixtures. Stop after writing docs/SIGNOFF.md for owner review. Never submit to HackerRank.


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
configs/                 # pilot.toml, full.toml: models, providers, trials, concurrency, rate limits
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
Implemented: all nine task/reference pairs plus `oracle`, `smoke`, `run`, and offline `report`. `configs/control.toml` is one Luna control trial; `configs/pilot.toml` is a Luna-only four-task/three-trial diagnostic pilot under the current restriction. A first pilot retained 12 API errors; the slower single control passed. Run dispatch is bounded by provider concurrency and halts new provider trials on an API error. `report results/<run_id>` generates metrics; `--allow-scratch` labels diagnostic previews and never promotes data. Full-run config remains planned.

```bash
cp .env.example .env                                   # fill keys locally; never commit
cargo build --release
cargo run --release -- oracle                          # replays reference oracle_calls; must score 1.0 on every task, no network
cargo test --workspace                                 # grader self-tests (oracle + mutants + schema + leak check)
cargo run --release -- smoke --config configs/pilot.toml   # 1 cheap tool-calling request per model; checks keys, model IDs, tool support
cargo run --release -- run --config configs/pilot.toml
cargo run --release -- run --config configs/full.toml
cargo run --release -- report results/<run_id>         # tables for the README
```
`run` writes to `runs/<run_id>/` by default. Only a run ozgur wants to report gets promoted to `results/` and `transcripts/` (`--out results` or a move), so scratch and debug runs never get committed by accident.

## Model providers (agent-facing only; do NOT pitch providers in the README)
The README says only which models were tested and how to supply keys. Provider choices, limits and costs live here and in `docs/PLAN.md` §8.

| Provider | Base URL (default) | Key env var | Wire API for tool calls | Limits (verified Oct 30, 2026 unless marked) |
|---|---|---|---|---|
| OpenAI (paid) | `https://api.openai.com/v1` | `OPENAI_API_KEY` | **Responses API** (`/v1/responses`). GPT-6 models: "Tool calling requires the Responses API" (gpt-6.1-sol model page); gpt-6-luna allows Chat Completions function calling only with `reasoning_effort: none`. | Per-tier RPM/TPM (e.g. gpt-6.1-sol Build tier 5,000 RPM / 1M TPM). **ozgur's account is rate-limited in practice; his tier is unknown, so treat limits as tight** and keep concurrency low. |
| Meta Model API (paid) | `https://api.meta.ai/v1` | `MODEL_API_KEY` | Responses for muse-spark-1.1 at medium reasoning | Separate USD 15 project ceiling, 6 RPM, one concurrent request and zero retries in the configs. Corrected live smoke passed. |
| DeepSeek (historical, outside current scope) | `https://api.deepseek.com` | `DEEPSEEK_API_KEY` | Chat Completions (OpenAI format); Responses API also listed as supported | Concurrency limit per account: 2,500 for `deepseek-flash`, 500 for `deepseek-v4-pro`; 429 when exceeded. Peak/off-peak pricing (see PLAN §8). |
| NVIDIA NIM API catalog (free) | `https://integrate.api.nvidia.com/v1` | `NVIDIA_API_KEY` (keys start with `nvapi-`) | Chat Completions (all NIM endpoints implement it per build.nvidia.com/llms.txt) | Free trial tier, **~40 RPM** for most models, per account, varies with model and load; your limit is shown on the build.nvidia.com dashboard. Free use is for prototyping/research/testing. **Per-model tool-calling support is unverified** until `smoke` runs with a key. |

Rules for the client (`mb-llm`):
- **One client, one code path per wire API.** A provider entry in the config sets `base_url`, `key_env`, `api = "responses" | "chat_completions"`, and its limits. Optional URL overrides include `META_BASE_URL`. Paid live evaluation requires the official provider endpoint. No OpenRouter, no other gateways.
- **Concurrency and rate limits are config, not code**, per provider: `max_concurrency` (semaphore), `rpm` (token bucket; NVIDIA's limit is shared across all NVIDIA models on one key, so the bucket is per provider, not per model), optional `tpm`.
- **Backoff for retry-enabled providers:** retry the *same* request on 429, 408, 5xx, timeouts and connection errors. Honour `Retry-After` and OpenAI's `x-ratelimit-reset-requests` / `x-ratelimit-reset-tokens` when present; otherwise exponential backoff with full jitter (base 2 s, cap 60 s), `max_retries` default 6. Other 4xx (400/401/403/404) fail fast: they are harness or config bugs. A trial whose request still fails is recorded as `api_error` (PLAN §7), never silently re-run.
- Never log request headers. Never write keys or `Authorization` values to `runs/`, `results/` or `transcripts/`.
- Record per request: provider, model ID **as returned by the API**, wire API, HTTP status, retries, token usage (input / cached input / output / reasoning) and timestamps. Cost is computed from the price table in the config and noted as an estimate.
- Historical starting config from Oct 30 (active limits and providers follow the owner order above):

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

Open implementation checks before the pilot (record the outcome in DECISIONS.md): DeepSeek thinking mode is the default; check its "Thinking Mode" doc for how reasoning content must be passed back in multi-turn tool calls. Check that each NVIDIA model in the list actually returns `tool_calls` (the `smoke` command); drop any that don't, with a reason.

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
7. **Submit nothing.** No agent uploads to HackerRank or publishes the video without ozgur's explicit go-ahead. Never commit or paste the HackerRank test login/invite link (it embeds a password). The GitHub repo is **public**: treat every push as publishing.

## Definition of done
- [ ] 9 tasks (8 trap + 1 control) in `tasks/`, each with a hidden reference and `oracle_calls`.
- [ ] `cargo run --release -- oracle` → 1.0 on all tasks; `cargo test --workspace` green (mutants fail, schema checks, leak check).
- [ ] At least 1 frontier model (target 5–7 models) × 5 trials × 9 tasks run, with raw outputs and transcripts saved for every model.
- [ ] Failing transcripts read by a human. At least one failure per task is quoted in the README, or the README says plainly that the task saw no failures.
- [ ] README: every required section filled, including versions, run dates, settings, runtime and cost.
- [ ] Clean-room rerun from a fresh clone following only the README (Rust toolchain via rustup + keys in `.env`).
- [ ] Zip < 50 MB, README.md at root, no `target/`, no videos, no keys, not password-protected.
- [ ] Self-scored against the rubric (`docs/HACKATHON.md`), and ozgur has personally reviewed every task, answer and rule.
- [ ] Last step: the video walkthrough, uploaded to YouTube as unlisted (ROADMAP Nov 8).
