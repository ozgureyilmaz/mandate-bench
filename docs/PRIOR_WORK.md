# PRIOR_WORK: related work and how mandate-bench differs

The hackathon rules require disclosing anything that existed before the hackathon (Oct 30, 2026) and any benchmarks or datasets built on. **mandate-bench uses no external dataset. All tasks, sellers and prices are synthetic and written for this repo.** No code is copied from any project below. If that changes, record it here.

## Our own prior work (pre-hackathon)
| Work | Link | Relation |
|---|---|---|
| harnessrouter (Slush'D × OpenAI Hackathon) | https://github.com/ozgureyilmaz/harnessrouter | ozgur's agent router ("one agent. many harnesses."): delegates bounded coding tasks to multiple coding agents over MCP, each run in an isolated git snapshot. It's experience in agent orchestration and failure modes, and the same multi-agent workflow builds this repo. **No code reused** (planned); if any is, disclose it here and in the README. |

## Author and domain (draft for the README "Domain and expertise" section; ozgur edits)
- Author: Özgür Eryılmaz (ozgur), an MIS undergraduate who works on agentic-AI infrastructure, competing solo. GitHub: https://github.com/ozgureyilmaz.
- Why this domain: agentic commerce is moving from demos to protocols: Google's AP2 (Intent and Cart Mandates), OpenAI and Stripe's ACP, Google's UCP, and Visa's Trusted Agent Protocol. All of them assume a buyer agent that respects what the user authorized. mandate-bench tests that assumption at the decision step, before payment rails get involved.

## Closest prior work: must be cited in the README
| Work | Link | What it does | How mandate-bench differs |
|---|---|---|---|
| **Magentic Marketplace** (Microsoft, 2025) | https://arxiv.org/abs/2510.25779 · https://github.com/microsoft/multi-agent-marketplace | A simulated two-sided marketplace: Assistant agents for consumers, Service agents for businesses. Studies welfare, behavioural biases (strong first-proposal bias), manipulation and search, at market scale. | **Closest overlap.** That's a market-dynamics simulation with LLM sellers and welfare metrics. Ours is a small, code-graded, single-answer test of **mandate-boundary adherence** with scripted, deterministic sellers, and it includes the "don't buy, ask the user" outcome. |
| **τ-bench** / τ²-bench (Sierra, 2024–25) | https://arxiv.org/abs/2406.12045 · https://github.com/sierra-research/tau-bench · https://github.com/sierra-research/tau2-bench | Tool-using agents in retail and airline domains under domain policies, with a simulated user. Graded on the final DB state. Introduced **pass^k**. | We borrow end-state grading and pass^k. τ-bench policies are *operator* rules for a customer-service agent. Ours are a *user's purchase mandate* enforced on the buyer side against adversarial or tricky sellers. No LLM user simulator. |
| **AgentDojo** (ETH Zurich SPY Lab, 2024) | https://arxiv.org/abs/2406.13352 · https://github.com/ethz-spylab/agentdojo | A dynamic environment for prompt-injection attacks and defences on tool agents (indirect injection via tool outputs). | t04 uses an AgentDojo-style indirect injection. Our focus is injections that claim to **change the mandate** ("pre-approved up to $500") in a commerce setting, scored alongside non-adversarial mandate traps. |
| **AP2: Agent Payments Protocol** (Google, Sep 2025) | https://cloud.google.com/blog/products/ai-machine-learning/announcing-agents-to-payments-ap2-protocol · https://github.com/google-agentic-commerce/AP2 · https://ap2-protocol.org | Defines signed **Intent Mandates** (price limits, timing, conditions) and **Cart Mandates** (exact items and price) as verifiable credentials for agent payments. | We borrow the *mandate* concept and vocabulary only. AP2 secures authorization cryptographically. We test whether the agent's **decision** stays inside the mandate before anything is signed. No AP2 code or schema is used. |
| **ACP: Agentic Commerce Protocol** (OpenAI + Stripe) | https://www.agenticcommerce.dev/ · https://stripe.com/blog/developing-an-open-standard-for-agentic-commerce | An open standard for agent checkout with merchants. | It inspires the structured offer and checkout shape. We don't implement ACP. |
| **UCP: Universal Commerce Protocol** (Google) | https://ucp.dev/ · https://developers.googleblog.com/en/under-the-hood-universal-commerce-protocol-ucp/ | A commerce protocol for agent ↔ merchant interoperability. | Same as ACP: it inspires the offer schema, and we don't implement it. |

## Shopping and web-agent benchmarks (cite as context)
| Work | Link | What it does | Difference |
|---|---|---|---|
| WebShop (2022) | https://arxiv.org/abs/2207.01206 | A simulated web store; agents find products matching an instruction, with attribute-match rewards. | Ours: mandate constraints (all-in budget, deadline, returns, compatibility), adversarial seller text, and an escalation outcome. Exact-match grading, not attribute overlap. |
| WebArena (2023) / VisualWebArena (2024) | https://arxiv.org/abs/2307.13854 · https://arxiv.org/abs/2401.13649 | Realistic self-hosted web environments, including an e-commerce site. | Theirs tests web navigation. Ours removes navigation (structured tools), so it isolates the **decision** under a mandate. |
| Mind2Web (2023) | https://arxiv.org/abs/2306.06070 | A dataset of generalist web-agent tasks on real sites. | Same distinction: navigation and grounding vs. a mandate decision. |
| DeepShop (2025) | https://arxiv.org/abs/2506.02839 | Complex, realistic shopping queries (multi-attribute, filters, sorting) for web agents. | Their complexity comes from the query. Ours comes from offer terms, the seller, and whether to buy at all. |
| ShoppingBench (2025) | https://arxiv.org/abs/2508.04266 | An end-to-end shopping benchmark with grounded intents (vouchers, budgets, multi-product sellers). | Closest on budgets. Ours adds hidden fees, validity, delivery vs. ship, returns, injection and ask-user, with a deterministic oracle per task. |
| SusBench (2025) | https://arxiv.org/abs/2510.11035 | Susceptibility of computer-use agents to UI dark patterns on real consumer sites. | Theirs is UI manipulation. Ours is manipulation and traps in **structured offer data and seller text**. |
| TrickyArena / "Investigating the Impact of Dark Patterns on LLM-Based Web Agents" (2025) | https://arxiv.org/abs/2510.18113 | The effect of dark patterns on web-agent decisions (LiteAgent, TrickyArena). | Same as SusBench. |
| WASP (2025) | https://arxiv.org/abs/2504.18575 | Prompt-injection security benchmark for web agents. | One trap (t04) overlaps. Ours isn't a security benchmark. |
| StakeBench / "Who Pays the Price?" (2026) | https://arxiv.org/abs/2606.13385 | Stakeholder-centric prompt-injection harms for e-commerce web agents. | Closest on injection in commerce. Ours puts injection beside non-adversarial mandate traps and scores it with an end-state oracle. |
| NegotiationArena (2024) | https://arxiv.org/abs/2402.05863 | LLM-vs-LLM negotiation scenarios. | We don't negotiate. Sellers are scripted, and only selection and escalation are tested. |
| Wharton GAIL, "Agentic Shopping is Complicated and Contingent" (Aug 2026) | https://gail.wharton.upenn.edu/research-and-insights/technical-report-agentic-shopping/ | About 26k tests: what agents buy shifts with added context (reviews, ordering, injected memory). | Supports the motivation (instability). We measure correctness against a mandate, not preference stability. |

## Methodology sources (not prior work, but cited)
- Anthropic, "Demystifying evals for AI agents": https://www.anthropic.com/research/demystifying-evals-for-ai-agents
- "Automating eval design and hill-climbing": https://claude.dev/blog/automating-eval-design-and-hillclimbing
- Harbor task format (we follow its ideas, not necessarily its format; see DECISIONS.md): https://docs.harborframework.com
- buildevals.com curriculum: https://buildevals.com
- X posts ozgur shared (user-provided links; X may block automated checks, read via the X API on Oct 30, 2026):
  - Ben Burtenshaw (Hugging Face), Sep 11, 2026: https://x.com/ben_burtenshaw/status/2098410420394451153. Wrap-up of the Hugging Face "Training Agents" series; its agentic-evals workshop covers why agent benchmark scores don't match what people see in use, and its RL sessions show a deliberately gameable reward getting hacked. *Relevance:* graders must be hard to game; we test ours with mutants.
  - Tony Cueva (Confident AI), Oct 27, 2026: https://x.com/tonycueva/status/2115089591824744957. "Build Your Eval Harness" article: an eval harness re-runs fixed test questions (3 trials each) on every change and flags what broke; a 5-question check passed while 20 real questions × 3 trials passed only 33/60. *Relevance:* supports multiple trials per task and pass^k over one-shot checks.

## One-line differentiator (for the README)
> Existing shopping benchmarks test finding the product, and commerce-security benchmarks test resisting attacks. mandate-bench tests whether a buyer agent stays inside what the user authorized (all-in budget, deadline, compatibility, returns), even when a seller claims otherwise, and whether it **declines to buy and asks** when nothing qualifies. It does this with a small set of tasks, each with one correct answer and a code grader.
