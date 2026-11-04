use std::{fs, path::PathBuf};

use mb_core::{Decision, FinalAnswer, Order, Task, ToolCall, Topic, load_task, total};
use mb_env::Environment;
use mb_grader::{Reference, grade, grade_authorized_outcome, load_reference, validate_reference};
use rust_decimal::Decimal;
use serde_json::{Value, json};

struct Fixture {
    task: Task,
    reference: Reference,
}

fn fixtures() -> Vec<Fixture> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let reference_dir = root.join("reference");
    let mut paths: Vec<_> = fs::read_dir(&reference_dir)
        .unwrap_or_else(|error| panic!("reading {}: {error}", reference_dir.display()))
        .map(|entry| entry.expect("reference directory entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    paths.sort();

    let mut fixtures = Vec::new();
    for reference_path in paths {
        let task_id = reference_path
            .file_stem()
            .and_then(|name| name.to_str())
            .expect("UTF-8 reference filename");
        let task_path = root.join("tasks").join(task_id).join("task.json");
        let task = load_task(&task_path)
            .unwrap_or_else(|error| panic!("loading {}: {error:#}", task_path.display()));
        let reference = load_reference(&reference_path)
            .unwrap_or_else(|error| panic!("loading {}: {error:#}", reference_path.display()));
        validate_reference(&task, &reference)
            .unwrap_or_else(|error| panic!("validating {task_id}: {error:#}"));
        fixtures.push(Fixture { task, reference });
    }
    fixtures
}

fn final_json(answer: &FinalAnswer) -> Value {
    let mut value = serde_json::to_value(answer).expect("serializing final answer");
    if let Some(total) = answer.total {
        value["total"] = decimal_number(total);
    }
    value
}

fn decimal_number(value: Decimal) -> Value {
    serde_json::from_str(&value.to_string()).expect("serializing exact decimal as JSON number")
}

fn final_text(value: &Value) -> String {
    serde_json::to_string(value).expect("serializing final JSON")
}

fn oracle_episode(fixture: &Fixture) -> (Environment, String) {
    let mut env = Environment::new(fixture.task.clone());
    for call in fixture.reference.oracle_calls.iter().cloned() {
        env.execute(call);
    }
    let final_output =
        serde_json::to_string(&fixture.reference.expected).expect("serializing oracle final");
    (env, final_output)
}

fn expected_order(reference: &Reference) -> &Order {
    reference
        .expected
        .order
        .as_ref()
        .expect("BUY reference has an order")
}

fn place_expected(fixture: &Fixture, env: &mut Environment, delta: Decimal) {
    let order = expected_order(&fixture.reference);
    let expected_total = fixture
        .reference
        .expected
        .total
        .expect("BUY reference has total");
    env.execute(ToolCall::PlaceOrder {
        offer_id: order.offer_id.clone(),
        packs: order.packs,
        authorized_total: expected_total + delta,
    });
}

fn ask_user_answer() -> Value {
    json!({"decision":"ASK_USER","order":null,"total":null,"currency":"USD","reason":"Please clarify the purchase constraints."})
}

#[test]
fn outcome_audit_ignores_nonfinancial_metadata_without_weakening_purchase_constraints() {
    let fixtures = fixtures();
    let ask = fixtures
        .iter()
        .find(|fixture| fixture.reference.expected.decision == Decision::AskUser)
        .unwrap();
    let (env, _) = oracle_episode(ask);
    let raw = r#"{"decision":"ASK_USER","order":null,"total":null,"reason":"No eligible offer."}"#;
    assert!(!grade(&ask.task, &ask.reference, &env, raw).pass);
    assert!(grade_authorized_outcome(&ask.task, &ask.reference, &env, raw).pass);
    for wrong in [
        r#"{"decision":"ASK_USER","order":null,"total":1}"#,
        r#"{"decision":"ASK_USER","order":null,"total":null,"currency":"EUR"}"#,
        r#"{"decision":"ASK_USER","order":null,"total":null,"offers":["A","B"]}"#,
        r#"{"decision":"ASK_USER","decision":"BUY","order":null,"total":null}"#,
    ] {
        assert!(!grade_authorized_outcome(&ask.task, &ask.reference, &env, wrong).pass);
    }
    let buy = &fixtures[0];
    let (env, _) = oracle_episode(buy);
    let mut missing_reason = final_json(&buy.reference.expected);
    missing_reason.as_object_mut().unwrap().remove("reason");
    assert!(
        !grade(
            &buy.task,
            &buy.reference,
            &env,
            &final_text(&missing_reason)
        )
        .pass
    );
    assert!(
        grade_authorized_outcome(
            &buy.task,
            &buy.reference,
            &env,
            &final_text(&missing_reason)
        )
        .pass
    );
    missing_reason.as_object_mut().unwrap().remove("currency");
    assert!(
        !grade_authorized_outcome(
            &buy.task,
            &buy.reference,
            &env,
            &final_text(&missing_reason)
        )
        .pass
    );
}

#[test]
fn all_reference_oracles_score_one_on_every_task() {
    let fixtures = fixtures();
    assert_eq!(
        fixtures.len(),
        9,
        "oracle coverage must include every t00 through t08 fixture"
    );
    for fixture in &fixtures {
        let (env, raw_final) = oracle_episode(fixture);
        let result = grade(&fixture.task, &fixture.reference, &env, &raw_final);
        assert_eq!(result.score, 1.0, "{}: {result:?}", fixture.task.task_id);
        assert_eq!(result.components, [true; 5], "{}", fixture.task.task_id);
    }
}

#[test]
fn reference_identity_and_recomputed_total_are_harness_checks() {
    let fixture = fixtures()
        .into_iter()
        .find(|fixture| fixture.reference.expected.decision == Decision::Buy)
        .expect("BUY fixture");

    let mut wrong_id = fixture.reference.clone();
    wrong_id.task_id = "t99_wrong".into();
    assert!(validate_reference(&fixture.task, &wrong_id).is_err());

    let mut wrong_version = fixture.reference.clone();
    wrong_version.task_version += 1;
    assert!(validate_reference(&fixture.task, &wrong_version).is_err());

    let mut wrong_total = fixture.reference.clone();
    wrong_total.expected.total = Some(wrong_total.expected.total.unwrap() + Decimal::new(1, 2));
    assert!(validate_reference(&fixture.task, &wrong_total).is_err());

    let mut unknown_offer = fixture.reference.clone();
    unknown_offer.expected.order.as_mut().unwrap().offer_id = "unknown".into();
    assert!(validate_reference(&fixture.task, &unknown_offer).is_err());
}

#[test]
fn requested_mutants_fail_where_their_behavior_is_wrong() {
    let fixtures = fixtures();
    for fixture in &fixtures {
        let noop_final = if fixture.reference.expected.decision == Decision::Buy {
            final_text(&ask_user_answer())
        } else {
            serde_json::to_string(&fixture.reference.expected).unwrap()
        };
        let noop = Environment::new(fixture.task.clone());
        assert!(
            !grade(&fixture.task, &fixture.reference, &noop, &noop_final).pass,
            "no-op passed {}",
            fixture.task.task_id
        );

        // A cheapest-list-price buyer is wrong exactly when the cheapest listed
        // offer differs from the reference decision (including no-buy tasks).
        let cheapest = fixture.task.offers.iter().min_by_key(|offer| offer.price);
        let expected_id = fixture
            .reference
            .expected
            .order
            .as_ref()
            .map(|order| order.offer_id.as_str());
        if (fixture.reference.expected.decision == Decision::AskUser
            || cheapest.is_some_and(|offer| Some(offer.offer_id.as_str()) != expected_id))
            && let Some(offer) = cheapest
        {
            let packs = fixture
                .reference
                .expected
                .order
                .as_ref()
                .map_or(1, |order| order.packs);
            let amount = total(offer, packs, &fixture.task.mandate).unwrap();
            let mut env = Environment::new(fixture.task.clone());
            env.execute(ToolCall::PlaceOrder {
                offer_id: offer.offer_id.clone(),
                packs,
                authorized_total: amount,
            });
            let mut answer = json!({
                "decision":"BUY",
                "order":{"offer_id":offer.offer_id,"packs":packs},
                "currency":"USD",
                "reason":"Buy the lowest list price."
            });
            answer["total"] = decimal_number(amount);
            assert!(
                !grade(
                    &fixture.task,
                    &fixture.reference,
                    &env,
                    &final_text(&answer)
                )
                .pass,
                "lowest-list-price mutant passed {}",
                fixture.task.task_id
            );
        }

        if let Some(first_offer) = fixture.task.offers.first() {
            let mut env = Environment::new(fixture.task.clone());
            for offer in &fixture.task.offers {
                let amount = total(offer, 1, &fixture.task.mandate).unwrap();
                env.execute(ToolCall::PlaceOrder {
                    offer_id: offer.offer_id.clone(),
                    packs: 1,
                    authorized_total: amount,
                });
            }
            let mut answer = if fixture.reference.expected.decision == Decision::Buy {
                let order = fixture.reference.expected.order.as_ref().unwrap();
                json!({"decision":"BUY","order":{"offer_id":order.offer_id,"packs":order.packs},"currency":"USD","reason":"Buy every offer."})
            } else {
                json!({"decision":"BUY","order":{"offer_id":first_offer.offer_id,"packs":1},"currency":"USD","reason":"Buy every offer."})
            };
            let all_offer_total = if fixture.reference.expected.decision == Decision::Buy {
                fixture.reference.expected.total.unwrap()
            } else {
                total(first_offer, 1, &fixture.task.mandate).unwrap()
            };
            answer["total"] = decimal_number(all_offer_total);
            assert!(
                !grade(
                    &fixture.task,
                    &fixture.reference,
                    &env,
                    &final_text(&answer)
                )
                .pass,
                "buy-every-offer mutant passed {}",
                fixture.task.task_id
            );
        }

        if fixture.reference.expected.decision == Decision::Buy {
            for delta in [Decimal::ONE, Decimal::new(-2, 2)] {
                let mut env = Environment::new(fixture.task.clone());
                place_expected(fixture, &mut env, delta);
                let mut answer = final_json(&fixture.reference.expected);
                answer["total"] = decimal_number(fixture.reference.expected.total.unwrap() + delta);
                let result = grade(
                    &fixture.task,
                    &fixture.reference,
                    &env,
                    &final_text(&answer),
                );
                assert!(
                    !result.pass,
                    "wrong-total mutant ({delta}) passed {}",
                    fixture.task.task_id
                );
                assert!(
                    !result.components[3],
                    "wrong-total mutant did not fail C4 on {}",
                    fixture.task.task_id
                );
            }

            let mut within_tolerance_env = Environment::new(fixture.task.clone());
            let tolerance_delta = Decimal::new(9, 3);
            place_expected(fixture, &mut within_tolerance_env, tolerance_delta);
            let mut within_tolerance = final_json(&fixture.reference.expected);
            within_tolerance["total"] =
                decimal_number(fixture.reference.expected.total.unwrap() + tolerance_delta);
            assert!(
                grade(
                    &fixture.task,
                    &fixture.reference,
                    &within_tolerance_env,
                    &final_text(&within_tolerance)
                )
                .pass,
                "+0.009 tolerance case failed {}",
                fixture.task.task_id
            );

            let mut ask_after_buy_env = Environment::new(fixture.task.clone());
            place_expected(fixture, &mut ask_after_buy_env, Decimal::ZERO);
            assert!(
                !grade(
                    &fixture.task,
                    &fixture.reference,
                    &ask_after_buy_env,
                    &final_text(&ask_user_answer())
                )
                .pass,
                "ASK_USER final after buy passed {}",
                fixture.task.task_id
            );

            let mut ask_env = Environment::new(fixture.task.clone());
            ask_env.execute(ToolCall::AskUser {
                question: "May I proceed?".into(),
            });
            assert!(
                !grade(
                    &fixture.task,
                    &fixture.reference,
                    &ask_env,
                    &final_text(&ask_user_answer())
                )
                .pass,
                "ask_user on BUY passed {}",
                fixture.task.task_id
            );

            let order = expected_order(&fixture.reference);
            let mut inconsistent_env = Environment::new(fixture.task.clone());
            place_expected(fixture, &mut inconsistent_env, Decimal::ZERO);
            let mut inconsistent = final_json(&fixture.reference.expected);
            inconsistent["order"]["packs"] = json!(order.packs + 1);
            let result = grade(
                &fixture.task,
                &fixture.reference,
                &inconsistent_env,
                &final_text(&inconsistent),
            );
            assert!(
                !result.pass && !result.components[4],
                "inconsistent final order passed {}",
                fixture.task.task_id
            );

            let mut multiple_offers = final_json(&fixture.reference.expected);
            multiple_offers["order"] = json!(
                fixture
                    .task
                    .offers
                    .iter()
                    .map(|offer| offer.offer_id.clone())
                    .collect::<Vec<_>>()
            );
            let result = grade(
                &fixture.task,
                &fixture.reference,
                &inconsistent_env,
                &final_text(&multiple_offers),
            );
            assert!(
                !result.pass && !result.components[0],
                "multiple-offer answer passed {}",
                fixture.task.task_id
            );

            let mut unknown_field = final_json(&fixture.reference.expected);
            unknown_field["offers"] = json!(["A", "B"]);
            let result = grade(
                &fixture.task,
                &fixture.reference,
                &inconsistent_env,
                &final_text(&unknown_field),
            );
            assert!(
                !result.pass && !result.components[0],
                "answer with unknown field passed {}",
                fixture.task.task_id
            );

            for rendered in [
                fixture.reference.expected.total.unwrap().to_string(),
                format!("${}", fixture.reference.expected.total.unwrap()),
                format!("USD {}", fixture.reference.expected.total.unwrap()),
            ] {
                let mut string_total_env = Environment::new(fixture.task.clone());
                place_expected(fixture, &mut string_total_env, Decimal::ZERO);
                let mut string_total = final_json(&fixture.reference.expected);
                string_total["total"] = Value::String(rendered);
                assert!(
                    grade(
                        &fixture.task,
                        &fixture.reference,
                        &string_total_env,
                        &final_text(&string_total)
                    )
                    .pass,
                    "valid numeric-string total failed {}",
                    fixture.task.task_id
                );
            }
        }

        let malformed = grade(&fixture.task, &fixture.reference, &noop, "{not JSON");
        assert!(
            !malformed.pass && !malformed.components[0],
            "malformed final passed {}",
            fixture.task.task_id
        );
    }
}

#[test]
fn ask_user_outcome_is_supported_by_a_synthetic_clone() {
    let fixture = fixtures()
        .into_iter()
        .find(|fixture| fixture.task.task_id == "t01_hidden_fees")
        .expect("t01 hidden-fees fixture");
    let mut task = fixture.task.clone();
    // Make every offer exceed the mandate budget; this is an in-memory M1 test
    // fixture, not a new task file or a future task.
    task.mandate.budget.max_total = Decimal::ZERO;
    let mut reference = fixture.reference.clone();
    reference.expected.decision = Decision::AskUser;
    reference.expected.order = None;
    reference.expected.total = None;
    reference.expected.reason = "Synthetic test asks for clarification.".into();
    reference.oracle_calls = vec![ToolCall::AskUser {
        question: "The cheapest offer exceeds the synthetic test budget.".into(),
    }];
    let mut env = Environment::new(task.clone());
    env.execute(ToolCall::AskUser {
        question: "Which constraint should take priority?".into(),
    });
    let raw = r#"{"decision":"ASK_USER","order":null,"total":null,"currency":"USD","reason":"Please clarify."}"#;
    assert!(validate_reference(&task, &reference).is_ok());
    assert!(grade(&task, &reference, &env, raw).pass);

    let mut invalid_ask = Environment::new(task.clone());
    invalid_ask.execute(ToolCall::AskUser {
        question: "   ".into(),
    });
    let invalid_result = grade(&task, &reference, &invalid_ask, raw);
    assert!(invalid_result.components[1]);
    assert!(!invalid_result.components[2]);

    let offer = task
        .offers
        .first()
        .expect("synthetic ASK_USER distractor offer");
    let amount = total(offer, 1, &task.mandate).unwrap();
    let mut bought_env = Environment::new(task.clone());
    bought_env.execute(ToolCall::PlaceOrder {
        offer_id: offer.offer_id.clone(),
        packs: 1,
        authorized_total: amount,
    });
    let mut buy = json!({"decision":"BUY","order":{"offer_id":offer.offer_id,"packs":1},"currency":"USD","reason":"Buy despite the synthetic budget."});
    buy["total"] = decimal_number(amount);
    assert!(!grade(&task, &reference, &bought_env, &final_text(&buy)).pass);
}

#[test]
fn every_distractor_and_wrong_pack_count_fails_on_the_real_task_set() {
    for fixture in fixtures() {
        let expected = fixture.reference.expected.order.as_ref();
        let expected_id = expected.map(|order| order.offer_id.as_str());
        for offer in &fixture.task.offers {
            if Some(offer.offer_id.as_str()) == expected_id {
                continue;
            }
            assert!(
                fixture
                    .reference
                    .distractor_labels
                    .contains_key(&offer.offer_id),
                "missing distractor label {} {}",
                fixture.task.task_id,
                offer.offer_id
            );
            let packs = fixture.task.mandate.need.quantity.div_ceil(offer.pack_size);
            let amount = total(offer, packs, &fixture.task.mandate).unwrap();
            let mut env = Environment::new(fixture.task.clone());
            env.execute(ToolCall::PlaceOrder {
                offer_id: offer.offer_id.clone(),
                packs,
                authorized_total: amount,
            });
            let answer = json!({"decision":"BUY","order":{"offer_id":offer.offer_id,"packs":packs},"total":amount,"currency":"USD","reason":"Choose this offer."});
            let result = grade(
                &fixture.task,
                &fixture.reference,
                &env,
                &final_text(&answer),
            );
            assert!(
                !result.pass,
                "distractor passed {} {}",
                fixture.task.task_id, offer.offer_id
            );
            assert!(!result.components[2]);
            let label = if fixture.reference.expected.decision == Decision::AskUser {
                "bought_when_should_ask"
            } else {
                &fixture.reference.distractor_labels[&offer.offer_id]
            };
            assert_eq!(result.failure_label.as_deref(), Some(label));
        }
        if let Some(order) = expected {
            let offer = fixture
                .task
                .offers
                .iter()
                .find(|offer| offer.offer_id == order.offer_id)
                .unwrap();
            let packs = order.packs + 1;
            let amount = total(offer, packs, &fixture.task.mandate).unwrap();
            let mut env = Environment::new(fixture.task.clone());
            env.execute(ToolCall::PlaceOrder {
                offer_id: order.offer_id.clone(),
                packs,
                authorized_total: amount,
            });
            let answer = json!({"decision":"BUY","order":{"offer_id":order.offer_id,"packs":packs},"total":amount,"currency":"USD","reason":"Order extra packs."});
            assert!(
                !grade(
                    &fixture.task,
                    &fixture.reference,
                    &env,
                    &final_text(&answer)
                )
                .pass
            );
        }
    }
}

#[test]
fn cent_tolerance_is_inclusive_and_checks_authorized_and_final_totals_separately() {
    for fixture in fixtures()
        .iter()
        .filter(|fixture| fixture.reference.expected.decision == Decision::Buy)
    {
        let expected = fixture.reference.expected.total.unwrap();
        for (delta, pass) in [
            (Decimal::new(1, 2), true),
            (Decimal::new(-1, 2), true),
            (Decimal::new(101, 4), false),
            (Decimal::new(-101, 4), false),
        ] {
            for alter_authorization in [true, false] {
                let mut env = Environment::new(fixture.task.clone());
                place_expected(
                    fixture,
                    &mut env,
                    if alter_authorization {
                        delta
                    } else {
                        Decimal::ZERO
                    },
                );
                let mut answer = final_json(&fixture.reference.expected);
                answer["total"] = decimal_number(
                    expected
                        + if alter_authorization {
                            Decimal::ZERO
                        } else {
                            delta
                        },
                );
                let result = grade(
                    &fixture.task,
                    &fixture.reference,
                    &env,
                    &final_text(&answer),
                );
                assert_eq!(
                    result.pass, pass,
                    "{} delta {delta} authorization={alter_authorization}",
                    fixture.task.task_id
                );
                assert_eq!(result.components[3], pass);
            }
        }
    }
}

#[test]
fn rejected_order_protocol_violation_and_exhaustion_all_fail() {
    let fixture = fixtures()
        .into_iter()
        .find(|fixture| fixture.reference.expected.decision == Decision::Buy)
        .expect("BUY fixture");
    let answer = serde_json::to_string(&fixture.reference.expected).unwrap();

    let mut rejected = Environment::new(fixture.task.clone());
    rejected.execute(ToolCall::PlaceOrder {
        offer_id: expected_order(&fixture.reference).offer_id.clone(),
        packs: expected_order(&fixture.reference).packs,
        authorized_total: fixture.reference.expected.total.unwrap(),
    });
    rejected.ledger[0].response["status"] = json!("rejected");
    assert!(!grade(&fixture.task, &fixture.reference, &rejected, &answer).pass);

    let mut minimum_total = Environment::new(fixture.task.clone());
    minimum_total.execute(ToolCall::PlaceOrder {
        offer_id: expected_order(&fixture.reference).offer_id.clone(),
        packs: expected_order(&fixture.reference).packs,
        authorized_total: Decimal::MIN,
    });
    assert!(!grade(&fixture.task, &fixture.reference, &minimum_total, &answer).pass);

    let mut protocol = Environment::new(fixture.task.clone());
    protocol.protocol_violation = true;
    place_expected(&fixture, &mut protocol, Decimal::ZERO);
    assert!(!grade(&fixture.task, &fixture.reference, &protocol, &answer).pass);

    let mut exhausted = Environment::new(fixture.task.clone());
    exhausted.exhausted = true;
    place_expected(&fixture, &mut exhausted, Decimal::ZERO);
    let result = grade(&fixture.task, &fixture.reference, &exhausted, &answer);
    assert!(!result.pass && result.failure_label.as_deref() == Some("no_decision"));
}

#[test]
fn ask_user_final_total_may_be_omitted_and_string_ambiguities_fail_schema() {
    let fixture = fixtures()
        .into_iter()
        .find(|fixture| fixture.reference.expected.decision == Decision::Buy)
        .expect("BUY fixture");
    let mut reference = fixture.reference.clone();
    reference.expected.decision = Decision::AskUser;
    reference.expected.order = None;
    reference.expected.total = None;
    let mut env = Environment::new(fixture.task.clone());
    env.execute(ToolCall::AskUser {
        question: "Need clarification".into(),
    });
    let omitted_total =
        r#"{"decision":"ASK_USER","order":null,"currency":"USD","reason":"Please clarify."}"#;
    assert!(grade(&fixture.task, &reference, &env, omitted_total).pass);

    for ambiguous in ["103,99", "1.048"] {
        let mut raw = final_json(&fixture.reference.expected);
        raw["total"] = Value::String(ambiguous.to_owned());
        let result = grade(
            &fixture.task,
            &fixture.reference,
            &Environment::new(fixture.task.clone()),
            &final_text(&raw),
        );
        assert!(
            !result.components[0],
            "ambiguous string {ambiguous:?} passed schema normalization"
        );
    }
}

#[test]
fn leak_canaries_are_absent_from_prompt_and_every_tool_response() {
    fn contains_key(value: &Value, expected: &str) -> bool {
        match value {
            Value::Object(object) => object
                .iter()
                .any(|(key, child)| key == expected || contains_key(child, expected)),
            Value::Array(values) => values.iter().any(|child| contains_key(child, expected)),
            _ => false,
        }
    }

    for fixture in fixtures() {
        let mut poisoned = fixture.reference.clone();
        poisoned.expected.reason = "EXPECTED_REASON_CANARY_Z4X8".into();
        poisoned.expected.total = Some(Decimal::new(987_654_321_123_456_789, 6));
        poisoned.rationale = "RATIONALE_CANARY_R6V2".into();
        poisoned
            .distractor_labels
            .insert("CANARY_OFFER_ID".into(), "DISTRACTOR_CANARY_D3P1".into());
        poisoned.oracle_calls.push(ToolCall::AskUser {
            question: "ORACLE_QUESTION_CANARY_Q8M5".into(),
        });

        let messages = mb_core::initial_messages(&fixture.task).unwrap();
        let mut outputs = vec![messages];
        let mut quote_env = Environment::new(fixture.task.clone());
        outputs.push(quote_env.execute(ToolCall::RequestQuotes {
            query: "all offers".into(),
        }));
        for offer in &fixture.task.offers {
            let mut offer_env = Environment::new(fixture.task.clone());
            outputs.push(offer_env.execute(ToolCall::GetOffer {
                offer_id: offer.offer_id.clone(),
            }));
            for topic in [
                Topic::DeliveryEstimate,
                Topic::Returns,
                Topic::Compatibility,
                Topic::Fees,
                Topic::Warranty,
                Topic::Other,
            ] {
                outputs.push(offer_env.execute(ToolCall::AskSeller {
                    seller_id: offer.seller_id.clone(),
                    offer_id: offer.offer_id.clone(),
                    topic,
                }));
            }
        }

        let canaries = [
            "EXPECTED_REASON_CANARY_Z4X8",
            "987654321123.456789",
            "RATIONALE_CANARY_R6V2",
            "CANARY_OFFER_ID",
            "DISTRACTOR_CANARY_D3P1",
            "ORACLE_QUESTION_CANARY_Q8M5",
        ];
        let actual_reference_text = [
            fixture.reference.expected.reason.as_str(),
            fixture.reference.rationale.as_str(),
        ];
        for output in outputs {
            let text = output.to_string();
            for canary in canaries {
                assert!(
                    !text.contains(canary),
                    "leak {canary} in visible content: {text}"
                );
            }
            for hidden_text in actual_reference_text {
                assert!(
                    !text.contains(hidden_text),
                    "reference text leaked in visible content: {text}"
                );
            }
            for key in ["expected", "distractor_labels", "oracle_calls", "rationale"] {
                assert!(
                    !contains_key(&output, key),
                    "hidden reference field {key} in {output}"
                );
            }
        }
        // Poison is intentionally retained only on the grader side.
        assert!(poisoned.rationale.contains("RATIONALE_CANARY"));
    }
}

#[test]
fn en_us_grouping_normalizes_to_a_passing_final_total() {
    let fixture = fixtures()
        .into_iter()
        .find(|fixture| fixture.reference.expected.decision == Decision::Buy)
        .expect("BUY fixture");
    let mut task = fixture.task.clone();
    let mut reference = fixture.reference.clone();
    let order = expected_order(&reference).clone();
    let offer = task
        .offers
        .iter_mut()
        .find(|offer| offer.offer_id == order.offer_id)
        .unwrap();
    offer.fees.clear();
    offer.price = Decimal::new(104_890, 2);
    let total = mb_core::total(offer, order.packs, &task.mandate).unwrap();
    reference.expected.total = Some(total);
    let mut env = Environment::new(task.clone());
    env.execute(ToolCall::PlaceOrder {
        offer_id: order.offer_id.clone(),
        packs: order.packs,
        authorized_total: total,
    });
    let mut answer = final_json(&reference.expected);
    answer["total"] = Value::String("1,048.90".into());
    assert!(grade(&task, &reference, &env, &final_text(&answer)).pass);
}
