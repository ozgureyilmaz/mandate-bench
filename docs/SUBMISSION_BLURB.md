# Submission description draft · Nov 8, 2026

Owner review is pending. The text below uses the archived Luna v1 pilot and the verified Muse tool smoke. Update the Muse sentence after the approved diagnostic.

## Form-field draft

mandate-bench tests whether a buyer agent stays within a delegated purchase mandate. Nine synthetic tasks cover all-in budgets, offer expiry, compatibility, pack quantities, delivery deadlines, return terms and seller instructions claiming extra authority. Scripted sellers and a deterministic Rust grader check the order ledger and final JSON, including when the agent should buy nothing and ask the user. A four-task Luna v1 pilot with three trials per task passed strict grading in 9 of 12 trials. Every failure followed a correct no-order escalation but omitted the required currency field. The observed limitation is machine-readable output reliability after a correct decision. Muse Spark's tool smoke passed. Its scored diagnostic awaits owner approval.

## Evidence for review

- The 12-trial Luna pilot used version-1 t00/t01/t04/t08 on Nov 4. Its three t08 failures had a successful ask_user and zero order attempts. [Scores](../results/diagnostics/run-20261104T145446.489461000Z-62620/scores.jsonl) · [Representative transcript](../results/diagnostics/run-20261104T145446.489461000Z-62620/transcripts/model-0-t08_no_valid_offer-trial-2.json)
- The Muse smoke tested a tool exchange. Scored task performance remains pending. [Spend and verification log](STATUS.md)
