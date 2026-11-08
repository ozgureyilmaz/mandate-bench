# STATUS

Last updated Nov 8, 2026 (Istanbul).

[M1 PR #1](https://github.com/ozgureyilmaz/mandate-bench/pull/1) merged at `3982bc4`. The current review branch is `feat/m4-hardening`. [PR #2](https://github.com/ozgureyilmaz/mandate-bench/pull/2).

| Work | State |
|---|---|
| Metrics | Prospective policy 2 frozen Nov 4 before new scored trials. It records outcome, strict outcome plus raw format, and independent raw format. The v1 Luna pilot remains 9/12 strict. The separate outcome audit is a retrospective of those trials. |
| Task hardening | t01/t02/t05/t06 and their references are v2. The task schema admits up to eight offers. Other task versions and historical run artifacts retain their original bytes. |
| Paid ledgers | OpenAI USD 1 and Muse USD 15 are independent. Each reserves before sending, reconciles usage, holds unknown usage at worst case, and stops on the first provider error. Muse has USD 14.51772725 remaining after the completed diagnostic. OpenAI capacity remains fully held for unresolved historical exposure. |
| Muse Spark 1.1 | Responses, medium reasoning, 2,048 max output tokens, serial dispatch at 6 RPM and zero retries. Live tool round trip passed after the tool-choice and replay fixes. All owner gates were completed. The four-trap diagnostic passed 12/12 outcome, strict and raw-format checks, with zero API errors. |
| NVIDIA NIM | Retired from active submission. The three candidates remain archived and untested; no live check or NVIDIA key is required. The Nov 5 missing-key dry run is preserved as historical evidence. [NIM_SMOKE.md](NIM_SMOKE.md) · [archived config](../configs/historical/nim-smoke.toml) |
| Final benchmark | Outside the current work order. Only the Muse-only four-trap diagnostic may proceed after its gates. The Luna v1 pilot remains frozen at 9/12 strict; no further OpenAI calls are authorized. NIM is outside the active submission. |

## Verification on Nov 5

All 144 workspace tests pass, including the local two-request Muse smoke, separate-ledger checks, policy 2 reporting, mutants and reference isolation. Formatting, denied-warning Clippy, the locked release build and packaging fixtures pass. The release oracle scored 1.0 on all nine tasks. The Muse live smoke passed with one echo tool call and a completed continuation. The default offline smoke preflight selects Muse. The historical NIM config is rejected before key lookup. The Muse ledger is unchanged.

## Spend log

Token-price estimates are distinct from the conservative ledger balances. Add an entry after each live attempt with provider, run ID, known cost and unresolved exposure.

| Date | Provider | Evidence | Known estimated USD | Unresolved usage or hold |
|---|---|---|---:|---|
| Nov 4 | OpenAI | Archived initial smoke | 0.00002370 | None reported |
| Nov 4 | OpenAI | Initial pilot, known successful requests | 0.00133736 | Error usage unknown. The full USD 1 ceiling covers historical exposure. |
| Nov 4 | OpenAI | Slower control | 0.00044962 | None reported |
| Nov 4 | OpenAI | Fresh serial pilot, 12 trials | 0.00490785 | None reported |
| Nov 5 | OpenAI | Offline implementation checks, no provider requests | 0 | Historical hold carried forward |
| Nov 5 | Meta | Offline implementation checks, no provider requests | 0 | Live smoke pending |
| Nov 5 | Meta | Live smoke `smoke-20261105T121126.314341000Z-49545`, one HTTP 400 | Unknown | USD 1.319424 held at worst case. Ledger halted. |
| Nov 5 | Meta | Corrected smoke `smoke-20261105T122337.636593000Z-54711`. Initial call returned HTTP 200 and the expected tool call. Local validation blocked the continuation before HTTP. | 0.00209495 | Initial call settled. Historical hold retained. |
| Nov 5 | Meta | Continuation `smoke-20261105T122821.305894000Z-57376`. One HTTP 200, exact echo result, zero extra calls. | 0.00159120 | Continuation settled. Historical USD 1.319424 hold retained. |
| Nov 5 | Meta | Audit of reservation 1 for the initial 400 at 17:04:16.643Z | 0 under project policy | Released USD 1.319424. Available capacity increased from USD 13.67664125 to USD 14.99606525. |
| Nov 8 | Meta | `muse-diagnostic-v1`, 12 completed trials, 94 HTTP 200 requests | 0.20152740 | Conservative settlement USD 0.47833800. Remaining capacity USD 14.51772725. |
| Nov 5 | NVIDIA NIM | Historical live checks deferred by owner, no provider requests | 0 | All three candidates later retired from active submission |
| Nov 5 | NVIDIA NIM | Historical dry run. Local key missing. | 0 | All three candidates remain untested and archived |

Archived live evidence is under `results/diagnostics/`. The initial pilot retained 12 HTTP 429 errors. The slower pilot completed 12 trials and its three strict failures omitted final currency after correct no-order escalation. [PILOT_REVIEW.md](PILOT_REVIEW.md).

## Task sign-off
Owner must solve each task from `task.json` alone; automated checks are not human sign-off.

| Task | Built | Oracle 1.0 | Mutants fail | ozgur sign-off |
|---|---|---|---|---|
| t00_control | ☑ | ☑ | ☑ | ✅ |
| t01_hidden_fees | ☑ | ☑ | ☑ | ✅ |
| t02_expired_offer | ☑ | ☑ | ☑ | ✅ |
| t03_compatibility | ☑ | ☑ | ☑ | ✅ |
| t04_seller_injection | ☑ | ☑ | ☑ | ✅ |
| t05_bundle_vs_unit | ☑ | ☑ | ☑ | ✅ |
| t06_ship_vs_delivery | ☑ | ☑ | ☑ | ✅ |
| t07_final_sale | ☑ | ☑ | ☑ | ✅ |
| t08_no_valid_offer | ☑ | ☑ | ☑ | ✅ |

## Remaining submission work

Owner task and grader review is complete. The approved Muse-only diagnostic is complete. Review its selected evidence and narrative before final submission. NIM key setup and qualification are not required; no new OpenAI calls are authorized. Finish the README with selected evidence, reproduce from a fresh clone, review the rubric and package a ZIP under 50 MB. The owner records and publishes the unlisted video and performs the final upload. Deadline Nov 9, 08:15 Istanbul / 05:15 GMT. [SUBMISSION_STATUS.md](SUBMISSION_STATUS.md)

## Owner checkpoint

The [nine-task answer sheet](SIGNOFF_ANSWERS.md) was rechecked against all nine committed tasks, references and grader rules on Nov 8. Decisions, totals and traps match every reference file; the release oracle scores 1.0 on all nine tasks. Both SIGNOFF boxes and all nine owner cells below are checked, so Muse scored-run CLI preflight passes the file gates. The owner replied **signed**, completing the diagnostic approval gate. The approved batch subsequently completed. The corrected Muse round trip used two successful requests with a combined reported-token estimate of USD 0.00368615. The ledger settled USD 0.00393475 at conservative rates. The audit released the original USD 1.319424 reservation as `unbillable_validation_error` under the owner’s project policy. Remaining ledger capacity before the diagnostic was USD 14.99606525. Raw smoke evidence stays under ignored `runs/`. [SIGNOFF.md](SIGNOFF.md)

## Nov 8 submission preparation

Both file approvals and the signed chat reply were recorded before the approved batch. That batch is complete.

The submission blurb and English video script are drafted using the archived Luna pilot and passing Muse smoke. Muse diagnostic results are now recorded in the selected four-task artifact set. README accepts YouTube, Loom, Google Drive, Vimeo or a similar host, with a login-free link and working audio. [Submission blurb](SUBMISSION_BLURB.md) · [Video script](VIDEO_SCRIPT.md)

The deadline is Nov 9 at 08:15 Istanbul / 05:15 GMT. Owner review, scored evidence and the video remain open. The packaging check also requires a committed nine-task run with at least five trials per task. The currently authorized four-task, three-trial diagnostic would not satisfy that internal gate.

A fresh packaging check will follow the diagnostic and README update. The current helper accepts YouTube URLs. Other officially permitted hosts require a validator update. The final video and the internal nine-task/five-trial coverage gate remain open.

## Muse diagnostic authorization · Nov 8

Both file approvals and the owner's signed chat confirmation are present. Preflight passed for one model, four tasks and three trials. The unchanged config projects USD 3.58896 against USD 14.99606525 in remaining Muse capacity. Start one Muse-only batch with the existing guard, serial dispatch and zero retries. Stop new dispatch on the first provider error or budget refusal.

## Muse diagnostic v1 completed · Nov 8

The run `run-20261108T072135.587310000Z-28902` completed all 12 approved trials on t01/t02/t05/t06 v2. Outcome, strict and raw-format rates were each 12/12. No-decision rate was 0/12, mean tool calls 6.83, and API errors 0. Runtime was about 15 minutes 32 seconds. Estimated cost was USD 0.20152740 and remaining conservative Muse capacity is USD 14.51772725. No additional batch is started. [Full diagnostic review](MUSE_DIAGNOSTIC_V1.md)

The selected diagnostic artifact directory is `results/diagnostics/run-20261108T072135.587310000Z-28902/`. All 29 files were copied byte-for-byte from the completed scratch run. Each assessed task saw no failures. Dedicated injection and ASK_USER behavior were outside this Muse run. The archived Luna v1 9/12 strict result remains historical evidence.
