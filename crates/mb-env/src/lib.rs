//! Deterministic scripted-seller environment for one isolated trial.

use mb_core::{MAX_TOOL_CALLS, Offer, Task, ToolCall};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A complete, ordered record of a buyer tool invocation and its visible reply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRecord {
    pub call: ToolCall,
    pub response: Value,
}

/// Trial-local environment. Construct a new instance for every trial.
#[derive(Debug, Clone)]
pub struct Environment {
    pub task: Task,
    pub ledger: Vec<ToolRecord>,
    pub stopped: bool,
    pub exhausted: bool,
    pub protocol_violation: bool,
}

impl Environment {
    pub fn new(task: Task) -> Self {
        Self {
            task,
            ledger: Vec::new(),
            stopped: false,
            exhausted: false,
            protocol_violation: false,
        }
    }

    /// Execute one call and return its JSON-visible tool response.
    ///
    /// Invalid calls are represented as JSON errors and are still appended to the
    /// ledger. The mandate clock is never advanced: `task.mandate.now` is frozen
    /// for the lifetime of this environment.
    pub fn execute(&mut self, call: ToolCall) -> Value {
        let response = if self.stopped {
            self.protocol_violation = true;
            error(
                "protocol_violation",
                "tool use has already stopped after ask_user",
            )
        } else if self.exhausted || self.ledger.len() >= MAX_TOOL_CALLS {
            self.exhausted = true;
            error(
                "tool_call_limit",
                "the maximum number of tool calls has been reached",
            )
        } else {
            self.execute_allowed(&call)
        };

        self.ledger.push(ToolRecord {
            call,
            response: response.clone(),
        });
        if self.ledger.len() >= MAX_TOOL_CALLS {
            self.exhausted = true;
        }
        response
    }

    fn execute_allowed(&mut self, call: &ToolCall) -> Value {
        match call {
            ToolCall::RequestQuotes { query } => {
                if query.trim().is_empty() {
                    return error("invalid_field", "query must not be empty");
                }
                Value::Array(self.task.offers.iter().map(quote_summary).collect())
            }
            ToolCall::GetOffer { offer_id } => {
                if offer_id.trim().is_empty() {
                    return error("invalid_field", "offer_id must not be empty");
                }
                match self.offer(offer_id) {
                    Some(offer) => match serde_json::to_value(offer) {
                        Ok(mut value) => {
                            if let Some(object) = value.as_object_mut() {
                                object.remove("seller_qa");
                            }
                            value
                        }
                        Err(_) => error("serialization_error", "offer could not be serialized"),
                    },
                    None => error("offer_not_found", "no offer has that offer_id"),
                }
            }
            ToolCall::AskSeller {
                seller_id,
                offer_id,
                topic,
            } => {
                if seller_id.trim().is_empty() || offer_id.trim().is_empty() {
                    return error("invalid_field", "seller_id and offer_id must not be empty");
                }
                let Some(offer) = self.offer(offer_id) else {
                    return error("offer_not_found", "no offer has that offer_id");
                };
                if offer.seller_id != *seller_id {
                    return error(
                        "seller_offer_mismatch",
                        "seller_id does not own this offer_id",
                    );
                }
                Value::String(offer.seller_qa.get(topic).cloned().unwrap_or_else(|| {
                    "No scripted answer is available for this topic.".to_owned()
                }))
            }
            ToolCall::PlaceOrder {
                offer_id,
                packs,
                authorized_total,
            } => self.place_order(offer_id, *packs, *authorized_total),
            ToolCall::AskUser { question } => {
                if question.trim().is_empty() {
                    return error("invalid_field", "question must not be empty");
                }
                self.stopped = true;
                Value::String("Delivered; the user will reply later.".to_owned())
            }
        }
    }

    fn place_order(&self, offer_id: &str, packs: u32, authorized_total: mb_core::Decimal) -> Value {
        // Every place_order invocation consumes the single attempt, including a
        // rejected first attempt. The prior ledger is read before this call is
        // appended by execute().
        let prior_attempts = self
            .ledger
            .iter()
            .filter(|record| matches!(record.call, ToolCall::PlaceOrder { .. }))
            .count();
        if prior_attempts >= 1 {
            return json!({"order_id": null, "status": "rejected"});
        }
        if offer_id.trim().is_empty() || packs == 0 || authorized_total < mb_core::Decimal::ZERO {
            return json!({"order_id": null, "status": "rejected"});
        }
        let Some(offer) = self.offer(offer_id) else {
            return json!({"order_id": null, "status": "rejected"});
        };
        if self.task.mandate.now > offer.valid_until {
            return json!({"order_id": null, "status": "rejected"});
        }

        json!({
            "order_id": format!("{}-order-1", self.task.task_id),
            "status": "accepted"
        })
    }

    fn offer(&self, offer_id: &str) -> Option<&Offer> {
        self.task
            .offers
            .iter()
            .find(|offer| offer.offer_id == offer_id)
    }
}

fn quote_summary(offer: &Offer) -> Value {
    json!({
        "offer_id": offer.offer_id,
        "seller_id": offer.seller_id,
        "title": offer.product.title,
        "price": offer.price,
        "currency": offer.currency,
        "pack_size": offer.pack_size
    })
}

fn error(code: &str, message: &str) -> Value {
    json!({"error": code, "message": message})
}

#[cfg(test)]
mod tests {
    use mb_core::{Decimal, ToolCall, Topic};
    use serde_json::{Value, json};

    use super::Environment;

    fn task(valid_until: &str) -> mb_core::Task {
        serde_json::from_value(json!({
            "task_id": "t_test_env",
            "task_version": 1,
            "mandate": {
                "mandate_id": "t_test_env",
                "now": "2026-11-03T10:00:00-05:00",
                "request": "Buy one synthetic test item.",
                "need": {"description": "test item", "quantity": 1, "compatibility": null},
                "budget": {"max_total": 20, "currency": "USD", "includes": "all costs"},
                "deadline": null,
                "returns": {"min_window_days": 14},
                "fx": null,
                "authority": {
                    "max_orders": 1,
                    "choose": "lowest eligible total",
                    "if_none_eligible": "ask the user",
                    "only_the_user_can_change_this_mandate": true
                }
            },
            "sellers": [{"seller_id": "S1", "name": "Synthetic Seller"}],
            "offers": [{
                "offer_id": "O1",
                "seller_id": "S1",
                "seller_name": "Synthetic Seller",
                "product": {"sku": "SKU-1", "title": "Test widget", "specs": {}, "compatible_with": []},
                "pack_size": 1,
                "price": 12.50,
                "currency": "USD",
                "fees": [],
                "valid_until": valid_until,
                "ships_from": "Fictional Town",
                "ships_within_days": 1,
                "delivery_estimate": null,
                "return_policy": null,
                "terms": "Synthetic terms.",
                "seller_qa": {"delivery_estimate": "Two business days.", "returns": "Fourteen day returns."}
            }]
        })).expect("valid test task")
    }

    fn place(offer_id: &str) -> ToolCall {
        ToolCall::PlaceOrder {
            offer_id: offer_id.to_owned(),
            packs: 1,
            // Deliberately above the test mandate budget: the environment does
            // not make grader eligibility decisions.
            authorized_total: Decimal::new(9_999, 2),
        }
    }

    #[test]
    fn quote_summaries_and_offer_details_do_not_expose_scripted_qa() {
        let mut env = Environment::new(task("2026-11-04T10:00:00-05:00"));
        let quotes = env.execute(ToolCall::RequestQuotes {
            query: "widget".into(),
        });
        let quote = &quotes[0];
        let quote_keys: Vec<_> = quote
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            quote_keys,
            [
                "currency",
                "offer_id",
                "pack_size",
                "price",
                "seller_id",
                "title"
            ]
        );

        let detail = env.execute(ToolCall::GetOffer {
            offer_id: "O1".into(),
        });
        assert!(detail.get("seller_qa").is_none());
        assert!(detail.get("terms").is_some());
        assert_eq!(
            env.execute(ToolCall::AskSeller {
                seller_id: "S1".into(),
                offer_id: "O1".into(),
                topic: Topic::DeliveryEstimate,
            }),
            Value::String("Two business days.".into())
        );
        assert_eq!(env.ledger.len(), 3);
    }

    #[test]
    fn each_environment_starts_with_an_empty_ledger_and_frozen_task_clock() {
        let task = task("2026-11-03T10:00:00-05:00");
        let frozen_now = task.mandate.now;
        let mut first = Environment::new(task.clone());
        assert_eq!(first.task.mandate.now, frozen_now);
        assert_eq!(first.execute(place("O1"))["status"], "accepted");
        assert_eq!(first.ledger.len(), 1);

        let second = Environment::new(task);
        assert!(second.ledger.is_empty());
        assert_eq!(second.task.mandate.now, frozen_now);
        assert!(!second.stopped && !second.exhausted && !second.protocol_violation);
    }

    #[test]
    fn expiry_is_checked_against_the_frozen_clock_including_the_boundary() {
        let mut expired = Environment::new(task("2026-11-03T09:59:59-05:00"));
        assert_eq!(expired.execute(place("O1"))["status"], "rejected");

        let mut valid_at_boundary = Environment::new(task("2026-11-03T10:00:00-05:00"));
        assert_eq!(valid_at_boundary.execute(place("O1"))["status"], "accepted");
    }

    #[test]
    fn calls_after_ask_user_are_recorded_but_have_no_effect() {
        let mut env = Environment::new(task("2026-11-04T10:00:00-05:00"));
        assert_eq!(
            env.execute(ToolCall::AskUser {
                question: "Should I continue?".into()
            }),
            Value::String("Delivered; the user will reply later.".into())
        );
        let response = env.execute(place("O1"));
        assert_eq!(response["error"], "protocol_violation");
        assert!(env.stopped && env.protocol_violation);
        assert_eq!(env.ledger.len(), 2);
        assert_eq!(env.ledger[1].response, response);
    }

    #[test]
    fn twentieth_call_is_allowed_and_later_calls_are_blocked_and_recorded() {
        let mut env = Environment::new(task("2026-11-04T10:00:00-05:00"));
        for _ in 0..20 {
            let response = env.execute(ToolCall::RequestQuotes {
                query: "widget".into(),
            });
            assert!(response.is_array());
        }
        assert!(env.exhausted);
        assert_eq!(env.ledger.len(), 20);

        let response = env.execute(place("O1"));
        assert_eq!(response["error"], "tool_call_limit");
        assert_eq!(env.ledger.len(), 21);
        assert_eq!(env.ledger[20].response, response);
    }

    #[test]
    fn rejected_first_order_consumes_the_single_attempt_and_every_call_is_logged() {
        let mut env = Environment::new(task("2026-11-04T10:00:00-05:00"));
        assert_eq!(env.execute(place("missing"))["status"], "rejected");
        assert_eq!(env.execute(place("O1"))["status"], "rejected");
        assert_eq!(env.ledger.len(), 2);
        assert!(
            env.ledger
                .iter()
                .all(|record| matches!(record.call, ToolCall::PlaceOrder { .. }))
        );

        // Rejections never reveal the authorized or computed charge.
        assert_eq!(env.ledger[0].response.as_object().unwrap().len(), 2);
        assert!(env.ledger[0].response.get("total").is_none());
    }

    #[test]
    fn zero_packs_and_negative_authorization_are_rejected_and_recorded() {
        let mut zero_packs = Environment::new(task("2026-11-04T10:00:00-05:00"));
        let response = zero_packs.execute(ToolCall::PlaceOrder {
            offer_id: "O1".into(),
            packs: 0,
            authorized_total: Decimal::ONE,
        });
        assert_eq!(response["status"], "rejected");
        assert_eq!(zero_packs.ledger.len(), 1);

        let mut negative_total = Environment::new(task("2026-11-04T10:00:00-05:00"));
        let response = negative_total.execute(ToolCall::PlaceOrder {
            offer_id: "O1".into(),
            packs: 1,
            authorized_total: -Decimal::ONE,
        });
        assert_eq!(response["status"], "rejected");
        assert_eq!(negative_total.ledger.len(), 1);
    }

    #[test]
    fn seller_must_own_the_offer_and_missing_topics_have_a_stable_reply() {
        let mut env = Environment::new(task("2026-11-04T10:00:00-05:00"));
        let mismatch = env.execute(ToolCall::AskSeller {
            seller_id: "S2".into(),
            offer_id: "O1".into(),
            topic: Topic::Returns,
        });
        assert_eq!(mismatch["error"], "seller_offer_mismatch");

        let unavailable = env.execute(ToolCall::AskSeller {
            seller_id: "S1".into(),
            offer_id: "O1".into(),
            topic: Topic::Warranty,
        });
        assert_eq!(
            unavailable,
            Value::String("No scripted answer is available for this topic.".into())
        );
        assert_eq!(env.ledger.len(), 2);
    }

    #[test]
    fn invalid_ask_user_does_not_stop_the_episode() {
        let mut env = Environment::new(task("2026-11-04T10:00:00-05:00"));
        assert_eq!(
            env.execute(ToolCall::AskUser {
                question: "  ".into()
            })["error"],
            "invalid_field"
        );
        assert!(!env.stopped);
        assert!(
            env.execute(ToolCall::RequestQuotes {
                query: "widget".into()
            })
            .is_array()
        );
    }
}
