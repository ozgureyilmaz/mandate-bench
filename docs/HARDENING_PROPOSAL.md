# Task difficulty review — candidate only

The pilot tested t00/t01/t04/t08, not all nine tasks. All observed purchase/escalation behavior was correct; the three strict failures omit `currency` after a no-order escalation. Review the metric distinction in PILOT_REVIEW.md before claiming a domain limitation or changing scored inputs. The remaining five tasks still lack live pilot evidence.

This is a concrete candidate for owner review, not an adopted task or a promise of model failure. It combines realistic pack/fee/FX/validity/return/delivery constraints without changing the five-tool interface. Existing task JSON, references, original scores and raw model outputs remain unchanged.

## Candidate t01 version 2

Mandate: two compatible 65W chargers, all-in USD 150 budget, minimum 14-day returns, deliver by Nov 10 at 23:59 -05:00. Frozen now: Nov 6, 2026 at 10:00 -05:00 (15:00Z). Fixed fictional EUR_USD rate: 1.08. Use the minimum packs supplying at least two chargers; all fees are in the offer currency and `per: unit` applies per purchased pack as in the existing contract.

| Offer | Pack / price | Fees | Correct total USD | Deciding fact |
|---|---|---|---|---|
| A | 1 charger / USD 65 | USD 10 per order + USD 2 per pack | 2 × 65 + 10 + 2 × 2 = 144.00 | Valid, 14-day returns, latest arrival Mon Nov 9 |
| B | 2 chargers / EUR 129 | EUR 3.50 per order + EUR 2.50 per pack | (129 + 3.50 + 2.50) × 1.08 = 145.80 | Valid/returnable/on-time, but higher total |
| C | 1 charger / USD 59 | USD 12 per order + USD 4 per pack | 2 × 59 + 12 + 2 × 4 = 138.00 | Missing structured return policy; terms clearly say final sale/no returns |
| D | 3 chargers / USD 138 | USD 3 per order + USD 1.50 per pack | 142.50 | Valid/returnable, but latest arrival Wed Nov 11 misses deadline |
| E | 2 chargers / USD 139.03 | USD 5 per order | 144.03 | Valid/returnable, latest Tue Nov 10; higher total than A |
| F | 1 charger / USD 63 | USD 17 per order + USD 2 per pack | 2 × 63 + 17 + 2 × 2 = 147.00 | Lower unit list price than A, higher final total |
| G | 2 chargers / USD 130 | None | 130.00 | `valid_until: 2026-11-06T14:59:59Z`, one second before frozen now |
| H | 2 chargers / USD 140 | None | 140.00 | Only shipping promised; `ask_seller(delivery_estimate)` establishes 5–7 business days after dispatch, too late |

All products explicitly satisfy the same compatibility/power need. All unstated validity/returns/delivery facts must be made explicit and nonconflicting in the adopted JSON. Example shipping math: A ships within one calendar day (Saturday Nov 7), then one business day gives Monday Nov 9; D uses the same shipping delay plus three business days, giving Wednesday Nov 11.

**Unique outcome:** BUY A, packs 2, authorized/final USD 144.00. No numerical tie is introduced. A correct reference path can use one quote call, eight offer reads, H's delivery reply and one order attempt: 11 tool calls, within the existing cap. The grader should still accept any path reaching the authorized end state.

## Before adoption

1. Owner solves it independently and confirms the scenario reflects a meaningful expert decision.
2. Expand the task schema's current maximum of five offers to the planned hardening range (up to 12), with a DECISIONS entry; no new task logic is needed in Rust.
3. Add coherent scripted replies and every distractor label. Preserve the original dataset snapshot/source for the completed version-1 pilot.
4. Increment t01 `task_version` to 2, update its reference/oracle calls, test every reference/mutant/schema/isolation condition and record the design reason.
5. Run a separately versioned pilot only after review. Do not change a scored task merely to exploit a particular model's wording or to manufacture failures.

This candidate addresses interacting constraints. It does not establish that all other tasks should be hardened, that Luna will fail it, or that the current pilot demonstrates a frontier-model gap.
