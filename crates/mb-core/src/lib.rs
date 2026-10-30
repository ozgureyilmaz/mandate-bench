//! Visible task types and shared exact arithmetic. No hidden references are loaded here.

use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Datelike, Days, FixedOffset, Weekday};
pub use rust_decimal::Decimal;
use rust_decimal::RoundingStrategy;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_TOOL_CALLS: usize = 20;
pub const SYSTEM_PROMPT: &str = include_str!("../../../prompts/system_v1.md");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub task_id: String,
    pub task_version: u32,
    pub mandate: Mandate,
    pub sellers: Vec<Seller>,
    pub offers: Vec<Offer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mandate {
    pub mandate_id: String,
    pub now: DateTime<FixedOffset>,
    pub request: String,
    pub need: Need,
    pub budget: Budget,
    pub deadline: Option<Deadline>,
    pub returns: Option<Returns>,
    pub fx: Option<BTreeMap<String, Decimal>>,
    pub authority: Authority,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Need {
    pub description: String,
    pub quantity: u32,
    pub compatibility: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub max_total: Decimal,
    pub currency: String,
    pub includes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deadline {
    pub deliver_by: DateTime<FixedOffset>,
    pub ship_to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Returns {
    pub min_window_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Authority {
    pub max_orders: u32,
    pub choose: String,
    pub if_none_eligible: String,
    pub only_the_user_can_change_this_mandate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seller {
    pub seller_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Offer {
    pub offer_id: String,
    pub seller_id: String,
    pub seller_name: String,
    pub product: Product,
    pub pack_size: u32,
    pub price: Decimal,
    pub currency: String,
    pub fees: Vec<Fee>,
    pub valid_until: DateTime<FixedOffset>,
    pub ships_from: String,
    pub ships_within_days: u64,
    pub delivery_estimate: Option<DeliveryEstimate>,
    pub return_policy: Option<ReturnPolicy>,
    pub terms: String,
    #[serde(default)]
    pub seller_qa: BTreeMap<Topic, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Product {
    pub sku: String,
    pub title: String,
    pub specs: BTreeMap<String, Value>,
    pub compatible_with: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fee {
    #[serde(rename = "type")]
    pub kind: String,
    pub amount: Decimal,
    pub per: FeeBasis,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeeBasis {
    Order,
    Unit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryEstimate {
    pub min_business_days: u32,
    pub max_business_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnPolicy {
    pub returnable: bool,
    pub window_days: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Topic {
    DeliveryEstimate,
    Returns,
    Compatibility,
    Fees,
    Warranty,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "name",
    content = "arguments",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ToolCall {
    RequestQuotes {
        query: String,
    },
    GetOffer {
        offer_id: String,
    },
    AskSeller {
        seller_id: String,
        offer_id: String,
        topic: Topic,
    },
    PlaceOrder {
        offer_id: String,
        packs: u32,
        authorized_total: Decimal,
    },
    AskUser {
        question: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Decision {
    Buy,
    AskUser,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Order {
    pub offer_id: String,
    pub packs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinalAnswer {
    pub decision: Decision,
    pub order: Option<Order>,
    pub total: Option<Decimal>,
    pub currency: String,
    pub reason: String,
}

/// Fees marked `unit` are per purchased pack, as specified by the mandate contract.
pub fn total(offer: &Offer, packs: u32, mandate: &Mandate) -> Result<Decimal> {
    ensure!(packs > 0, "packs must be positive");
    let packs = Decimal::from(packs);
    let mut amount = offer.price.checked_mul(packs).context("price overflow")?;
    for fee in &offer.fees {
        let multiplier = match fee.per {
            FeeBasis::Order => Decimal::ONE,
            FeeBasis::Unit => packs,
        };
        amount = amount
            .checked_add(fee.amount.checked_mul(multiplier).context("fee overflow")?)
            .context("total overflow")?;
    }
    if offer.currency != mandate.budget.currency {
        let pair = format!("{}_{}", offer.currency, mandate.budget.currency);
        let rate = mandate
            .fx
            .as_ref()
            .and_then(|fx| fx.get(&pair))
            .context("missing FX rate")?;
        ensure!(*rate > Decimal::ZERO, "FX rate must be positive");
        amount = amount.checked_mul(*rate).context("FX overflow")?;
    }
    Ok(amount.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero))
}

/// Calendar shipping days, then Mon–Fri transit days; keep the frozen clock's offset/time.
pub fn latest_delivery(
    now: DateTime<FixedOffset>,
    ships_within_days: u64,
    max_business_days: u32,
) -> Result<DateTime<FixedOffset>> {
    let mut date = now
        .checked_add_days(Days::new(ships_within_days))
        .context("shipping date overflow")?;
    let mut remaining = max_business_days;
    while remaining > 0 {
        date = date
            .checked_add_days(Days::new(1))
            .context("delivery date overflow")?;
        if !matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
            remaining -= 1;
        }
    }
    Ok(date)
}

pub fn validate_schema(name: &str, value: &Value) -> Result<()> {
    let source = match name {
        "mandate" => include_str!("../../../schemas/mandate.schema.json"),
        "offer" => include_str!("../../../schemas/offer.schema.json"),
        "task" => include_str!("../../../schemas/task.schema.json"),
        "reference" => include_str!("../../../schemas/reference.schema.json"),
        "final_answer" => include_str!("../../../schemas/final_answer.schema.json"),
        _ => bail!("unknown schema {name}"),
    };
    let schema: Value = serde_json::from_str(source)?;
    let validator = jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)?;
    validator
        .validate(value)
        .map_err(|error| anyhow::anyhow!("{name}: {error}"))
}

pub fn load_task(path: &Path) -> Result<Task> {
    let raw: Value = serde_json::from_str(
        &fs::read_to_string(path).with_context(|| path.display().to_string())?,
    )?;
    validate_schema("task", &raw)?;
    let task: Task = serde_json::from_value(raw)?;
    ensure!(
        task.mandate.authority.max_orders == 1,
        "only one-order mandates are supported"
    );
    let mut offer_ids = std::collections::BTreeSet::new();
    let mut seller_ids = std::collections::BTreeSet::new();
    for seller in &task.sellers {
        ensure!(seller_ids.insert(&seller.seller_id), "duplicate seller ID");
    }
    for offer in &task.offers {
        ensure!(offer_ids.insert(&offer.offer_id), "duplicate offer ID");
        ensure!(
            seller_ids.contains(&offer.seller_id),
            "offer has unknown seller"
        );
        ensure!(
            offer
                .delivery_estimate
                .as_ref()
                .is_none_or(|d| d.min_business_days <= d.max_business_days),
            "invalid delivery range"
        );
    }
    Ok(task)
}

/// Only the mandate is in the initial user prompt; offers are discovered through tools.
pub fn initial_messages(task: &Task) -> Result<Value> {
    Ok(serde_json::json!([
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": serde_json::to_string(&task.mandate)?}
    ]))
}
