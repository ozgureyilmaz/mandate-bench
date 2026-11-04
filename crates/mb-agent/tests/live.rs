use std::{collections::VecDeque, sync::Mutex};

use mb_agent::{EpisodeOutcome, run_episode, tool_definitions};
use mb_core::{Task, load_task};
use mb_llm::{
    ApiError, AttemptRecord, CompletionBackend, CompletionFuture, FunctionCall, ModelConfig,
    ModelReply, RequestRecord, TokenUsage, ToolChoice, ToolDefinition, WireApi,
};
use serde_json::{Value, json};

struct RequestSnapshot {
    history: Vec<Value>,
    choice: String,
    tool_count: usize,
}

struct MockBackend {
    api: WireApi,
    replies: Mutex<VecDeque<Result<ModelReply, ApiError>>>,
    requests: Mutex<Vec<RequestSnapshot>>,
}

impl MockBackend {
    fn new(api: WireApi, replies: Vec<Result<ModelReply, ApiError>>) -> Self {
        Self {
            api,
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

impl CompletionBackend for MockBackend {
    fn api(&self) -> WireApi {
        self.api
    }

    fn complete<'a>(
        &'a self,
        _model: &'a ModelConfig,
        history: &'a [Value],
        tools: &'a [ToolDefinition],
        choice: ToolChoice,
    ) -> CompletionFuture<'a> {
        let choice = match choice {
            ToolChoice::Auto => "auto",
            ToolChoice::None => "none",
            ToolChoice::Required(_) => "required",
        }
        .to_owned();
        self.requests.lock().unwrap().push(RequestSnapshot {
            history: history.to_vec(),
            choice,
            tool_count: tools.len(),
        });
        let response = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected completion request");
        Box::pin(async move { response })
    }
}

fn task() -> Task {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tasks/t00_control/task.json");
    load_task(&path).unwrap()
}

fn model() -> ModelConfig {
    ModelConfig {
        id: "mock-model".into(),
        provider: "openai".into(),
        max_output_tokens: 1024,
        temperature: None,
        reasoning_effort: None,
        thinking: None,
        seed: None,
        price: None,
    }
}

fn record(api: WireApi) -> RequestRecord {
    RequestRecord {
        provider: "mock".into(),
        requested_model: "mock-model".into(),
        model: Some("mock-returned-model".into()),
        api,
        http_status: Some(200),
        retries: 0,
        started_at: "2026-11-02T12:00:00Z".into(),
        ended_at: "2026-11-02T12:00:01Z".into(),
        usage: TokenUsage::default(),
        attempts: vec![AttemptRecord {
            http_status: Some(200),
            started_at: "2026-11-02T12:00:00Z".into(),
            ended_at: "2026-11-02T12:00:01Z".into(),
            request_id: Some("mock-request".into()),
            retry_delay_ms: None,
        }],
    }
}

fn reply(
    api: WireApi,
    text: &str,
    calls: Vec<FunctionCall>,
    history: Vec<Value>,
    finish_reason: &str,
) -> ModelReply {
    ModelReply {
        text: text.into(),
        calls,
        history,
        raw: json!({"model":"mock-returned-model","marker":"raw-response"}),
        finish_reason: Some(finish_reason.into()),
        record: record(api),
    }
}

fn function_call(id: &str, name: &str, arguments: &str) -> FunctionCall {
    FunctionCall {
        id: id.into(),
        name: name.into(),
        arguments: arguments.into(),
    }
}

fn call(id: &str, name: &str, arguments: Value) -> FunctionCall {
    function_call(id, name, &serde_json::to_string(&arguments).unwrap())
}

fn request_quotes_call(id: &str) -> FunctionCall {
    call(id, "request_quotes", json!({"query":"charger"}))
}

fn assistant_history(api: WireApi, suffix: &str, text: &str, calls: &[FunctionCall]) -> Vec<Value> {
    match api {
        WireApi::Responses => {
            let mut items = vec![json!({
                "type":"reasoning",
                "id":format!("reasoning-{suffix}"),
                "encrypted_content":format!("encrypted-{suffix}")
            })];
            items.extend(calls.iter().map(|call| {
                json!({
                    "type":"function_call",
                    "id":format!("fc-{}",call.id),
                    "call_id":call.id,
                    "name":call.name,
                    "arguments":call.arguments
                })
            }));
            if !text.is_empty() {
                items.push(json!({
                    "type":"message",
                    "role":"assistant",
                    "content":[{"type":"output_text","text":text}]
                }));
            }
            items
        }
        WireApi::ChatCompletions => vec![json!({
            "role":"assistant",
            "content":if text.is_empty() { Value::Null } else { Value::String(text.into()) },
            "reasoning_content":format!("reasoning-{suffix}"),
            "tool_calls":calls.iter().map(|call| json!({
                "id":call.id,
                "type":"function",
                "function":{"name":call.name,"arguments":call.arguments}
            })).collect::<Vec<_>>()
        })],
    }
}

fn final_buy() -> &'static str {
    r#"{"decision":"BUY","order":{"offer_id":"B","packs":1},"total":59.50,"currency":"USD","reason":"Selected the lowest eligible total."}"#
}

fn final_ask_user() -> &'static str {
    r#"{"decision":"ASK_USER","order":null,"total":null,"currency":"USD","reason":"Please clarify the blocker."}"#
}

fn buy_replies(api: WireApi) -> Vec<Result<ModelReply, ApiError>> {
    let quote_call = request_quotes_call("quotes-1");
    let order_call = call(
        "order-1",
        "place_order",
        json!({"offer_id":"B","packs":1,"authorized_total":59.50}),
    );
    vec![
        Ok(reply(
            api,
            final_buy(), // Text beside calls is not a final answer.
            vec![quote_call.clone()],
            assistant_history(api, "turn-1", final_buy(), &[quote_call]),
            "tool_calls",
        )),
        Ok(reply(
            api,
            "Tool response is not a final answer.",
            vec![order_call.clone()],
            assistant_history(
                api,
                "turn-2",
                "Tool response is not a final answer.",
                &[order_call],
            ),
            "tool_calls",
        )),
        Ok(reply(
            api,
            final_buy(),
            Vec::new(),
            assistant_history(api, "final", final_buy(), &[]),
            "stop",
        )),
    ]
}

#[tokio::test]
async fn live_buy_executes_calls_then_uses_later_final_for_both_wire_histories() {
    for api in [WireApi::Responses, WireApi::ChatCompletions] {
        let backend = MockBackend::new(api, buy_replies(api));
        let episode = run_episode(&backend, &model(), task(), 3).await;

        assert_eq!(episode.outcome, EpisodeOutcome::Complete);
        assert_eq!(episode.transcript.final_output, final_buy());
        assert_eq!(episode.transcript.tool_call_count, 2);
        assert_eq!(episode.transcript.replies.len(), 3);
        assert_eq!(episode.transcript.replies[0].raw["marker"], "raw-response");
        assert_eq!(episode.environment.ledger.len(), 2);
        assert_eq!(episode.environment.ledger[1].response["status"], "accepted");
        assert_eq!(episode.transcript.tool_calls.len(), 2);

        let requests = backend.requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(requests[0].history[0]["content"].is_string());
        assert!(
            requests[1]
                .history
                .iter()
                .any(|item| item.to_string().contains("reasoning-turn-1")
                    || item.to_string().contains("encrypted-turn-1"))
        );
        assert!(requests[1].history.iter().any(|item| match api {
            WireApi::Responses => item["type"] == "function_call_output",
            WireApi::ChatCompletions => item["role"] == "tool",
        }));
        assert!(requests.iter().all(|request| request.tool_count == 5));
        assert!(requests.iter().all(|request| request.choice == "auto"));

        let expected_history_tool = mb_llm::tool_result(
            api,
            &request_quotes_call("quotes-1"),
            &serde_json::Value::Array(
                task()
                    .offers
                    .iter()
                    .map(|offer| {
                        json!({
                            "offer_id":offer.offer_id,
                            "seller_id":offer.seller_id,
                            "title":offer.product.title,
                            "price":offer.price,
                            "currency":offer.currency,
                            "pack_size":offer.pack_size
                        })
                    })
                    .collect(),
            ),
        );
        assert!(episode.transcript.messages.contains(&expected_history_tool));
    }
}

#[tokio::test]
async fn ask_user_forces_one_final_request_with_tools_retained() {
    let api = WireApi::ChatCompletions;
    let backend = MockBackend::new(
        api,
        vec![
            {
                let ask_call = call(
                    "ask-1",
                    "ask_user",
                    json!({"question":"Which constraint should take priority?"}),
                );
                Ok(reply(
                    api,
                    "",
                    vec![ask_call.clone()],
                    assistant_history(api, "ask", "", &[ask_call]),
                    "tool_calls",
                ))
            },
            Ok(reply(
                api,
                final_ask_user(),
                Vec::new(),
                assistant_history(api, "ask-final", final_ask_user(), &[]),
                "stop",
            )),
        ],
    );
    let episode = run_episode(&backend, &model(), task(), 5).await;

    assert_eq!(episode.outcome, EpisodeOutcome::Complete);
    assert_eq!(episode.transcript.final_output, final_ask_user());
    assert!(episode.environment.stopped);
    assert_eq!(episode.transcript.tool_calls.len(), 1);
    assert_eq!(backend.request_count(), 2);
    let requests = backend.requests.lock().unwrap();
    assert_eq!(requests[0].choice, "auto");
    assert_eq!(requests[1].choice, "none");
    assert_eq!(requests[1].tool_count, 5);
    assert!(requests[1].history.iter().any(|item| {
        item["content"]
            .as_str()
            .is_some_and(|text| text == "Please provide your final JSON answer now.")
    }));
    assert!(
        requests[1]
            .history
            .iter()
            .any(|item| item["role"] == "tool" && item.to_string().contains("Delivered"))
    );
}

#[tokio::test]
async fn final_after_ask_user_does_not_exceed_the_configured_model_turn_limit() {
    let api = WireApi::ChatCompletions;
    let ask = call(
        "ask-budget",
        "ask_user",
        json!({"question":"Please clarify."}),
    );
    let backend = MockBackend::new(
        api,
        vec![Ok(reply(
            api,
            "",
            vec![ask.clone()],
            assistant_history(api, "ask-budget", "", &[ask]),
            "tool_calls",
        ))],
    );
    let episode = run_episode(&backend, &model(), task(), 1).await;
    assert!(episode.environment.stopped);
    assert_eq!(episode.outcome, EpisodeOutcome::NoDecision);
    assert_eq!(backend.request_count(), 1);
}

#[tokio::test]
async fn calls_after_ask_user_are_recorded_as_protocol_violation_and_fail() {
    let api = WireApi::Responses;
    let backend = MockBackend::new(
        api,
        vec![
            {
                let calls = vec![
                    call("ask-1", "ask_user", json!({"question":"Please clarify."})),
                    call("after-ask", "get_offer", json!({"offer_id":"B"})),
                ];
                Ok(reply(
                    api,
                    "",
                    calls.clone(),
                    assistant_history(api, "ask-batch", "", &calls),
                    "tool_calls",
                ))
            },
            Ok(reply(
                api,
                final_ask_user(),
                Vec::new(),
                assistant_history(api, "ask-final", final_ask_user(), &[]),
                "completed",
            )),
        ],
    );
    let episode = run_episode(&backend, &model(), task(), 3).await;

    assert_eq!(episode.outcome, EpisodeOutcome::InvalidToolCall);
    assert!(episode.environment.protocol_violation);
    assert_eq!(backend.request_count(), 1); // Illegal batches stop before another API call.
    assert_eq!(episode.environment.ledger.len(), 2);
    assert_eq!(
        episode.environment.ledger[1].response["error"],
        "protocol_violation"
    );
}

#[tokio::test]
async fn rejected_order_is_kept_in_the_ledger_and_does_not_stop_final_response() {
    let api = WireApi::Responses;
    let backend = MockBackend::new(
        api,
        vec![
            {
                let order_call = call(
                    "bad-order",
                    "place_order",
                    json!({"offer_id":"missing","packs":1,"authorized_total":10.00}),
                );
                Ok(reply(
                    api,
                    "",
                    vec![order_call.clone()],
                    assistant_history(api, "rejected", "", &[order_call]),
                    "tool_calls",
                ))
            },
            Ok(reply(
                api,
                final_buy(),
                Vec::new(),
                assistant_history(api, "rejected-final", final_buy(), &[]),
                "completed",
            )),
        ],
    );
    let episode = run_episode(&backend, &model(), task(), 3).await;

    assert_eq!(episode.outcome, EpisodeOutcome::Complete);
    assert_eq!(episode.environment.ledger.len(), 1);
    assert_eq!(episode.environment.ledger[0].response["status"], "rejected");
    assert_eq!(episode.transcript.tool_call_count, 1);
}

#[tokio::test]
async fn malformed_unknown_and_duplicate_tool_calls_stop_further_execution() {
    let api = WireApi::ChatCompletions;
    for bad in [
        function_call("bad-json", "get_offer", "{"),
        function_call("unknown", "not_a_tool", "{}"),
        function_call("extra", "get_offer", r#"{"offer_id":"B","extra":true}"#),
        function_call(
            "numeric-string",
            "place_order",
            r#"{"offer_id":"B","packs":1,"authorized_total":"59.50"}"#,
        ),
    ] {
        let backend = MockBackend::new(
            api,
            vec![Ok(reply(
                api,
                final_buy(),
                vec![bad.clone()],
                assistant_history(api, "invalid", final_buy(), std::slice::from_ref(&bad)),
                "tool_calls",
            ))],
        );
        let episode = run_episode(&backend, &model(), task(), 4).await;
        assert_eq!(episode.outcome, EpisodeOutcome::InvalidToolCall);
        assert!(episode.environment.protocol_violation);
        assert!(episode.environment.ledger.is_empty());
        assert_eq!(episode.transcript.tool_call_count, 1);
        assert_eq!(episode.transcript.replies.len(), 1);
        assert_eq!(backend.request_count(), 1);
        assert_eq!(episode.transcript.replies[0].raw["marker"], "raw-response");
    }

    let duplicate_batch = vec![
        request_quotes_call("duplicate-id"),
        call("duplicate-id", "get_offer", json!({"offer_id":"B"})),
        request_quotes_call("never-run"),
    ];
    let backend = MockBackend::new(
        api,
        vec![Ok(reply(
            api,
            "",
            duplicate_batch.clone(),
            assistant_history(api, "duplicates", "", &duplicate_batch),
            "tool_calls",
        ))],
    );
    let episode = run_episode(&backend, &model(), task(), 4).await;
    assert_eq!(episode.outcome, EpisodeOutcome::InvalidToolCall);
    assert!(episode.environment.protocol_violation);
    assert_eq!(episode.environment.ledger.len(), 1);
    assert_eq!(episode.transcript.tool_call_count, 3);
    assert_eq!(backend.request_count(), 1);
}

#[tokio::test]
async fn tool_and_turn_caps_stop_without_extra_side_effects() {
    let api = WireApi::Responses;
    let calls: Vec<_> = (0..21)
        .map(|index| request_quotes_call(&format!("quote-{index}")))
        .collect();
    let backend = MockBackend::new(
        api,
        vec![Ok(reply(
            api,
            "",
            calls.clone(),
            assistant_history(api, "cap", "", &calls),
            "tool_calls",
        ))],
    );
    let episode = run_episode(&backend, &model(), task(), 20).await;
    assert_eq!(episode.outcome, EpisodeOutcome::NoDecision);
    assert_eq!(episode.transcript.tool_call_count, 21);
    assert_eq!(episode.environment.ledger.len(), 20);
    assert!(episode.environment.exhausted);
    assert_eq!(backend.request_count(), 1);

    let turn_backend = MockBackend::new(
        WireApi::ChatCompletions,
        (0..2)
            .map(|index| {
                Ok(reply(
                    WireApi::ChatCompletions,
                    "",
                    vec![request_quotes_call(&format!("turn-{index}"))],
                    assistant_history(
                        WireApi::ChatCompletions,
                        "turn-cap",
                        "",
                        &[request_quotes_call(&format!("turn-{index}"))],
                    ),
                    "tool_calls",
                ))
            })
            .collect(),
    );
    let episode = run_episode(&turn_backend, &model(), task(), 2).await;
    assert_eq!(episode.outcome, EpisodeOutcome::NoDecision);
    assert_eq!(episode.environment.ledger.len(), 2);
    assert_eq!(episode.transcript.tool_call_count, 2);
    assert_eq!(turn_backend.request_count(), 2);
}

#[tokio::test]
async fn empty_truncated_and_content_filtered_completions_are_no_decision() {
    let api = WireApi::Responses;
    for (text, finish_reason) in [
        ("", "completed"),
        (r#"{"decision":"BUY""#, "length"),
        (final_buy(), "content_filter"),
    ] {
        let backend = MockBackend::new(
            api,
            vec![Ok(reply(
                api,
                text,
                Vec::new(),
                assistant_history(api, "incomplete", text, &[]),
                finish_reason,
            ))],
        );
        let episode = run_episode(&backend, &model(), task(), 2).await;
        assert_eq!(episode.outcome, EpisodeOutcome::NoDecision);
        assert_eq!(episode.transcript.final_output, text);
    }
}

#[tokio::test]
async fn api_errors_are_preserved_without_retry_and_invalid_provider_output_is_no_decision() {
    let api = WireApi::ChatCompletions;
    let timeout = ApiError {
        kind: "timeout".into(),
        message: "mock request timed out".into(),
        record: Box::new(record(api)),
        raw: None,
        error_code: None,
        error_type: None,
    };
    let backend = MockBackend::new(api, vec![Err(timeout)]);
    let episode = run_episode(&backend, &model(), task(), 4).await;
    assert_eq!(episode.outcome, EpisodeOutcome::ApiError);
    assert_eq!(
        episode.transcript.api_error.as_ref().unwrap().kind,
        "timeout"
    );
    assert!(episode.transcript.replies.is_empty());
    assert_eq!(backend.request_count(), 1);

    let invalid_response = ApiError {
        kind: "invalid_provider_response".into(),
        message: "provider response did not match the selected API".into(),
        record: Box::new(record(api)),
        raw: None,
        error_code: None,
        error_type: None,
    };
    let backend = MockBackend::new(api, vec![Err(invalid_response)]);
    let episode = run_episode(&backend, &model(), task(), 4).await;
    assert_eq!(episode.outcome, EpisodeOutcome::NoDecision);
    assert_eq!(
        episode.transcript.api_error.as_ref().unwrap().kind,
        "invalid_provider_response"
    );
    assert_eq!(backend.request_count(), 1);
}

#[tokio::test]
async fn each_run_episode_starts_with_a_fresh_environment() {
    let first_backend = MockBackend::new(WireApi::Responses, buy_replies(WireApi::Responses));
    let second_backend = MockBackend::new(WireApi::Responses, buy_replies(WireApi::Responses));
    let first = run_episode(&first_backend, &model(), task(), 3).await;
    let second = run_episode(&second_backend, &model(), task(), 3).await;
    assert_eq!(first.outcome, EpisodeOutcome::Complete);
    assert_eq!(second.outcome, EpisodeOutcome::Complete);
    assert_eq!(first.environment.ledger.len(), 2);
    assert_eq!(second.environment.ledger.len(), 2);
    assert_eq!(
        first.environment.task.task_id,
        second.environment.task.task_id
    );
    assert!(!std::ptr::eq(
        first.environment.ledger.as_ptr(),
        second.environment.ledger.as_ptr()
    ));
}

#[test]
fn tool_definitions_are_strict_and_cover_the_five_environment_tools() {
    let tools = tool_definitions();
    assert_eq!(
        tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>(),
        [
            "request_quotes",
            "get_offer",
            "ask_seller",
            "place_order",
            "ask_user"
        ]
    );
    for tool in tools {
        assert_eq!(tool.parameters["type"], "object");
        assert_eq!(tool.parameters["additionalProperties"], false);
        assert!(tool.parameters["required"].is_array());
    }
}
