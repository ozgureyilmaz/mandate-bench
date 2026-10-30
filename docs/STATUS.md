# STATUS

Last updated: **Fri Oct 30, 2026, late evening (Istanbul)**, by Grok Bot.
Update this file at the end of every unit of work. Keep it short.

## Done
- Idea 5a chosen (buyer agent + scripted sellers + mandate). Reasons in DECISIONS.md.
- Repo name: `mandate-bench`. Public repo: https://github.com/ozgureyilmaz/mandate-bench (docs only, no code yet).
- Hackathon rules, deadlines, rubric and checklist captured (docs/HACKATHON.md).
- Design draft v0.2: USD/global tasks, Rust harness, schemas, tools, 9 tasks, grader, metrics (docs/PLAN.md).
- Decided: providers OpenAI + DeepSeek + NVIDIA NIM (no OpenRouter), USD/global, Rust, unlisted YouTube video last, strict `.gitignore` (DECISIONS.md).
- Model list and budget confirmed by ozgur on Oct 30 (PLAN.md §8): core set without gpt-6-astra, about $25–35 total. NVIDIA models still need a tool-calling smoke test. Provider details for agents are in AGENTS.md.
- X links and buildevals.com added (docs/LINKS.md, docs/PRIOR_WORK.md).
- micro1 Participant Agreement accepted by ozgur (Oct 30).
- ozgur's reason for 5a recorded (DECISIONS.md).

## In progress
- ozgur reviewing PLAN.md, especially §8.

## Next
1. ozgur answers the open questions below.
2. Codex: scaffold the Cargo workspace (env, tools, LLM client, agent loop, total-formula function, grader, self-tests).
3. Smoke-test each provider once keys are set locally (`cargo run --release -- smoke`); drop NVIDIA models without working tool calls.
4. Pilot tasks t00, t01, t04, t08 with references and oracle; pilot run on Oct 31.

## Blockers / open questions for ozgur
- [ ] LinkedIn URL (https://www.linkedin.com/in/ozgure?) and personal site: OK to list in the README? Please confirm the exact URLs.

## Task sign-off (ozgur hand-solves each task from task.json alone)
| Task | Built | Oracle 1.0 | Mutants fail | ozgur sign-off |
|---|---|---|---|---|
| t00_control | ☐ | ☐ | ☐ | ☐ |
| t01_hidden_fees | ☐ | ☐ | ☐ | ☐ |
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
