# Owner sign-off · Nov 5, 2026

Replay fix revision `1b2d3ad` is published on `feat/m4-hardening`. [PR #2](https://github.com/ozgureyilmaz/mandate-bench/pull/2)

The release oracle scores 1.0 on all nine tasks. All 139 workspace tests pass, including policy 2 metrics, hidden-reference isolation, hardening mutants, both ledgers and the local Muse tool round trip. Formatting, Clippy, release build and packaging fixtures pass. The original Luna v1 pilot remains 9/12 strict. [STATUS.md](STATUS.md) · [PILOT_REVIEW.md](PILOT_REVIEW.md)

## Task and grader review

Hand-solve each visible task before opening its reference. Check the authorized action, minimum pack count, total, delivery, compatibility and returns. Review the outcome, strict and raw-format checks, including successful no-order escalation with missing final currency. Record the nine individual approvals in [STATUS.md](STATUS.md) and complete the two owner checkboxes below.

| Task | Version | Visible input | Hidden reference |
|---|---:|---|---|
| t00_control | 1 | [Task](../tasks/t00_control/task.json) | [Reference](../reference/t00_control.json) |
| t01_hidden_fees | 2 | [Task](../tasks/t01_hidden_fees/task.json) | [Reference](../reference/t01_hidden_fees.json) |
| t02_expired_offer | 2 | [Task](../tasks/t02_expired_offer/task.json) | [Reference](../reference/t02_expired_offer.json) |
| t03_compatibility | 1 | [Task](../tasks/t03_compatibility/task.json) | [Reference](../reference/t03_compatibility.json) |
| t04_seller_injection | 1 | [Task](../tasks/t04_seller_injection/task.json) | [Reference](../reference/t04_seller_injection.json) |
| t05_bundle_vs_unit | 2 | [Task](../tasks/t05_bundle_vs_unit/task.json) | [Reference](../reference/t05_bundle_vs_unit.json) |
| t06_ship_vs_delivery | 2 | [Task](../tasks/t06_ship_vs_delivery/task.json) | [Reference](../reference/t06_ship_vs_delivery.json) |
| t07_final_sale | 1 | [Task](../tasks/t07_final_sale/task.json) | [Reference](../reference/t07_final_sale.json) |
| t08_no_valid_offer | 1 | [Task](../tasks/t08_no_valid_offer/task.json) | [Reference](../reference/t08_no_valid_offer.json) |

- [ ] Task and reference sign-off
- [ ] Grader and metrics sign-off

The task contract now permits eight offers. The four v2 fixtures retain the existing tool, fee, quantity and deadline semantics. [Hardening details](HARDENING_PROPOSAL.md) · [Frozen policy](DECISIONS.md)

## Muse live verification

The corrected `muse-spark-1.1` live smoke passed. The initial call returned the exact tool invocation. The continuation returned `smoke-roundtrip-confirmed` with zero additional calls. The first reply was reused after the local replay fix, so only the pending continuation was sent. Both requests returned HTTP 200. Local evidence is in `runs/smoke-20261105T122337.636593000Z-54711/` and `runs/smoke-20261105T122821.305894000Z-57376/`. The NIM live checks remain deferred by the owner. [Smoke config](../configs/muse-smoke.toml) · [NIM status](NIM_SMOKE.md)

| Spend ledger | Project ceiling | Current checkpoint |
|---|---:|---|
| OpenAI | USD 1 | Full historical hold. Reconcile billing before another paid Luna request. |
| Muse | USD 15 | USD 0.00368615 estimated for the two successful requests. The original validation reservation was released by audit. Remaining conservative capacity is USD 14.99606525. |

After both sign-offs and a passing Muse smoke, the diagnostic selects t01/t02/t05/t06 at three trials each. Its pre-batch estimate is USD 3.58896. Each outbound request still reserves at the full documented input bound and configured output cap. Review coverage, outcome, format and spend before admitting Muse to the final comparison. [Diagnostic config](../configs/muse-diagnostic.toml) · [Accounting decisions](DECISIONS.md)

## Submission checkpoint

Final five-trial evidence across nine tasks, owner transcript review, final README, clean-room reproduction, rubric review, ZIP and unlisted walkthrough remain open. The owner handles final submission. [Submission readiness](SUBMISSION_STATUS.md)
