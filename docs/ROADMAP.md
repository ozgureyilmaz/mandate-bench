# ROADMAP: Oct 30 → Nov 9, 2026 (all times Istanbul, UTC+3)

Hard deadline: **Nov 9, 08:15 Istanbul (05:15 GMT)**. Internal target for the final upload: **Nov 8, 22:00**.
**Midterms Nov 2–6:** only background runs and short reviews (≤ 30 min/day for ozgur). No new features.
Owners: **O** = ozgur, **C** = implementation agent, **CC** = Cursor cloud agents, **G** = Grok Bot.

## Fri Oct 30: decide and scaffold
- [x] Pick idea 5a; name it `mandate-bench` (O)
- [x] Docs set: AGENTS, PLAN, ROADMAP, HACKATHON, PRIOR_WORK, LINKS, STATUS, DECISIONS, AI_USE (G)
- [x] Public GitHub repo created and docs pushed (O's go-ahead)
- [x] Decisions: USD/global tasks, Rust harness, OpenAI + DeepSeek + NVIDIA NIM, strict .gitignore, unlisted YouTube video last (O)
- [ ] ozgur reviews PLAN.md §2–8 (schemas, tools, grader, model list) and answers the remaining open questions in STATUS.md (O)
- [ ] Scaffold the Cargo workspace: `rust-toolchain.toml`, `Cargo.lock`, schemas, scripted env + ledger, tool layer, LLM client with per-provider base_url + rate limiting/backoff, agent loop, total-formula function (C)
- [ ] Grader + self-test harness (oracle / mutant / leak checks) (C or CC)
- [ ] Pilot tasks t00, t01, t04, t08: task.json + reference + oracle path (C; O verifies the numbers)

## Sat Oct 31: pilot and gate
- [ ] `cargo run --release -- oracle` = 1.0; `cargo test --workspace` green (C)
- [ ] `cargo run --release -- smoke` per provider; drop NVIDIA models without working tool calls (C)
- [ ] Pilot: 4 tasks × 3 models (one per provider) × 3 trials (C runs, G tracks cost)
- [ ] Read every failing pilot transcript (O)
- [ ] **Gate:** best model ≥ 85% on pilot trap trials → harden tasks (PLAN §9); control fails → fix bugs; record the outcome in DECISIONS.md (O + C)

## Sun Nov 1: complete the task set
- [ ] Remaining tasks t02, t03, t05, t06, t07, with references and oracle paths (C / CC in parallel, one task per agent)
- [ ] Mutant suite extended to all 9 tasks; tolerance and number-format tests (C)
- [ ] O hand-solves every task from `task.json` alone and signs off in STATUS.md (O)
- [ ] Freeze task_version v1 and system prompt v1 (O)
- [ ] `configs/full.toml` with the confirmed model list (O confirms, C writes)

## Mon Nov 2 – Fri Nov 6: midterms (background only)
- [ ] One full run: 9 tasks × 5 trials × 5–7 models, started one evening (DeepSeek off-peak) (C); G monitors progress, rate-limit errors and cost
- [ ] Commit raw outputs, scores and transcripts as soon as they land (C)
- [ ] ≤ 30 min/day: O skims the failure list G prepares (task, model, label, transcript link)
- [ ] G keeps STATUS.md and the draft results tables in sync; no task edits during the run

## Sat Nov 7: analysis
- [ ] Failure analysis: confirm labels by reading transcripts; pick quotes; find a near-miss task (O, with G's help drafting)
- [ ] Fix genuinely ambiguous tasks only, with the reason in DECISIONS.md; re-run the affected tasks for all models (C)
- [ ] Results tables (pass rate, pass^k, spread, cost) generated from `results/` (C)
- [ ] Write the hot take (O)

## Sun Nov 8: package and upload
- [ ] README: every required section filled, no TODOs left (O + G)
- [ ] AI_USE.md and PRIOR_WORK finalized (O)
- [ ] Clean-room rerun from a fresh clone following only the README (oracle + 1 cheap model) (C or CC)
- [ ] Build the zip (`git archive`, so `target/`, `runs/` and ignored files stay out); check size < 50 MB, scan for secrets, make sure there are no videos (C); G runs the HACKATHON.md checklist
- [x] Participant Agreement accepted (O, personally; confirmed Oct 30)
- [ ] **Last step:** record the video (≤ 5 min, with audio), upload it to YouTube as **unlisted**, test the link in a logged-out browser, put the link in the README (O)
- [ ] **O uploads one final zip, fills in the video link and the description**, target 22:00

## Mon Nov 9
- [ ] 08:15 Istanbul: hard deadline (05:15 GMT). Buffer only.
