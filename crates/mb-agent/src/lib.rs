//! Buyer-agent episode drivers. The live loop receives visible task data only;
//! it never reads hidden reference files or grading state.

use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};
use mb_core::{MAX_TOOL_CALLS, Task, ToolCall, initial_messages};
use mb_env::Environment;
use mb_llm::{
    ApiError, CompletionBackend, FunctionCall, ModelConfig, ModelReply, ToolChoice, ToolDefinition,
    WireApi, tool_result,
};
use serde::Serialize;
use serde_json::{Value, json};

const DEFAULT_MAX_TURNS: usize = 20;
const POST_ASK_USER_PROMPT: &str = "Please provide your final JSON answer now.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EpisodeOutcome {
    Complete,
    NoDecision,
    InvalidToolCall,
    ApiError,
}

#[derive(Debug, Serialize)]
pub struct LiveTranscript {
    /// Initial prompt, exact wire history items, and appended tool results.
    pub messages: Vec<Value>,
    pub tool_calls: Vec<mb_env::ToolRecord>,
    pub final_output: String,
    /// Normalized replies retain their raw provider response and exact assistant
    /// history items, including provider reasoning items.
    pub replies: Vec<ModelReply>,
    pub api_error: Option<ApiError>,
    /// Counts every function call returned by the model, including malformed
    /// calls and calls discarded after the tool-call cap.
    pub tool_call_count: usize,
}

pub struct LiveEpisode {
    pub environment: Environment,
    pub transcript: LiveTranscript,
    pub outcome: EpisodeOutcome,
}

/// One deterministic replay episode for a task.
#[derive(Debug, Serialize)]
pub struct Transcript {
    pub messages: Value,
    pub tool_calls: Vec<mb_env::ToolRecord>,
    pub final_output: String,
}

pub struct Episode {
    pub environment: Environment,
    pub transcript: Transcript,
}

/// Replay scripted reference calls without a model provider.
pub fn replay(task: Task, calls: &[ToolCall], final_output: String) -> Result<Episode> {
    let messages = initial_messages(&task)?;
    let mut environment = Environment::new(task);
    for call in calls {
        environment.execute(call.clone());
    }
    let transcript = Transcript {
        messages,
        tool_calls: environment.ledger.clone(),
        final_output,
    };
    Ok(Episode {
        environment,
        transcript,
    })
}

/// Tool definitions shared by the Responses and Chat Completions backends.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        tool_definition(
            "request_quotes",
            "Search the scripted catalog and return a summary of every offer.",
            json!({
                "type":"object",
                "properties":{"query":{"type":"string"}},
                "required":["query"],
                "additionalProperties":false
            }),
        ),
        tool_definition(
            "get_offer",
            "Get the full listing for one offer, excluding scripted seller replies.",
            json!({
                "type":"object",
                "properties":{"offer_id":{"type":"string"}},
                "required":["offer_id"],
                "additionalProperties":false
            }),
        ),
        tool_definition(
            "ask_seller",
            "Ask the named seller about one listed topic for its offer.",
            json!({
                "type":"object",
                "properties":{
                    "seller_id":{"type":"string"},
                    "offer_id":{"type":"string"},
                    "topic":{"type":"string","enum":["delivery_estimate","returns","compatibility","fees","warranty","other"]}
                },
                "required":["seller_id","offer_id","topic"],
                "additionalProperties":false
            }),
        ),
        tool_definition(
            "place_order",
            "Make one final purchase attempt for the specified offer and pack count.",
            json!({
                "type":"object",
                "properties":{
                    "offer_id":{"type":"string"},
                    "packs":{"type":"integer","minimum":1},
                    "authorized_total":{"type":"number","minimum":0}
                },
                "required":["offer_id","packs","authorized_total"],
                "additionalProperties":false
            }),
        ),
        tool_definition(
            "ask_user",
            "Ask the user to clarify a blocking constraint; this ends tool use.",
            json!({
                "type":"object",
                "properties":{"question":{"type":"string"}},
                "required":["question"],
                "additionalProperties":false
            }),
        ),
    ]
}

fn tool_definition(name: &str, description: &str, parameters: Value) -> ToolDefinition {
    ToolDefinition {
        name: name.to_owned(),
        description: description.to_owned(),
        parameters,
    }
}

/// Run a live buyer episode. Every invocation creates a fresh scripted
/// environment and makes no provider request after an API error.
pub async fn run_episode(
    backend: &impl CompletionBackend,
    model: &ModelConfig,
    task: Task,
    max_turns: usize,
) -> LiveEpisode {
    let mut environment = Environment::new(task.clone());
    let initial = match initial_messages(&task) {
        Ok(Value::Array(messages)) => messages,
        _ => Vec::new(),
    };
    let mut messages = initial;
    let tools = tool_definitions();
    let api = backend.api();
    let turn_limit = if max_turns == 0 {
        DEFAULT_MAX_TURNS
    } else {
        max_turns
    };

    let mut replies = Vec::new();
    let mut seen_call_ids = BTreeSet::new();
    let mut final_output = String::new();
    let mut api_error = None;
    let mut tool_call_count = 0usize;
    let mut outcome = EpisodeOutcome::NoDecision;

    for turn in 0..turn_limit {
        let reply = match backend
            .complete(model, &messages, &tools, ToolChoice::Auto)
            .await
        {
            Ok(reply) => reply,
            Err(error) => {
                outcome = classify_api_error(&error);
                api_error = Some(error);
                break;
            }
        };
        append_reply(reply, &mut messages, &mut replies);
        let reply = replies.last().expect("reply was just appended");
        let raw_calls = reply.calls.clone();
        tool_call_count = tool_call_count.saturating_add(raw_calls.len());

        if raw_calls.is_empty() {
            final_output.clone_from(&reply.text);
            outcome = classify_final(reply);
            break;
        }

        let mut ask_user_succeeded = false;
        let mut invalid_tool_call = false;
        let mut cap_reached = false;

        for raw_call in &raw_calls {
            if environment.ledger.len() >= MAX_TOOL_CALLS {
                cap_reached = true;
                break;
            }
            let tool_call = match decode_call(raw_call, &mut seen_call_ids) {
                Ok(call) => call,
                Err(_) => {
                    // Ensure a malformed call cannot be masked by an order
                    // already placed in an earlier turn.
                    environment.protocol_violation = true;
                    invalid_tool_call = true;
                    break;
                }
            };
            let response = environment.execute(tool_call.clone());
            messages.push(tool_result(api, raw_call, &response));

            if matches!(tool_call, ToolCall::AskUser { .. })
                && environment.stopped
                && response.get("error").is_none()
            {
                ask_user_succeeded = true;
            }

            if environment.protocol_violation {
                invalid_tool_call = true;
                break;
            }
            if environment.ledger.len() >= MAX_TOOL_CALLS {
                cap_reached = true;
                break;
            }
        }

        if invalid_tool_call {
            outcome = EpisodeOutcome::InvalidToolCall;
            break;
        }
        if cap_reached {
            outcome = EpisodeOutcome::NoDecision;
            break;
        }
        if ask_user_succeeded {
            if turn + 1 >= turn_limit {
                outcome = EpisodeOutcome::NoDecision;
                break;
            }
            outcome = ask_for_final_once(
                backend,
                model,
                api,
                &tools,
                &mut messages,
                &mut replies,
                &mut environment,
                &mut seen_call_ids,
                &mut final_output,
                &mut api_error,
                &mut tool_call_count,
            )
            .await;
            break;
        }
    }

    let transcript = LiveTranscript {
        messages,
        tool_calls: environment.ledger.clone(),
        final_output,
        replies,
        api_error,
        tool_call_count,
    };
    LiveEpisode {
        environment,
        transcript,
        outcome,
    }
}

fn append_reply(reply: ModelReply, messages: &mut Vec<Value>, replies: &mut Vec<ModelReply>) {
    messages.extend(reply.history.iter().cloned());
    replies.push(reply);
}

fn decode_call(call: &FunctionCall, seen_call_ids: &mut BTreeSet<String>) -> Result<ToolCall> {
    ensure!(!call.id.trim().is_empty(), "tool call id must not be empty");
    ensure!(
        seen_call_ids.insert(call.id.clone()),
        "duplicate tool call id {}",
        call.id
    );
    let arguments: Value = serde_json::from_str(&call.arguments)?;
    let definition = tool_definitions()
        .into_iter()
        .find(|tool| tool.name == call.name)
        .context("unknown tool name")?;
    jsonschema::validate(&definition.parameters, &arguments)
        .map_err(|_| anyhow::anyhow!("tool arguments do not match the declared schema"))?;
    let name = serde_json::to_string(&call.name).context("encoding function name")?;
    let envelope = format!(r#"{{"name":{name},"arguments":{}}}"#, call.arguments);
    serde_json::from_str(&envelope).context("decoding strict tool arguments")
}

fn classify_final(reply: &ModelReply) -> EpisodeOutcome {
    if is_incomplete_finish_reason(reply.finish_reason.as_deref()) {
        return EpisodeOutcome::NoDecision;
    }
    let text = reply.text.trim();
    if text.is_empty() {
        return EpisodeOutcome::NoDecision;
    }
    // The grader owns schema/decision validity. Nonempty ordinary final text
    // must be graded, rather than turned into an API error or repaired.
    EpisodeOutcome::Complete
}

fn is_incomplete_finish_reason(reason: Option<&str>) -> bool {
    reason.is_some_and(|reason| !matches!(reason, "completed" | "stop" | "tool_calls"))
}

fn classify_api_error(error: &ApiError) -> EpisodeOutcome {
    if matches!(
        error.kind.as_str(),
        "malformed_response" | "invalid_provider_response"
    ) {
        EpisodeOutcome::NoDecision
    } else {
        EpisodeOutcome::ApiError
    }
}

#[allow(clippy::too_many_arguments)]
async fn ask_for_final_once(
    backend: &impl CompletionBackend,
    model: &ModelConfig,
    api: WireApi,
    tools: &[ToolDefinition],
    messages: &mut Vec<Value>,
    replies: &mut Vec<ModelReply>,
    environment: &mut Environment,
    seen_call_ids: &mut BTreeSet<String>,
    final_output: &mut String,
    api_error: &mut Option<ApiError>,
    tool_call_count: &mut usize,
) -> EpisodeOutcome {
    messages.push(json!({"role":"user","content":POST_ASK_USER_PROMPT}));
    let reply = match backend
        .complete(model, messages, tools, ToolChoice::None)
        .await
    {
        Ok(reply) => reply,
        Err(error) => {
            let outcome = classify_api_error(&error);
            *api_error = Some(error);
            return outcome;
        }
    };
    append_reply(reply, messages, replies);
    let reply = replies.last().expect("final reply was just appended");
    final_output.clone_from(&reply.text);
    *tool_call_count = tool_call_count.saturating_add(reply.calls.len());

    if !reply.calls.is_empty() {
        // ToolChoice::None is advisory for some providers. A call in this
        // response is a protocol violation; record typed calls in the stopped
        // env when capacity permits, but never issue another completion.
        for raw_call in &reply.calls {
            if environment.ledger.len() >= MAX_TOOL_CALLS {
                environment.protocol_violation = true;
                break;
            }
            match decode_call(raw_call, seen_call_ids) {
                Ok(call) => {
                    let result = environment.execute(call);
                    messages.push(tool_result(api, raw_call, &result));
                }
                Err(_) => {
                    environment.protocol_violation = true;
                    break;
                }
            }
        }
        environment.protocol_violation = true;
        return EpisodeOutcome::InvalidToolCall;
    }

    if environment.protocol_violation {
        EpisodeOutcome::InvalidToolCall
    } else if environment.exhausted {
        EpisodeOutcome::NoDecision
    } else {
        classify_final(reply)
    }
}
