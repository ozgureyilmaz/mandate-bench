//! One authenticated, bounded HTTP transport for both supported wire APIs.

use std::{net::IpAddr, sync::Arc, time::Duration};

use anyhow::{Result, anyhow};
use chrono::{SecondsFormat, Utc};
use reqwest::{
    Client as HttpClient, Url,
    header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue},
    redirect::Policy,
};
use serde_json::Value;

use crate::{
    budget::{
        BudgetGuard, BudgetReservation, ReconcileResult, validate_budgeted_history_for_provider,
    },
    limits::ProviderLimits,
    retry::{is_retryable_status, retry_delay},
    types::{
        ApiError, AttemptRecord, CompletionBackend, CompletionFuture, ModelConfig, ModelReply,
        ProviderConfig, RequestRecord, TokenUsage, ToolChoice, ToolDefinition, WireApi,
    },
    wire::{build_request, parse_response},
};

/// Provider credentials are deliberately private and this type has no `Debug` impl.
#[derive(Clone)]
pub struct Client {
    name: String,
    config: ProviderConfig,
    key: String,
    http: HttpClient,
    endpoint: Url,
    authorization: HeaderValue,
    limits: Arc<ProviderLimits>,
    budget: Option<Arc<BudgetGuard>>,
}

impl Client {
    pub fn new(name: String, config: ProviderConfig, key: String) -> Result<Self> {
        let config = config
            .resolved(&name)
            .map_err(|_| anyhow!("provider configuration could not be resolved"))?;
        Self::new_resolved(name, config, key)
    }

    fn new_resolved(name: String, config: ProviderConfig, key: String) -> Result<Self> {
        if key.trim().is_empty() {
            return Err(anyhow!("provider API key is empty"));
        }
        if config.timeout_seconds == 0 {
            return Err(anyhow!("provider timeout must be positive"));
        }
        let endpoint = endpoint_url(&config.base_url, config.api)?;
        let mut authorization = HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|_| anyhow!("provider API key cannot be used as an authorization header"))?;
        authorization.set_sensitive(true);
        let http = HttpClient::builder()
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .map_err(|_| anyhow!("HTTP client could not be initialized"))?;
        let limits = ProviderLimits::for_provider(&name, &config)?;

        Ok(Self {
            name,
            config,
            key,
            http,
            endpoint,
            authorization,
            limits,
            budget: None,
        })
    }

    pub fn from_env(name: &str, config: &ProviderConfig) -> Result<Self> {
        let resolved = config
            .resolved(name)
            .map_err(|_| anyhow!("provider configuration could not be resolved"))?;
        let key = std::env::var(&resolved.key_env)
            .map_err(|_| anyhow!("required provider API key is not set"))?;
        Self::new_resolved(name.to_owned(), resolved, key)
    }

    pub fn with_budget(mut self, budget: Arc<BudgetGuard>) -> Self {
        self.budget = Some(budget);
        self
    }

    async fn complete_inner(
        &self,
        model: &ModelConfig,
        history: &[Value],
        tools: &[ToolDefinition],
        choice: ToolChoice,
    ) -> Result<ModelReply, ApiError> {
        let started_at = timestamp();
        let mut record = RequestRecord {
            provider: self.name.clone(),
            requested_model: model.id.clone(),
            model: None,
            api: self.config.api,
            http_status: None,
            retries: 0,
            started_at,
            ended_at: String::new(),
            usage: TokenUsage::default(),
            attempts: Vec::new(),
            budget: None,
        };

        if self.name == "meta" && self.budget.is_none() && !self.is_meta_loopback_test_endpoint() {
            record.ended_at = timestamp();
            record.budget = Some(BudgetGuard::mark_policy_rejected());
            return Err(api_error(
                "budget_required",
                "Meta Muse Spark requests require the durable paid budget guard",
                record,
            ));
        }

        let mut request_value = match build_request(self.config.api, model, history, tools, choice)
        {
            Ok(value) => value,
            Err(_) => {
                record.ended_at = timestamp();
                return Err(api_error(
                    "request_build",
                    "provider request could not be built",
                    record,
                ));
            }
        };
        if self.budget.is_some() {
            if model.provider != self.name
                || !self.budget_endpoint_allowed()
                || validate_budgeted_history_for_provider(&self.name, history).is_err()
            {
                record.ended_at = timestamp();
                record.budget = Some(BudgetGuard::mark_policy_rejected());
                return Err(api_error(
                    "budget_policy",
                    "budgeted requests require an approved official-provider Responses contract",
                    record,
                ));
            }
            match self.name.as_str() {
                "openai" => {
                    request_value["service_tier"] = Value::String("default".into());
                    request_value
                        .as_object_mut()
                        .expect("request is object")
                        .remove("include");
                }
                "meta" => {
                    // Meta's stateless Responses loop requires encrypted reasoning replay.
                    if request_value["store"] != false
                        || request_value["include"]
                            != serde_json::json!(["reasoning.encrypted_content"])
                        || request_value.get("service_tier").is_some()
                    {
                        record.ended_at = timestamp();
                        record.budget = Some(BudgetGuard::mark_policy_rejected());
                        return Err(api_error(
                            "budget_policy",
                            "Meta requests must use stateless encrypted reasoning replay",
                            record,
                        ));
                    }
                }
                _ => {
                    record.ended_at = timestamp();
                    record.budget = Some(BudgetGuard::mark_policy_rejected());
                    return Err(api_error(
                        "budget_policy",
                        "budgeted requests are not enabled for this provider",
                        record,
                    ));
                }
            }
        }
        let body = match serde_json::to_vec(&request_value) {
            Ok(bytes) => bytes,
            Err(_) => {
                record.ended_at = timestamp();
                return Err(api_error(
                    "request_serialization",
                    "provider request could not be serialized",
                    record,
                ));
            }
        };
        let estimated_tokens =
            (body.len() as u64).saturating_add(u64::from(model.max_output_tokens));

        let reservation = if let Some(budget) = &self.budget {
            match budget.reserve(model, body.len()) {
                Ok(reservation) => Some(reservation),
                Err(_) => {
                    record.ended_at = timestamp();
                    record.budget = Some(BudgetGuard::mark_policy_rejected());
                    return Err(api_error(
                        "budget_exhausted_or_policy",
                        "durable paid budget reservation refused; no request sent",
                        record,
                    ));
                }
            }
        } else {
            None
        };
        if let Some(reservation) = reservation {
            record.budget = Some(crate::types::BudgetEvidence {
                reservation_id: Some(reservation.id),
                reserved_usd: reservation.reserved_usd,
                charged_usd: None,
                state: crate::types::BudgetReservationState::Reserved,
                input_token_bound: Some(reservation.input_token_bound),
                output_token_bound: Some(reservation.output_token_bound),
            });
        }
        // Each budgeted logical request has exactly one HTTP attempt. A failure
        // retains the worst-case reservation and prevents a new paid request.
        let result = self.complete_http(&body, estimated_tokens, record).await;
        self.reconcile_budget(reservation, result)
    }

    fn budget_endpoint_allowed(&self) -> bool {
        if self.config.api != WireApi::Responses {
            return false;
        }
        #[cfg(test)]
        if self.endpoint.scheme() == "http"
            && self
                .endpoint
                .host_str()
                .is_some_and(|host| host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback()))
        {
            return true;
        }
        match self.name.as_str() {
            "openai" => self.endpoint.as_str() == "https://api.openai.com/v1/responses",
            "meta" => self.endpoint.as_str() == "https://api.meta.ai/v1/responses",
            _ => false,
        }
    }

    fn is_meta_loopback_test_endpoint(&self) -> bool {
        self.name == "meta"
            && self.endpoint.scheme() == "http"
            && self.endpoint.path() == "/v1/responses"
            && self
                .endpoint
                .host_str()
                .and_then(|host| host.parse::<IpAddr>().ok())
                .is_some_and(|address| address.is_loopback())
    }

    fn reconcile_budget(
        &self,
        reservation: Option<BudgetReservation>,
        mut result: Result<ModelReply, ApiError>,
    ) -> Result<ModelReply, ApiError> {
        let (Some(budget), Some(reservation)) = (&self.budget, reservation) else {
            return result;
        };
        let failed = result.is_err();
        let record: &mut RequestRecord = match &mut result {
            Ok(reply) => &mut reply.record,
            Err(error) => &mut error.record,
        };
        let accounting = if result_is_provider_success(record) {
            budget
                .reconcile(reservation.id, &record.usage, failed)
                .map(|(evidence, state)| (evidence, state == ReconcileResult::Overrun))
        } else {
            budget
                .mark_provider_error(reservation.id)
                .map(|evidence| (evidence, false))
        };
        match accounting {
            Ok((evidence, overrun)) => {
                let record: &mut RequestRecord = match &mut result {
                    Ok(reply) => &mut reply.record,
                    Err(error) => &mut error.record,
                };
                record.budget = Some(evidence);
                if overrun {
                    return Err(budget_failure(
                        &result,
                        "budget_usage_overrun",
                        "reported usage exceeded the reserved bound; paid ledger halted",
                    ));
                }
                result
            }
            Err(_) => Err(budget_failure(
                &result,
                "budget_accounting",
                "budget reconciliation failed; reservation remains held and paid dispatch stopped",
            )),
        }
    }

    async fn complete_http(
        &self,
        body: &[u8],
        estimated_tokens: u64,
        mut record: RequestRecord,
    ) -> Result<ModelReply, ApiError> {
        let max_retries = if self.budget.is_some() {
            0
        } else {
            self.config.max_retries
        };

        for attempt_index in 0..=max_retries {
            record.http_status = None;
            if self.limits.validate_reservation(estimated_tokens).is_err() {
                record.ended_at = timestamp();
                return Err(api_error(
                    "tpm_capacity",
                    "request token reservation exceeds provider capacity",
                    record,
                ));
            }

            let permit = match self.limits.begin_attempt(estimated_tokens).await {
                Ok(permit) => permit,
                Err(_) => {
                    record.ended_at = timestamp();
                    return Err(api_error(
                        "concurrency_limit",
                        "provider concurrency limit is unavailable",
                        record,
                    ));
                }
            };

            let attempt_started = timestamp();
            let send_result = self.request_builder(body).send().await;
            match send_result {
                Err(error) => {
                    let retryable = is_retryable_transport(&error);
                    let mut attempt = AttemptRecord {
                        http_status: None,
                        started_at: attempt_started,
                        ended_at: timestamp(),
                        request_id: None,
                        retry_delay_ms: None,
                    };
                    if retryable && attempt_index < max_retries {
                        let delay = retry_delay(&HeaderMap::new(), attempt_index + 1);
                        attempt.retry_delay_ms = Some(duration_millis(delay));
                        record.attempts.push(attempt);
                        record.retries = attempt_index + 1;
                        record.ended_at = timestamp();
                        drop(permit);
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    record.attempts.push(attempt);
                    record.ended_at = timestamp();
                    drop(permit);
                    return Err(api_error(
                        if error.is_timeout() {
                            "timeout"
                        } else if error.is_connect() {
                            "connection"
                        } else {
                            "transport"
                        },
                        transport_message(&error),
                        record,
                    ));
                }
                Ok(response) => {
                    let status = response.status();
                    let headers = response.headers().clone();
                    let request_id = response_request_id(&headers, &self.key);
                    record.http_status = Some(status.as_u16());

                    if status.is_success() {
                        match response.bytes().await {
                            Ok(bytes) => {
                                let attempt = AttemptRecord {
                                    http_status: Some(status.as_u16()),
                                    started_at: attempt_started,
                                    ended_at: timestamp(),
                                    request_id,
                                    retry_delay_ms: None,
                                };
                                let raw: Value = match serde_json::from_slice(&bytes) {
                                    Ok(raw) => raw,
                                    Err(_) => {
                                        record.attempts.push(attempt);
                                        record.ended_at = timestamp();
                                        drop(permit);
                                        let mut error = api_error(
                                            "malformed_response",
                                            "provider returned malformed JSON",
                                            record,
                                        );
                                        error.raw = Some(Value::String(redact_string(
                                            &String::from_utf8_lossy(&bytes),
                                            &self.key,
                                        )));
                                        return Err(error);
                                    }
                                };
                                let raw = redact_value(raw, &self.key);
                                record_response_metadata(&mut record, self.config.api, &raw);
                                record.retries = attempt_index;
                                record.attempts.push(attempt);
                                record.ended_at = timestamp();
                                drop(permit);
                                let error_record = record.clone();
                                let failure_raw = raw.clone();
                                return parse_response(self.config.api, raw, record).map_err(
                                    |_| {
                                        // Parser details can contain provider payload text; keep them out of diagnostics.
                                        let mut error = api_error(
                                            "invalid_provider_response",
                                            "provider response did not match the selected API",
                                            error_record,
                                        );
                                        error.raw = Some(failure_raw);
                                        error
                                    },
                                );
                            }
                            Err(error) => {
                                let retryable = is_retryable_transport(&error);
                                let mut attempt = AttemptRecord {
                                    http_status: Some(status.as_u16()),
                                    started_at: attempt_started,
                                    ended_at: timestamp(),
                                    request_id,
                                    retry_delay_ms: None,
                                };
                                if retryable && attempt_index < max_retries {
                                    let delay = retry_delay(&HeaderMap::new(), attempt_index + 1);
                                    attempt.retry_delay_ms = Some(duration_millis(delay));
                                    record.attempts.push(attempt);
                                    record.retries = attempt_index + 1;
                                    record.ended_at = timestamp();
                                    drop(permit);
                                    tokio::time::sleep(delay).await;
                                    continue;
                                }
                                record.attempts.push(attempt);
                                record.ended_at = timestamp();
                                drop(permit);
                                return Err(api_error(
                                    if error.is_timeout() {
                                        "timeout"
                                    } else if error.is_connect() {
                                        "connection"
                                    } else {
                                        "transport"
                                    },
                                    transport_message(&error),
                                    record,
                                ));
                            }
                        }
                    }

                    let diagnostics = response.json::<Value>().await.ok();
                    let error_code = diagnostics
                        .as_ref()
                        .and_then(|value| safe_error_field(value, "code", &self.key));
                    let error_type = diagnostics
                        .as_ref()
                        .and_then(|value| safe_error_field(value, "type", &self.key));
                    let mut attempt = AttemptRecord {
                        http_status: Some(status.as_u16()),
                        started_at: attempt_started,
                        ended_at: timestamp(),
                        request_id,
                        retry_delay_ms: None,
                    };
                    if is_retryable_status(status) && attempt_index < max_retries {
                        let delay = retry_delay(&headers, attempt_index + 1);
                        attempt.retry_delay_ms = Some(duration_millis(delay));
                        record.attempts.push(attempt);
                        record.retries = attempt_index + 1;
                        record.ended_at = timestamp();
                        drop(permit);
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    record.attempts.push(attempt);
                    record.ended_at = timestamp();
                    drop(permit);
                    let mut error =
                        api_error("http_status", &format!("HTTP {}", status.as_u16()), record);
                    error.error_code = error_code.map(String::into_boxed_str);
                    error.error_type = error_type.map(String::into_boxed_str);
                    return Err(error);
                }
            }
        }

        record.ended_at = timestamp();
        Err(api_error(
            "retries_exhausted",
            "provider request retries exhausted",
            record,
        ))
    }

    fn request_builder(&self, body: &[u8]) -> reqwest::RequestBuilder {
        self.http
            .post(self.endpoint.clone())
            .header(AUTHORIZATION, self.authorization.clone())
            .header(CONTENT_TYPE, "application/json")
            .body(body.to_vec())
    }
}

impl CompletionBackend for Client {
    fn api(&self) -> WireApi {
        self.config.api
    }

    fn complete<'a>(
        &'a self,
        model: &'a ModelConfig,
        history: &'a [Value],
        tools: &'a [ToolDefinition],
        choice: ToolChoice,
    ) -> CompletionFuture<'a> {
        Box::pin(self.complete_inner(model, history, tools, choice))
    }
}

fn result_is_provider_success(record: &RequestRecord) -> bool {
    record
        .http_status
        .is_some_and(|status| (200..300).contains(&status))
}

fn budget_failure(result: &Result<ModelReply, ApiError>, kind: &str, message: &str) -> ApiError {
    let (record, raw) = match result {
        Ok(reply) => (reply.record.clone(), Some(reply.raw.clone())),
        Err(error) => ((*error.record).clone(), error.raw.clone()),
    };
    let mut error = api_error(kind, message, record);
    error.raw = raw;
    error
}

fn endpoint_url(base: &str, api: WireApi) -> Result<Url> {
    let parsed = Url::parse(base).map_err(|_| anyhow!("provider base URL is invalid"))?;
    if !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.host_str().is_none()
    {
        return Err(anyhow!("provider base URL contains a forbidden component"));
    }
    let https = parsed.scheme() == "https";
    let literal_loopback = parsed
        .host_str()
        .and_then(|host| host.trim_matches(['[', ']']).parse::<IpAddr>().ok())
        .is_some_and(|address| address.is_loopback());
    if !https && !(parsed.scheme() == "http" && literal_loopback) {
        return Err(anyhow!(
            "provider URL must use HTTPS outside literal loopback tests"
        ));
    }

    let suffix = match api {
        WireApi::Responses => "responses",
        WireApi::ChatCompletions => "chat/completions",
    };
    let endpoint = format!("{}/{}", base.trim_end_matches('/'), suffix);
    Url::parse(&endpoint).map_err(|_| anyhow!("provider endpoint URL is invalid"))
}

fn response_request_id(headers: &HeaderMap, key: &str) -> Option<String> {
    [
        "x-request-id",
        "request-id",
        "openai-request-id",
        "x-ds-trace-id",
        "nvcf-reqid",
        "x-nvidia-request-id",
    ]
    .iter()
    .find_map(|name| headers.get(*name).and_then(|value| value.to_str().ok()))
    .map(|value| redact_string(value, key))
}

fn is_retryable_transport(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request() || error.is_body()
}

fn transport_message(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "provider request timed out"
    } else if error.is_connect() {
        "provider connection failed"
    } else {
        "provider transport failed"
    }
}

fn api_error(kind: &str, message: &str, record: RequestRecord) -> ApiError {
    ApiError {
        kind: kind.to_owned(),
        message: message.to_owned(),
        record: Box::new(record),
        raw: None,
        error_code: None,
        error_type: None,
    }
}

fn safe_error_field(body: &Value, field: &str, key: &str) -> Option<String> {
    let text = body.get("error")?.get(field)?.as_str()?;
    let redacted = redact_string(text, key);
    (!redacted.is_empty()
        && redacted.len() <= 128
        && redacted
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')))
    .then_some(redacted)
}

fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn duration_millis(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn redact_string(value: &str, key: &str) -> String {
    if key.is_empty() {
        return value.to_owned();
    }
    value.replace(key, "[REDACTED]")
}

fn redact_value(value: Value, key: &str) -> Value {
    match value {
        Value::String(value) => Value::String(redact_string(&value, key)),
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|value| redact_value(value, key))
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(name, value)| (redact_string(&name, key), redact_value(value, key)))
                .collect(),
        ),
        scalar => scalar,
    }
}

fn record_response_metadata(record: &mut RequestRecord, api: WireApi, raw: &Value) {
    record.model = raw
        .get("model")
        .and_then(Value::as_str)
        .filter(|model| !model.trim().is_empty())
        .map(str::to_owned);
    let usage = raw.get("usage");
    let count = |value: Option<&Value>| value.and_then(Value::as_u64);
    record.usage = match api {
        WireApi::Responses => TokenUsage {
            input: count(usage.and_then(|usage| usage.get("input_tokens"))),
            cached_input: count(
                usage.and_then(|usage| usage.pointer("/input_tokens_details/cached_tokens")),
            ),
            output: count(usage.and_then(|usage| usage.get("output_tokens"))),
            reasoning: count(
                usage.and_then(|usage| usage.pointer("/output_tokens_details/reasoning_tokens")),
            ),
        },
        WireApi::ChatCompletions => {
            let cached_input = usage
                .and_then(|usage| usage.get("prompt_cache_hit_tokens"))
                .filter(|value| !value.is_null())
                .or_else(|| {
                    usage.and_then(|usage| usage.pointer("/prompt_tokens_details/cached_tokens"))
                });
            TokenUsage {
                input: count(usage.and_then(|usage| usage.get("prompt_tokens"))),
                cached_input: count(cached_input),
                output: count(usage.and_then(|usage| usage.get("completion_tokens"))),
                reasoning: count(usage.and_then(|usage| {
                    usage.pointer("/completion_tokens_details/reasoning_tokens")
                })),
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use std::{
        fs,
        io::{Read, Write},
        net::TcpListener,
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
    };

    use super::*;
    use crate::types::BudgetConfig;

    static NEXT_BUDGET_TEST: AtomicUsize = AtomicUsize::new(0);
    struct BudgetTest(PathBuf);
    impl BudgetTest {
        fn new(initial: rust_decimal::Decimal) -> (Self, Arc<BudgetGuard>) {
            let path = std::env::temp_dir().join(format!(
                "mandate-budget-client-{}-{}",
                std::process::id(),
                NEXT_BUDGET_TEST.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            let guard = BudgetGuard::open(
                path.join("ledger.json"),
                &BudgetConfig {
                    ceiling_usd: rust_decimal::Decimal::ONE,
                    initial_charge_usd: initial,
                },
            )
            .unwrap();
            (Self(path), guard)
        }

        fn new_meta() -> (Self, Arc<BudgetGuard>) {
            let path = std::env::temp_dir().join(format!(
                "mandate-meta-budget-client-{}-{}",
                std::process::id(),
                NEXT_BUDGET_TEST.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            let config = BudgetConfig {
                ceiling_usd: crate::budget::META_MUSE_BUDGET_CEILING_USD,
                initial_charge_usd: rust_decimal::Decimal::ZERO,
            };
            let guard =
                BudgetGuard::open_for_provider(path.join("ledger.json"), "meta", &config).unwrap();
            (Self(path), guard)
        }
    }
    impl Drop for BudgetTest {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn budget_model() -> ModelConfig {
        ModelConfig {
            id: "gpt-6-luna".into(),
            provider: "openai".into(),
            max_output_tokens: 1024,
            reasoning_effort: Some("none".into()),
            temperature: None,
            thinking: None,
            seed: None,
            price: None,
        }
    }
    fn budget_client(listener: &TcpListener, guard: Arc<BudgetGuard>) -> Client {
        Client::new_resolved(
            "openai".into(),
            ProviderConfig {
                base_url: format!("http://{}/v1", listener.local_addr().unwrap()),
                key_env: "LOCAL_TEST_KEY".into(),
                api: WireApi::Responses,
                max_concurrency: 1,
                rpm: None,
                tpm: None,
                max_retries: 6,
                timeout_seconds: 2,
            },
            "local-test-only".into(),
        )
        .unwrap()
        .with_budget(guard)
    }

    fn meta_client_without_budget(base_url: &str) -> Client {
        Client::new_resolved(
            "meta".into(),
            ProviderConfig {
                base_url: base_url.to_owned(),
                key_env: "LOCAL_TEST_KEY".into(),
                api: WireApi::Responses,
                max_concurrency: 1,
                rpm: None,
                tpm: None,
                max_retries: 0,
                timeout_seconds: 2,
            },
            "local-test-only".into(),
        )
        .unwrap()
    }

    fn meta_budget_client(listener: &TcpListener, guard: Arc<BudgetGuard>) -> Client {
        Client::new_resolved(
            "meta".into(),
            ProviderConfig {
                base_url: format!("http://{}/v1", listener.local_addr().unwrap()),
                key_env: "LOCAL_TEST_KEY".into(),
                api: WireApi::Responses,
                max_concurrency: 1,
                rpm: None,
                tpm: None,
                max_retries: 0,
                timeout_seconds: 2,
            },
            "local-test-only".into(),
        )
        .unwrap()
        .with_budget(guard)
    }

    fn meta_model() -> ModelConfig {
        ModelConfig {
            id: "muse-spark-1.1".into(),
            provider: "meta".into(),
            max_output_tokens: 1024,
            reasoning_effort: Some("medium".into()),
            temperature: None,
            thinking: None,
            seed: None,
            price: None,
        }
    }
    fn budget_server(
        listener: TcpListener,
        path: PathBuf,
        responses: Vec<(u16, Value)>,
    ) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            for (status, response) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = Vec::new();
                let header_end = loop {
                    let mut buffer = [0; 4096];
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(i) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&bytes[..header_end]);
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .unwrap();
                while bytes.len() < header_end + length {
                    let mut buffer = [0; 4096];
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                }
                let request: Value =
                    serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
                if request["model"] == "muse-spark-1.1" {
                    assert_eq!(request["store"], false);
                    assert_eq!(request["include"], json!(["reasoning.encrypted_content"]));
                    assert_eq!(request["reasoning"]["effort"], "medium");
                    assert!(request.get("service_tier").is_none());
                    for call in request["input"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|item| item["type"] == "function_call_output")
                    {
                        let call_id = call["call_id"].as_str().unwrap();
                        assert!(request["input"].as_array().unwrap().iter().any(|item| {
                            item["type"] == "function_call" && item["call_id"] == call_id
                        }));
                    }
                } else {
                    assert_eq!(request["service_tier"], "default");
                    assert!(request.get("include").is_none());
                }
                let ledger: Value =
                    serde_json::from_slice(&fs::read(path.join("ledger.json")).unwrap()).unwrap();
                assert_eq!(
                    ledger["entries"].as_array().unwrap().last().unwrap()["state"],
                    "reserved",
                    "reservation must be durable before HTTP arrives"
                );
                let body = serde_json::to_vec(&response).unwrap();
                write!(stream,"HTTP/1.1 {status} Local\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
                stream.write_all(&body).unwrap();
            }
        })
    }

    #[tokio::test]
    async fn paid_reservation_refuses_before_network_when_historical_ceiling_is_held() {
        let (_temp, guard) = BudgetTest::new(rust_decimal::Decimal::ONE);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let client = budget_client(&listener, guard.clone());
        let error = client
            .complete(
                &budget_model(),
                &[json!({"role":"user","content":"hello"})],
                &[],
                ToolChoice::None,
            )
            .await
            .unwrap_err();
        assert!(error.record.attempts.is_empty());
        assert_eq!(
            guard.snapshot().unwrap().remaining_usd,
            rust_decimal::Decimal::ZERO
        );
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[tokio::test]
    async fn meta_requests_are_refused_before_network_without_the_paid_guard() {
        let client = meta_client_without_budget("https://api.meta.ai/v1");
        assert!(!client.is_meta_loopback_test_endpoint());
        let error = client
            .complete(
                &meta_model(),
                &[json!({"role":"user","content":"hello"})],
                &[],
                ToolChoice::None,
            )
            .await
            .unwrap_err();
        assert_eq!(error.kind, "budget_required");
        assert!(error.record.attempts.is_empty());
        assert!(error.record.http_status.is_none());

        let loopback = meta_client_without_budget("http://127.0.0.1:9900/v1");
        assert!(loopback.is_meta_loopback_test_endpoint());
    }

    #[tokio::test]
    async fn meta_budgeted_tool_round_trip_reserves_and_replays_encrypted_reasoning() {
        let (temp, guard) = BudgetTest::new_meta();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = meta_budget_client(&listener, guard.clone());
        let server = budget_server(
            listener,
            temp.0.clone(),
            vec![
                (
                    200,
                    json!({
                        "model":"muse-spark-1.1",
                        "status":"completed",
                        "output":[
                            {"type":"reasoning","id":"rs_local","summary":[],"encrypted_content":"opaque-test-state"},
                            {"type":"function_call","id":"fc_local","call_id":"call_local","name":"get_offer","arguments":r#"{"offer_id":"A"}"#,"status":"completed"}
                        ],
                        "usage":{"input_tokens":10,"output_tokens":7,"output_tokens_details":{"reasoning_tokens":4}}
                    }),
                ),
                (
                    200,
                    json!({
                        "model":"muse-spark-1.1",
                        "status":"completed",
                        "output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Offer A costs USD 42."}]}],
                        "usage":{"input_tokens":15,"output_tokens":5,"output_tokens_details":{"reasoning_tokens":2}}
                    }),
                ),
            ],
        );
        let model = meta_model();
        let tools = [ToolDefinition {
            name: "get_offer".into(),
            description: "Fetch one offer.".into(),
            parameters: json!({
                "type":"object",
                "properties":{"offer_id":{"type":"string"}},
                "required":["offer_id"],
                "additionalProperties":false
            }),
        }];
        let initial_history = vec![
            json!({"role":"system","content":"Use the offer tool."}),
            json!({"role":"user","content":"Find offer A."}),
        ];
        let first = client
            .complete(&model, &initial_history, &tools, ToolChoice::Auto)
            .await
            .unwrap();
        assert_eq!(first.calls.len(), 1);
        assert_eq!(first.record.model.as_deref(), Some("muse-spark-1.1"));
        let mut replay = initial_history;
        replay.extend(first.history);
        replay.push(crate::wire::tool_result(
            WireApi::Responses,
            &first.calls[0],
            &json!({"offer_id":"A","price":42}),
        ));
        let second = client
            .complete(&model, &replay, &tools, ToolChoice::Auto)
            .await
            .unwrap();
        assert_eq!(second.text, "Offer A costs USD 42.");
        assert_eq!(second.record.model.as_deref(), Some("muse-spark-1.1"));
        assert_eq!(
            second.record.budget.as_ref().unwrap().state,
            crate::types::BudgetReservationState::Settled
        );
        assert!(guard.snapshot().unwrap().settled_usd > rust_decimal::Decimal::ZERO);
        assert!(!guard.snapshot().unwrap().halted);
        server.join().unwrap();
    }

    #[tokio::test]
    async fn paid_response_reconciles_and_first_provider_error_stops_all_retries() {
        let (temp, guard) = BudgetTest::new(rust_decimal::Decimal::ZERO);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = budget_client(&listener, guard.clone());
        let success = json!({"model":"gpt-6-luna","status":"completed","output":[],"usage":{"input_tokens":10,"output_tokens":5}});
        let server = budget_server(
            listener,
            temp.0.clone(),
            vec![
                (200, success),
                (429, json!({"error":{"code":"rate_limit_exceeded"}})),
            ],
        );
        let model = budget_model();
        let history = [json!({"role":"user","content":"hello"})];
        let reply = client
            .complete(&model, &history, &[], ToolChoice::None)
            .await
            .unwrap();
        assert_eq!(
            reply.record.budget.unwrap().state,
            crate::types::BudgetReservationState::Settled
        );
        assert_eq!(
            guard.snapshot().unwrap().settled_usd,
            rust_decimal::Decimal::new(875, 8)
        );
        let error = tokio::time::timeout(
            Duration::from_secs(3),
            client.complete(&model, &history, &[], ToolChoice::None),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert_eq!(error.record.attempts.len(), 1);
        assert_eq!(error.record.retries, 0);
        assert!(guard.snapshot().unwrap().halted);
        assert!(guard.snapshot().unwrap().held_usd > rust_decimal::Decimal::ZERO);
        let stopped = client
            .complete(&model, &history, &[], ToolChoice::None)
            .await
            .unwrap_err();
        assert!(stopped.record.attempts.is_empty());
        server.join().unwrap();
    }

    #[tokio::test]
    async fn successful_response_without_usage_retains_the_whole_reservation() {
        let (temp, guard) = BudgetTest::new(rust_decimal::Decimal::ZERO);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = budget_client(&listener, guard.clone());
        let server = budget_server(
            listener,
            temp.0.clone(),
            vec![(
                200,
                json!({"model":"gpt-6-luna","status":"completed","output":[]}),
            )],
        );
        let reply = client
            .complete(
                &budget_model(),
                &[json!({"role":"user","content":"hello"})],
                &[],
                ToolChoice::None,
            )
            .await
            .unwrap();
        let evidence = reply.record.budget.unwrap();
        assert_eq!(
            evidence.state,
            crate::types::BudgetReservationState::Unknown
        );
        assert_eq!(guard.snapshot().unwrap().held_usd, evidence.reserved_usd);
        assert_eq!(
            guard.snapshot().unwrap().settled_usd,
            rust_decimal::Decimal::ZERO
        );
        server.join().unwrap();
    }

    #[test]
    fn endpoint_validation_rejects_insecure_or_credential_bearing_urls() {
        assert!(endpoint_url("http://example.com/v1", WireApi::ChatCompletions).is_err());
        assert!(endpoint_url("http://localhost:8888/v1", WireApi::ChatCompletions).is_err());
        assert!(endpoint_url("https://user:pass@example.com/v1", WireApi::Responses).is_err());
        assert!(endpoint_url("https://example.com/v1?token=x", WireApi::Responses).is_err());
        assert!(endpoint_url("http://127.0.0.1:8888/v1", WireApi::Responses).is_ok());
        assert!(endpoint_url("http://[::1]:8888/v1", WireApi::Responses).is_ok());
    }

    #[test]
    fn nested_reply_strings_and_keys_redact_the_exact_credential() {
        let value = json!({
            "top": "prefix-secret-value-suffix",
            "nested": ["secret-value", {"secret-value": "secret-value"}],
            "number": 7
        });
        let redacted = redact_value(value, "secret-value");
        assert_eq!(redacted["top"], "prefix-[REDACTED]-suffix");
        assert_eq!(redacted["nested"][0], "[REDACTED]");
        assert_eq!(redacted["nested"][1]["[REDACTED]"], "[REDACTED]");
        assert_eq!(redacted["number"], 7);
        assert_eq!(
            safe_error_field(
                &json!({"error":{"code":"rate_limit_exceeded"}}),
                "code",
                "secret-value"
            ),
            Some("rate_limit_exceeded".into())
        );
        assert_eq!(
            safe_error_field(
                &json!({"error":{"code":"secret-value"}}),
                "code",
                "secret-value"
            ),
            None
        );
        assert_eq!(
            safe_error_field(
                &json!({"error":{"code":"arbitrary error message with spaces"}}),
                "code",
                "secret-value"
            ),
            None
        );
    }
}
