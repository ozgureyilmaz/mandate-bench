# M4 worker review — Nov 4, 2026

Three native workers were requested as `gpt-6-luna` at `xhigh` for read-only grading, task-difficulty and release-readiness reviews. They made no file changes, evaluation API requests or external writes. The primary consolidated findings and recorded the owner's new USD 1 Luna-only ceiling in the docs. This is AI review, not owner sign-off or adoption of the attached M4 brief.

## Verified findings

1. **Keep the original pilot result intact.** Strict v1 is 9/12. The separate 12/12 outcome audit is retrospective, proposed-policy evidence, not new model performance. Future outcome/strict/format metrics need exact definitions and a dated freeze before new evaluation. See [PILOT_REVIEW.md](PILOT_REVIEW.md).
2. **Separate tool errors from final format.** The runner forces C1 false on an invalid tool call; the audit currently uses C1 for its schema count. That does not affect this pilot, whose episodes all completed, but a future standalone final-format rate must be computed independently of tool-call errors. Evidence: `runner.rs` around the episode-outcome override and `audit.rs` around `strict_schema_passes`.
3. **The USD 1 ceiling is not implemented as a runtime guard.** Existing cost accounting happens after requests return. Output/turn/rate/concurrency controls are useful limits, but they do not independently guarantee a cumulative dollar ceiling. Resolve a verified budget stop/reservation/reconciliation plan before another paid batch; unknown usage must not be assumed zero.
4. **The t01 v2 candidate has correct arithmetic but is not a complete adopted task.** A is the unique eligible minimum at USD 144.00, ahead of E at 144.03, B at 145.80 and F at 147.00; C/D/G/H fail their declared return/delivery/expiry conditions. It needs all remaining validity/returns/shipping/Q&A fields populated, a versioned reference and the offer cap raised from five to at least eight. Its 11-call reference path fits the current tool budget. See [HARDENING_PROPOSAL.md](HARDENING_PROPOSAL.md).
5. **Several pasted suggestions require contract changes.** There is no `shipping` seller topic; use supported `fees` or explicitly version the topic contract. `get_offer` exposes every fee today, so a genuinely seller-only fee needs a generic visibility mechanism that preserves the one shared total function. Delivery skips weekends and has no holiday calendar. Current quantity semantics are minimum coverage, so an exact-unit prohibition must be explicitly represented/stated rather than silently graded. Conflicting title/spec compatibility requires an unambiguous source-priority rule.
6. **Packaging is a structural gate, not proof of final scientific readiness.** It can accept full selected coverage made entirely of API errors; it does not re-grade transcripts/recompute report metrics itself. Before final packaging, require usable graded evidence per selected task, run offline grading/report validation, inspect the actual archive, and perform human video/review checks. Existing preflight still correctly blocks the current unfinished README/video/sign-offs/final-run state.

Only t00/t01/t04/t08 have live pilot evidence. The other five tasks have reference/oracle evidence only. Do not label all nine tasks proven too easy, promise a 50–80% future pass rate, or claim that weaker-model comparison exists under the present Luna-only restriction.

## Recommended next bounded slice — not executed

1. Define and freeze prospective outcome pass, strict pass and independent format compliance; keep old v1 scores and clearly disclose the pilot-motivated change. Include v2's explanation-metadata and no-order currency rules explicitly.
2. Add/verify the USD 1 cumulative budget boundary, including pending/unknown request costs and provider-error stop behavior, before paid use.
3. Fully specify one t01 v2 candidate and its oracle/mutants, with the minimum necessary schema-cap change; preserve t00 and all v1 evidence. Obtain owner task/grader review.
4. Run a separately versioned three-trial Luna diagnostic only after scope/policy/budget checks. Evaluate meaningful errors and ambiguity, not a desired failure percentage. Keep all errors and both metrics.

The attachment's USD 35/multi-model/merge/publication directions remain inactive pending an explicit work order. No PR, merge, push, tag, metric adoption, scored task change or new model trial was performed by this review.
