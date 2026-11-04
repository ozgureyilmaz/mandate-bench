# STATUS

Last updated: **Nov 4, 2026 (Istanbul)**, by the implementation agent.

## Current position
- M1 offline harness is implemented; [PR #1](https://github.com/ozgureyilmaz/mandate-bench/pull/1) is still open/unmerged (checked Nov 4).
- M2 provider client/buyer loop/smoke/run implementation is complete. The live Luna smoke passed Nov 4: one Responses request, HTTP 200, returned ID `gpt-6-luna`, correct tool call, zero retries. Operational usage: 142 input / 19 output / 0 reasoning tokens; estimated cost **USD 0.0000237**. Proof: ignored `runs/smoke-20261104T140330.529129000Z-49678/`. This is transport evidence, not benchmark accuracy.
- M3 adds all remaining task/reference pairs, committed as `0b80d45`. All nine oracles score **1.0**; the M3 suite passed 82 tests. With safe diagnostics, partial-cost accounting, bounded dispatch and report integration, all **89 workspace tests** pass; format, denied-warning Clippy and the locked release build also pass.
- Work is local on `feat/m4-luna-pilot`, based on local M2/M3. M2/M3 have not been pushed or merged; nothing has been uploaded to the hackathon.
- Owner confirmed dashboard credit of USD 4.97 after the failed pilot. Only `gpt-6-luna` is authorized for live work. The historical USD 25–35 / multi-model plan is superseded for current spending; account balance is not inferred from token estimates.

## Pilot outcome and next step
- Attempted the prepared Luna-only 4-task × 3-trial pilot after the owner's continuation request. Source `0b80d45` was clean at start. Run: ignored `runs/run-20261104T141828.777967000Z-53091/`.
- Outcome: 20 HTTP 200 responses followed by 12 HTTP 429 responses; every trial ended in `api_error`, so **zero graded trials and no usable pass rate**. Raw replies, transcripts and all 12 score rows are retained unchanged; no failed trial was automatically rerun. No simulated order was placed.
- Known successful-request usage: 25,766 input tokens (18,236 cached) and 804 output tokens, estimating USD 0.00133736 for those successful requests. Full run cost remains unknown because error usage is unreported; do not infer actual account balance. The old run metadata's zero known-cost subtotal is an accounting limitation, not proof of no spend; request records permit reconstruction.
- Further paid calls stopped. HTTP 429 alone cannot distinguish rate, credit or spend limits; original code/type was not retained. New safe error-code/type capture and known-partial-cost accounting are tested without provider calls. The pilot config is reduced from 30 to 6 RPM as conservative preparation, not a verified account limit or a successful rerun.
- Account credit/RPM/TPM confirmation, a fresh bounded diagnostic and a valid pilot are next. The task hardening gate is still unevaluated; human task review remains pending.
- After the credit confirmation, `configs/control.toml` prepares one fresh t00 diagnostic at 6 RPM, zero retries and 1,024 output tokens. It preserves the original failed pilot; its own result will be recorded separately.
- That fresh control passed all five grader components: six requests, zero API errors, estimated USD 0.00044962; frozen `runs/run-20261104T143901.927556000Z-57636/`, clean source `474f074`. This is one completed control episode, not repeated trap-task performance.
- Offline `report` is implemented and checked against both frozen runs. It shows 12 errors/zero graded outcomes and null accuracy for the first pilot, and one passed control with k=3/5 unavailable. It derives known request costs without rewriting old artifacts. Final report tables still require completed evaluation data.
- Run scheduling now bounds active episodes by provider concurrency and halts new dispatch after a provider API error; unstarted work is retained as an explicit incomplete remainder.
- Following the successful slower control, a fresh 12-trial Luna pilot is the next bounded run at 6 RPM with the new dispatch guard. Preserve both prior runs and stop new provider trials on any API error; no full evaluation is started automatically.

## Missing for submission
See [SUBMISSION_STATUS.md](SUBMISSION_STATUS.md) for the full checklist.
1. Owner hand-solves/reviews every task, reference and grading rule; author sign-off is still blank below.
2. Resolve the HTTP 429 cause, then run a valid pilot, inspect failures and apply the documented ceiling/hardening gate. Decide how to address the planned frontier comparison within the current Luna-only budget restriction.
3. Run/report the final multiple-trial evaluation; retain raw outputs, transcripts, API errors, versions/settings/runtime/cost in `results/` and `transcripts/`.
4. Generate final report tables from selected completed evaluation runs; complete failure analysis and finish README from real evidence. Stable description/domain/prior-work/AI-use sections are drafted, but owner review, model findings and video are pending.
5. Clean-room rerun of the final version, final ZIP under 50 MB with tree/history secret scan and rubric review.
6. Owner records the final ≤5-minute video, uploads it unlisted, checks the public link/audio, then performs the final HackerRank upload with description. Target Nov 8, 22:00 Istanbul; hard deadline Nov 9, 08:15 Istanbul.

## Task sign-off
Owner must solve each task from `task.json` alone; automated checks are not human sign-off.

| Task | Built | Oracle 1.0 | Mutants fail | ozgur sign-off |
|---|---|---|---|---|
| t00_control | ☑ | ☑ | ☑ | ☐ |
| t01_hidden_fees | ☑ | ☑ | ☑ | ☐ |
| t02_expired_offer | ☑ | ☑ | ☑ | ☐ |
| t03_compatibility | ☑ | ☑ | ☑ | ☐ |
| t04_seller_injection | ☑ | ☑ | ☑ | ☐ |
| t05_bundle_vs_unit | ☑ | ☑ | ☑ | ☐ |
| t06_ship_vs_delivery | ☑ | ☑ | ☑ | ☐ |
| t07_final_sale | ☑ | ☑ | ☑ | ☐ |
| t08_no_valid_offer | ☑ | ☑ | ☑ | ☐ |

## Provenance notes
- Tasks, sellers, prices and addresses are synthetic. M1 t00/t01 and the fixed prompt were preserved while adding t02–t08.
- Live API use consists of the successful smoke and the pilot attempt above; local test replies are marked `local_mock`. No model pass-rate claim exists yet.
- The M1 source passed a fresh local clone check. A final-version clean-room rerun with model evidence remains pending.
- `.env` is ignored and owner-only; no credentials are printed or committed. Blank optional URL overrides use valid configured defaults.
- Participant Agreement was accepted personally by ozgur (confirmed Oct 30). Optional profile URLs are not submission blockers and should be omitted until confirmed.
