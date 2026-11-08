use std::{collections::BTreeMap, fmt, future::Future, pin::Pin};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireApi {
    Responses,
    ChatCompletions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    pub base_url: String,
    pub key_env: String,
    pub api: WireApi,
    pub max_concurrency: usize,
    pub rpm: Option<u32>,
    pub tpm: Option<u64>,
    #[serde(default = "default_retries")]
    pub max_retries: u32,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}
fn default_retries() -> u32 {
    6
}
fn default_timeout() -> u64 {
    120
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConfig {
    pub id: String,
    pub provider: String,
    #[serde(default = "default_tokens")]
    pub max_output_tokens: u32,
    pub temperature: Option<f64>,
    pub reasoning_effort: Option<String>,
    pub thinking: Option<bool>,
    pub seed: Option<u64>,
    pub price: Option<Price>,
}
fn default_tokens() -> u32 {
    4096
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Price {
    pub input_per_million: Decimal,
    pub cached_input_per_million: Option<Decimal>,
    pub output_per_million: Decimal,
}

/// Hard ceiling for an owner-authorized paid run. The ledger path is supplied by the CLI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetConfig {
    pub ceiling_usd: Decimal,
    /// Amount reserved on first ledger creation for unresolved historical exposure.
    pub initial_charge_usd: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunConfig {
    pub providers: BTreeMap<String, ProviderConfig>,
    pub models: Vec<ModelConfig>,
    pub tasks: Vec<String>,
    #[serde(default = "default_trials")]
    pub trials: u32,
    #[serde(default = "default_turns")]
    pub max_turns: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<BudgetConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muse_budget: Option<BudgetConfig>,
}
fn default_trials() -> u32 {
    1
}
fn default_turns() -> usize {
    20
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolChoice {
    Auto,
    None,
    Required(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// Cached input is included in input, and reasoning is included in output.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: Option<u64>,
    pub cached_input: Option<u64>,
    pub output: Option<u64>,
    pub reasoning: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptRecord {
    pub http_status: Option<u16>,
    pub started_at: String,
    pub ended_at: String,
    pub request_id: Option<String>,
    pub retry_delay_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetReservationState {
    Rejected,
    Reserved,
    Settled,
    Unknown,
    Overrun,
    HistoricalUnresolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetEvidence {
    pub reservation_id: Option<u64>,
    pub reserved_usd: Decimal,
    pub charged_usd: Option<Decimal>,
    pub state: BudgetReservationState,
    pub input_token_bound: Option<u64>,
    pub output_token_bound: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetSnapshot {
    pub ceiling_usd: Decimal,
    pub initial_charge_usd: Decimal,
    pub settled_usd: Decimal,
    pub held_usd: Decimal,
    pub remaining_usd: Decimal,
    pub halted: bool,
    pub reservation_count: u64,
    pub unknown_count: u64,
    pub overrun_count: u64,
}

/// Owner-reviewed evidence for a narrowly supported Meta validation-error release.
/// `generation_started = false` is an explicit caller determination; the guard does
/// not infer provider billing behavior from the status code on its own.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetReconciliationEvidence {
    pub provider: String,
    pub api: WireApi,
    pub http_status: u16,
    pub generation_started: bool,
    pub usage_reported: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetReconciliationReason {
    UnbillableValidationError,
}

/// Immutable audit event appended when an owner-reviewed validation error releases a hold.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetReconciliationRecord {
    pub reason: BudgetReconciliationReason,
    pub reservation_id: u64,
    pub run_id: String,
    pub reconciled_at_utc: String,
    pub released_usd: Decimal,
    /// Remaining spendable USD immediately before this release.
    pub balance_before_usd: Decimal,
    /// Remaining spendable USD immediately after this release.
    pub balance_after_usd: Decimal,
    pub evidence: BudgetReconciliationEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestRecord {
    pub provider: String,
    pub requested_model: String,
    pub model: Option<String>,
    pub api: WireApi,
    pub http_status: Option<u16>,
    pub retries: u32,
    pub started_at: String,
    pub ended_at: String,
    pub usage: TokenUsage,
    pub attempts: Vec<AttemptRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<BudgetEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelReply {
    pub text: String,
    pub calls: Vec<FunctionCall>,
    /// Exact assistant/output items for the selected wire API, including reasoning.
    pub history: Vec<Value>,
    pub raw: Value,
    pub finish_reason: Option<String>,
    pub record: RequestRecord,
}

/// Contains safe diagnostics only. Never attach headers, credentials or server error bodies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub kind: String,
    pub message: String,
    pub record: Box<RequestRecord>,
    /// Sanitized model output or an allowlisted provider error diagnostic, if available.
    pub raw: Option<Value>,
    /// Bounded machine-readable fields only; never retain the HTTP error body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<Box<str>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_type: Option<Box<str>>,
}
impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}
impl std::error::Error for ApiError {}

pub type CompletionFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ModelReply, ApiError>> + Send + 'a>>;
pub trait CompletionBackend: Send + Sync {
    fn api(&self) -> WireApi;
    fn complete<'a>(
        &'a self,
        model: &'a ModelConfig,
        history: &'a [Value],
        tools: &'a [ToolDefinition],
        choice: ToolChoice,
    ) -> CompletionFuture<'a>;
}
