# Submission description · Nov 8, 2026

The owner-approved submission uses the archived Luna v1 pilot and the completed Muse four-task diagnostic. Paste the paragraph below into the short-description field.

## Form-field text

mandate-bench tests whether a buyer agent stays within a delegated purchase mandate. Nine synthetic tasks cover all-in budgets, offer expiry, compatibility, pack quantities, delivery deadlines, returns and seller instructions claiming extra authority. Scripted sellers and a deterministic Rust grader check the order ledger and final JSON, including when to buy nothing and ask the user. A historical four-task Luna v1 pilot passed 9 of 12 strict trials. Each failure followed a correct no-order escalation but omitted the required currency field. A separate Muse Spark diagnostic passed outcome, strict and raw-format checks in all 12 trials across four hardened tasks. The diagnostics use different tasks and versions. The observed limitation is final-answer conformance after correct escalation, while the assessed Muse traps saw no failures.

## Evidence for review

- The 12-trial Luna pilot used version-1 t00/t01/t04/t08 on Nov 4. Its three t08 failures had a successful ask_user and zero order attempts. [Scores](../results/diagnostics/run-20261104T145446.489461000Z-62620/scores.jsonl) · [Representative transcript](../results/diagnostics/run-20261104T145446.489461000Z-62620/transcripts/model-0-t08_no_valid_offer-trial-2.json)
- The Muse four-task diagnostic completed 12 trials with all three metrics passing. Its coverage differs from the Luna pilot. [Spend and verification log](STATUS.md)

- [Muse diagnostic scores](../results/diagnostics/run-20261108T072135.587310000Z-28902/scores.jsonl) · [Diagnostic review](MUSE_DIAGNOSTIC_V1.md)
