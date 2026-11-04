use std::{fs, path::PathBuf};

use mb_core::{Decimal, Task, ToolCall, latest_delivery, load_task, total, validate_schema};
use mb_env::Environment;
use mb_grader::{Reference, grade, load_reference, validate_reference};
use serde_json::{Value, json};

struct Fixture {
    task: Task,
    reference: Reference,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(task_id: &str) -> Fixture {
    let root = root();
    let task_path = root.join("tasks").join(task_id).join("task.json");
    let reference_path = root.join("reference").join(format!("{task_id}.json"));
    let task =
        load_task(&task_path).unwrap_or_else(|error| panic!("{}: {error:#}", task_path.display()));
    let reference = load_reference(&reference_path)
        .unwrap_or_else(|error| panic!("{}: {error:#}", reference_path.display()));
    validate_reference(&task, &reference).unwrap_or_else(|error| panic!("{task_id}: {error:#}"));
    assert_eq!(
        task.task_version, 2,
        "{task_id} must be the hardening v2 fixture"
    );
    assert_eq!(
        reference.task_version, 2,
        "{task_id} reference version mismatch"
    );
    Fixture { task, reference }
}

fn offer<'a>(task: &'a Task, offer_id: &str) -> &'a mb_core::Offer {
    task.offers
        .iter()
        .find(|offer| offer.offer_id == offer_id)
        .unwrap()
}

fn assert_compatible_and_returnable(task: &Task, offer: &mb_core::Offer) {
    let required = task.mandate.need.compatibility.as_ref().unwrap();
    assert!(
        required
            .values()
            .all(|device| offer.product.compatible_with.contains(device)),
        "{}",
        offer.offer_id
    );
    let returns = offer.return_policy.as_ref().unwrap();
    assert!(
        returns.returnable
            && returns.window_days >= task.mandate.returns.as_ref().unwrap().min_window_days,
        "{}",
        offer.offer_id
    );
    assert!(offer.valid_until >= task.mandate.now, "{}", offer.offer_id);
}

fn money(value: Decimal) -> Value {
    serde_json::from_str(&value.to_string()).expect("exact JSON decimal")
}

fn answer_json(offer_id: &str, packs: u32, total: Decimal) -> String {
    json!({
        "decision": "BUY",
        "order": {"offer_id": offer_id, "packs": packs},
        "total": money(total),
        "currency": "USD",
        "reason": "Test answer."
    })
    .to_string()
}

fn play_oracle(fixture: &Fixture) -> (Environment, String) {
    let mut env = Environment::new(fixture.task.clone());
    for call in fixture.reference.oracle_calls.iter().cloned() {
        env.execute(call);
    }
    let final_output = serde_json::to_string(&fixture.reference.expected).unwrap();
    (env, final_output)
}

#[test]
fn all_four_v2_references_validate_and_score_one() {
    let ids = [
        "t01_hidden_fees",
        "t02_expired_offer",
        "t05_bundle_vs_unit",
        "t06_ship_vs_delivery",
    ];
    for task_id in ids {
        let fixture = fixture(task_id);
        let (env, final_output) = play_oracle(&fixture);
        let result = grade(&fixture.task, &fixture.reference, &env, &final_output);
        assert_eq!(result.score, 1.0, "{task_id}: {result:?}");
        assert_eq!(result.components, [true; 5], "{task_id}");
        assert!(!env.protocol_violation, "{task_id}");
    }
}

#[test]
fn t01_v2_totals_leave_a_unique_eligible_minimum_and_use_the_shared_formula() {
    let fixture = fixture("t01_hidden_fees");
    let task = &fixture.task;
    assert_eq!(task.offers.len(), 8);
    assert_eq!(task.mandate.need.quantity, 2);
    assert_eq!(
        task.mandate.fx.as_ref().unwrap()["EUR_USD"],
        Decimal::new(108, 2)
    );

    let expected = [
        ("A", 2, Decimal::new(14_400, 2)),
        ("B", 1, Decimal::new(14_580, 2)),
        ("C", 2, Decimal::new(13_800, 2)),
        ("D", 1, Decimal::new(14_250, 2)),
        ("E", 1, Decimal::new(14_403, 2)),
        ("F", 2, Decimal::new(14_700, 2)),
        ("G", 1, Decimal::new(13_000, 2)),
        ("H", 1, Decimal::new(14_000, 2)),
    ];
    for (offer_id, packs, amount) in expected {
        assert_eq!(
            total(offer(task, offer_id), packs, &task.mandate).unwrap(),
            amount,
            "{offer_id}"
        );
    }
    let a_total = total(offer(task, "A"), 2, &task.mandate).unwrap();
    for offer_id in ["B", "E", "F"] {
        let alternative = offer(task, offer_id);
        let packs = task.mandate.need.quantity.div_ceil(alternative.pack_size);
        assert!(
            total(alternative, packs, &task.mandate).unwrap() > a_total,
            "{offer_id}"
        );
    }
    assert_eq!(
        fixture.reference.expected.order.as_ref().unwrap().offer_id,
        "A"
    );
    assert_eq!(fixture.reference.expected.order.as_ref().unwrap().packs, 2);
    assert!(offer(task, "C").return_policy.is_none());
    assert!(
        offer(task, "C")
            .terms
            .to_ascii_lowercase()
            .contains("final sale")
    );
    assert!(task.mandate.now > offer(task, "G").valid_until);

    for offer_id in ["A", "B", "E", "F"] {
        let current = offer(task, offer_id);
        assert_compatible_and_returnable(task, current);
        let estimate = current.delivery_estimate.as_ref().unwrap();
        let latest = latest_delivery(
            task.mandate.now,
            current.ships_within_days,
            estimate.max_business_days,
        )
        .unwrap();
        assert!(
            latest <= task.mandate.deadline.as_ref().unwrap().deliver_by,
            "{offer_id}: {latest}"
        );
    }
    let d = offer(task, "D");
    let d_latest = latest_delivery(
        task.mandate.now,
        d.ships_within_days,
        d.delivery_estimate.as_ref().unwrap().max_business_days,
    )
    .unwrap();
    assert!(d_latest > task.mandate.deadline.as_ref().unwrap().deliver_by);
    assert_compatible_and_returnable(task, d);
    let h = offer(task, "H");
    assert!(h.delivery_estimate.is_none());
    assert!(
        latest_delivery(task.mandate.now, h.ships_within_days, 7).unwrap()
            > task.mandate.deadline.as_ref().unwrap().deliver_by
    );
    assert_compatible_and_returnable(task, h);
    assert_eq!(
        total(offer(task, "A"), 2, &task.mandate).unwrap(),
        Decimal::new(14_400, 2)
    );
    assert_eq!(
        total(offer(task, "E"), 1, &task.mandate).unwrap(),
        Decimal::new(14_403, 2)
    );
    assert_eq!(
        total(offer(task, "F"), 2, &task.mandate).unwrap(),
        Decimal::new(14_700, 2)
    );

    let ask_count = fixture
        .reference
        .oracle_calls
        .iter()
        .filter(|call| matches!(call, ToolCall::AskSeller { .. }))
        .count();
    assert_eq!(ask_count, 2);
    assert_eq!(fixture.reference.oracle_calls.len(), 12);
    assert!(fixture.reference.oracle_calls.len() < mb_core::MAX_TOOL_CALLS);
}

#[test]
fn t02_v2_expiry_compares_instants_across_offsets() {
    let fixture = fixture("t02_expired_offer");
    let task = &fixture.task;
    let expired = offer(task, "A");
    assert_eq!(task.mandate.now.to_rfc3339(), "2026-11-06T15:00:00+00:00");
    assert_eq!(
        expired.valid_until.to_rfc3339(),
        "2026-11-06T09:59:59-05:00"
    );
    assert!(task.mandate.now > expired.valid_until);
    assert_eq!(
        total(offer(task, "B"), 1, &task.mandate).unwrap(),
        Decimal::new(4_800, 2)
    );
    assert_eq!(
        total(offer(task, "C"), 1, &task.mandate).unwrap(),
        Decimal::new(4_200, 2)
    );
    for offer_id in ["B", "C"] {
        let current = offer(task, offer_id);
        assert_compatible_and_returnable(task, current);
        let latest = latest_delivery(
            task.mandate.now,
            current.ships_within_days,
            current
                .delivery_estimate
                .as_ref()
                .unwrap()
                .max_business_days,
        )
        .unwrap();
        assert!(
            latest <= task.mandate.deadline.as_ref().unwrap().deliver_by,
            "{offer_id}: {latest}"
        );
    }
    assert_eq!(
        fixture.reference.expected.order.as_ref().unwrap().offer_id,
        "C"
    );
}

#[test]
fn t05_v2_uses_minimum_packs_and_allows_required_oversupply() {
    let fixture = fixture("t05_bundle_vs_unit");
    let task = &fixture.task;
    assert_eq!(task.mandate.need.quantity, 4);
    let a_packs = task
        .mandate
        .need
        .quantity
        .div_ceil(offer(task, "A").pack_size);
    assert_eq!(a_packs, 2);
    assert_eq!(a_packs * offer(task, "A").pack_size, 6);
    let packs = task
        .mandate
        .need
        .quantity
        .div_ceil(offer(task, "B").pack_size);
    assert_eq!(packs, 2);
    assert_eq!(packs * offer(task, "B").pack_size, 4);
    for current in &task.offers {
        assert_compatible_and_returnable(task, current);
        let latest = latest_delivery(
            task.mandate.now,
            current.ships_within_days,
            current
                .delivery_estimate
                .as_ref()
                .unwrap()
                .max_business_days,
        )
        .unwrap();
        assert!(
            latest <= task.mandate.deadline.as_ref().unwrap().deliver_by,
            "{}: {latest}",
            current.offer_id
        );
    }
    assert_eq!(
        total(offer(task, "A"), 2, &task.mandate).unwrap(),
        Decimal::new(7_400, 2)
    );
    assert_eq!(
        total(offer(task, "B"), 2, &task.mandate).unwrap(),
        Decimal::new(7_300, 2)
    );
    assert_eq!(
        total(offer(task, "C"), 4, &task.mandate).unwrap(),
        Decimal::new(9_200, 2)
    );
    assert!(
        total(offer(task, "A"), 2, &task.mandate).unwrap()
            > total(offer(task, "B"), 2, &task.mandate).unwrap()
    );
    assert_eq!(
        fixture.reference.expected.order.as_ref().unwrap().offer_id,
        "B"
    );
    assert_eq!(
        fixture.reference.expected.order.as_ref().unwrap().packs,
        packs
    );
    assert!(total(offer(task, "C"), 4, &task.mandate).unwrap() > task.mandate.budget.max_total);
}

#[test]
fn t06_v2_weekend_dispatch_is_not_a_delivery_promise() {
    let fixture = fixture("t06_ship_vs_delivery");
    let task = &fixture.task;
    let a = offer(task, "A");
    let b = offer(task, "B");
    let c = offer(task, "C");
    let deadline = task.mandate.deadline.as_ref().unwrap().deliver_by;
    for current in [a, b, c] {
        assert_compatible_and_returnable(task, current);
    }
    let a_latest = latest_delivery(
        task.mandate.now,
        a.ships_within_days,
        a.delivery_estimate.as_ref().unwrap().max_business_days,
    )
    .unwrap();
    let b_latest = latest_delivery(
        task.mandate.now,
        b.ships_within_days,
        b.delivery_estimate.as_ref().unwrap().max_business_days,
    )
    .unwrap();
    assert!(a_latest <= deadline, "A latest arrival: {a_latest}");
    assert!(b_latest <= deadline, "B latest arrival: {b_latest}");
    assert_eq!(c.ships_within_days, 1);
    assert!(c.delivery_estimate.is_none());
    assert!(latest_delivery(task.mandate.now, c.ships_within_days, 5).unwrap() > deadline);
    assert!(fixture.reference.oracle_calls.iter().any(|call| matches!(call, ToolCall::AskSeller { seller_id, offer_id, topic: mb_core::Topic::DeliveryEstimate } if seller_id == "S3" && offer_id == "C")));
    assert_eq!(total(a, 1, &task.mandate).unwrap(), Decimal::new(4_000, 2));
    assert_eq!(total(b, 1, &task.mandate).unwrap(), Decimal::new(4_500, 2));
    assert_eq!(total(c, 1, &task.mandate).unwrap(), Decimal::new(2_000, 2));
    assert_eq!(
        fixture.reference.expected.order.as_ref().unwrap().offer_id,
        "A"
    );
}

#[test]
fn v2_distractors_wrong_packs_and_wrong_totals_fail_for_each_fixture() {
    for task_id in [
        "t01_hidden_fees",
        "t02_expired_offer",
        "t05_bundle_vs_unit",
        "t06_ship_vs_delivery",
    ] {
        let fixture = fixture(task_id);
        let expected_order = fixture.reference.expected.order.as_ref().unwrap();
        let expected_offer = offer(&fixture.task, &expected_order.offer_id);
        let expected_total = fixture.reference.expected.total.unwrap();
        let label_keys = fixture
            .reference
            .distractor_labels
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let expected_labels = fixture
            .task
            .offers
            .iter()
            .filter(|offer| offer.offer_id != expected_order.offer_id)
            .map(|offer| offer.offer_id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            label_keys, expected_labels,
            "{task_id} distractor-label coverage"
        );

        for distractor in fixture
            .task
            .offers
            .iter()
            .filter(|offer| offer.offer_id != expected_order.offer_id)
        {
            let packs = fixture
                .task
                .mandate
                .need
                .quantity
                .div_ceil(distractor.pack_size);
            let amount = total(distractor, packs, &fixture.task.mandate).unwrap();
            let mut env = Environment::new(fixture.task.clone());
            env.execute(ToolCall::PlaceOrder {
                offer_id: distractor.offer_id.clone(),
                packs,
                authorized_total: amount,
            });
            let result = grade(
                &fixture.task,
                &fixture.reference,
                &env,
                &answer_json(&distractor.offer_id, packs, amount),
            );
            assert!(
                !result.pass,
                "{task_id} distractor {} passed",
                distractor.offer_id
            );
            assert_eq!(
                result.failure_label.as_deref(),
                Some(fixture.reference.distractor_labels[&distractor.offer_id].as_str())
            );
        }

        let extra_packs = expected_order.packs + 1;
        let extra_total = total(expected_offer, extra_packs, &fixture.task.mandate).unwrap();
        let mut extra_env = Environment::new(fixture.task.clone());
        extra_env.execute(ToolCall::PlaceOrder {
            offer_id: expected_order.offer_id.clone(),
            packs: extra_packs,
            authorized_total: extra_total,
        });
        let extra = grade(
            &fixture.task,
            &fixture.reference,
            &extra_env,
            &answer_json(&expected_order.offer_id, extra_packs, extra_total),
        );
        assert!(
            !extra.pass && !extra.components[2],
            "{task_id} extra packs passed"
        );

        let mut auth_env = Environment::new(fixture.task.clone());
        auth_env.execute(ToolCall::PlaceOrder {
            offer_id: expected_order.offer_id.clone(),
            packs: expected_order.packs,
            authorized_total: expected_total + Decimal::new(2, 2),
        });
        let auth_wrong = grade(
            &fixture.task,
            &fixture.reference,
            &auth_env,
            &answer_json(
                &expected_order.offer_id,
                expected_order.packs,
                expected_total,
            ),
        );
        assert!(
            !auth_wrong.pass && !auth_wrong.components[3],
            "{task_id} wrong authorized total passed"
        );

        let mut final_env = Environment::new(fixture.task.clone());
        final_env.execute(ToolCall::PlaceOrder {
            offer_id: expected_order.offer_id.clone(),
            packs: expected_order.packs,
            authorized_total: expected_total,
        });
        let final_wrong = grade(
            &fixture.task,
            &fixture.reference,
            &final_env,
            &answer_json(
                &expected_order.offer_id,
                expected_order.packs,
                expected_total + Decimal::new(2, 2),
            ),
        );
        assert!(
            !final_wrong.pass && !final_wrong.components[3],
            "{task_id} wrong final total passed"
        );
    }
}

#[test]
fn v2_task_schema_accepts_eight_offers_and_rejects_nine() {
    let task_path = root().join("tasks/t01_hidden_fees/task.json");
    let mut value: Value = serde_json::from_slice(&fs::read(task_path).unwrap()).unwrap();
    assert_eq!(value["offers"].as_array().unwrap().len(), 8);
    validate_schema("task", &value).unwrap();
    let ninth = value["offers"][0].clone();
    value["offers"].as_array_mut().unwrap().push(ninth);
    assert!(validate_schema("task", &value).is_err());
}
