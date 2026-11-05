//! Durable, provider-specific USD admission guard for owner-authorized paid calls.

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, bail, ensure};
use chrono::{SecondsFormat, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::{
    BudgetConfig, BudgetEvidence, BudgetReconciliationEvidence, BudgetReconciliationReason,
    BudgetReconciliationRecord, BudgetReservationState, BudgetSnapshot, ModelConfig, TokenUsage,
    WireApi,
};

pub const USD_BUDGET_CEILING: Decimal = Decimal::ONE;
pub const META_MUSE_BUDGET_CEILING_USD: Decimal = Decimal::from_parts(15, 0, 0, false, 0);
pub const MAX_INPUT_CONTEXT_TOKENS: u64 = 1_050_000;
pub const META_MAX_INPUT_CONTEXT_TOKENS: u64 = 1_048_576;
pub const MAX_BUDGETED_BODY_BYTES: usize = 272_000;
const OPENAI_INPUT_PER_MILLION: Decimal = Decimal::from_parts(50, 0, 0, false, 2);
const OPENAI_OUTPUT_PER_MILLION: Decimal = Decimal::from_parts(75, 0, 0, false, 2);
const META_INPUT_PER_MILLION: Decimal = Decimal::from_parts(125, 0, 0, false, 2);
const META_OUTPUT_PER_MILLION: Decimal = Decimal::from_parts(425, 0, 0, false, 2);
const TOKENS_PER_MILLION: u64 = 1_000_000;
const LEDGER_VERSION: u32 = 1;

#[derive(Clone, Copy)]
struct PinnedRates {
    input_per_million: Decimal,
    output_per_million: Decimal,
    input_context_tokens: u64,
}

/// Validate local history against the restrictions for a specific budgeted provider.
/// Meta may replay its encrypted reasoning items; OpenAI budget mode remains text-only.
pub(crate) fn validate_budgeted_history_for_provider(
    provider: &str,
    history: &[Value],
) -> Result<()> {
    ensure!(
        matches!(provider, "openai" | "meta"),
        "provider has no supported budgeted history contract"
    );
    ensure!(
        !history.is_empty(),
        "budgeted request history must not be empty"
    );
    for item in history {
        validate_history_item_for_provider(item, provider)?;
    }
    Ok(())
}

fn validate_history_item_for_provider(item: &Value, provider: &str) -> Result<()> {
    let object = item
        .as_object()
        .context("budgeted history item must be a JSON object")?;
    if provider == "meta" && object.get("type").and_then(Value::as_str) == Some("reasoning") {
        return validate_meta_reasoning_item(object);
    }
    reject_opaque_fields(item)?;
    match object.get("type").and_then(Value::as_str) {
        Some("function_call") => {
            ensure!(
                object.get("name").and_then(Value::as_str).is_some()
                    && object.get("arguments").and_then(Value::as_str).is_some(),
                "budgeted function call history must contain text name and arguments"
            );
        }
        Some("function_call_output") => {
            ensure!(
                object.get("output").and_then(Value::as_str).is_some(),
                "budgeted function output history must contain text output"
            );
        }
        Some("message") => {
            ensure!(
                object.get("role").and_then(Value::as_str) == Some("assistant"),
                "budgeted Responses message history must be assistant text"
            );
            validate_text_content(object.get("content"))?;
        }
        Some(_) => bail!("budgeted history contains a non-text Responses item"),
        None => {
            let role = object.get("role").and_then(Value::as_str);
            ensure!(
                matches!(role, Some("system" | "user" | "assistant")),
                "budgeted history role must be system, user or assistant"
            );
            validate_text_content(object.get("content"))?;
        }
    }
    Ok(())
}

fn validate_meta_reasoning_item(object: &serde_json::Map<String, Value>) -> Result<()> {
    const ALLOWED_FIELDS: &[&str] = &["type", "id", "encrypted_content", "summary", "status"];
    ensure!(
        object
            .keys()
            .all(|key| ALLOWED_FIELDS.contains(&key.as_str())),
        "Meta reasoning item contains unsupported fields"
    );
    if let Some(status) = object.get("status") {
        ensure!(
            status.as_str() == Some("completed"),
            "Meta replay requires a completed reasoning item"
        );
    }
    ensure!(
        object
            .get("encrypted_content")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "Meta reasoning item must contain encrypted_content"
    );
    if let Some(id) = object.get("id") {
        ensure!(
            id.as_str()
                .is_some_and(|value| value.starts_with("rs_") && value.len() > 3),
            "Meta reasoning item id must be an rs_ identifier"
        );
    }
    let summary = object
        .get("summary")
        .and_then(Value::as_array)
        .context("Meta reasoning item must contain a summary array")?;
    for entry in summary {
        let entry = entry
            .as_object()
            .context("Meta reasoning summary entries must be objects")?;
        ensure!(
            entry.get("type").and_then(Value::as_str) == Some("summary_text")
                && entry.get("text").and_then(Value::as_str).is_some(),
            "Meta reasoning summary entries must contain summary text"
        );
        ensure!(
            entry
                .keys()
                .all(|key| matches!(key.as_str(), "type" | "text")),
            "Meta reasoning summary contains unsupported fields"
        );
    }
    Ok(())
}

fn validate_text_content(content: Option<&Value>) -> Result<()> {
    match content {
        Some(Value::String(_)) | Some(Value::Null) => Ok(()),
        Some(Value::Array(parts)) => {
            for part in parts {
                let object = part
                    .as_object()
                    .context("budgeted message content must contain text parts")?;
                let kind = object.get("type").and_then(Value::as_str);
                ensure!(
                    matches!(
                        kind,
                        Some("input_text" | "output_text" | "text" | "refusal")
                    ) && object.get("text").and_then(Value::as_str).is_some(),
                    "budgeted message content contains a non-text part"
                );
                reject_opaque_fields(part)?;
            }
            Ok(())
        }
        _ => bail!("budgeted message content must be text, text parts or null"),
    }
}

fn reject_opaque_fields(value: &Value) -> Result<()> {
    const OPAQUE_FIELDS: &[&str] = &[
        "encrypted_content",
        "reasoning_content",
        "previous_response_id",
        "conversation_id",
        "image",
        "image_url",
        "input_image",
        "audio",
        "input_audio",
        "file",
        "file_id",
        "input_file",
        "video",
        "input_video",
        "url",
        "uri",
    ];
    match value {
        Value::Object(object) => {
            for key in object.keys() {
                ensure!(
                    !OPAQUE_FIELDS.contains(&key.as_str()),
                    "budgeted history contains opaque or remotely referenced content"
                );
            }
            for value in object.values() {
                reject_opaque_fields(value)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                reject_opaque_fields(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Durable state shared by every budgeted client/run that uses the same ledger path.
pub struct BudgetGuard {
    ledger_path: PathBuf,
    lock_path: PathBuf,
    provider: String,
    config: BudgetConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BudgetLedger {
    schema_version: u32,
    #[serde(default = "default_ledger_provider")]
    provider: String,
    currency: String,
    ceiling_usd: Decimal,
    initial_charge_usd: Decimal,
    next_reservation_id: u64,
    halted: bool,
    entries: Vec<LedgerEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    reconciliation_audit: Vec<BudgetReconciliationRecord>,
}

fn default_ledger_provider() -> String {
    "openai".to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LedgerEntry {
    id: u64,
    provider: String,
    requested_model: String,
    state: BudgetReservationState,
    reserved_usd: Decimal,
    charged_usd: Option<Decimal>,
    input_token_bound: Option<u64>,
    output_token_bound: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
pub struct BudgetReservation {
    pub id: u64,
    pub input_token_bound: u64,
    pub output_token_bound: u64,
    pub reserved_usd: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReconcileResult {
    Settled,
    Unknown,
    Overrun,
}

impl BudgetGuard {
    /// Open or initialize the ignored persistent ledger. Existing ledgers are never reset.
    pub fn open(path: impl AsRef<Path>, config: &BudgetConfig) -> Result<Arc<Self>> {
        Self::open_for_provider(path, "openai", config)
    }

    /// Open the provider's durable ledger. Each provider must use a distinct path.
    pub fn open_for_provider(
        path: impl AsRef<Path>,
        provider: &str,
        config: &BudgetConfig,
    ) -> Result<Arc<Self>> {
        let rates = pinned_rates(provider)?;
        ensure!(
            config.ceiling_usd == ceiling_for_provider(provider)?,
            "budget ceiling does not match the fixed provider ceiling"
        );
        ensure!(
            config.initial_charge_usd >= Decimal::ZERO
                && config.initial_charge_usd <= config.ceiling_usd,
            "initial historical exposure must be between zero and the provider budget ceiling"
        );

        let ledger_path = path.as_ref().to_path_buf();
        let parent = ledger_path
            .parent()
            .context("budget ledger path must have a parent directory")?;
        let parent_metadata = fs::symlink_metadata(parent)
            .context("budget ledger parent directory must already exist")?;
        ensure!(
            parent_metadata.file_type().is_dir(),
            "budget ledger parent must be a real directory"
        );
        let lock_path = ledger_path.with_file_name(format!(
            "{}.lock",
            ledger_path
                .file_name()
                .and_then(|name| name.to_str())
                .context("budget ledger file name must be valid UTF-8")?
        ));
        let guard = Arc::new(Self {
            ledger_path,
            lock_path,
            provider: provider.to_owned(),
            config: config.clone(),
        });
        ensure!(
            rates.input_context_tokens > 0,
            "provider context limit is invalid"
        );
        guard.ensure_initialized()?;
        Ok(guard)
    }

    /// Current durable ledger snapshot for run metadata.
    pub fn snapshot(&self) -> Result<BudgetSnapshot> {
        self.with_read_ledger(snapshot)
    }

    /// Conservative estimate for a single request at the provider's full input cap.
    pub fn projected_request_cost(&self, model: &ModelConfig) -> Result<Decimal> {
        validate_budgeted_model(&self.provider, model)?;
        self.projected_usage_cost(
            pinned_rates(&self.provider)?.input_context_tokens,
            u64::from(model.max_output_tokens),
        )
    }

    /// Cost projection for a caller-supplied input/output token envelope.
    pub fn projected_usage_cost(&self, input_tokens: u64, output_tokens: u64) -> Result<Decimal> {
        usage_cost(pinned_rates(&self.provider)?, input_tokens, output_tokens)
    }

    /// Verify a proposed batch projection against current durable exposure without writing it.
    pub fn ensure_batch_fits(&self, projected: Decimal) -> Result<()> {
        ensure!(
            projected >= Decimal::ZERO,
            "batch projection must be nonnegative"
        );
        self.with_read_ledger(|ledger| {
            let current = snapshot(ledger)?;
            ensure!(
                !current.halted,
                "paid budget ledger is halted after an unresolved provider error"
            );
            ensure!(
                projected <= current.remaining_usd,
                "projected batch cost exceeds remaining USD budget"
            );
            Ok(())
        })
    }

    /// Admit one outbound attempt only after its full-context worst-case amount is persisted.
    pub fn reserve(&self, model: &ModelConfig, body_bytes: usize) -> Result<BudgetReservation> {
        ensure!(
            body_bytes <= MAX_BUDGETED_BODY_BYTES,
            "budgeted request exceeds the local request-body size limit"
        );
        validate_budgeted_model(&self.provider, model)?;
        let input_bound = pinned_rates(&self.provider)?.input_context_tokens;
        let output_bound = u64::from(model.max_output_tokens);
        let reserved_usd = worst_case_cost_for_provider(&self.provider, input_bound, output_bound)?;

        self.with_ledger(|ledger| {
            ensure!(
                !ledger.halted,
                "paid budget ledger is halted after an unresolved provider error"
            );
            let current = snapshot(ledger)?;
            ensure!(
                reserved_usd <= current.remaining_usd,
                "worst-case request reservation exceeds remaining USD budget"
            );
            let id = ledger.next_reservation_id;
            ledger.next_reservation_id = id
                .checked_add(1)
                .context("budget reservation ID overflow")?;
            ledger.entries.push(LedgerEntry {
                id,
                provider: model.provider.clone(),
                requested_model: model.id.clone(),
                state: BudgetReservationState::Reserved,
                reserved_usd,
                charged_usd: None,
                input_token_bound: Some(input_bound),
                output_token_bound: Some(output_bound),
            });
            Ok(BudgetReservation {
                id,
                input_token_bound: input_bound,
                output_token_bound: output_bound,
                reserved_usd,
            })
        })
    }

    /// Resolve a successful HTTP response. Missing usage keeps the entire reservation held.
    pub(crate) fn reconcile(
        &self,
        reservation_id: u64,
        usage: &TokenUsage,
        halt_after_response_error: bool,
    ) -> Result<(BudgetEvidence, ReconcileResult)> {
        self.with_ledger(|ledger| {
            let (budget_evidence, outcome, halt) = {
                let entry = reservation_mut(ledger, reservation_id)?;
                ensure!(
                    entry.state == BudgetReservationState::Reserved,
                    "budget reservation is not pending"
                );
                match (usage.input, usage.output) {
                    (Some(input), Some(output)) => {
                        let cached = usage.cached_input.unwrap_or(0);
                        let reasoning_inconsistent =
                            usage.reasoning.is_some_and(|reasoning| reasoning > output);
                        if cached > input || reasoning_inconsistent {
                            entry.state = BudgetReservationState::Unknown;
                            let ev = evidence(entry);
                            (ev, ReconcileResult::Overrun, true)
                        } else {
                            let actual = conservative_usage_cost(&self.provider, usage)
                                .context("could not reconcile usage at conservative prices")?;
                            let exceeds_token_bound = input > entry.input_token_bound.unwrap_or(0)
                                || output > entry.output_token_bound.unwrap_or(0);
                            if exceeds_token_bound || actual > entry.reserved_usd {
                                entry.state = BudgetReservationState::Overrun;
                                entry.charged_usd = Some(actual.max(entry.reserved_usd));
                                let ev = evidence(entry);
                                (ev, ReconcileResult::Overrun, true)
                            } else {
                                entry.state = BudgetReservationState::Settled;
                                entry.charged_usd = Some(actual);
                                let ev = evidence(entry);
                                (ev, ReconcileResult::Settled, halt_after_response_error)
                            }
                        }
                    }
                    _ => {
                        entry.state = BudgetReservationState::Unknown;
                        let ev = evidence(entry);
                        (ev, ReconcileResult::Unknown, halt_after_response_error)
                    }
                }
            };
            if halt {
                ledger.halted = true;
            }
            Ok((budget_evidence, outcome))
        })
    }

    /// A provider or transport error makes billable usage unknown and halts future paid calls.
    pub(crate) fn mark_provider_error(&self, reservation_id: u64) -> Result<BudgetEvidence> {
        self.with_ledger(|ledger| {
            let evidence = {
                let entry = reservation_mut(ledger, reservation_id)?;
                ensure!(
                    entry.state == BudgetReservationState::Reserved,
                    "budget reservation is not pending"
                );
                entry.state = BudgetReservationState::Unknown;
                evidence(entry)
            };
            ledger.halted = true;
            Ok(evidence)
        })
    }

    /// Release one Meta hold after the caller has reviewed evidence of a pre-generation
    /// validation error with no reported usage. The original ledger entry is left intact;
    /// this appends an auditable adjustment and deliberately does not clear the halt flag.
    pub fn reconcile_unbillable_validation_error(
        &self,
        reservation_id: u64,
        run_id: &str,
        evidence: &BudgetReconciliationEvidence,
    ) -> Result<BudgetReconciliationRecord> {
        validate_unbillable_validation_evidence(&self.provider, evidence)?;
        validate_run_id(run_id)?;

        self.with_ledger(|ledger| {
            ensure!(
                ledger.provider == "meta",
                "unbillable validation-error reconciliation is supported only for Meta"
            );
            ensure!(
                !ledger
                    .reconciliation_audit
                    .iter()
                    .any(|event| event.reservation_id == reservation_id),
                "budget reservation already has an audit reconciliation"
            );
            let before_snapshot = snapshot(ledger)?;
            ensure!(
                before_snapshot.overrun_count == 0,
                "cannot reconcile a hold while the budget ledger contains an overrun"
            );
            let used_before = before_snapshot
                .settled_usd
                .checked_add(before_snapshot.held_usd)
                .context("budget exposure overflow before reconciliation")?;
            ensure!(
                used_before <= ledger.ceiling_usd,
                "cannot reconcile a hold while total budget exposure exceeds its ceiling"
            );
            let before = before_snapshot.remaining_usd;
            let entry = ledger
                .entries
                .iter()
                .find(|entry| entry.id == reservation_id && entry.id != 0)
                .context("budget reservation ID is missing")?;
            ensure!(
                entry.provider == "meta",
                "unbillable validation-error reconciliation target is not a Meta reservation"
            );
            ensure!(
                entry.state == BudgetReservationState::Unknown,
                "only an unknown Meta reservation can be reconciled"
            );
            ensure!(
                entry.charged_usd.is_none(),
                "a charged reservation cannot be reconciled as unbillable"
            );
            let released_usd = entry.reserved_usd;
            ensure!(
                released_usd > Decimal::ZERO,
                "a zero-value reservation cannot be reconciled"
            );
            let after = before
                .checked_add(released_usd)
                .context("budget balance overflow during reconciliation")?;
            ensure!(
                after <= ledger.ceiling_usd,
                "reconciliation would exceed the fixed provider budget ceiling"
            );
            let record = BudgetReconciliationRecord {
                reason: BudgetReconciliationReason::UnbillableValidationError,
                reservation_id,
                run_id: run_id.to_owned(),
                reconciled_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
                released_usd,
                balance_before_usd: before,
                balance_after_usd: after,
                evidence: evidence.clone(),
            };
            ledger.reconciliation_audit.push(record.clone());
            Ok(record)
        })
    }

    pub(crate) fn mark_policy_rejected() -> BudgetEvidence {
        BudgetEvidence {
            reservation_id: None,
            reserved_usd: Decimal::ZERO,
            charged_usd: None,
            state: BudgetReservationState::Rejected,
            input_token_bound: None,
            output_token_bound: None,
        }
    }

    fn ensure_initialized(&self) -> Result<()> {
        let lock_was_present = path_exists(&self.lock_path)?;
        let lock = self.lock()?;
        if path_exists(&self.ledger_path)? {
            let ledger = self.read_ledger()?;
            self.validate_ledger(&ledger)?;
            return Ok(());
        }
        ensure!(
            !lock_was_present,
            "budget ledger is missing while its lock file exists; refusing to reset historical exposure"
        );
        let ledger = BudgetLedger::new(&self.provider, &self.config);
        self.write_ledger(&ledger)?;
        drop(lock);
        Ok(())
    }

    fn lock(&self) -> Result<File> {
        let parent = self
            .lock_path
            .parent()
            .context("budget lock path must have a parent")?;
        let metadata = fs::symlink_metadata(parent).context("budget lock directory is missing")?;
        ensure!(
            metadata.file_type().is_dir(),
            "budget lock parent must be a real directory"
        );
        if let Ok(metadata) = fs::symlink_metadata(&self.lock_path) {
            ensure!(
                metadata.file_type().is_file(),
                "budget lock must be a regular file"
            );
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&self.lock_path)
            .context("opening budget lock")?;
        file.lock()
            .context("acquiring exclusive budget ledger lock")?;
        Ok(file)
    }

    fn read_ledger(&self) -> Result<BudgetLedger> {
        let metadata =
            fs::symlink_metadata(&self.ledger_path).context("budget ledger is missing")?;
        ensure!(
            metadata.file_type().is_file(),
            "budget ledger must be a regular file"
        );
        let bytes = fs::read(&self.ledger_path).context("reading budget ledger")?;
        let ledger: BudgetLedger = serde_json::from_slice(&bytes)
            .context("budget ledger is malformed; refusing to reset it")?;
        self.validate_ledger(&ledger)?;
        Ok(ledger)
    }

    fn validate_ledger(&self, ledger: &BudgetLedger) -> Result<()> {
        ensure!(
            ledger.schema_version == LEDGER_VERSION,
            "unsupported budget ledger version"
        );
        ensure!(
            ledger.provider == self.provider,
            "budget ledger provider differs from the requested provider"
        );
        ensure!(
            ledger.currency == "USD",
            "budget ledger currency is unsupported"
        );
        ensure!(
            ledger.ceiling_usd == self.config.ceiling_usd,
            "budget ledger ceiling differs from configuration; refusing to reset it"
        );
        ensure!(
            ledger.initial_charge_usd == self.config.initial_charge_usd,
            "budget ledger carry-in differs from configuration; reconcile it explicitly before reuse"
        );
        ensure!(
            ledger.next_reservation_id > 0,
            "budget ledger reservation counter is invalid"
        );
        let mut ids = std::collections::BTreeSet::new();
        for entry in &ledger.entries {
            ensure!(
                ids.insert(entry.id),
                "budget ledger contains duplicate reservation IDs"
            );
            ensure!(
                entry.reserved_usd >= Decimal::ZERO,
                "budget ledger contains a negative reservation"
            );
            ensure!(
                entry.charged_usd.is_none_or(|value| value >= Decimal::ZERO),
                "budget ledger contains a negative charge"
            );
            match entry.state {
                BudgetReservationState::HistoricalUnresolved => {
                    ensure!(
                        entry.id == 0,
                        "historical carry-in must use reservation ID zero"
                    );
                    ensure!(
                        ledger.initial_charge_usd > Decimal::ZERO,
                        "unexpected historical carry-in entry"
                    );
                    ensure!(
                        entry.provider == "historical_unresolved",
                        "historical carry-in provider is invalid"
                    );
                    ensure!(
                        entry.requested_model == "unknown",
                        "historical carry-in model is invalid"
                    );
                    ensure!(
                        entry.reserved_usd == ledger.initial_charge_usd,
                        "historical carry-in amount mismatch"
                    );
                    ensure!(
                        entry.charged_usd.is_none(),
                        "historical carry-in cannot be reconciled automatically"
                    );
                    ensure!(
                        entry.input_token_bound.is_none() && entry.output_token_bound.is_none(),
                        "historical carry-in must not claim token usage"
                    );
                }
                BudgetReservationState::Reserved
                | BudgetReservationState::Unknown
                | BudgetReservationState::Settled
                | BudgetReservationState::Overrun => {
                    ensure!(entry.id > 0, "active reservations must use positive IDs");
                    ensure!(
                        entry.provider == self.provider
                            && entry.requested_model == model_id_for_provider(&self.provider)?,
                        "ledger contains an unsupported budgeted model"
                    );
                    let input = entry
                        .input_token_bound
                        .context("reservation is missing its input token bound")?;
                    let output = entry
                        .output_token_bound
                        .context("reservation is missing its output token bound")?;
                    ensure!(
                        input == pinned_rates(&self.provider)?.input_context_tokens && output > 0,
                        "reservation token bounds are invalid"
                    );
                    ensure!(
                        entry.reserved_usd
                            == worst_case_cost_for_provider(&self.provider, input, output)?,
                        "reservation amount does not match its conservative token bounds"
                    );
                    match entry.state {
                        BudgetReservationState::Reserved | BudgetReservationState::Unknown => {
                            ensure!(
                                entry.charged_usd.is_none(),
                                "unresolved reservation cannot have a charge"
                            );
                        }
                        BudgetReservationState::Settled => {
                            ensure!(
                                entry
                                    .charged_usd
                                    .is_some_and(|value| value <= entry.reserved_usd),
                                "settled reservation is missing its charge or exceeds its reservation"
                            );
                        }
                        BudgetReservationState::Overrun => {
                            ensure!(
                                entry
                                    .charged_usd
                                    .is_some_and(|value| value >= entry.reserved_usd),
                                "overrun charge is missing or below its reservation"
                            );
                            ensure!(ledger.halted, "overrun ledger is not halted");
                        }
                        _ => unreachable!("state handled by outer match"),
                    }
                }
                BudgetReservationState::Rejected => {
                    bail!("rejected attempts must not be persisted as ledger reservations");
                }
            }
            if entry.id > 0 {
                ensure!(
                    entry.id < ledger.next_reservation_id,
                    "budget ledger counter is behind a reservation"
                );
            }
        }
        let historical_count = ledger
            .entries
            .iter()
            .filter(|entry| entry.state == BudgetReservationState::HistoricalUnresolved)
            .count();
        let expected_historical = if ledger.initial_charge_usd > Decimal::ZERO {
            1
        } else {
            0
        };
        ensure!(
            historical_count == expected_historical,
            "budget ledger is missing or duplicating its historical carry-in"
        );
        validate_reconciliation_audit(ledger)?;
        Ok(())
    }

    fn with_read_ledger<T>(&self, operation: impl FnOnce(&BudgetLedger) -> Result<T>) -> Result<T> {
        let _lock = self.lock()?;
        let ledger = self.read_ledger()?;
        operation(&ledger)
    }

    fn with_ledger<T>(&self, operation: impl FnOnce(&mut BudgetLedger) -> Result<T>) -> Result<T> {
        let _lock = self.lock()?;
        let mut ledger = self.read_ledger()?;
        let value = operation(&mut ledger)?;
        self.write_ledger(&ledger)?;
        Ok(value)
    }

    fn write_ledger(&self, ledger: &BudgetLedger) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(ledger)?;
        let file_name = self
            .ledger_path
            .file_name()
            .and_then(|name| name.to_str())
            .context("budget ledger file name must be valid UTF-8")?;
        let temporary = self
            .ledger_path
            .with_file_name(format!(".{file_name}.tmp-{}", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .context("creating temporary budget ledger")?;
        if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
            let _ = fs::remove_file(&temporary);
            return Err(error).context("writing durable budget ledger");
        }
        drop(file);
        if let Err(error) = fs::rename(&temporary, &self.ledger_path) {
            let _ = fs::remove_file(&temporary);
            return Err(error).context("replacing durable budget ledger");
        }
        if let Some(parent) = self.ledger_path.parent() {
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .context("syncing budget ledger directory")?;
        }
        Ok(())
    }
}

impl BudgetLedger {
    fn new(provider: &str, config: &BudgetConfig) -> Self {
        let mut entries = Vec::new();
        if config.initial_charge_usd > Decimal::ZERO {
            entries.push(LedgerEntry {
                id: 0,
                provider: "historical_unresolved".to_owned(),
                requested_model: "unknown".to_owned(),
                state: BudgetReservationState::HistoricalUnresolved,
                reserved_usd: config.initial_charge_usd,
                charged_usd: None,
                input_token_bound: None,
                output_token_bound: None,
            });
        }
        Self {
            schema_version: LEDGER_VERSION,
            provider: provider.to_owned(),
            currency: "USD".to_owned(),
            ceiling_usd: config.ceiling_usd,
            initial_charge_usd: config.initial_charge_usd,
            next_reservation_id: 1,
            halted: false,
            entries,
            reconciliation_audit: Vec::new(),
        }
    }
}

fn validate_unbillable_validation_evidence(
    guard_provider: &str,
    evidence: &BudgetReconciliationEvidence,
) -> Result<()> {
    ensure!(
        guard_provider == "meta" && evidence.provider == "meta",
        "unbillable validation-error evidence must identify the Meta provider"
    );
    ensure!(
        evidence.api == WireApi::Responses,
        "unbillable Meta validation-error evidence must use the Responses API"
    );
    ensure!(
        evidence.http_status == 400,
        "only HTTP 400 validation errors are supported for unbillable reconciliation"
    );
    ensure!(
        !evidence.generation_started,
        "cannot release a hold when generation may have started"
    );
    ensure!(
        !evidence.usage_reported,
        "cannot release a hold when usage was reported"
    );
    Ok(())
}

fn validate_run_id(run_id: &str) -> Result<()> {
    ensure!(
        !run_id.is_empty()
            && run_id.len() <= 128
            && run_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte)),
        "budget reconciliation run ID is empty or contains unsupported characters"
    );
    Ok(())
}

fn validate_reconciliation_audit(ledger: &BudgetLedger) -> Result<()> {
    let mut reconciled_reservations = std::collections::BTreeSet::new();
    for event in &ledger.reconciliation_audit {
        ensure!(
            event.reason == BudgetReconciliationReason::UnbillableValidationError,
            "budget ledger contains an unsupported reconciliation reason"
        );
        ensure!(
            reconciled_reservations.insert(event.reservation_id),
            "budget ledger contains duplicate reconciliation events"
        );
        validate_run_id(&event.run_id)?;
        validate_unbillable_validation_evidence("meta", &event.evidence)?;
        let timestamp = chrono::DateTime::parse_from_rfc3339(&event.reconciled_at_utc)
            .context("budget reconciliation timestamp is invalid")?;
        ensure!(
            timestamp.offset().local_minus_utc() == 0 && event.reconciled_at_utc.ends_with('Z'),
            "budget reconciliation timestamp must be UTC"
        );
        ensure!(
            event.reservation_id > 0
                && event.released_usd > Decimal::ZERO
                && event.balance_before_usd >= Decimal::ZERO
                && event.balance_after_usd >= Decimal::ZERO,
            "budget reconciliation contains invalid IDs or amounts"
        );
        let expected_after = event
            .balance_before_usd
            .checked_add(event.released_usd)
            .context("budget reconciliation balance overflow")?;
        ensure!(
            expected_after == event.balance_after_usd
                && event.balance_after_usd <= ledger.ceiling_usd,
            "budget reconciliation balances do not match the released hold"
        );
        let entry = ledger
            .entries
            .iter()
            .find(|entry| entry.id == event.reservation_id)
            .context("budget reconciliation refers to a missing reservation")?;
        ensure!(
            entry.provider == "meta"
                && entry.state == BudgetReservationState::Unknown
                && entry.charged_usd.is_none()
                && entry.reserved_usd == event.released_usd,
            "budget reconciliation does not match an uncharged unknown Meta reservation"
        );
    }
    Ok(())
}

fn snapshot(ledger: &BudgetLedger) -> Result<BudgetSnapshot> {
    let mut settled = Decimal::ZERO;
    let mut held = Decimal::ZERO;
    let mut unknown_count = 0_u64;
    let mut overrun_count = 0_u64;
    let mut reservation_count = 0_u64;
    let reconciled_reservations: std::collections::BTreeSet<u64> = ledger
        .reconciliation_audit
        .iter()
        .map(|event| event.reservation_id)
        .collect();
    for entry in &ledger.entries {
        if entry.id > 0 {
            reservation_count += 1;
        }
        match entry.state {
            BudgetReservationState::Settled | BudgetReservationState::Overrun => {
                settled = settled
                    .checked_add(entry.charged_usd.unwrap_or(entry.reserved_usd))
                    .context("budget settled subtotal overflow")?;
                if entry.state == BudgetReservationState::Overrun {
                    overrun_count += 1;
                }
            }
            BudgetReservationState::Reserved
            | BudgetReservationState::Unknown
            | BudgetReservationState::HistoricalUnresolved => {
                let released_by_audit = reconciled_reservations.contains(&entry.id);
                if !released_by_audit {
                    held = held
                        .checked_add(entry.reserved_usd)
                        .context("budget held subtotal overflow")?;
                }
                if !released_by_audit
                    && matches!(
                        entry.state,
                        BudgetReservationState::Unknown
                            | BudgetReservationState::HistoricalUnresolved
                    )
                {
                    unknown_count += 1;
                }
            }
            BudgetReservationState::Rejected => {}
        }
    }
    let used = settled
        .checked_add(held)
        .context("budget total usage overflow")?;
    ensure!(
        used >= ledger.initial_charge_usd,
        "budget ledger usage omitted historical carry-in"
    );
    let remaining = ledger
        .ceiling_usd
        .checked_sub(used)
        .unwrap_or(Decimal::ZERO)
        .max(Decimal::ZERO);
    Ok(BudgetSnapshot {
        ceiling_usd: ledger.ceiling_usd,
        initial_charge_usd: ledger.initial_charge_usd,
        settled_usd: settled,
        held_usd: held,
        remaining_usd: remaining,
        halted: ledger.halted,
        reservation_count,
        unknown_count,
        overrun_count,
    })
}

fn reservation_mut(ledger: &mut BudgetLedger, reservation_id: u64) -> Result<&mut LedgerEntry> {
    ledger
        .entries
        .iter_mut()
        .find(|entry| entry.id == reservation_id && entry.id != 0)
        .context("budget reservation ID is missing")
}

fn evidence(entry: &LedgerEntry) -> BudgetEvidence {
    BudgetEvidence {
        reservation_id: (entry.id != 0).then_some(entry.id),
        reserved_usd: entry.reserved_usd,
        charged_usd: entry.charged_usd,
        state: entry.state,
        input_token_bound: entry.input_token_bound,
        output_token_bound: entry.output_token_bound,
    }
}

fn pinned_rates(provider: &str) -> Result<PinnedRates> {
    match provider {
        "openai" => Ok(PinnedRates {
            input_per_million: OPENAI_INPUT_PER_MILLION,
            output_per_million: OPENAI_OUTPUT_PER_MILLION,
            input_context_tokens: MAX_INPUT_CONTEXT_TOKENS,
        }),
        "meta" => Ok(PinnedRates {
            input_per_million: META_INPUT_PER_MILLION,
            output_per_million: META_OUTPUT_PER_MILLION,
            input_context_tokens: META_MAX_INPUT_CONTEXT_TOKENS,
        }),
        _ => bail!("provider has no pinned budget rates"),
    }
}

fn ceiling_for_provider(provider: &str) -> Result<Decimal> {
    match provider {
        "openai" => Ok(USD_BUDGET_CEILING),
        "meta" => Ok(META_MUSE_BUDGET_CEILING_USD),
        _ => bail!("provider has no fixed budget ceiling"),
    }
}

fn model_id_for_provider(provider: &str) -> Result<&'static str> {
    match provider {
        "openai" => Ok("gpt-6-luna"),
        "meta" => Ok("muse-spark-1.1"),
        _ => bail!("provider has no supported budgeted model"),
    }
}

fn validate_budgeted_model(provider: &str, model: &ModelConfig) -> Result<()> {
    ensure!(
        model.provider == provider && model.id == model_id_for_provider(provider)?,
        "budgeted requests are restricted to the configured provider model"
    );
    ensure!(
        model.max_output_tokens > 0,
        "budgeted requests require a positive maximum output token count"
    );
    if provider == "openai" {
        ensure!(
            model.reasoning_effort.as_deref() == Some("none") && model.thinking.is_none(),
            "budgeted gpt-6-luna requests require reasoning_effort=none"
        );
    }
    Ok(())
}

fn worst_case_cost_for_provider(
    provider: &str,
    input_tokens: u64,
    output_tokens: u64,
) -> Result<Decimal> {
    usage_cost(pinned_rates(provider)?, input_tokens, output_tokens)
}

fn usage_cost(rates: PinnedRates, input_tokens: u64, output_tokens: u64) -> Result<Decimal> {
    let input_cost = Decimal::from(input_tokens)
        .checked_mul(rates.input_per_million)
        .context("budget input reservation overflow")?
        .checked_div(Decimal::from(TOKENS_PER_MILLION))
        .context("budget input reservation division failed")?;
    let output_cost = Decimal::from(output_tokens)
        .checked_mul(rates.output_per_million)
        .context("budget output reservation overflow")?
        .checked_div(Decimal::from(TOKENS_PER_MILLION))
        .context("budget output reservation division failed")?;
    input_cost
        .checked_add(output_cost)
        .context("total budget reservation overflow")
}

fn conservative_usage_cost(provider: &str, usage: &TokenUsage) -> Option<Decimal> {
    let input = usage.input?;
    let output = usage.output?;
    usage_cost(pinned_rates(provider).ok()?, input, output).ok()
}

fn path_exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("inspecting {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        process::{Command, Stdio},
        sync::atomic::{AtomicU64, Ordering},
        thread,
        time::{Duration, Instant},
    };

    use rust_decimal::Decimal;
    use serde_json::Value;

    use super::{BudgetGuard, MAX_BUDGETED_BODY_BYTES, MAX_INPUT_CONTEXT_TOKENS, ReconcileResult};
    use crate::types::{
        BudgetConfig, BudgetReconciliationEvidence, BudgetReconciliationReason,
        BudgetReservationState, ModelConfig, TokenUsage, WireApi,
    };

    static NEXT_SANDBOX: AtomicU64 = AtomicU64::new(0);

    struct Sandbox(PathBuf);

    impl Sandbox {
        fn new() -> Self {
            let id = NEXT_SANDBOX.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("mandate-budget-test-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn ledger(&self) -> PathBuf {
            self.0.join("paid-budget-ledger.json")
        }

        fn muse_ledger(&self) -> PathBuf {
            self.0.join("muse-spend-ledger.json")
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn config(initial_charge_usd: Decimal) -> BudgetConfig {
        BudgetConfig {
            ceiling_usd: Decimal::ONE,
            initial_charge_usd,
        }
    }

    fn meta_config(initial_charge_usd: Decimal) -> BudgetConfig {
        BudgetConfig {
            ceiling_usd: super::META_MUSE_BUDGET_CEILING_USD,
            initial_charge_usd,
        }
    }

    fn model(max_output_tokens: u32) -> ModelConfig {
        ModelConfig {
            id: "gpt-6-luna".into(),
            provider: "openai".into(),
            max_output_tokens,
            temperature: None,
            reasoning_effort: Some("none".into()),
            thinking: None,
            seed: None,
            price: None,
        }
    }

    fn meta_model(max_output_tokens: u32) -> ModelConfig {
        ModelConfig {
            id: "muse-spark-1.1".into(),
            provider: "meta".into(),
            max_output_tokens,
            temperature: None,
            reasoning_effort: None,
            thinking: None,
            seed: None,
            price: None,
        }
    }

    fn meta_validation_error_evidence() -> BudgetReconciliationEvidence {
        BudgetReconciliationEvidence {
            provider: "meta".into(),
            api: WireApi::Responses,
            http_status: 400,
            generation_started: false,
            usage_reported: false,
        }
    }

    fn expected_reservation(output_tokens: u32) -> Decimal {
        super::usage_cost(
            super::pinned_rates("openai").unwrap(),
            MAX_INPUT_CONTEXT_TOKENS,
            u64::from(output_tokens),
        )
        .unwrap()
    }

    #[test]
    fn initial_full_historical_carry_in_refuses_before_creating_a_request_reservation() {
        let sandbox = Sandbox::new();
        let guard = BudgetGuard::open(sandbox.ledger(), &config(Decimal::ONE)).unwrap();
        let snapshot = guard.snapshot().unwrap();
        assert_eq!(snapshot.initial_charge_usd, Decimal::ONE);
        assert_eq!(snapshot.held_usd, Decimal::ONE);
        assert_eq!(snapshot.remaining_usd, Decimal::ZERO);
        assert_eq!(snapshot.reservation_count, 0);
        assert!(guard.reserve(&model(2048), 100).is_err());
        assert_eq!(guard.snapshot().unwrap().reservation_count, 0);
    }

    #[test]
    fn worst_case_reservation_blocks_the_next_attempt_and_survives_reopen() {
        let sandbox = Sandbox::new();
        let config = config(Decimal::ZERO);
        let guard = BudgetGuard::open(sandbox.ledger(), &config).unwrap();
        let first = guard.reserve(&model(2048), 100).unwrap();
        assert_eq!(first.reserved_usd, Decimal::new(526_536, 6));
        assert_eq!(
            guard.snapshot().unwrap().remaining_usd,
            Decimal::new(473_464, 6)
        );
        assert!(guard.reserve(&model(2048), 100).is_err());
        drop(guard);

        let reopened = BudgetGuard::open(sandbox.ledger(), &config).unwrap();
        let snapshot = reopened.snapshot().unwrap();
        assert_eq!(snapshot.held_usd, first.reserved_usd);
        assert_eq!(snapshot.remaining_usd, Decimal::new(473_464, 6));
        assert!(reopened.reserve(&model(2048), 100).is_err());
    }

    #[test]
    fn known_usage_reconciles_at_conservative_cache_write_rates() {
        let sandbox = Sandbox::new();
        let guard = BudgetGuard::open(sandbox.ledger(), &config(Decimal::ZERO)).unwrap();
        let reservation = guard.reserve(&model(2048), 100).unwrap();
        let usage = TokenUsage {
            input: Some(1_000),
            cached_input: Some(500),
            output: Some(200),
            reasoning: Some(150),
        };
        let (evidence, outcome) = guard.reconcile(reservation.id, &usage, false).unwrap();
        assert_eq!(outcome, ReconcileResult::Settled);
        assert_eq!(evidence.state, BudgetReservationState::Settled);
        assert_eq!(evidence.charged_usd, Some(Decimal::new(650, 6)));
        let snapshot = guard.snapshot().unwrap();
        assert_eq!(snapshot.settled_usd, Decimal::new(650, 6));
        assert_eq!(snapshot.held_usd, Decimal::ZERO);
        assert_eq!(snapshot.remaining_usd, Decimal::new(999_350, 6));
        assert!(!snapshot.halted);
    }

    #[test]
    fn missing_usage_keeps_the_full_reservation_and_carry_survives_restart() {
        let sandbox = Sandbox::new();
        let config = config(Decimal::ZERO);
        let guard = BudgetGuard::open(sandbox.ledger(), &config).unwrap();
        let reservation = guard.reserve(&model(2048), 100).unwrap();
        let (_, outcome) = guard
            .reconcile(reservation.id, &TokenUsage::default(), false)
            .unwrap();
        assert_eq!(outcome, ReconcileResult::Unknown);
        assert_eq!(guard.snapshot().unwrap().held_usd, reservation.reserved_usd);
        assert_eq!(
            guard.snapshot().unwrap().remaining_usd,
            Decimal::new(473_464, 6)
        );
        drop(guard);

        let reopened = BudgetGuard::open(sandbox.ledger(), &config).unwrap();
        assert_eq!(
            reopened.snapshot().unwrap().held_usd,
            reservation.reserved_usd
        );
        assert!(reopened.reserve(&model(2048), 100).is_err());
    }

    #[test]
    fn first_provider_error_halts_future_budgeted_requests_and_survives_restart() {
        let sandbox = Sandbox::new();
        let config = config(Decimal::ZERO);
        let guard = BudgetGuard::open(sandbox.ledger(), &config).unwrap();
        let reservation = guard.reserve(&model(2048), 100).unwrap();
        let evidence = guard.mark_provider_error(reservation.id).unwrap();
        assert_eq!(evidence.state, BudgetReservationState::Unknown);
        assert!(guard.snapshot().unwrap().halted);
        assert!(guard.reserve(&model(2048), 100).is_err());
        drop(guard);

        let reopened = BudgetGuard::open(sandbox.ledger(), &config).unwrap();
        assert!(reopened.snapshot().unwrap().halted);
        assert!(reopened.reserve(&model(2048), 100).is_err());
    }

    #[test]
    fn meta_provider_error_keeps_full_hold_and_halts_after_restart() {
        let sandbox = Sandbox::new();
        let config = meta_config(Decimal::ZERO);
        let guard = BudgetGuard::open_for_provider(sandbox.muse_ledger(), "meta", &config).unwrap();
        let reservation = guard.reserve(&meta_model(4096), 100).unwrap();
        assert_eq!(
            guard.mark_provider_error(reservation.id).unwrap().state,
            BudgetReservationState::Unknown
        );
        assert_eq!(guard.snapshot().unwrap().held_usd, reservation.reserved_usd);
        assert!(guard.reserve(&meta_model(4096), 100).is_err());
        drop(guard);

        let reopened =
            BudgetGuard::open_for_provider(sandbox.muse_ledger(), "meta", &config).unwrap();
        assert!(reopened.snapshot().unwrap().halted);
        assert!(reopened.reserve(&meta_model(4096), 100).is_err());
    }

    #[test]
    fn audited_meta_400_release_keeps_original_entry_and_successful_charge_unchanged() {
        let sandbox = Sandbox::new();
        let config = meta_config(Decimal::ZERO);
        let guard = BudgetGuard::open_for_provider(sandbox.muse_ledger(), "meta", &config).unwrap();
        let successful = guard.reserve(&meta_model(4096), 100).unwrap();
        let (successful_evidence, outcome) = guard
            .reconcile(
                successful.id,
                &TokenUsage {
                    input: Some(1_000),
                    cached_input: Some(0),
                    output: Some(200),
                    reasoning: Some(150),
                },
                false,
            )
            .unwrap();
        assert_eq!(outcome, ReconcileResult::Settled);
        let successful_charge = successful_evidence.charged_usd.unwrap();

        let failed = guard.reserve(&meta_model(4096), 100).unwrap();
        guard.mark_provider_error(failed.id).unwrap();
        let bytes_before = fs::read(sandbox.muse_ledger()).unwrap();
        let ledger_before: Value = serde_json::from_slice(&bytes_before).unwrap();
        let original_entry = ledger_before["entries"][1].clone();
        let before_snapshot = guard.snapshot().unwrap();
        assert_eq!(before_snapshot.held_usd, failed.reserved_usd);
        assert_eq!(before_snapshot.unknown_count, 1);
        assert!(before_snapshot.halted);

        let record = guard
            .reconcile_unbillable_validation_error(
                failed.id,
                "smoke-20261105T121126.314341000Z-49545",
                &meta_validation_error_evidence(),
            )
            .unwrap();
        assert_eq!(
            record.reason,
            BudgetReconciliationReason::UnbillableValidationError
        );
        assert_eq!(record.reservation_id, failed.id);
        assert_eq!(record.run_id, "smoke-20261105T121126.314341000Z-49545");
        assert_eq!(record.released_usd, failed.reserved_usd);
        assert_eq!(record.balance_before_usd, before_snapshot.remaining_usd);
        assert_eq!(
            record.balance_after_usd,
            before_snapshot.remaining_usd + failed.reserved_usd
        );
        assert!(record.reconciled_at_utc.ends_with('Z'));
        assert_eq!(record.evidence.http_status, 400);

        let ledger_after: Value =
            serde_json::from_slice(&fs::read(sandbox.muse_ledger()).unwrap()).unwrap();
        assert_eq!(ledger_after["entries"][1], original_entry);
        assert_eq!(
            ledger_after["reconciliation_audit"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let after_snapshot = guard.snapshot().unwrap();
        assert_eq!(after_snapshot.settled_usd, successful_charge);
        assert_eq!(after_snapshot.held_usd, Decimal::ZERO);
        assert_eq!(after_snapshot.unknown_count, 0);
        assert_eq!(
            after_snapshot.remaining_usd,
            config.ceiling_usd - successful_charge
        );
        assert!(
            after_snapshot.halted,
            "reconciliation must preserve first-error halt"
        );

        drop(guard);
        let reopened =
            BudgetGuard::open_for_provider(sandbox.muse_ledger(), "meta", &config).unwrap();
        assert_eq!(
            reopened.snapshot().unwrap().remaining_usd,
            after_snapshot.remaining_usd
        );
        assert!(
            reopened
                .reconcile_unbillable_validation_error(
                    failed.id,
                    "smoke-20261105T121126.314341000Z-49545",
                    &meta_validation_error_evidence(),
                )
                .is_err()
        );
    }

    #[test]
    fn unsupported_or_non_meta_reconciliation_evidence_fails_without_writing() {
        let cases = [
            BudgetReconciliationEvidence {
                provider: "meta".into(),
                api: WireApi::Responses,
                http_status: 401,
                generation_started: false,
                usage_reported: false,
            },
            BudgetReconciliationEvidence {
                provider: "meta".into(),
                api: WireApi::Responses,
                http_status: 400,
                generation_started: true,
                usage_reported: false,
            },
            BudgetReconciliationEvidence {
                provider: "meta".into(),
                api: WireApi::Responses,
                http_status: 400,
                generation_started: false,
                usage_reported: true,
            },
            BudgetReconciliationEvidence {
                provider: "meta".into(),
                api: WireApi::ChatCompletions,
                http_status: 400,
                generation_started: false,
                usage_reported: false,
            },
            BudgetReconciliationEvidence {
                provider: "openai".into(),
                api: WireApi::Responses,
                http_status: 400,
                generation_started: false,
                usage_reported: false,
            },
        ];
        let sandbox = Sandbox::new();
        let meta_configuration = meta_config(Decimal::ZERO);
        let guard =
            BudgetGuard::open_for_provider(sandbox.muse_ledger(), "meta", &meta_configuration)
                .unwrap();
        let reservation = guard.reserve(&meta_model(4096), 100).unwrap();
        guard.mark_provider_error(reservation.id).unwrap();
        for evidence in &cases {
            let before = fs::read(sandbox.muse_ledger()).unwrap();
            assert!(
                guard
                    .reconcile_unbillable_validation_error(reservation.id, "run-1", evidence)
                    .is_err()
            );
            assert_eq!(fs::read(sandbox.muse_ledger()).unwrap(), before);
        }

        let openai_guard = BudgetGuard::open(sandbox.ledger(), &config(Decimal::ZERO)).unwrap();
        let openai_reservation = openai_guard.reserve(&model(2048), 100).unwrap();
        openai_guard
            .mark_provider_error(openai_reservation.id)
            .unwrap();
        let before = fs::read(sandbox.ledger()).unwrap();
        assert!(
            openai_guard
                .reconcile_unbillable_validation_error(
                    openai_reservation.id,
                    "run-1",
                    &meta_validation_error_evidence(),
                )
                .is_err()
        );
        assert_eq!(fs::read(sandbox.ledger()).unwrap(), before);
    }

    #[test]
    fn reconciliation_rejects_reserved_charged_and_overrun_entries() {
        let evidence = meta_validation_error_evidence();

        let reserved_sandbox = Sandbox::new();
        let meta_configuration = meta_config(Decimal::ZERO);
        let reserved_guard = BudgetGuard::open_for_provider(
            reserved_sandbox.muse_ledger(),
            "meta",
            &meta_configuration,
        )
        .unwrap();
        let reserved = reserved_guard.reserve(&meta_model(4096), 100).unwrap();
        let before = fs::read(reserved_sandbox.muse_ledger()).unwrap();
        assert!(
            reserved_guard
                .reconcile_unbillable_validation_error(reserved.id, "run-1", &evidence)
                .is_err()
        );
        assert_eq!(fs::read(reserved_sandbox.muse_ledger()).unwrap(), before);

        let charged_sandbox = Sandbox::new();
        let charged_guard = BudgetGuard::open_for_provider(
            charged_sandbox.muse_ledger(),
            "meta",
            &meta_configuration,
        )
        .unwrap();
        let charged = charged_guard.reserve(&meta_model(4096), 100).unwrap();
        charged_guard
            .reconcile(
                charged.id,
                &TokenUsage {
                    input: Some(100),
                    cached_input: None,
                    output: Some(100),
                    reasoning: None,
                },
                false,
            )
            .unwrap();
        let before = fs::read(charged_sandbox.muse_ledger()).unwrap();
        assert!(
            charged_guard
                .reconcile_unbillable_validation_error(charged.id, "run-1", &evidence)
                .is_err()
        );
        assert_eq!(fs::read(charged_sandbox.muse_ledger()).unwrap(), before);

        let overrun_sandbox = Sandbox::new();
        let overrun_guard = BudgetGuard::open_for_provider(
            overrun_sandbox.muse_ledger(),
            "meta",
            &meta_configuration,
        )
        .unwrap();
        let overrun = overrun_guard.reserve(&meta_model(4096), 100).unwrap();
        let unknown = overrun_guard.reserve(&meta_model(4096), 100).unwrap();
        overrun_guard
            .reconcile(
                overrun.id,
                &TokenUsage {
                    input: Some(super::META_MAX_INPUT_CONTEXT_TOKENS + 1),
                    cached_input: Some(0),
                    output: Some(0),
                    reasoning: None,
                },
                false,
            )
            .unwrap();
        overrun_guard.mark_provider_error(unknown.id).unwrap();
        let before = fs::read(overrun_sandbox.muse_ledger()).unwrap();
        assert!(
            overrun_guard
                .reconcile_unbillable_validation_error(unknown.id, "run-1", &evidence)
                .is_err()
        );
        assert_eq!(fs::read(overrun_sandbox.muse_ledger()).unwrap(), before);
    }

    #[test]
    fn usage_over_bound_halts_even_when_cost_could_fit_in_the_reservation() {
        let sandbox = Sandbox::new();
        let guard = BudgetGuard::open(sandbox.ledger(), &config(Decimal::ZERO)).unwrap();
        let reservation = guard.reserve(&model(2048), 100).unwrap();
        let usage = TokenUsage {
            input: Some(MAX_INPUT_CONTEXT_TOKENS + 1),
            cached_input: Some(0),
            output: Some(0),
            reasoning: Some(0),
        };
        let (evidence, outcome) = guard.reconcile(reservation.id, &usage, false).unwrap();
        assert_eq!(outcome, ReconcileResult::Overrun);
        assert_eq!(evidence.state, BudgetReservationState::Overrun);
        assert!(guard.snapshot().unwrap().halted);
    }

    #[test]
    fn inconsistent_cached_or_reasoning_usage_is_unknown_and_halts() {
        let invalid_usage = [
            TokenUsage {
                input: Some(100),
                cached_input: Some(101),
                output: Some(100),
                reasoning: Some(100),
            },
            TokenUsage {
                input: Some(100),
                cached_input: Some(100),
                output: Some(100),
                reasoning: Some(101),
            },
        ];
        for usage in invalid_usage {
            let sandbox = Sandbox::new();
            let guard = BudgetGuard::open(sandbox.ledger(), &config(Decimal::ZERO)).unwrap();
            let reservation = guard.reserve(&model(2048), 100).unwrap();
            let (evidence, outcome) = guard.reconcile(reservation.id, &usage, false).unwrap();
            assert_eq!(outcome, ReconcileResult::Overrun);
            assert_eq!(evidence.state, BudgetReservationState::Unknown);
            assert_eq!(evidence.charged_usd, None);
            assert_eq!(guard.snapshot().unwrap().held_usd, reservation.reserved_usd);
            assert!(guard.snapshot().unwrap().halted);
        }
    }

    #[test]
    fn reopening_with_a_different_historical_carry_in_fails_closed() {
        let sandbox = Sandbox::new();
        let _guard = BudgetGuard::open(sandbox.ledger(), &config(Decimal::ZERO)).unwrap();
        assert!(BudgetGuard::open(sandbox.ledger(), &config(Decimal::ONE)).is_err());
    }

    #[test]
    fn a_positive_carry_in_cannot_be_removed_by_editing_away_its_ledger_entry() {
        let sandbox = Sandbox::new();
        let _guard = BudgetGuard::open(sandbox.ledger(), &config(Decimal::ONE)).unwrap();
        let mut ledger: Value =
            serde_json::from_slice(&fs::read(sandbox.ledger()).unwrap()).unwrap();
        ledger["entries"] = serde_json::json!([]);
        fs::write(sandbox.ledger(), serde_json::to_vec(&ledger).unwrap()).unwrap();
        assert!(BudgetGuard::open(sandbox.ledger(), &config(Decimal::ONE)).is_err());
    }

    #[test]
    fn body_limit_is_resource_only_and_does_not_replace_the_full_context_reservation() {
        let sandbox = Sandbox::new();
        let guard = BudgetGuard::open(sandbox.ledger(), &config(Decimal::ZERO)).unwrap();
        let reservation = guard
            .reserve(&model(2048), MAX_BUDGETED_BODY_BYTES)
            .unwrap();
        assert_eq!(reservation.input_token_bound, MAX_INPUT_CONTEXT_TOKENS);
        assert_eq!(reservation.reserved_usd, expected_reservation(2048));
        assert!(
            guard
                .reserve(&model(2048), MAX_BUDGETED_BODY_BYTES + 1)
                .is_err()
        );
    }

    #[test]
    fn meta_reserves_full_documented_context_and_pinned_uncached_rates() {
        let sandbox = Sandbox::new();
        let guard = BudgetGuard::open_for_provider(
            sandbox.muse_ledger(),
            "meta",
            &meta_config(Decimal::ZERO),
        )
        .unwrap();
        let expected = Decimal::new(1_328_128, 6);
        assert_eq!(
            guard.projected_request_cost(&meta_model(4096)).unwrap(),
            expected
        );

        let first = guard.reserve(&meta_model(4096), 100).unwrap();
        let second = guard.reserve(&meta_model(4096), 100).unwrap();
        assert_eq!(
            first.input_token_bound,
            super::META_MAX_INPUT_CONTEXT_TOKENS
        );
        assert_eq!(first.reserved_usd, expected);
        assert_eq!(second.reserved_usd, expected);
        assert_eq!(
            guard.snapshot().unwrap().held_usd,
            expected * Decimal::new(2, 0)
        );
    }

    #[test]
    fn provider_ledgers_are_independent_and_reject_cross_provider_models() {
        let sandbox = Sandbox::new();
        let openai = BudgetGuard::open(sandbox.ledger(), &config(Decimal::ZERO)).unwrap();
        let meta = BudgetGuard::open_for_provider(
            sandbox.muse_ledger(),
            "meta",
            &meta_config(Decimal::ZERO),
        )
        .unwrap();
        assert!(openai.reserve(&meta_model(4096), 100).is_err());
        assert!(meta.reserve(&model(2048), 100).is_err());

        openai.reserve(&model(2048), 100).unwrap();
        meta.reserve(&meta_model(4096), 100).unwrap();
        assert_eq!(openai.snapshot().unwrap().reservation_count, 1);
        assert_eq!(meta.snapshot().unwrap().reservation_count, 1);
        drop(openai);
        drop(meta);
        assert_eq!(
            BudgetGuard::open(sandbox.ledger(), &config(Decimal::ZERO))
                .unwrap()
                .snapshot()
                .unwrap()
                .reservation_count,
            1
        );
        assert_eq!(
            BudgetGuard::open_for_provider(
                sandbox.muse_ledger(),
                "meta",
                &meta_config(Decimal::ZERO)
            )
            .unwrap()
            .snapshot()
            .unwrap()
            .reservation_count,
            1
        );
    }

    #[test]
    fn provider_identity_and_ceiling_mismatches_fail_closed() {
        let sandbox = Sandbox::new();
        let openai = BudgetGuard::open(sandbox.ledger(), &config(Decimal::ZERO)).unwrap();
        drop(openai);
        assert!(
            BudgetGuard::open_for_provider(sandbox.ledger(), "meta", &meta_config(Decimal::ZERO))
                .is_err()
        );

        let wrong_luna_ceiling = BudgetConfig {
            ceiling_usd: Decimal::new(2, 0),
            initial_charge_usd: Decimal::ZERO,
        };
        assert!(BudgetGuard::open(sandbox.0.join("wrong-luna.json"), &wrong_luna_ceiling).is_err());
        let wrong_muse_ceiling = BudgetConfig {
            ceiling_usd: Decimal::new(14, 0),
            initial_charge_usd: Decimal::ZERO,
        };
        assert!(
            BudgetGuard::open_for_provider(sandbox.muse_ledger(), "meta", &wrong_muse_ceiling)
                .is_err()
        );
    }

    #[test]
    fn old_openai_ledger_without_provider_field_remains_readable() {
        let sandbox = Sandbox::new();
        let guard = BudgetGuard::open(sandbox.ledger(), &config(Decimal::ZERO)).unwrap();
        guard.reserve(&model(2048), 100).unwrap();
        drop(guard);

        let mut ledger: Value =
            serde_json::from_slice(&fs::read(sandbox.ledger()).unwrap()).unwrap();
        ledger.as_object_mut().unwrap().remove("provider");
        fs::write(sandbox.ledger(), serde_json::to_vec(&ledger).unwrap()).unwrap();
        let reopened = BudgetGuard::open(sandbox.ledger(), &config(Decimal::ZERO)).unwrap();
        assert_eq!(reopened.snapshot().unwrap().reservation_count, 1);
        assert!(
            serde_json::from_slice::<Value>(&fs::read(sandbox.ledger()).unwrap())
                .unwrap()
                .get("provider")
                .is_none()
        );
    }

    #[test]
    fn usage_projection_uses_provider_rates_without_cached_discount_or_reasoning_double_count() {
        let sandbox = Sandbox::new();
        let guard = BudgetGuard::open_for_provider(
            sandbox.muse_ledger(),
            "meta",
            &meta_config(Decimal::ZERO),
        )
        .unwrap();
        assert_eq!(
            guard.projected_usage_cost(100_000, 2_048).unwrap(),
            Decimal::new(133_704, 6)
        );

        let reservation = guard.reserve(&meta_model(4096), 100).unwrap();
        let usage = TokenUsage {
            input: Some(1_000),
            cached_input: Some(500),
            output: Some(200),
            reasoning: Some(150),
        };
        let (evidence, outcome) = guard.reconcile(reservation.id, &usage, false).unwrap();
        assert_eq!(outcome, ReconcileResult::Settled);
        assert_eq!(evidence.charged_usd, Some(Decimal::new(21, 4)));
    }

    #[test]
    fn batch_admission_compares_current_exposure_without_rewriting_the_ledger() {
        let sandbox = Sandbox::new();
        let guard = BudgetGuard::open(sandbox.ledger(), &config(Decimal::ZERO)).unwrap();
        let reservation = guard.reserve(&model(2048), 100).unwrap();
        let before = fs::read(sandbox.ledger()).unwrap();
        assert!(guard.ensure_batch_fits(Decimal::new(5, 1)).is_err());
        assert!(guard.ensure_batch_fits(Decimal::new(3, 1)).is_ok());
        assert!(reservation.reserved_usd > Decimal::new(3, 1));
        assert_eq!(fs::read(sandbox.ledger()).unwrap(), before);
    }

    #[test]
    fn provider_specific_history_allows_only_valid_meta_reasoning_replay() {
        use super::validate_budgeted_history_for_provider;

        let valid = serde_json::json!({
            "type": "reasoning",
            "id": "rs_123",
            "summary": [{"type": "summary_text", "text": "summary"}],
            "encrypted_content": "opaque-ciphertext",
            "status": "completed"
        });
        assert!(
            validate_budgeted_history_for_provider("meta", std::slice::from_ref(&valid)).is_ok()
        );
        let mut in_progress = valid.clone();
        in_progress["status"] = serde_json::json!("in_progress");
        assert!(validate_budgeted_history_for_provider("meta", &[in_progress]).is_err());
        assert!(validate_budgeted_history_for_provider("openai", &[valid]).is_err());
        let missing_encrypted = serde_json::json!({
            "type": "reasoning",
            "summary": []
        });
        assert!(validate_budgeted_history_for_provider("meta", &[missing_encrypted]).is_err());
        let opaque_state = serde_json::json!({
            "type": "reasoning",
            "id": "rs_123",
            "summary": [],
            "encrypted_content": "ciphertext",
            "previous_response_id": "resp_123"
        });
        assert!(validate_budgeted_history_for_provider("meta", &[opaque_state]).is_err());
    }

    #[test]
    fn budget_ledger_lock_is_exclusive_across_processes() {
        let sandbox = Sandbox::new();
        let config = config(Decimal::ZERO);
        let guard = BudgetGuard::open(sandbox.ledger(), &config).unwrap();
        let lock = guard.lock().unwrap();
        let started = sandbox.0.join("child-started");
        let finished = sandbox.0.join("child-finished");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "budget::tests::budget_lock_helper_process"])
            .env("MANDATE_BUDGET_TEST_LEDGER", sandbox.ledger())
            .env("MANDATE_BUDGET_TEST_STARTED", &started)
            .env("MANDATE_BUDGET_TEST_FINISHED", &finished)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let start = Instant::now();
        while !started.exists() && start.elapsed() < Duration::from_secs(3) {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            started.exists(),
            "child process did not start its ledger access"
        );
        thread::sleep(Duration::from_millis(100));
        assert!(
            !finished.exists(),
            "child passed the held cross-process lock"
        );

        drop(lock);
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            if start.elapsed() > Duration::from_secs(3) {
                let _ = child.kill();
                panic!("child process did not resume after the budget lock was released");
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(finished.exists());
    }

    #[test]
    fn budget_lock_helper_process() {
        let Ok(ledger) = std::env::var("MANDATE_BUDGET_TEST_LEDGER") else {
            return;
        };
        let started = std::env::var("MANDATE_BUDGET_TEST_STARTED").unwrap();
        let finished = std::env::var("MANDATE_BUDGET_TEST_FINISHED").unwrap();
        fs::write(started, b"started").unwrap();
        let guard = BudgetGuard::open(ledger, &config(Decimal::ZERO)).unwrap();
        guard.snapshot().unwrap();
        fs::write(finished, b"finished").unwrap();
    }
}
