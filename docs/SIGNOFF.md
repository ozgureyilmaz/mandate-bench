# Owner sign-off · Nov 5, 2026

Replay fix revision `1b2d3ad` is published on `feat/m4-hardening`. [PR #2](https://github.com/ozgureyilmaz/mandate-bench/pull/2)

The release oracle scores 1.0 on all nine tasks. All 144 workspace tests pass, including policy 2 metrics, hidden-reference isolation, hardening mutants, both ledgers and the local Muse tool round trip. Formatting, Clippy, release build and packaging fixtures pass. The original Luna v1 pilot remains 9/12 strict. [STATUS.md](STATUS.md) · [PILOT_REVIEW.md](PILOT_REVIEW.md)

## Task and grader review

Set aside about 20 minutes. Solve each task from its `task.json` alone, then compare with the [nine-task answer sheet](SIGNOFF_ANSWERS.md). Check the authorized action, minimum pack count, total, delivery and returns. Review the outcome, strict and raw-format checks, including successful no-order escalation with missing final currency. Check both boxes below and mark all nine owner cells as ☑ or ✅ in [STATUS.md](STATUS.md). Reply **signed** after both files are complete.

**Nov 8, 2026:** Both owner boxes and all nine STATUS cells are checked. The answer sheet matches every committed `reference/<task>.json` decision, pack count and total; the release oracle still scores 1.0 on all nine tasks. Muse CLI preflight for scored runs accepts the file gates; the owner then replied **signed** in chat, completing the dispatch gate.

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

- [x] Task and reference sign-off
- [x] Grader and metrics sign-off

The task contract now permits eight offers. The four v2 fixtures retain the existing tool, fee, quantity and deadline semantics. [Hardening details](HARDENING_PROPOSAL.md) · [Frozen policy](DECISIONS.md)

## Muse live verification

The corrected `muse-spark-1.1` live smoke passed. The initial call returned the exact tool invocation. The continuation returned `smoke-roundtrip-confirmed` with zero additional calls. The first reply was reused after the local replay fix, so only the pending continuation was sent. Both requests returned HTTP 200. Local evidence is in `runs/smoke-20261105T122337.636593000Z-54711/` and `runs/smoke-20261105T122821.305894000Z-57376/`. [Smoke config](../configs/muse-smoke.toml)

| Spend ledger | Project ceiling | Current checkpoint |
|---|---:|---|
| OpenAI | USD 1 | Full historical hold for the archived Luna evidence. |
| Muse | USD 15 | USD 0.00368615 estimated for the two successful requests. The original validation reservation was released by audit. The completed diagnostic leaves USD 14.51772725 in conservative capacity. |

The two owner checkboxes, all nine STATUS cells and the owner's **signed** reply were complete before dispatch. The approved Muse-only diagnostic ran once on t01/t02/t05/t06 v2 at three trials each. Outcome, strict and raw format each passed 12/12, with zero API errors. Estimated batch cost was USD 0.20152740. [Completed diagnostic](MUSE_DIAGNOSTIC_V1.md)

## Submission checkpoint

Final five-trial evidence across nine tasks, owner transcript review, final README, clean-room reproduction, rubric review, ZIP and unlisted walkthrough remain open. The owner handles final submission. [Submission readiness](SUBMISSION_STATUS.md)
