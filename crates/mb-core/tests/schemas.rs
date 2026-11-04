use std::{fs, path::Path};

use mb_core::validate_schema;
use serde_json::{Value, json};

fn fixture(path: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path);
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn task_schema_rejects_hidden_fields_string_money_and_unfrozen_dates() {
    let task = fixture("tasks/t01_hidden_fees/task.json");
    validate_schema("task", &task).unwrap();

    for (pointer, invalid) in [
        ("/mandate/budget/max_total", json!("150.00")),
        ("/mandate/budget/currency", json!("EUR")),
        ("/mandate/now", json!("2026-11-03T10:00:00")),
        ("/offers/0/pack_size", json!(0)),
        ("/offers/0/fees/0/amount", json!(-1)),
    ] {
        let mut changed = task.clone();
        *changed.pointer_mut(pointer).unwrap() = invalid;
        assert!(
            validate_schema("task", &changed).is_err(),
            "accepted invalid {pointer}"
        );
    }
    let mut contaminated = task;
    contaminated["expected"] = json!({"decision": "BUY"});
    assert!(validate_schema("task", &contaminated).is_err());
}

#[test]
fn reference_schema_rejects_invalid_oracle_arguments_and_answer_shapes() {
    let reference = fixture("reference/t01_hidden_fees.json");
    validate_schema("reference", &reference).unwrap();
    let mut wrong_quantity = reference.clone();
    wrong_quantity["oracle_calls"][4]["arguments"]["packs"] = json!(0);
    assert!(validate_schema("reference", &wrong_quantity).is_err());
    let mut extra_argument = reference.clone();
    extra_argument["oracle_calls"][4]["arguments"]["confirmed"] = json!(true);
    assert!(validate_schema("reference", &extra_argument).is_err());
    let mut ambiguous_answer = reference;
    ambiguous_answer["expected"]["decision"] = json!("ASK_USER");
    assert!(validate_schema("reference", &ambiguous_answer).is_err());
}

#[test]
fn all_schema_references_are_internal_so_validation_is_offline() {
    fn check(value: &Value) {
        match value {
            Value::Object(object) => {
                if let Some(reference) = object.get("$ref") {
                    assert!(reference.as_str().unwrap().starts_with('#'));
                }
                for child in object.values() {
                    check(child);
                }
            }
            Value::Array(values) => {
                for child in values {
                    check(child);
                }
            }
            _ => {}
        }
    }
    for name in ["mandate", "offer", "task", "reference", "final_answer"] {
        check(&fixture(&format!("schemas/{name}.schema.json")));
    }
}
