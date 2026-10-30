# micro1 Eval Hackathon: rules, deadlines, submission, rubric

Sources: the HackerRank test page text (Q1/Q2, as captured Oct 30, 2026) and the official Participant Guide PDF (see LINKS.md). If this file and the official pages disagree, **the official pages win**. Fix this file when that happens.

> The HackerRank test login/invite link embeds a password. **Never** copy it into this repo, transcripts, chats or the zip.

## Key dates
| Event | GMT | Istanbul (UTC+3) | PT |
|---|---|---|---|
| Registration closes | Oct 30, 2026 07:00 | Oct 30, 10:00 | Oct 30, 00:00 |
| Submissions open / test login window opens | Oct 30, 07:15 | Oct 30, 10:15 | Oct 30, 00:15 |
| **Submission deadline / login window closes** | **Nov 9, 05:15** | **Nov 9, 08:15** | **Nov 8, 21:15** |
| Winners announced | Nov 23, 2026 | Nov 23 | Nov 23 |

- The HackerRank test timer showed **9 days 14 hr** remaining when ozgur opened it (the test is configured for 14,280 min). Whichever ends first, the timer or the window, is the hard stop.
- **Internal target: final upload by Nov 8, 2026, 22:00 Istanbul (19:00 GMT)**, which leaves about 10 hours of buffer. Midterms run Nov 2–6, so nothing that needs ozgur at the keyboard is scheduled for those days.

## Rules (summary)
1. Solo participation, one submission per participant.
2. The work must be original. Disclose any prior work, and any existing benchmarks or datasets you build on.
3. AI coding tools are allowed, but you are responsible for quality and correctness.
4. Cheating, including fabricated results, means disqualification.
5. You must accept the micro1 Participant Agreement. **ozgur does this himself.** No agent touches it. ✅ *Accepted by ozgur (confirmed Oct 30, 2026).*
6. The zip must not contain videos or large datasets, confidential or private data, third-party material without rights, or keys, passwords or credentials.

## What to submit
- **Q1, file upload:** one `.zip`, **≤ 50 MB**, `README.md` at the root, not password-protected, one file only.
- **Q2, video link:** ≤ 5 min. Hosted on YouTube (public or unlisted), Google Drive, Loom, Vimeo or similar. *Our choice: unlisted YouTube, recorded as the last step.* It must open without a login or access request, and it must play with audio. Put the same link in the README. Don't upload the video file itself.
- **Short description:** a few sentences, both in the field and in the README.

### Required README sections
Short description · Video walkthrough link · Domain and expertise · What it tests and why it matters · Setup and run instructions (keys via env vars, approximate runtime and cost) · Models tested and results (exact IDs/versions, run date, settings such as temperature and runs per task, scores) · Failure analysis · Prior work and sources.
The guide also recommends: a task overview, grading validation, provenance and licences, transcripts with at least one failure per task, the main failure mode / hot take, and how AI tools were used.

### Required in the zip
- Tasks: inputs plus expected outputs or grading criteria, covering edge cases and judgment calls.
- Scoring logic, with an explanation of how scores are calculated. If an LLM judge is used, name it, include its prompt and show a reliability check. *mandate-bench uses no LLM judge.*
- Results: raw model outputs and scores from our own run.
- A requirements file *or equivalent*, and `.env.example` (names only). For this Rust repo the equivalent is `Cargo.toml` + the committed `Cargo.lock`, with the toolchain pinned in `rust-toolchain.toml`.

### Video structure (from the guide)
About 1 min on the problem and why it matters → about 1.5 min on the tasks and grading → about 2 min on results and transcripts → about 0.5 min on the takeaway.

## Rubric (100 points)
| Criterion | Pts | What strong work looks like | Self-check | Our plan |
|---|---|---|---|---|
| Real-world relevance | 15 | A limitation that matters in practice in a field you know; README says who relies on it and what failure costs | Would a practitioner care? | Delegated purchasing under a mandate (AP2 Intent/Cart Mandates; ACP/UCP). Cost of failure: overspend, non-returnable item, missed deadline, manipulated purchase |
| Task design | 20 | Realistic, clear, unambiguous; defensible answer + reference; everything graded is stated in the task | Would two experts independently agree? | Mandate states the total formula, tie-break and escalation rule; one unique correct end state per task |
| Grader reliability | 20 | No false passes or fails; accepts references, rejects deliberately wrong answers; LLM judge (if any) checked | Tested on right **and** wrong answers? | Code grader (Rust, `cargo test`); oracle = 1.0; mutant suite (no-op, cheapest-list, buy-all, wrong total, wrong label) all fail; tolerance boundary tests |
| Evidence of model limitations | 15 | Frontier models, several trials per task, clear spread, transcripts back each failure explanation | Can I show *why* they fail? | 5–7 models (OpenAI, DeepSeek, open-weight via NVIDIA NIM) × 5 trials; pass rate + pass^k; per-task failure labels with transcript links |
| Reproducibility | 15 | A judge reruns from a clean env via the README and reaches the main results; commands, deps, versions, data, runtime, cost | Could someone reproduce without asking me? | One-command oracle run (`cargo run --release -- oracle`), pinned toolchain + `Cargo.lock`, `run_meta.json`, clean-room rerun Nov 8 |
| Human expertise and review | 10 | Author's own expertise; every task, answer and rule checked; no unreviewed AI filler; README states AI use and review | Did I check every task myself? | ozgur signs off each task in STATUS.md; AI_USE.md |
| Insight / hot take | 5 | Turns a failure mode into a clear lesson and what would close the gap | What did I learn that I didn't expect? | Pending results (hypothesis: helpfulness bias → buys instead of asking; seller text treated as user authority) |

## Pre-submission checklist (guide's 14 items + HackerRank confirmations)
- [ ] Reference solutions pass the grader.
- [ ] Deliberately wrong answers fail the grader.
- [ ] Everything the grader checks is stated in the task.
- [ ] At least one frontier model was tested with multiple trials per task.
- [ ] Failing transcripts were read, not just counted.
- [ ] README covers reproducibility: versions, cost, runtime.
- [ ] Zip < 50 MB, no videos.
- [ ] No keys or private data anywhere in the zip.
- [ ] Prior work disclosed.
- [ ] Video opens without requesting access and plays with audio.
- [ ] Short description filled in (field and README).
- [ ] Transcripts included for every model.
- [ ] Self-scored against the rubric above.
- [ ] Every task, answer and rule reviewed personally by ozgur.
- [ ] README includes the video link and short description, and both HackerRank fields are filled.
- [ ] The eval runs from scratch using only the README (clean-room rerun done).
- [ ] Results, raw outputs and model versions are included.
- [ ] The zip opens, and is not password-protected.
- [x] Participant Agreement accepted (ozgur, personally; confirmed Oct 30, 2026).

## Note: "update" vs. "one submission"
The Q1 deadline text says *"You can update your submission until then. Only your latest complete submission is evaluated."* The rules say *"Solo participation only, with one submission per participant. Only your latest upload counts."* In a HackerRank test, finishing or submitting the test can also end the attempt, and it is not clear whether you can reopen it to replace the file.
**Plan: make one final upload, only when everything is ready** (zip, video link and description together), and treat it as final. Don't upload a placeholder expecting to replace it. If something is unclear, ozgur can email **hackathon@micro1.ai** or ask on the Discord linked from the event page.

## Contact
- Email: **hackathon@micro1.ai**
- Discord: linked from the event page (https://www.hackerrank.com/event/micro1-eval-hackathon)
