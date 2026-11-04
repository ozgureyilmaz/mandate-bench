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
        };

        let request_value = match build_request(self.config.api, model, history, tools, choice) {
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

        for attempt_index in 0..=self.config.max_retries {
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
            let send_result = self.request_builder(&body).send().await;
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
                    if retryable && attempt_index < self.config.max_retries {
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
                                if retryable && attempt_index < self.config.max_retries {
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
                    if is_retryable_status(status) && attempt_index < self.config.max_retries {
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

    use super::{endpoint_url, redact_value, safe_error_field};
    use crate::types::WireApi;

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
