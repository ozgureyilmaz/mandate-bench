# AI_USE: disclosure (template; ozgur finalizes before submission)

The hackathon allows AI tools, and the author stays responsible for correctness. The rubric (Human expertise and review, 10 points) asks the README to state how AI tools were used and how their output was reviewed. Copy the final version of this into the README's "AI use" section.

## Tools used
| Tool | Used for | Not used for |
|---|---|---|
| Codex | Main implementation (Rust): env, tools, LLM client, agent loop, grader, tests | TODO |
| Cursor cloud agents | Bounded code tasks (individual task files, grader tests, fixes) | TODO |
| Grok Bot | Research on related work and link verification; drafting docs; status tracking; run monitoring | Writing eval logic; choosing expected answers |
| Models under test | Only as subjects of the eval | Grading (no LLM judge) |

## What the author did personally
- [ ] Chose the domain and the trap taxonomy (from TODO: own experience).
- [ ] Wrote or approved every task's scenario and numbers.
- [ ] Hand-computed every expected answer from `task.json` alone, independently of the oracle (STATUS.md sign-off table).
- [ ] Reviewed every grading rule and the mutant suite.
- [ ] Read the failing transcripts behind every claim in Failure analysis.
- [ ] Wrote the hot take.

## How AI output was reviewed
- Code: every merged change reviewed by ozgur; grader correctness enforced by the oracle/mutant tests (`cargo test --workspace`).
- Docs: TODO (for example, "every factual claim checked against LINKS.md sources; numbers only from `results/`").
- Tasks: TODO.

## What AI did *not* do
- Fabricate or edit any result: all numbers come from files in `results/`.
- Generate task answers that weren't independently verified.
- Submit anything: ozgur uploaded the submission personally.
