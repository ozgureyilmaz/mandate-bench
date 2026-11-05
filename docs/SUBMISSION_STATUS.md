# Submission readiness — Nov 5, 2026

**Not ready to upload yet.** Software and task data are implemented; benchmark evidence, human review and the final submission materials remain pending.

| Item | Current evidence | Still needed |
|---|---|---|
| Task set | 9 synthetic task/reference pairs; 8 traps + 1 control | Owner hand-solution and approval of every scenario, answer and rule |
| Grader | 9/9 offline oracles; workspace and packaging checks documented in STATUS.md | Owner review of policy 2 and the four v2 task/reference pairs |
| Live integration | Smoke passed; a fresh one-trial control also passed the full buyer loop/grader at 6 RPM, estimated USD 0.00044962 | Repeated trap-task performance still unmeasured |
| Pilot | Original 12 API errors preserved; fresh slower 12-trial pilot completed without API errors | Review 3 strict format-only failures: correct ASK_USER but missing currency; strict 9/12 versus proposed outcome audit 12/12. Review the frozen metrics and adopted hardening before final run |
| Final model results | Diagnostic phases committed under results/diagnostics; no final selected benchmark dataset | Reviewed five-trial evidence for all nine tasks; preserve errored/failed runs and distinguish original strict scores from audits/regrades |
| Model comparison / budget | Separate cumulative ceilings of USD 1 for Luna and USD 15 for Muse Spark 1.1 | Reconcile historical Luna usage and qualify NIM when resumed. Muse is excluded after its HTTP 400 smoke failure. |
| Analysis/report | Offline report + versioned outcome audit implemented; pilot review and four versioned changes | Human confirmation and a supported domain takeaway; current observed failures are not unauthorized purchases |
| README | Setup and task/grading descriptions exist | Finish required short description, domain/expertise, model-results/settings/runtime/cost, failure analysis, sources/prior work, reviewed AI-use disclosure and final video link |
| Reproducibility | Rust pin/Cargo.lock; earlier M1 fresh-clone check | Reproduce the final version from only the submitted README, including a bounded model check |
| Release | M1 PR #1 merged. Current work on PR #2. | Owner-selected source revision/repository release; review changes before merge |
| ZIP | Gated packaging utility and synthetic archive tests pass; real preflight correctly refuses current project | Reviewed data/README/video/sign-offs first, then explicit --out ZIP (<50 MB), tree/history scan and actual archive inspection |
| Video | Not recorded | ≤5 minutes, final results walkthrough with audio, unlisted link accessible without login; same link in README/upload field |
| Human/final upload | Agreement already accepted by owner | Task/grader/failure review, self-score against rubric, final ZIP + video link + description uploaded by owner |

The public event rules call for one ZIP (maximum 50 MB), a short video link and a short description. They allow low-cost models and encourage frontier-model evidence. Our detailed project checklist separately targets at least one frontier model with multiple trials; Luna-only smoke evidence does not satisfy that internal target. Sources: [official event/FAQ](https://www.hackerrank.com/event/micro1-eval-hackathon), [captured detailed rules](HACKATHON.md), [project definition of done](../AGENTS.md).

**Deadline:** Nov 9, 2026 at 08:15 Istanbul / 05:15 GMT. Internal target: Nov 8 at 22:00 Istanbul. Agents do not submit, publish the video or merge without explicit owner authorization.

The current checkpoint is owner review after the failed Muse smoke. The pending model runs follow the separate budget ledgers and smoke gates. [SIGNOFF.md](SIGNOFF.md)
