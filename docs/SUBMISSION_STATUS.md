# Submission readiness — Nov 8, 2026

**Not ready to upload yet.** Software and task data are implemented; benchmark evidence, human review and the final submission materials remain pending.

| Item | Current evidence | Still needed |
|---|---|---|
| Task set | 9 synthetic task/reference pairs; 8 traps + 1 control | Owner hand-solution and approval of every scenario, answer and rule |
| Grader | 9/9 offline oracles; workspace and packaging checks documented in STATUS.md | Owner review of policy 2 and the four v2 task/reference pairs |
| Live integration | Smoke passed; a fresh one-trial control also passed the full buyer loop/grader at 6 RPM, estimated USD 0.00044962 | Repeated trap-task performance still unmeasured |
| Pilot | Original 12 API errors preserved; fresh slower 12-trial pilot completed without API errors | Review 3 strict format-only failures: correct ASK_USER but missing currency; strict 9/12 versus proposed outcome audit 12/12. Review the frozen metrics and adopted hardening before final run |
| Final model results | Diagnostic phases committed under results/diagnostics; no final selected benchmark dataset | Reviewed five-trial evidence for all nine tasks; preserve errored/failed runs and distinguish original strict scores from audits/regrades |
| Model comparison / budget | Active diagnostic scope is Muse Spark 1.1 only, under its separate USD 15 ceiling. The Luna v1 pilot is frozen at 9/12 strict and OpenAI capacity remains held for unresolved historical exposure. | Both SIGNOFF boxes, all nine STATUS owner cells and the owner's explicit signed chat reply must be present before the Muse diagnostic. No additional OpenAI requests are authorized. NIM candidates are archived and untested; no key or live qualification is required. |
| Analysis/report | Offline report + versioned outcome audit implemented; pilot review and four versioned changes | Human confirmation and a supported domain takeaway; current observed failures are not unauthorized purchases |
| README / description | Short description tightened and 111-word submission-field draft prepared from archived evidence | Owner review, Muse results after approval, final runtime/cost and transcript analysis, video URL. [Blurb](SUBMISSION_BLURB.md) |
| Reproducibility | Rust pin/Cargo.lock; earlier M1 fresh-clone check | Reproduce the final version from only the submitted README, including a bounded model check |
| Release | M1 PR #1 merged. Current work on PR #2. | Owner-selected source revision/repository release; review changes before merge |
| ZIP | Read-only preflight remains blocked | README TODOs, video URL, nine owner approvals and a committed nine-task/five-trial result set. Current helper recognizes YouTube links. A different permitted host would need a validator update. The authorized four-task/three-trial diagnostic does not meet the existing result-coverage gate. |
| Video | English script and shot list drafted. [Script](VIDEO_SCRIPT.md) | Owner records at most five minutes. YouTube, Loom, Drive, Vimeo or similar must open without login and play with audio. Same URL in README and submission field. |
| Human/final upload | Agreement already accepted by owner | Task/grader/failure review, self-score against rubric, final ZIP + video link + description uploaded by owner |

The public event rules call for one ZIP (maximum 50 MB), a short video link and a short description. They allow low-cost models and encourage frontier-model evidence. Our detailed project checklist separately targets at least one frontier model with multiple trials; Luna-only smoke evidence does not satisfy that internal target. Sources: [official event/FAQ](https://www.hackerrank.com/event/micro1-eval-hackathon), [captured detailed rules](HACKATHON.md), [project definition of done](../AGENTS.md).

**Deadline:** Nov 9, 2026 at 08:15 Istanbul / 05:15 GMT. Internal target: Nov 8 at 22:00 Istanbul. Agents do not submit, publish the video or merge without explicit owner authorization.

The current checkpoint is owner task and grader review after the passing Muse smoke. The Muse-only diagnostic is gated on both SIGNOFF boxes, all nine STATUS owner cells and the owner's explicit signed chat reply; the final full run is outside the current work order. NIM qualification is retired from the active submission. [SIGNOFF.md](SIGNOFF.md) · [NIM smoke archive](NIM_SMOKE.md)

## Nov 8 gate check

The two SIGNOFF approvals and all nine STATUS owner cells are empty. The current work is submission drafting. The next owner action is to review the answer sheet, mark both files and reply **signed**. [Review packet](SIGNOFF.md)

A later diagnostic may support a clearly labeled partial-coverage submission, but the packaging helper's stricter nine-task/five-trial condition remains in force. Changing that condition requires an explicit project decision. This order authorizes neither an extra paid batch nor a real ZIP.

The clean Nov 8 `scripts/package.sh --check` returned 12 pending gates. These are one README marker gate, one video-link gate, nine owner-approval gates and one committed result-coverage gate. The draft links resolve. No real ZIP was created.
