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
    /// Sanitized model output from malformed successful responses, if available.
    pub raw: Option<Value>,
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
