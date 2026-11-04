# Task difficulty review — adopted bounded slice, Nov 4

The pilot tested t00/t01/t04/t08, not all nine tasks. All observed purchase/escalation behavior was correct; the three strict failures omit `currency` after a no-order escalation. Review the metric distinction in PILOT_REVIEW.md before claiming a domain limitation or changing scored inputs. The remaining five tasks still lack live pilot evidence.

The owner approved the bounded hardening slice on Nov 4. t01/t02/t05/t06 move to task/reference version 2; the original scored version-1 inputs remain in Git history and original results are unchanged. This combines pack/fee/FX/validity/return/delivery constraints without changing the five-tool interface. It is not evidence that a model will fail. Human task sign-off remains pending.

## Adopted t01 version 2

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

**Unique outcome:** BUY A, packs 2, authorized/final USD 144.00. No numerical tie is introduced. The reference path uses one quote call, eight offer reads, A's supported `fees` reply, H's delivery reply and one order attempt: 12 tool calls, within the existing cap. Fees remain visible in full offer records; the fees reply confirms their meaning. The grader should still accept any path reaching the authorized end state.

## Adoption and remaining review

1. Owner still needs to solve it independently and confirm the scenario reflects a meaningful expert decision.
2. Task-schema contract v2 raises the offer cap from five to exactly eight, with a DECISIONS entry; no new task logic is needed in Rust.
3. Add coherent scripted replies and every distractor label. Preserve the original dataset snapshot/source for the completed version-1 pilot.
4. Increment t01 `task_version` to 2, update its reference/oracle calls, test every reference/mutant/schema/isolation condition and record the design reason.
5. No new model run accompanies this hardening. Run a separately versioned pilot only after review and budget reconciliation. Do not change a scored task merely to exploit a particular model's wording or to manufacture failures.

This candidate addresses interacting constraints. It does not establish that all other tasks should be hardened, that Luna will fail it, or that the current pilot demonstrates a frontier-model gap.


## Three other fixtures in the approved slice

- **t02 v2:** frozen 15:00Z; A expires at 09:59:59-05:00 (14:59:59Z), one second earlier. Otherwise eligible B=48 and C=42; C wins. No new timestamp rules.
- **t05 v2:** need four items, minimum pack coverage (oversupply remains allowed). A requires two three-packs and totals 74; B requires two two-packs and totals 73; C requires four single packs and totals 92, exceeding budget 80. B wins; no exact-unit rule is added.
- **t06 v2:** frozen Friday Nov 6. A ships by Saturday then two business days, latest Tuesday Nov 10; B ships by Sunday then two business days, also Tuesday but more expensive. C's seller-only four-to-five-business-day transit makes latest Friday Nov 13. A wins at USD 40. No holiday calendar is introduced.

These four version changes are data hardening under existing money, fee, quantity and weekday-delivery contracts. The sole schema expansion is the versioned offer cap. t00/t03/t04/t07/t08 remain unchanged.
