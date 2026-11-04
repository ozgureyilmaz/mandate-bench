# STATUS

Last updated: **Nov 4, 2026 (Istanbul)**, by the implementation agent.

## Current position
- Core software is implemented: Rust workspace, exact totals/frozen clocks, scripted environment, buyer loop, provider client, grader, oracle, smoke/run/report, proposed-policy offline audit and gated package utility.
- All **9 task/reference pairs** are built; all 9 oracles score **1.0**. Current checks pass **90 Rust tests**, formatting, denied-warning Clippy and the locked release build. Packaging shell fixtures pass too.
- Live Luna smoke and a complete control buyer episode passed. A fresh 4-task × 3-trial pilot completed with **zero API errors**: **9/12 strict passes**, with all 3 failures caused by missing final currency on correct zero-order ASK_USER answers.
- The original strict grader/scores/prompt/task inputs remain unchanged. Proposed outcome policy v2 audits the same frozen trials offline: **12/12 outcome passes, 9/12 strict**. It is a review comparison, not extra model trials or an adopted replacement headline. See [PILOT_REVIEW.md](PILOT_REVIEW.md).
- Native workers were requested as Luna/xhigh. Owner-approved Nov 4 work order: [M1 PR #1](https://github.com/ozgureyilmaz/mandate-bench/pull/1) is merged into main at `3982bc4`; existing M2/M3/M4 commits and diagnostic data are being published on `feat/m4-hardening`. This PR remains separate from main. Nothing has been uploaded to the hackathon.

## Budget and preserved evidence
- Current approved live-evaluation spending ceiling: **USD 1 total, `gpt-6-luna` only** (owner update Nov 4). This supersedes older USD 5/25–35 allowances. The earlier dashboard balance of USD 4.97 is historical account information, not permission to spend that amount.
- The existing runner records estimated cost but does not enforce a cumulative dollar ceiling. Budget enforcement/bounded-call reconciliation must be resolved before another paid batch; the current worker reviews make no evaluation API calls.
- Live phases: smoke USD 0.0000237 estimate; initial 12-error pilot full cost unknown (known successful-request subtotal USD 0.00133736); slower control USD 0.00044962; completed fresh pilot USD 0.00490785. Known successful-request token-price estimates total about **USD 0.00672**; the original errors did not report usage/full billing.
- Raw outputs, full logs, original score rows, metadata and derived diagnostic reports/audit are byte-preserved and committed in `results/diagnostics/`; original ignored `runs/` copies remain intact. No final benchmark run has been selected/promoted directly under `results/<run_id>`.
- The first pilot's 12 HTTP 429 errors are not removed or counted as model failures. The slower control and serial pilot completed at 6 RPM. The earlier 429's precise rate/quota/spend cause remains unknown because its machine code/type was not retained; future errors now retain safe code/type fields.
- Active trials are bounded by provider concurrency. A provider API error halts new dispatch, preserves completed/in-flight records and marks unstarted coverage incomplete. No failed trial is silently rerun.

## Main review checkpoint
The pilot found no substantive mandate breach in its four tested tasks. The three strict failures are format-only. The strict trap gate is 6/9 and does not trigger its ≥85% threshold, but the proposed outcome audit is a ceiling warning. Recommendation: outcome headline with strict format diagnostic; freeze and disclose the final policy before final evaluation. [HARDENING_PROPOSAL.md](HARDENING_PROPOSAL.md) is an unadopted candidate, not a change to scored inputs. The other five tasks still lack live pilot evidence.

Three requested Luna/xhigh workers completed read-only grading, task and readiness reviews. Findings/recommended next slice are in [M4_WORKER_REVIEW.md](M4_WORKER_REVIEW.md). Primary changes in this review are documentation only: record the USD 1 cap and review findings. No new model call, metric adoption, task edit, public action or human sign-off occurred.

## Missing for submission
See [SUBMISSION_STATUS.md](SUBMISSION_STATUS.md).

1. Owner hand-solves/reviews all tasks, references and grading rules; finalize the intended metric and task difficulty. Human sign-off remains blank below.
2. Freeze reviewed task/prompt/grader versions, then collect reviewed five-trial evidence for all nine tasks. The planned frontier comparison needs an owner model/budget decision beyond the current Luna-only restriction.
3. Select final results, generate final tables, confirm failure labels/quotes and write an evidence-supported takeaway. Stable README description/domain/prior-work/AI-use sections are drafted; final findings, costs/runtime/settings and video remain pending.
4. Reproduce the final version from only the submitted README. Build/inspect the final ZIP (<50 MB, README at root, no keys/build cache/video) after tree/history scans and rubric review.
5. Owner records the final ≤5-minute video, uploads it unlisted, verifies public playback/audio and performs the HackerRank upload with description. Internal target Nov 8, 22:00 Istanbul; hard deadline **Nov 9, 08:15 Istanbul / 05:15 GMT**.

## Packaging readiness
`scripts/package.sh --check` is read-only and correctly refuses the current project: unfinished README/video, nine unchecked owner sign-offs and no qualifying committed nine-task/five-trial final run. Default invocation creates no ZIP. Git/rg/jq are maintainer-only prerequisites, not benchmark runtime dependencies. Human video playback/audio and final authorization cannot be verified by the helper.

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
- All task data is synthetic. Adding t02–t08 preserved t00/t01 and the fixed prompt. Later difficulty/metric changes must be versioned and explained; original data is not rewritten to improve scores.
- The earlier M1 source passed a fresh local clone check. The final-version clean-room rerun remains pending.
- `.env` stays ignored and owner-only; no credentials are printed/committed. Blank optional URL overrides retain the configured defaults.
- Participant Agreement was accepted personally by ozgur (confirmed Oct 30). Optional profile URLs are not submission blockers and should be omitted until confirmed.

## Active owner-approved Nov 4 slice
- Step 1 complete: M1 PR #1 merged, all prior local commits published on [PR #2](https://github.com/ozgureyilmaz/mandate-bench/pull/2), branch `feat/m4-hardening`. Secret/tree/history and neutral-wording scans passed before publication.
- Step 2 complete: prospective policy 2 (outcome, strict outcome+raw format, independent raw format) frozen in DECISIONS.md before new provider requests. Original v1 9/12 and raw data remain unchanged; pilot motivation disclosed.
- Next: cumulative paid-budget enforcement, free-model tool smokes, up to four versioned fixture changes and owner SIGNOFF. No new paid run has been made in this slice.
