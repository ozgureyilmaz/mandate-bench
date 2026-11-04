# Submission readiness — Nov 4, 2026

**Not ready to upload yet.** Software and task data are implemented; benchmark evidence, human review and the final submission materials remain pending.

| Item | Current evidence | Still needed |
|---|---|---|
| Task set | 9 synthetic task/reference pairs; 8 traps + 1 control | Owner hand-solution and approval of every scenario, answer and rule |
| Grader | 9/9 offline oracles; 90 Rust tests plus packaging shell checks pass | Owner review/freeze of strict versus proposed outcome metric and final validation after any task changes |
| Live integration | Smoke passed; a fresh one-trial control also passed the full buyer loop/grader at 6 RPM, estimated USD 0.00044962 | Repeated trap-task performance still unmeasured |
| Pilot | Original 12 API errors preserved; fresh slower 12-trial pilot completed without API errors | Review 3 strict format-only failures: correct ASK_USER but missing currency; strict 9/12 versus proposed outcome audit 12/12. Decide metric/task difficulty before final run |
| Final model results | Diagnostic phases committed under results/diagnostics; no final selected benchmark dataset | Reviewed five-trial evidence for all nine tasks; preserve errored/failed runs and distinguish original strict scores from audits/regrades |
| Model comparison | Only Luna is currently approved under the approximately USD 5 credit restriction | Resolve the internal frontier-evidence goal/model plan before adding any other model; do not infer permission from the historical model table |
| Analysis/report | Offline report + versioned outcome audit implemented; pilot review and hardening candidate drafted | Human confirmation, final metric selection and a supported domain takeaway; current observed failures are not unauthorized purchases |
| README | Setup and task/grading descriptions exist | Finish required short description, domain/expertise, model-results/settings/runtime/cost, failure analysis, sources/prior work, reviewed AI-use disclosure and final video link |
| Reproducibility | Rust pin/Cargo.lock; earlier M1 fresh-clone check | Reproduce the final version from only the submitted README, including a bounded model check |
| Release | M1 PR #1 open; M2/M3 local | Owner-selected source revision/repository release; review changes before any public push or merge |
| ZIP | Gated packaging utility and synthetic archive tests pass; real preflight correctly refuses current project | Reviewed data/README/video/sign-offs first, then explicit --out ZIP (<50 MB), tree/history scan and actual archive inspection |
| Video | Not recorded | ≤5 minutes, final results walkthrough with audio, unlisted link accessible without login; same link in README/upload field |
| Human/final upload | Agreement already accepted by owner | Task/grader/failure review, self-score against rubric, final ZIP + video link + description uploaded by owner |

The public event rules call for one ZIP (maximum 50 MB), a short video link and a short description. They allow low-cost models and encourage frontier-model evidence. Our detailed project checklist separately targets at least one frontier model with multiple trials; Luna-only smoke evidence does not satisfy that internal target. Sources: [official event/FAQ](https://www.hackerrank.com/event/micro1-eval-hackathon), [captured detailed rules](HACKATHON.md), [project definition of done](../AGENTS.md).

**Deadline:** Nov 9, 2026 at 08:15 Istanbul / 05:15 GMT. Internal target: Nov 8 at 22:00 Istanbul. Agents do not submit, publish the video or merge without explicit owner authorization.

The remaining implementation can be progressed without spending on additional models. Paid pilot/full runs need to follow the current model/budget restriction and human-review workflow; a larger model must be a separate owner decision.
