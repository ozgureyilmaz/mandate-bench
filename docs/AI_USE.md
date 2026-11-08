# AI_USE: disclosure (template; ozgur finalizes before submission)

The hackathon allows AI tools, and the author stays responsible for correctness. The rubric (Human expertise and review, 10 points) asks the README to state how AI tools were used and how their output was reviewed. Copy the final version of this into the README's "AI use" section.

## Tools used
| Tool | Used for | Not used for |
|---|---|---|
| AI coding assistant | M1 workspace/env/grader/two fixtures; M2 provider/loop/CLI and tests; M3 remaining seven fixtures and all-task test coverage. M4 versioned hardening, policy 2 metrics, provider ledgers and the Muse integration. Native workers were requested as `gpt-6-luna` at `xhigh`, as authorized by ozgur; the primary integrated and reviewed their work. | No fabricated benchmark results, human sign-off or submission; the single live M2 smoke is recorded separately from benchmark performance |
| Cursor cloud agents | Planned for later bounded tasks; not used in M1 | No M1 implementation |
| Grok Bot | Research on related work and link verification; drafting docs; status tracking; run monitoring | Writing eval logic; choosing expected answers |
| Models under test | Luna produced archived diagnostic trials. Meta Model API / Muse Spark 1.1 is configured as a candidate under test, with a passing live tool round trip and a completed four-task, 12-trial diagnostic after the first HTTP 400 and replay fixes. | Grading (no LLM judge) |

The owner signed task, answer and grader review on Nov 8 in SIGNOFF.md and STATUS.md before the Muse batch. The disclosure checklist below remains for the owner to finalize.

## What the author did personally
- [ ] Chose the domain and the trap taxonomy (from TODO: own experience).
- [ ] Wrote or approved every task's scenario and numbers.
- [ ] Hand-computed every expected answer from `task.json` alone, independently of the oracle (STATUS.md sign-off table).
- [ ] Reviewed every grading rule and the mutant suite.
- [ ] Read the failing transcripts behind every claim in Failure analysis.
- [ ] Wrote the hot take.

## How AI output was reviewed
- Code: the primary implementation agent reviewed worker changes and runs formatting, Clippy, oracle/mutant/schema/isolation tests. The owner signed task/grader review. Final narrative review and merge approval remain owner actions.
- Docs: diagnostic counts and costs were checked against committed artifacts. Final narrative approval remains with the owner.
- Tasks: the owner completed the nine STATUS approvals and SIGNOFF checkboxes before dispatch.

## What AI did *not* do
- Fabricate or edit any result: all numbers come from files in `results/`.
- Generate task answers that weren't independently verified.
- Submit anything: no submission has been made by an agent; the final upload is reserved for ozgur.
