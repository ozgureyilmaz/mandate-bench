# LINKS

✅ = opened on Oct 30, 2026, and returned the expected page (HTTP 200 via curl, or the content was read). ⚠️ = not verified. ❓ = URL unknown. 👤 = user-provided by ozgur (X may block automated checks). Links are stored without tracking parameters.
Never add a URL nobody has opened. **The HackerRank test login/invite link is deliberately not listed (it embeds a password).**

## Hackathon
| ✓ | Link | Note |
|---|---|---|
| ✅ | https://www.hackerrank.com/event/micro1-eval-hackathon | Event page (Discord link lives here) |
| ✅ | https://hrcdn.net/s3_pub/istreet-assets/R_8vs8ejamSPapSbUxmcWQ/Eval_Hackathon_Participant_Guide.pdf | Official Participant Guide (PDF) |
| ✅ | https://www.micro1.ai/hackathon/agreement | Participant Agreement (accepted by ozgur himself, Oct 30) |
| — | hackathon@micro1.ai | Contact email |

## Eval methodology and tooling
| ✓ | Link | Note |
|---|---|---|
| ✅ | https://www.anthropic.com/research/demystifying-evals-for-ai-agents | Anthropic, "Demystifying evals for AI agents" (the /engineering/ URL redirects here) |
| ✅ | https://claude.dev/blog/automating-eval-design-and-hillclimbing | Automating eval design and hill-climbing (cited in the guide) |
| 👤✅ | https://buildevals.com | "Learn to Build Evals for AI Agents", a free curriculum (by Tamas Szuromi); also shared by ozgur |
| ✅ | https://buildevals.com/modules/1-fundamentals | Module 1: fundamentals |
| ✅ | https://docs.harborframework.com | Harbor docs |
| ✅ | https://docs.harborframework.com/tasks/configuration | Harbor task configuration (task.toml etc.) |
| ✅ | https://github.com/harbor-framework/harbor | Harbor repo (laude-institute/harbor redirects here) |
| ✅ | https://www.tbench.ai | Terminal-Bench |
| 👤✅ | https://x.com/ben_burtenshaw/status/2098410420394451153 | Ben Burtenshaw (Hugging Face), Sep 11, 2026: Training Agents series wrap-up; agent benchmark scores vs. real use, gameable rewards (read via X API) |
| 👤✅ | https://x.com/tonycueva/status/2115089591824744957 | Tony Cueva (Confident AI), Oct 27, 2026: "Build Your Eval Harness": re-run fixed cases ×3 on every change; 33/60 vs. a passing 5-question check (read via X API) |

## Model providers (agent-facing; see AGENTS.md)
| ✓ | Link | Note |
|---|---|---|
| ✅ | https://platform.openai.com/docs/pricing | OpenAI API pricing (gpt-6.1-sol, gpt-6-luna, gpt-6-astra) |
| ✅ | https://platform.openai.com/docs/models/gpt-6.1-sol | Model page: tool calling requires the Responses API; per-tier rate limits |
| ✅ | https://platform.openai.com/docs/models/gpt-6-luna | Model page: Chat Completions tool calls only with reasoning_effort none |
| ✅ | https://platform.openai.com/docs/guides/rate-limits | OpenAI rate-limits guide (HTTP 200; content not reviewed) |
| ✅ | https://api-docs.deepseek.com/quick_start/pricing | DeepSeek models and pricing (deepseek-flash, deepseek-v4-pro; peak/off-peak) |
| ✅ | https://api-docs.deepseek.com/quick_start/rate_limit | DeepSeek concurrency limits |
| ✅ | https://build.nvidia.com/llms.txt | NVIDIA NIM: base URL, auth, OpenAI compatibility |
| ✅ | https://integrate.api.nvidia.com/v1/models | NVIDIA NIM model list (public, no key needed) |
| ✅ | https://forums.developer.nvidia.com/t/nvidia-nim-faq/300317 | NIM FAQ: free trial endpoints, rate limits vary by model and load |
| ✅ | https://rustup.rs | Rust installer (README setup) |
| ✅ | https://static.rust-lang.org/dist/channel-rust-stable.toml | Rust stable 1.99.1 (Oct 13, 2026) for the toolchain pin |

## Public evals referenced by the guide
| ✓ | Link | Note |
|---|---|---|
| ✅ | https://arxiv.org/abs/2310.06770 | SWE-bench |
| ✅ | https://openai.com/index/introducing-swe-bench-verified/ | SWE-bench Verified |
| ✅ | https://arxiv.org/abs/2406.12045 | τ-bench paper |
| ✅ | https://github.com/sierra-research/tau-bench | τ-bench repo |
| ✅ | https://github.com/sierra-research/tau2-bench | τ²-bench repo |
| ✅ | https://openai.com/index/gdpval/ | GDPval |

## Related benchmarks and environments
| ✓ | Link | Note |
|---|---|---|
| ✅ | https://arxiv.org/abs/2510.25779 | Magentic Marketplace (closest prior work) |
| ✅ | https://github.com/microsoft/multi-agent-marketplace | Magentic Marketplace code |
| ✅ | https://arxiv.org/abs/2406.13352 | AgentDojo |
| ✅ | https://github.com/ethz-spylab/agentdojo | AgentDojo code |
| ✅ | https://arxiv.org/abs/2207.01206 | WebShop |
| ✅ | https://arxiv.org/abs/2307.13854 | WebArena |
| ✅ | https://arxiv.org/abs/2401.13649 | VisualWebArena |
| ✅ | https://arxiv.org/abs/2306.06070 | Mind2Web |
| ✅ | https://arxiv.org/abs/2506.02839 | DeepShop |
| ✅ | https://arxiv.org/abs/2508.04266 | ShoppingBench |
| ✅ | https://arxiv.org/abs/2510.11035 | SusBench |
| ✅ | https://arxiv.org/abs/2510.18113 | Dark patterns vs. LLM web agents (TrickyArena) |
| ✅ | https://arxiv.org/abs/2504.18575 | WASP |
| ✅ | https://arxiv.org/abs/2606.13385 | StakeBench, "Who Pays the Price?" |
| ✅ | https://arxiv.org/abs/2402.05863 | NegotiationArena |
| ✅ | https://gail.wharton.upenn.edu/research-and-insights/technical-report-agentic-shopping/ | Wharton GAIL, "Agentic Shopping is Complicated and Contingent" |

## Agentic-commerce protocols (domain context)
| ✓ | Link | Note |
|---|---|---|
| ✅ | https://cloud.google.com/blog/products/ai-machine-learning/announcing-agents-to-payments-ap2-protocol | AP2 announcement (Intent and Cart Mandates) |
| ✅ | https://github.com/google-agentic-commerce/AP2 | AP2 repo |
| ✅ | https://ap2-protocol.org | AP2 site |
| ✅ | https://www.agenticcommerce.dev/ | ACP site |
| ✅ | https://openai.com/index/buy-it-in-chatgpt/ | OpenAI: Instant Checkout + ACP (Sep 29, 2025) |
| ✅ | https://stripe.com/blog/developing-an-open-standard-for-agentic-commerce | Stripe on ACP |
| ✅ | https://ucp.dev/ | Universal Commerce Protocol |
| ✅ | https://developers.googleblog.com/en/under-the-hood-universal-commerce-protocol-ucp/ | Google: UCP deep dive |
| ✅ | https://corporate.visa.com/en/sites/visa-perspectives/newsroom/visa-unveils-trusted-agent-protocol-for-ai-commerce.html | Visa Trusted Agent Protocol |

## ozgur's projects and profiles
| ✓ | Link | Note |
|---|---|---|
| ✅ | https://github.com/ozgureyilmaz | GitHub profile |
| ✅ | https://github.com/ozgureyilmaz/harnessrouter | harnessrouter (prior work) |
| ✅ | https://ozgureyilmaz.github.io | Personal site (returns 200; content not reviewed) |
| ⚠️ | https://www.linkedin.com/in/ozgure | LinkedIn (from memory; LinkedIn blocks automated checks; ozgur to confirm) |
