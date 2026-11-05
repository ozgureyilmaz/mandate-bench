# STATUS

Last updated Nov 5, 2026 (Istanbul).

[M1 PR #1](https://github.com/ozgureyilmaz/mandate-bench/pull/1) merged at `3982bc4`. The current review branch is `feat/m4-hardening`. [PR #2](https://github.com/ozgureyilmaz/mandate-bench/pull/2).

| Work | State |
|---|---|
| Metrics | Prospective policy 2 frozen Nov 4 before new scored trials. It records outcome, strict outcome plus raw format, and independent raw format. The v1 Luna pilot remains 9/12 strict. The separate outcome audit is a retrospective of those trials. |
| Task hardening | t01/t02/t05/t06 and their references are v2. The task schema admits up to eight offers. Other task versions and historical run artifacts retain their original bytes. |
| Paid ledgers | OpenAI USD 1 and Muse USD 15 are independent. Each reserves before sending, reconciles usage, holds unknown usage at worst case, and stops on the first provider error. OpenAI capacity is fully held for unresolved historical exposure. |
| Muse Spark 1.1 | Responses, medium reasoning, 2,048 max output tokens, serial dispatch at 6 RPM and zero retries. Live tool round trip passed after the tool-choice and replay fixes. The four-trap diagnostic awaits owner task and grader sign-off. |
| NVIDIA NIM | Three candidate entries configured. The owner left live smoke pending. Each needs a passing live tool check for admission. [NIM_SMOKE.md](NIM_SMOKE.md) |
| Final benchmark | Pending owner review, live qualification and budget admission. Eligible models for the final set are Luna, passing NIM models and Muse if its smoke and diagnostic pass review. |

## Verification on Nov 5

All 139 workspace tests pass, including the local two-request Muse smoke, separate-ledger checks, policy 2 reporting, mutants and reference isolation. Formatting, denied-warning Clippy, the locked release build and packaging fixtures pass. The release oracle scored 1.0 on all nine tasks. The Muse live smoke passed with one echo tool call and a completed continuation. OpenAI key presence was confirmed by dry run.

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
| Nov 5 | NVIDIA NIM | Live checks deferred by owner, no provider requests | 0 | Live eligibility pending |

Archived live evidence is under `results/diagnostics/`. The initial pilot retained 12 HTTP 429 errors. The slower pilot completed 12 trials and its three strict failures omitted final currency after correct no-order escalation. [PILOT_REVIEW.md](PILOT_REVIEW.md).

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

## Remaining submission work

Owner hand-solves and checks the tasks, references and grader. Freeze the eligible model set after live qualification and budget clearance, then collect five trials per task. Muse passed its smoke and is ready for the diagnostic after owner approval. Finish the README with selected evidence, reproduce from a fresh clone, review the rubric and package a ZIP under 50 MB. The owner records and publishes the unlisted video and performs the final upload. Deadline Nov 9, 08:15 Istanbul / 05:15 GMT. [SUBMISSION_STATUS.md](SUBMISSION_STATUS.md)

## Owner checkpoint

The review packet is ready. The corrected Muse round trip used two successful requests with a combined reported-token estimate of USD 0.00368615. The ledger settled USD 0.00393475 at conservative rates. The audit released the original USD 1.319424 reservation as `unbillable_validation_error` under the owner’s project policy. Remaining ledger capacity is USD 14.99606525. Raw smoke evidence stays under ignored `runs/`. Scored work awaits owner review. [SIGNOFF.md](SIGNOFF.md)
