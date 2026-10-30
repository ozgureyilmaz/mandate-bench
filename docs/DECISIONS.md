# DECISIONS

Append-only. Format: date (Istanbul) · decision · reasons · alternatives considered · who.

## 2026-10-30 · Chose idea 5a over 1 and 5b · ozgur
**Decision:** build 5a. A buyer agent chooses among structured offers from scripted seller agents and must stay inside the user's mandate (budget all-in, deadline, compatibility, returns). The correct outcome is either the exact offer, quantity and total, or no purchase plus `ask_user`.

**Context, stated honestly:** the pre-decision comparison (`idea_1_vs_5.md`, Grok Bot research, Oct 30) actually *recommended* 5b + H1 (human-style sites vs. an agent-ready structured channel). It rated 5a lowest on estimated rubric score (63–82, vs. 70–89 for idea 1 and 76–93 for 5b) because of ceiling risk and the overlap with Magentic Marketplace. ozgur chose 5a anyway, after checking that it fits the hackathon rules.

**ozgur's reason, in his own words (translated from Turkish):** "I work a lot in this space; I'm connecting the dots to build something bigger, and this is one of those dots."

**Reasons for 5a (from the same comparison):**
- **Lowest effort and cost**, the best fit for 9 days that include midterms (Nov 2–6). Low-to-medium effort, about 1 day for the env + sellers plus 1–2 h per task, and an estimated $15–40.
- **Grader reliability is very high.** Everything is graded by code: exact match on offer and quantity, total ±0.01, a decision label and a schema check. There's no LLM judge to validate.
- **Reproducibility is high.** No Docker, deterministic scripted sellers (the comparison assumed Python; the harness is now Rust, see below).
- **The domain is current.** AP2 mandates, ACP and UCP all assume the agent respects the user's authorization.

**Why not idea 1 (Harness Bug-Fix Bench, Harbor/Docker):** effort and cost were the highest ($50–150), reproducibility was only medium (Docker/Harbor), and the plan carried the most risk alongside midterms. Its novelty was also limited (AgentIssue-Bench exists).

**Why not 5b (agent shopping on human-style sites):** it means building realistic multi-page static stores (search, product pages, fine print, cart), which is more effort and cost ($20–80) than structured offers.

**Known risks and mitigations:**
- Ceiling (structured JSON may be easy): the Oct 31 pilot gate. At ≥85% on trap trials → harden (PLAN §9).
- Novelty vs. Magentic Marketplace: disclose it prominently, and differentiate on mandate adherence, ask-user, and the small code-graded design.
- Realism: offer fields modelled on ACP/UCP-style structured data.
- Fallback, if hardening fails: the structured offers could later become "channel B" of 5b/H1. Not planned.

## 2026-10-30 · Repo name `mandate-bench` · ozgur
**Reasons:** the name says what's measured. "Mandate" is the AP2 term for what the user authorized (price limits, timing, conditions), and the bench is whether the agent stays inside it. It's short and searchable.

## 2026-10-30 · Proposed (pending ozgur's confirmation)
- Scripted, rule-based sellers (no LLM sellers), for determinism and reproducibility.
- No LLM judge. Everything is graded by code.
- `ask_user` ends the episode. Calling it on a BUY task fails the task (`over_escalation`).
- Every `place_order` call is final and graded, even if the env rejects it. At most 1 order.
- Own harness, not the Harbor format. It's simpler, needs no Docker, and has a quicker clean-room rerun. *Language and providers decided below.*
- ~~Currency TRY, delivery to Istanbul~~ → superseded: USD/global (below). A frozen `now` per task stays.

## 2026-10-30 · Model providers: OpenAI + DeepSeek + NVIDIA NIM; no OpenRouter · ozgur
**Decision:** use OpenAI (paid; ozgur's account has tight API rate limits), DeepSeek (paid API) and NVIDIA NIM free trial endpoints for open-weight models. No OpenRouter. (ozgur, paraphrased: OpenAI plus DeepSeek APIs or NVIDIA's free APIs; OpenAI is paid and rate-limited.)
**How:** one OpenAI-compatible client in Rust with a per-provider `base_url`, a per-provider `api` (OpenAI GPT-6 models need the Responses API for tool calls; DeepSeek and NVIDIA use Chat Completions), plus a concurrency cap, an RPM bucket, and backoff on 429/5xx that honours `Retry-After`. Details are in AGENTS.md.
**Visibility:** ozgur asked that provider and rate-limit details be visible only to agents, in the docs ("bunlar sadece agentlara docs tarafında görünür olsun"). So they live in AGENTS.md and PLAN.md §8. README says only which models were tested and how to supply keys. `.env.example` has variable names only.
**Model list:** proposed in PLAN.md §8 (gpt-6.1-sol, gpt-6-luna, deepseek-v4-pro, deepseek-flash, and 3 NVIDIA-hosted open-weight models; gpt-6-astra optional). *Pending ozgur's confirmation.* NVIDIA tool-call support is unverified until the smoke test.
**Alternatives:** OpenRouter as one gateway (dropped by ozgur); direct Anthropic/Google keys (not available).

## 2026-10-30 · USD and global tasks · ozgur
**Decision:** all prices in USD, sellers and addresses fictional and not tied to a country, and the delivery-time trap phrased against the US. ozgur: "global ve usd". **Reason:** the judges are international, and USD totals are easy to check by hand. Task numbers recomputed in PLAN.md §5. **Alternative:** TRY with Istanbul delivery (the earlier draft).

## 2026-10-30 · Harness in Rust · ozgur
**Decision:** Rust (a Cargo workspace; toolchain pinned in `rust-toolchain.toml`; `Cargo.lock` committed). Commands: `cargo run --release -- …`, `cargo test --workspace`. Crates: reqwest, tokio, serde, serde_json, clap, dotenvy, plus rust_decimal for exact money. Tasks stay JSON. `requirements.txt` is removed; the hackathon's "requirements file or equivalent" is met by `Cargo.toml` + `Cargo.lock`.
**Reasons:** ozgur prefers Rust "if it causes no problems". For this project it doesn't: it's an HTTP + JSON + exact-arithmetic harness with no ML libraries; AI coding assistants write the code; one static binary makes the clean-room rerun simple; and `rust_decimal` avoids float errors in the ±0.01 total check.
**Costs, accepted:** a first build takes a few minutes; judges need rustup (one command in the README).
**Alternatives:** Python (the earlier draft; the most common for evals); TypeScript (no real advantage here).

## 2026-10-30 · Video: unlisted YouTube, recorded last · ozgur
**Decision:** host the ≤ 5 min video on YouTube as **unlisted** and record it as the **last** step, after packaging, so it shows the final results. ozgur: "yt ama bu en son iş". **Alternative:** Loom.

## 2026-10-30 · Public repo, strict `.gitignore` · ozgur
**Decision:** the repo is public (github.com/ozgureyilmaz/mandate-bench), so pushing means publishing. `.gitignore` is strict: `.env*` except `.env.example`, keys/certs/secrets, `target/`, `runs/` (scratch), logs, archives and videos, other-language junk, OS/editor files. `results/` and `transcripts/` stay committed because the submission needs them. A secret scan of both the tree and the git history runs before every push (AGENTS.md). ozgur (typos fixed): "herhangi bir açık istemiyorum ve gereksiz paylaşım".

## 2026-10-30 · M1 offline boundary and concrete data contract · implementation agent
**Decision:** implement only the Cargo skeleton, t00/t01, scripted environment, grader and offline oracle in this session. `mb-agent` drives offline replays; `mb-llm` remains an explicit placeholder. Provider transports, configs, key loading and paid/smoke/model runs are deferred to M2. `oracle` writes nothing by default; optional scratch artifacts explicitly identify their source as offline reference replay. No model-result claims are made.
**Reasons:** this is the kickoff's M1 scope and enables grader validation before API spending. Adding nonfunctional provider commands or making model calls now would exceed that scope.
**Data contract:** tasks contain `task_id`, `task_version`, `mandate`, `sellers` and `offers`; references contain `expected`, `rationale`, `distractor_labels` and `oracle_calls`. Five strict, self-contained Draft 2020-12 schemas validate these records without remote references. The hidden Reference type and loader live in `mb-grader`, outside the environment/episode driver. Future optional fields remain nullable/omittable as specified.
**Alternatives:** implementing the full provider loop immediately; remote schema references; allowing unknown record fields. All deferred or rejected to keep the offline contract explicit and reproducible. Owner hand-review and publication remain separate.

## 2026-10-30 · Hidden-reference isolation wording clarified · ozgur, implemented by the implementation agent
**Decision:** test that hidden reference files/fields, full expected explanations, rationales, labels and oracle-call canaries do not enter initial prompts or tool responses. Legitimate task facts such as offer IDs and prices remain visible. ozgur explicitly confirmed this interpretation in the implementation chat.
**Reason:** a literal prohibition on every expected value would forbid displaying the winning offer ID or its price, making a solvable task impossible. Isolation is enforced by crate/API boundaries and tests; `get_offer` also strips the scripted `seller_qa` map, which is accessible only through `ask_seller`.
**Alternative:** substring bans on every reference scalar; rejected because they confuse hidden answers with shared task facts.

## 2026-10-30 · Money, time and terminal-call edge cases · implementation agent
**Decision:** preserve money as exact JSON numbers using rust_decimal with both `serde-float` and `serde-arbitrary-precision`; this combination serializes through an exact `serde_json::Number`, without an f64 conversion. A regression test checks this representation. Round half-up once after the shared fee/FX calculation. Numeric JSON totals retain sub-cent precision for the inclusive USD 0.01 tolerance; formatted USD/en-US strings are normalized before final schema validation, while ambiguous strings, duplicate keys and reserved serializer objects are rejected.
**Time/quantity:** `valid_until` is inclusive. Calendar shipping days precede Mon–Fri transit days, retaining the frozen offset/time and assuming no holidays. Use the minimum pack count covering the requested quantity. These conventions are stated in the buyer prompt.
**Ledger:** every order attempt consumes authority, including rejected attempts. `ask_user` must succeed to count as escalation; later tool calls are recorded, blocked and fail the protocol. The twentieth tool call marks the episode exhausted and fails even if a caller supplies a final-looking payload afterward. The environment checks order existence, argument validity and expiry; the hidden-reference grader checks the authorized outcome without exposing the correct charge to the buyer.
**Alternatives:** float money, implicit current time, forgiving rejected exploratory orders or treating any `ask_user` attempt as success; rejected because they introduce rounding errors or false passes.

## 2026-10-30 · Neutral repository wording and feature branch · ozgur
**Decision:** publish M1 from `feat/m1-offline-harness` with neutral implementation-role wording in repository docs and pull-request metadata. The AI-assisted implementation and review workflow remains disclosed in AI_USE.md. Existing published history is retained; unpublished M1 work is consolidated into one reviewable commit.
**Reason:** ozgur requested an unbranded branch and repository presentation before opening the M1 pull request. This changes names and attribution wording, not evaluation behavior or task data.
