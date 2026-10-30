# STATUS

Last updated: **Oct 30, 2026 (Istanbul)**, by the implementation agent.
Update this file at the end of every unit of work. Keep it short.

## Done
- Idea 5a chosen (buyer agent + scripted sellers + mandate). Reasons in DECISIONS.md.
- Repo name: `mandate-bench`. Public repo: https://github.com/ozgureyilmaz/mandate-bench. M1 is prepared for pull-request review; no hackathon submission has been made.
- Hackathon rules, deadlines, rubric and checklist captured (docs/HACKATHON.md).
- Design draft v0.2: USD/global tasks, Rust harness, schemas, tools, 9 tasks, grader, metrics (docs/PLAN.md).
- Decided: providers OpenAI + DeepSeek + NVIDIA NIM (no OpenRouter), USD/global, Rust, unlisted YouTube video last, strict `.gitignore` (DECISIONS.md).
- Model list and budget confirmed by ozgur on Oct 30 (PLAN.md §8): core set without gpt-6-astra, about $25–35 total. NVIDIA models still need a tool-calling smoke test. Provider details for agents are in AGENTS.md.
- X links and buildevals.com added (docs/LINKS.md, docs/PRIOR_WORK.md).
- micro1 Participant Agreement accepted by ozgur (Oct 30).
- ozgur's reason for 5a recorded (DECISIONS.md).
- M1: six-crate Cargo workspace, Rust 1.99.1 pin with rustfmt/Clippy, Cargo.lock, five strict offline schemas, shared exact-decimal total/delivery functions, fixed buyer prompt, scripted five-tool env, ledger and offline replay/CLI.
- t00/t01 visible task JSON plus hidden references/oracle calls. Control reference: B × 1, USD 59.50; hidden-fee reference: B × 1, USD 103.99. These are oracle fixtures, not model results.
- Hidden-reference isolation interpretation confirmed by ozgur; normal offer IDs/prices stay visible. Details in DECISIONS.md.
- Native worker requests used `gpt-6-luna` / `xhigh` for schemas/fixtures, environment and grader. Shared-doc updates were consolidated by the primary to avoid overlapping writes.
- Verified on Rust 1.99.1: `cargo build --release --locked`, `cargo test --workspace --locked` (27 tests), `cargo fmt --all --check`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` all pass. Offline oracle: t00/t01 each 1.0 (2/2). Scratch replay is in ignored `runs/m1-verification-20261030/`; it is not a model run.
- Fresh local clone of the M1 source snapshot verified with a new build directory: release build, oracle (2/2), all 27 tests, format and Clippy all pass with `--locked --offline` where applicable. Toolchain/registry were already installed on this machine; this is not the later fresh-machine live rerun. Tree/history marker scans found only documented placeholder strings and `ask-user` substring matches, no credential values.
- M1 branch: `feat/m1-offline-harness`. M1 implementation is complete, with zero evaluation API cost. Owner authorized publishing this branch and opening a pull request; merge and submission remain pending.
- Repository role wording is neutral, AI-use disclosure is retained, and M1 is consolidated into one commit for review.

## Pending owner review
- Owner task/answer/grading review remains pending; no sign-off inferred from automated checks.

## Next
1. ozgur hand-solves/reviews t00/t01 and reviews the M1 implementation.
2. M2 (not started): provider client, limits/backoff, live buyer loop and smoke command. Check DeepSeek multi-turn reasoning requirements before the pilot; NVIDIA tool calling remains unverified.
3. M3: remaining task fixtures, references and extended self-tests.
4. Pilot gate as in PLAN.md; no evaluation APIs were called in M1.

## Blockers / open questions for ozgur
- [ ] LinkedIn URL (https://www.linkedin.com/in/ozgure?) and personal site: OK to list in the README? Please confirm the exact URLs.

## Task sign-off (ozgur hand-solves each task from task.json alone)
| Task | Built | Oracle 1.0 | Mutants fail | ozgur sign-off |
|---|---|---|---|---|
| t00_control | ☑ | ☑ | ☑ | ☐ |
| t01_hidden_fees | ☑ | ☑ | ☑ | ☐ |
| t02_expired_offer | ☐ | ☐ | ☐ | ☐ |
| t03_compatibility | ☐ | ☐ | ☐ | ☐ |
| t04_seller_injection | ☐ | ☐ | ☐ | ☐ |
| t05_bundle_vs_unit | ☐ | ☐ | ☐ | ☐ |
| t06_ship_vs_delivery | ☐ | ☐ | ☐ | ☐ |
| t07_final_sale | ☐ | ☐ | ☐ | ☐ |
| t08_no_valid_offer | ☐ | ☐ | ☐ | ☐ |

## Spend log
| Date | Run | Models | Cost (USD) |
|---|---|---|---|
| — | — | — | 0 |
