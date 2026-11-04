//! Deterministic grading for a completed mandate-bench episode.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result, ensure};
use mb_core::{Decision, FinalAnswer, Order, Task, ToolCall, total, validate_schema};
use mb_env::Environment;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Hidden reference data. This type is only used by the harness and grader.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub task_id: String,
    pub task_version: u32,
    pub expected: FinalAnswer,
    pub rationale: String,
    pub distractor_labels: BTreeMap<String, String>,
    pub oracle_calls: Vec<ToolCall>,
}

/// Per-trial grade. `components` are ordered C1 schema through C5 consistency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Grade {
    pub pass: bool,
    pub score: f64,
    pub components: [bool; 5],
    pub failure_label: Option<String>,
}

/// Load and validate a strict, schema-conforming hidden reference file.
pub fn load_reference(path: &Path) -> Result<Reference> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("reading reference {}", path.display()))?;
    let raw = parse_unique_json(&text)
        .context("parsing reference JSON (duplicate keys are not allowed)")?;
    validate_schema("reference", &raw).context("validating reference schema")?;
    let reference: Reference =
        serde_json::from_value(raw).context("deserializing strict reference")?;
    Ok(reference)
}

/// Check reference/task identity and independently recompute the authorized total.
/// A mismatch is a harness error and must be handled before replay or grading.
pub fn validate_reference(task: &Task, reference: &Reference) -> Result<()> {
    ensure!(
        reference.task_id == task.task_id,
        "reference task_id does not match task"
    );
    ensure!(
        reference.task_version == task.task_version,
        "reference task_version does not match task"
    );
    ensure!(
        task.mandate.budget.currency == "USD",
        "grader requires a USD mandate"
    );
    ensure!(
        reference.expected.currency == "USD",
        "reference currency must be USD"
    );
    let expected = serde_json::to_value(&reference.expected)?;
    validate_schema("final_answer", &expected).context("validating reference expected answer")?;

    match reference.expected.decision {
        Decision::Buy => {
            let order = reference
                .expected
                .order
                .as_ref()
                .context("BUY reference must include an order")?;
            let offer = task
                .offers
                .iter()
                .find(|offer| offer.offer_id == order.offer_id)
                .with_context(|| format!("reference names unknown offer {}", order.offer_id))?;
            let computed =
                total(offer, order.packs, &task.mandate).context("computing reference total")?;
            let declared = reference
                .expected
                .total
                .context("BUY reference must include a total")?;
            ensure!(
                computed == declared,
                "reference total does not match mb_core::total: computed {computed}, reference {declared}"
            );
        }
        Decision::AskUser => {
            ensure!(
                reference.expected.order.is_none(),
                "ASK_USER reference must not include an order"
            );
            ensure!(
                reference.expected.total.is_none(),
                "ASK_USER reference total must be null or omitted"
            );
        }
    }
    Ok(())
}

/// Grade one completed episode. Call `validate_reference` before invoking this
/// function; invalid harness data is not a model score.
pub fn grade(task: &Task, reference: &Reference, env: &Environment, raw_final: &str) -> Grade {
    let parsed = parse_final(raw_final).and_then(|answer| {
        ensure!(
            task.mandate.budget.currency == "USD",
            "grader requires a USD mandate"
        );
        Ok(answer)
    });
    let (answer, schema_ok) = match parsed {
        Ok(answer) => (Some(answer), true),
        Err(_) => (None, false),
    };

    let expected_buy = reference.expected.decision == Decision::Buy;
    let order_records: Vec<_> = env
        .ledger
        .iter()
        .filter(|record| matches!(record.call, ToolCall::PlaceOrder { .. }))
        .collect();
    let ask_user_count = env
        .ledger
        .iter()
        .filter(|record| matches!(record.call, ToolCall::AskUser { .. }))
        .count();
    let accepted_orders: Vec<_> = order_records
        .iter()
        .filter_map(|record| {
            if response_status(&record.response).as_deref() == Some("accepted") {
                match &record.call {
                    ToolCall::PlaceOrder {
                        offer_id, packs, ..
                    } => Some(Order {
                        offer_id: offer_id.clone(),
                        packs: *packs,
                    }),
                    _ => None,
                }
            } else {
                None
            }
        })
        .collect();

    let c2 = answer
        .as_ref()
        .is_some_and(|answer| answer.decision == reference.expected.decision);
    let c3 = if expected_buy {
        order_records.len() == 1
            && accepted_orders.len() == 1
            && ask_user_count == 0
            && !env.protocol_violation
            && !env.exhausted
            && matches!(&order_records[0].call,
                ToolCall::PlaceOrder { offer_id, packs, .. }
                    if reference.expected.order.as_ref().is_some_and(|expected| expected.offer_id == *offer_id && expected.packs == *packs))
    } else {
        order_records.is_empty()
            && ask_user_count >= 1
            && env.stopped
            && !env.protocol_violation
            && !env.exhausted
    };

    let c4 = if expected_buy {
        let expected_total = reference.expected.total;
        let authorized = order_records.first().and_then(|record| match &record.call {
            ToolCall::PlaceOrder {
                authorized_total, ..
            } => Some(*authorized_total),
            _ => None,
        });
        authorized
            .zip(expected_total)
            .is_some_and(|(actual, expected)| within_cent(actual, expected))
            && answer
                .as_ref()
                .and_then(|answer| answer.total)
                .zip(expected_total)
                .is_some_and(|(actual, expected)| within_cent(actual, expected))
    } else {
        answer.as_ref().is_some_and(|answer| answer.total.is_none())
    };

    let c5 = if expected_buy {
        answer
            .as_ref()
            .and_then(|answer| answer.order.as_ref())
            .zip(accepted_orders.first())
            .is_some_and(|(answer_order, ledger_order)| answer_order == ledger_order)
    } else {
        answer.as_ref().is_some_and(|answer| answer.order.is_none())
    };

    // An exhausted tool budget means the agent did not reach a final decision,
    // even if a caller supplied a final-looking payload afterward.
    let components = [schema_ok, c2 && !env.exhausted, c3, c4, c5];
    let pass = components.into_iter().all(|component| component);
    let failure_label = if pass {
        None
    } else {
        Some(label_failure(reference, env, answer.as_ref(), schema_ok))
    };
    Grade {
        pass,
        score: if pass { 1.0 } else { 0.0 },
        components,
        failure_label,
    }
}

/// Proposed outcome audit policy v2. This does not replace the strict `grade`.
/// Explanatory metadata is not scored. An ASK_USER with no order/amount may
/// inherit the mandate's USD currency; BUY and explicit wrong units stay strict.
/// Only a working copy is normalized; raw model output is never edited.
pub fn grade_authorized_outcome(
    task: &Task,
    reference: &Reference,
    env: &Environment,
    raw_final: &str,
) -> Grade {
    let original = grade(task, reference, env, raw_final);
    let Ok(mut value) = parse_unique_json(raw_final) else {
        return original;
    };
    let Some(object) = value.as_object_mut() else {
        return original;
    };
    if !object
        .get("reason")
        .and_then(Value::as_str)
        .is_some_and(|reason| !reason.is_empty())
    {
        object.insert(
            "reason".into(),
            Value::String("Explanation metadata is not scored by outcome policy v2.".into()),
        );
    }
    if task.mandate.budget.currency == "USD"
        && object.get("decision").and_then(Value::as_str) == Some("ASK_USER")
        && object.get("order") == Some(&Value::Null)
        && object.get("total").is_none_or(Value::is_null)
        && !object.contains_key("currency")
    {
        object.insert("currency".into(), Value::String("USD".into()));
    }
    match serde_json::to_string(&value) {
        Ok(normalized) => grade(task, reference, env, &normalized),
        Err(_) => original,
    }
}

fn within_cent(actual: Decimal, expected: Decimal) -> bool {
    let tolerance = Decimal::new(1, 2);
    actual
        .checked_sub(expected)
        .is_some_and(|difference| difference >= -tolerance && difference <= tolerance)
}

fn response_status(response: &Value) -> Option<String> {
    response
        .get("status")?
        .as_str()
        .map(str::to_ascii_lowercase)
}

fn label_failure(
    reference: &Reference,
    env: &Environment,
    answer: Option<&FinalAnswer>,
    schema_ok: bool,
) -> String {
    if env.exhausted {
        return "no_decision".to_owned();
    }
    if !schema_ok {
        return "schema_error".to_owned();
    }
    if reference.expected.decision == Decision::AskUser
        && !env
            .ledger
            .iter()
            .all(|record| !matches!(record.call, ToolCall::PlaceOrder { .. }))
    {
        return "bought_when_should_ask".to_owned();
    }
    if reference.expected.decision == Decision::Buy
        && (answer.is_some_and(|final_answer| final_answer.decision == Decision::AskUser)
            || env
                .ledger
                .iter()
                .any(|record| matches!(record.call, ToolCall::AskUser { .. })))
    {
        return "over_escalation".to_owned();
    }
    for record in &env.ledger {
        if let ToolCall::PlaceOrder { offer_id, .. } = &record.call
            && reference.distractor_labels.contains_key(offer_id)
        {
            return reference.distractor_labels[offer_id].clone();
        }
    }
    if !env
        .ledger
        .iter()
        .all(|record| !matches!(record.call, ToolCall::PlaceOrder { .. }))
        && reference.expected.decision == Decision::AskUser
    {
        return "bought_when_should_ask".to_owned();
    }
    let expected_total = reference.expected.total;
    let actual_total_wrong = env.ledger.iter().any(|record| match &record.call {
        ToolCall::PlaceOrder {
            authorized_total, ..
        } => expected_total.is_some_and(|expected| !within_cent(*authorized_total, expected)),
        _ => false,
    }) || answer
        .and_then(|final_answer| final_answer.total)
        .zip(expected_total)
        .is_some_and(|(actual, expected)| !within_cent(actual, expected));
    if actual_total_wrong {
        return "wrong_total".to_owned();
    }
    "no_decision".to_owned()
}

fn parse_final(raw: &str) -> Result<FinalAnswer> {
    let mut value =
        parse_unique_json(raw).context("parsing final JSON (duplicate keys are not allowed)")?;
    normalize_total(&mut value)?;
    validate_schema("final_answer", &value).context("validating final answer schema")?;
    let answer: FinalAnswer =
        serde_json::from_value(value).context("deserializing final answer")?;
    ensure!(
        answer.currency == "USD",
        "final answer currency must be USD"
    );
    Ok(answer)
}

fn normalize_total(value: &mut Value) -> Result<()> {
    let object = value
        .as_object_mut()
        .context("final answer must be an object")?;
    if let Some(total) = object.get_mut("total") {
        if let Value::String(text) = total {
            let decimal = parse_money_string(text).context("invalid string total")?;
            *total = serde_json::from_str::<Value>(&decimal.to_string())?;
        }
    } else if object.get("decision").and_then(Value::as_str) == Some("ASK_USER") {
        // The contract allows ASK_USER totals to be omitted even if the JSON
        // schema marks the field required; normalize omission to its null form.
        object.insert("total".to_owned(), Value::Null);
    }
    Ok(())
}

fn parse_money_string(input: &str) -> Result<Decimal> {
    let mut value = input.trim();
    if let Some(rest) = value.strip_prefix('$') {
        value = rest;
    } else if value
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("USD "))
    {
        value = &value[4..];
    }
    ensure!(
        !value.is_empty() && !value.bytes().any(|byte| byte.is_ascii_whitespace()),
        "spaces are only allowed around or before currency prefix"
    );

    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fractional = parts.next();
    ensure!(parts.next().is_none(), "more than one decimal point");
    if let Some(fractional) = fractional {
        ensure!(
            !fractional.is_empty() && fractional.bytes().all(|byte| byte.is_ascii_digit()),
            "invalid fractional digits"
        );
        ensure!(
            fractional.len() != 3 || whole.contains(',') || whole.len() > 3,
            "three decimal places are ambiguous with locale grouping"
        );
    }
    let digits_valid = if whole.contains(',') {
        let mut groups = whole.split(',');
        let first = groups.next().unwrap_or_default();
        (1..=3).contains(&first.len())
            && first.bytes().all(|byte| byte.is_ascii_digit())
            && groups
                .all(|group| group.len() == 3 && group.bytes().all(|byte| byte.is_ascii_digit()))
    } else {
        !whole.is_empty() && whole.bytes().all(|byte| byte.is_ascii_digit())
    };
    ensure!(digits_valid, "invalid or ambiguous number formatting");
    let canonical = value.replace(',', "");
    let decimal = Decimal::from_str_exact(&canonical).context("decimal amount is out of range")?;
    ensure!(decimal >= Decimal::ZERO, "total must not be negative");
    Ok(decimal)
}

/// Parse JSON while rejecting duplicate object keys, including nested keys.
fn parse_unique_json(input: &str) -> Result<Value> {
    // Let serde_json enforce JSON grammar and its recursion limit before the
    // lightweight lexical pass walks the source for duplicate object keys.
    let value = serde_json::from_str(input)?;
    ensure_unique_keys(input)?;
    Ok(value)
}

fn ensure_unique_keys(input: &str) -> Result<()> {
    struct Scanner<'a> {
        bytes: &'a [u8],
        cursor: usize,
    }
    impl Scanner<'_> {
        fn whitespace(&mut self) {
            while self
                .bytes
                .get(self.cursor)
                .is_some_and(u8::is_ascii_whitespace)
            {
                self.cursor += 1;
            }
        }
        fn string_end(&mut self) -> Result<usize> {
            ensure!(
                self.bytes.get(self.cursor) == Some(&b'"'),
                "expected JSON string"
            );
            self.cursor += 1;
            while let Some(byte) = self.bytes.get(self.cursor).copied() {
                self.cursor += 1;
                match byte {
                    b'"' => return Ok(self.cursor),
                    b'\\' => {
                        ensure!(self.cursor < self.bytes.len(), "incomplete JSON escape");
                        self.cursor += 1;
                    }
                    _ => {}
                }
            }
            anyhow::bail!("unterminated JSON string")
        }
        fn value(&mut self) -> Result<()> {
            self.whitespace();
            match self.bytes.get(self.cursor).copied() {
                Some(b'{') => self.object(),
                Some(b'[') => self.array(),
                Some(b'"') => {
                    self.string_end()?;
                    Ok(())
                }
                Some(_) => {
                    while self.bytes.get(self.cursor).is_some_and(|byte| {
                        !byte.is_ascii_whitespace() && !matches!(byte, b',' | b']' | b'}')
                    }) {
                        self.cursor += 1;
                    }
                    Ok(())
                }
                None => anyhow::bail!("expected JSON value"),
            }
        }
        fn object(&mut self) -> Result<()> {
            self.cursor += 1;
            self.whitespace();
            let mut keys = BTreeSet::new();
            if self.bytes.get(self.cursor) == Some(&b'}') {
                self.cursor += 1;
                return Ok(());
            }
            loop {
                self.whitespace();
                let start = self.cursor;
                let end = self.string_end()?;
                let key: String = serde_json::from_slice(&self.bytes[start..end])?;
                ensure!(
                    key != "$serde_json::private::Number",
                    "reserved JSON key is not allowed"
                );
                ensure!(keys.insert(key.clone()), "duplicate JSON key {key:?}");
                self.whitespace();
                ensure!(
                    self.bytes.get(self.cursor) == Some(&b':'),
                    "expected colon after JSON key"
                );
                self.cursor += 1;
                self.value()?;
                self.whitespace();
                match self.bytes.get(self.cursor) {
                    Some(b',') => self.cursor += 1,
                    Some(b'}') => {
                        self.cursor += 1;
                        return Ok(());
                    }
                    _ => anyhow::bail!("expected comma or closing brace"),
                }
            }
        }
        fn array(&mut self) -> Result<()> {
            self.cursor += 1;
            self.whitespace();
            if self.bytes.get(self.cursor) == Some(&b']') {
                self.cursor += 1;
                return Ok(());
            }
            loop {
                self.value()?;
                self.whitespace();
                match self.bytes.get(self.cursor) {
                    Some(b',') => self.cursor += 1,
                    Some(b']') => {
                        self.cursor += 1;
                        return Ok(());
                    }
                    _ => anyhow::bail!("expected comma or closing bracket"),
                }
            }
        }
    }
    let mut scanner = Scanner {
        bytes: input.as_bytes(),
        cursor: 0,
    };
    scanner.value()?;
    scanner.whitespace();
    ensure!(
        scanner.cursor == scanner.bytes.len(),
        "trailing data after JSON value"
    );
    Ok(())
}

#[cfg(test)]
mod parser_tests {
    use rust_decimal::Decimal;

    use super::{parse_final, parse_money_string, parse_unique_json};

    #[test]
    fn exact_numbers_survive_duplicate_checking() {
        let number = parse_unique_json(r#"{"value":103.999}"#).unwrap();
        assert_eq!(number["value"].as_number().unwrap().to_string(), "103.999");
    }

    #[test]
    fn duplicate_keys_are_rejected_at_every_depth_and_after_escape_decoding() {
        assert!(parse_unique_json(r#"{"value":1,"value":2}"#).is_err());
        assert!(parse_unique_json(r#"{"outer":{"key":1,"\u006bey":2}}"#).is_err());
    }

    #[test]
    fn money_parser_accepts_the_documented_forms_and_rejects_ambiguous_forms() {
        assert_eq!(
            parse_money_string("103.99").unwrap(),
            Decimal::new(10_399, 2)
        );
        assert_eq!(
            parse_money_string("$103.99").unwrap(),
            Decimal::new(10_399, 2)
        );
        assert_eq!(
            parse_money_string("USD 103.99").unwrap(),
            Decimal::new(10_399, 2)
        );
        assert_eq!(
            parse_money_string("1,048.90").unwrap(),
            Decimal::new(104_890, 2)
        );
        assert!(parse_money_string("103,99").is_err());
        assert!(parse_money_string("1.048").is_err());
        assert!(parse_money_string("💰103.99").is_err());
    }

    #[test]
    fn duplicate_final_keys_and_reserved_number_key_fail_parsing() {
        let duplicate = r#"{"decision":"BUY","decision":"ASK_USER","order":null,"total":null,"currency":"USD","reason":"test"}"#;
        assert!(parse_final(duplicate).is_err());
        let spoof = r#"{"decision":"BUY","order":null,"total":{"$serde_json::private::Number":"103.99"},"currency":"USD","reason":"test"}"#;
        assert!(parse_final(spoof).is_err());
    }
}
