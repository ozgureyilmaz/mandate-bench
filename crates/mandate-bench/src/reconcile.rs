use std::{fs, path::Path};

use anyhow::{Context, Result, ensure};
use mb_llm::{
    ApiError, BudgetConfig, BudgetGuard, BudgetReconciliationEvidence, BudgetReservationState,
    WireApi,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};

const RUN_ID: &str = "smoke-20261105T121126.314341000Z-49545";
const SOURCE: &str = "eb4310b4430b8905c5d86c2dab95f5df8572526d";

/// The pinned source emitted a named tool choice, which Meta rejects during
/// validation. This migration applies only to the reviewed historical attempt.
pub fn muse_smoke(root: &Path) -> Result<()> {
    let runs = root.join("runs");
    let run = runs.join(RUN_ID);
    let meta: Value = serde_json::from_slice(&fs::read(run.join("run_meta.json"))?)
        .context("reading original Muse smoke metadata")?;
    let transcript: Value =
        serde_json::from_slice(&fs::read(run.join("transcripts/model-0.json"))?)
            .context("reading original Muse smoke transcript")?;
    let error: ApiError = serde_json::from_value(transcript["api_error"].clone())
        .context("reading original Muse validation error")?;
    let output: Value = serde_json::from_slice(&fs::read(run.join("outputs/model-0.json"))?)?;
    let record = &error.record;
    ensure!(
        meta["run_id"] == RUN_ID
            && meta["git_commit"] == SOURCE
            && meta["git_dirty"] == false
            && meta["kind"] == "smoke"
            && meta["state"] == "failed"
            && meta["source"] == "provider_api"
            && meta["providers"]["meta"]["base_url"] == "https://api.meta.ai/v1"
            && meta["http_attempts"] == 1
            && meta["requests"] == json!([record])
            && output.is_null(),
        "evidence does not match the reviewed Muse smoke"
    );
    ensure!(
        error.kind == "http_status"
            && error.error_type.as_deref() == Some("invalid_request_error")
            && record.provider == "meta"
            && record.requested_model == "muse-spark-1.1"
            && record.api == WireApi::Responses
            && record.http_status == Some(400)
            && record.retries == 0
            && record.model.is_none()
            && record.attempts.len() == 1
            && record.attempts[0].http_status == Some(400)
            && record.usage.input.is_none()
            && record.usage.cached_input.is_none()
            && record.usage.output.is_none()
            && record.usage.reasoning.is_none()
            && record.budget.as_ref().is_some_and(|budget| {
                budget.reservation_id == Some(1)
                    && budget.state == BudgetReservationState::Unknown
                    && budget.charged_usd.is_none()
                    && budget.reserved_usd == Decimal::new(1319424, 6)
            }),
        "record does not prove the reviewed pre-generation rejection"
    );
    let ledger = runs.join("muse-spend-ledger.json");
    ensure!(ledger.is_file(), "existing Muse spend ledger is required");
    let guard = BudgetGuard::open_for_provider(
        ledger,
        "meta",
        &BudgetConfig {
            ceiling_usd: Decimal::from(15),
            initial_charge_usd: Decimal::ZERO,
        },
    )?;
    let evidence = BudgetReconciliationEvidence {
        provider: "meta".into(),
        api: WireApi::Responses,
        http_status: 400,
        generation_started: false,
        usage_reported: false,
    };
    let event = guard.reconcile_unbillable_validation_error(1, RUN_ID, &evidence)?;
    println!("{}", serde_json::to_string_pretty(&event)?);
    Ok(())
}
