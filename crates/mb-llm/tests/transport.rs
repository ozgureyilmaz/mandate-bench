use std::{
    future::pending,
    io,
    sync::{Arc, Mutex},
};

use mb_llm::{Client, CompletionBackend, ModelConfig, Price, ProviderConfig, ToolChoice, WireApi};
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

const TEST_KEY: &str = "transport-test-secret-sentinel";
static BASE_URL_ENV_LOCK: Mutex<()> = Mutex::new(());

struct MockReply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

impl MockReply {
    fn json(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: Some(body.into()),
        }
    }

    fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    fn hang() -> Self {
        Self {
            status: 200,
            headers: Vec::new(),
            body: None,
        }
    }
}

struct MockServer {
    base_url: String,
    requests: Arc<Mutex<Vec<Vec<u8>>>>,
    task: JoinHandle<()>,
}

impl MockServer {
    async fn start(replies: Vec<MockReply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);
        let task = tokio::spawn(async move {
            for reply in replies {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let Ok(request) = read_request(&mut stream).await else {
                    return;
                };
                recorded.lock().unwrap().push(request);
                if reply.status == 0 {
                    continue;
                } // Close before sending response headers.
                let Some(body) = reply.body else {
                    tokio::spawn(async move {
                        let _stream = stream;
                        pending::<()>().await;
                    });
                    continue;
                };

                let reason = match reply.status {
                    200 => "OK",
                    302 => "Found",
                    401 => "Unauthorized",
                    408 => "Request Timeout",
                    429 => "Too Many Requests",
                    500 => "Internal Server Error",
                    503 => "Service Unavailable",
                    _ => "Mock Response",
                };
                let mut response = format!(
                    "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
                    reply.status,
                    reason,
                    body.len()
                );
                for (name, value) in reply.headers {
                    response.push_str(&format!("{name}: {value}\r\n"));
                }
                response.push_str("\r\n");
                response.push_str(&body);
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            }
        });
        Self {
            base_url: format!("http://{address}/v1"),
            requests,
            task,
        }
    }

    async fn finish(self) -> Vec<Vec<u8>> {
        let _ = self.task.await;
        Arc::try_unwrap(self.requests)
            .unwrap()
            .into_inner()
            .unwrap()
    }

    fn abort(self) -> Arc<Mutex<Vec<Vec<u8>>>> {
        self.task.abort();
        Arc::clone(&self.requests)
    }
}

fn client(base_url: &str, max_retries: u32, timeout_seconds: u64, tpm: Option<u64>) -> Client {
    let _guard = BASE_URL_ENV_LOCK.lock().unwrap();
    let previous = std::env::var_os("NVIDIA_BASE_URL");
    // The test serializes all client construction and points the override only at
    // its loopback server, so an ambient user override can never send test traffic out.
    unsafe { std::env::set_var("NVIDIA_BASE_URL", base_url) };
    let result = Client::new(
        "nvidia".into(),
        ProviderConfig {
            base_url: base_url.to_owned(),
            key_env: "NVIDIA_API_KEY".into(),
            api: WireApi::ChatCompletions,
            max_concurrency: 2,
            rpm: None,
            tpm,
            max_retries,
            timeout_seconds,
        },
        TEST_KEY.into(),
    );
    unsafe {
        if let Some(previous) = previous {
            std::env::set_var("NVIDIA_BASE_URL", previous);
        } else {
            std::env::remove_var("NVIDIA_BASE_URL");
        }
    }
    result.unwrap()
}

fn model() -> ModelConfig {
    ModelConfig {
        id: "nvidia/test-model".into(),
        provider: "nvidia".into(),
        max_output_tokens: 32,
        temperature: None,
        reasoning_effort: None,
        thinking: None,
        seed: None,
        price: Some(Price {
            input_per_million: 0.into(),
            cached_input_per_million: None,
            output_per_million: 0.into(),
        }),
    }
}

async fn complete(client: &Client) -> Result<mb_llm::ModelReply, mb_llm::ApiError> {
    let model = model();
    let history = [json!({"role":"user", "content":"test"})];
    client
        .complete(&model, &history, &[], ToolChoice::None)
        .await
}

async fn read_request(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 2048];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "request ended early",
            ));
        }
        bytes.extend_from_slice(&chunk[..read]);
        let Some(header_end) = find_header_end(&bytes) else {
            continue;
        };
        let headers = String::from_utf8_lossy(&bytes[..header_end]);
        let content_length = headers
            .lines()
            .skip(1)
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        if bytes.len() >= header_end + 4 + content_length {
            return Ok(bytes);
        }
    }
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn request_body(request: &[u8]) -> &[u8] {
    let header_end = find_header_end(request).unwrap();
    &request[header_end + 4..]
}

fn chat_reply(model: &str, content: &str) -> String {
    json!({
        "model": model,
        "choices": [{
            "finish_reason": "stop",
            "message": {"role": "assistant", "content": content}
        }],
        "usage": {"prompt_tokens": 12, "completion_tokens": 4}
    })
    .to_string()
}

#[tokio::test]
async fn a_dropped_connection_retries_the_same_request_without_hidden_http_retries() {
    let server = MockServer::start(vec![
        MockReply::json(0, ""),
        MockReply::json(200, chat_reply("nvidia/test-model", "ok")),
    ])
    .await;
    let client = client(&server.base_url, 1, 5, None);
    let reply = complete(&client).await.unwrap();
    let requests = server.finish().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(request_body(&requests[0]), request_body(&requests[1]));
    assert_eq!(reply.record.retries, 1);
    assert_eq!(reply.record.attempts[0].http_status, None);
    assert_eq!(reply.record.attempts[1].http_status, Some(200));
}

#[tokio::test]
async fn retry_reuses_identical_body_and_redacts_success_payload_recursively() {
    let server = MockServer::start(vec![
        MockReply::json(429, format!("{{\"error\":\"{TEST_KEY}\"}}"))
            .with_header("Retry-After", "0")
            .with_header("x-request-id", &format!("retry-{TEST_KEY}")),
        MockReply::json(
            200,
            chat_reply(
                &format!("nvidia/{TEST_KEY}-model"),
                &format!("echo {TEST_KEY}"),
            ),
        )
        .with_header("x-request-id", &format!("success-{TEST_KEY}")),
    ])
    .await;
    let client = client(&server.base_url, 2, 5, None);
    let reply = complete(&client).await.unwrap();
    let requests = server.finish().await;

    assert_eq!(requests.len(), 2);
    assert_eq!(request_body(&requests[0]), request_body(&requests[1]));
    assert!(String::from_utf8_lossy(&requests[0]).lines().any(|line| {
        line.to_ascii_lowercase().starts_with("authorization:") && line.contains(TEST_KEY)
    }));
    assert_eq!(reply.text, "echo [REDACTED]");
    assert_eq!(
        reply.record.model.as_deref(),
        Some("nvidia/[REDACTED]-model")
    );
    assert_eq!(reply.record.retries, 1);
    assert_eq!(reply.record.attempts.len(), 2);
    assert_eq!(reply.record.attempts[0].http_status, Some(429));
    assert_eq!(
        reply.record.attempts[0].request_id.as_deref(),
        Some("retry-[REDACTED]")
    );
    assert_eq!(reply.record.attempts[0].retry_delay_ms, Some(0));
    assert_eq!(reply.record.attempts[1].http_status, Some(200));
    assert_eq!(
        reply.record.attempts[1].request_id.as_deref(),
        Some("success-[REDACTED]")
    );
    assert!(!reply.raw.to_string().contains(TEST_KEY));
}

#[tokio::test]
async fn unauthorized_fails_fast_and_never_includes_server_body_or_key_in_error() {
    let server = MockServer::start(vec![
        MockReply::json(401, format!("{{\"error\":\"{TEST_KEY}\"}}"))
            .with_header("request-id", &format!("failed-{TEST_KEY}")),
    ])
    .await;
    let client = client(&server.base_url, 3, 5, None);
    let error = complete(&client).await.unwrap_err();
    let requests = server.finish().await;

    assert_eq!(requests.len(), 1);
    assert_eq!(error.kind, "http_status");
    assert_eq!(error.message, "HTTP 401");
    assert!(!error.to_string().contains(TEST_KEY));
    assert_eq!(error.record.http_status, Some(401));
    assert_eq!(error.record.attempts.len(), 1);
    assert_eq!(
        error.record.attempts[0].request_id.as_deref(),
        Some("failed-[REDACTED]")
    );
}

#[tokio::test]
async fn retries_exhaust_after_the_configured_number_and_record_every_attempt() {
    let server = MockServer::start(vec![
        MockReply::json(500, "internal error").with_header("Retry-After", "0"),
        MockReply::json(500, "internal error").with_header("Retry-After", "0"),
        MockReply::json(503, "unavailable").with_header("Retry-After", "0"),
    ])
    .await;
    let client = client(&server.base_url, 2, 5, None);
    let error = complete(&client).await.unwrap_err();
    let requests = server.finish().await;

    assert_eq!(requests.len(), 3);
    assert_eq!(error.kind, "http_status");
    assert_eq!(error.message, "HTTP 503");
    assert_eq!(error.record.retries, 2);
    assert_eq!(
        error
            .record
            .attempts
            .iter()
            .map(|attempt| attempt.http_status)
            .collect::<Vec<_>>(),
        [Some(500), Some(500), Some(503)]
    );
}

#[tokio::test]
async fn timeout_is_retried_and_each_attempt_is_recorded() {
    let server = MockServer::start(vec![MockReply::hang(), MockReply::hang()]).await;
    let client = client(&server.base_url, 1, 1, None);
    let error = complete(&client).await.unwrap_err();
    let requests = server.abort();

    assert_eq!(requests.lock().unwrap().len(), 2);
    assert_eq!(error.kind, "timeout");
    assert_eq!(error.record.retries, 1);
    assert_eq!(error.record.attempts.len(), 2);
    assert!(
        error
            .record
            .attempts
            .iter()
            .all(|attempt| attempt.http_status.is_none())
    );
}

#[tokio::test]
async fn final_transport_failure_clears_the_prior_http_status() {
    let server = MockServer::start(vec![
        MockReply::json(500, "temporary failure").with_header("Retry-After", "0"),
        MockReply::hang(),
    ])
    .await;
    let client = client(&server.base_url, 1, 1, None);
    let error = complete(&client).await.unwrap_err();
    let requests = server.abort();

    assert_eq!(requests.lock().unwrap().len(), 2);
    assert_eq!(error.kind, "timeout");
    assert_eq!(error.record.http_status, None);
    assert_eq!(
        error
            .record
            .attempts
            .iter()
            .map(|attempt| attempt.http_status)
            .collect::<Vec<_>>(),
        [Some(500), None]
    );
}

#[tokio::test]
async fn malformed_success_fails_without_retry() {
    let server = MockServer::start(vec![MockReply::json(200, "not-json")]).await;
    let client = client(&server.base_url, 3, 5, None);
    let error = complete(&client).await.unwrap_err();
    let requests = server.finish().await;

    assert_eq!(requests.len(), 1);
    assert_eq!(error.kind, "malformed_response");
    assert_eq!(error.record.retries, 0);
    assert_eq!(error.record.attempts[0].http_status, Some(200));
}

#[tokio::test]
async fn redirects_are_not_followed() {
    let redirect_server = MockServer::start(vec![
        MockReply::json(302, "").with_header("Location", "/v1/chat/completions"),
        MockReply::json(200, chat_reply("nvidia/test-model", "unexpected follow")),
    ])
    .await;
    let redirect_client = client(&redirect_server.base_url, 1, 5, None);
    let error = complete(&redirect_client).await.unwrap_err();
    let seen = redirect_server.abort();
    assert_eq!(error.message, "HTTP 302");
    assert_eq!(seen.lock().unwrap().len(), 1);
}
