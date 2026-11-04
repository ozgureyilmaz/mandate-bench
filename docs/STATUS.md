# STATUS

Last updated: **Nov 4, 2026 (Istanbul)**, by the implementation agent.

## Current position
- M1 offline harness is implemented; [PR #1](https://github.com/ozgureyilmaz/mandate-bench/pull/1) is still open/unmerged (checked Nov 4).
- M2 provider client/buyer loop/smoke/run implementation is complete. The live Luna smoke passed Nov 4: one Responses request, HTTP 200, returned ID `gpt-6-luna`, correct tool call, zero retries. Operational usage: 142 input / 19 output / 0 reasoning tokens; estimated cost **USD 0.0000237**. Proof: ignored `runs/smoke-20261104T140330.529129000Z-49678/`. This is transport evidence, not benchmark accuracy.
- M3 adds all remaining task/reference pairs. All nine oracles score **1.0**; all **82 workspace tests** pass, including every distractor, extra packs/orders, tolerance boundaries, schema and hidden-reference isolation checks. Formatting, denied-warning Clippy and the locked release build also pass.
- Work is local on `feat/m3-task-set`, based on local M2. M2/M3 have not been pushed or merged; nothing has been uploaded to the hackathon.
- Current account credit is owner-reported at approximately USD 5. Only `gpt-6-luna` is authorized for live work. The historical USD 25–35 / multi-model plan is superseded for current spending; actual account balance is not inferred from the estimate above.

## Prepared next step
- `configs/pilot.toml`: Luna only, t00/t01/t04/t08, three trials each, concurrency 1, zero retries, 2,048 output tokens. Dry-run preflight passes. No pilot or full evaluation has been run.
- Pilot task hardening gate remains pending. Task version 1 is pre-pilot and still needs owner review.

## Missing for submission
See [SUBMISSION_STATUS.md](SUBMISSION_STATUS.md) for the full checklist.
1. Owner hand-solves/reviews every task, reference and grading rule; author sign-off is still blank below.
2. Run the pilot, inspect failures and apply the documented ceiling/hardening gate. Decide how to address the planned frontier comparison within the current Luna-only budget restriction.
3. Run/report the final multiple-trial evaluation; retain raw outputs, transcripts, API errors, versions/settings/runtime/cost in `results/` and `transcripts/`.
4. Implement report/metrics output, complete failure analysis and finish README/AI-use/prior-work text from real evidence. Owner reads the failing transcripts and confirms claims.
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
- Only a smoke request has used the live evaluation API so far; local test replies are marked `local_mock`. No model pass-rate claim exists yet.
- The M1 source passed a fresh local clone check. A final-version clean-room rerun with model evidence remains pending.
- `.env` is ignored and owner-only; no credentials are printed or committed. Blank optional URL overrides use valid configured defaults.
- Participant Agreement was accepted personally by ozgur (confirmed Oct 30). Optional profile URLs are not submission blockers and should be omitted until confirmed.
