use std::{collections::BTreeMap, path::Path, str::FromStr};

use mb_core::{Decimal, Fee, FeeBasis, latest_delivery, load_task, total};

fn decimal(value: &str) -> Decimal {
    Decimal::from_str(value).unwrap()
}

#[test]
fn fees_are_per_order_or_pack_and_round_only_after_fx() {
    let task = load_task(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tasks/t01_hidden_fees/task.json"),
    )
    .unwrap();
    let mut mandate = task.mandate;
    let mut offer = task.offers[0].clone();
    offer.price = decimal("1.004");
    offer.currency = "EUR".into();
    offer.fees = vec![
        Fee {
            kind: "handling".into(),
            amount: decimal("0.001"),
            per: FeeBasis::Unit,
        },
        Fee {
            kind: "shipping".into(),
            amount: decimal("0.005"),
            per: FeeBasis::Order,
        },
    ];
    mandate.fx = Some(BTreeMap::from([("EUR_USD".into(), decimal("1.08"))]));
    assert_eq!(total(&offer, 3, &mandate).unwrap(), decimal("3.26"));
    offer.currency = "USD".into();
    offer.price = decimal("1.005");
    offer.fees.clear();
    assert_eq!(total(&offer, 1, &mandate).unwrap(), decimal("1.01"));
    assert!(total(&offer, 0, &mandate).is_err());
    offer.currency = "GBP".into();
    assert!(total(&offer, 1, &mandate).is_err());
}

#[test]
fn delivery_uses_calendar_shipping_then_weekday_transit_at_frozen_offset() {
    let now = "2026-10-30T10:00:00-05:00".parse().unwrap(); // Friday
    assert_eq!(
        latest_delivery(now, 1, 2).unwrap().to_rfc3339(),
        "2026-11-03T10:00:00-05:00"
    );
    assert_eq!(latest_delivery(now, 0, 0).unwrap(), now);
    let same_instant: chrono::DateTime<chrono::FixedOffset> =
        "2026-10-30T15:00:00+00:00".parse().unwrap();
    assert_eq!(now, same_instant);
}

#[test]
fn amounts_are_json_numbers_without_binary_float_round_trip() {
    let value = decimal("103.999");
    assert_eq!(serde_json::to_string(&value).unwrap(), "103.999");
    assert_eq!(serde_json::from_str::<Decimal>("103.999").unwrap(), value);
}
