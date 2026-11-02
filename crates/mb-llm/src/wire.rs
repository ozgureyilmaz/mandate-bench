use std::collections::BTreeSet;

use anyhow::{Result, bail, ensure};
use serde_json::{Map, Value, json};

use crate::{
    config::{validate_model_settings, validate_provider_api},
    types::{
        FunctionCall, ModelConfig, ModelReply, RequestRecord, TokenUsage, ToolChoice,
        ToolDefinition, WireApi,
    },
};

/// Build one request body for the configured wire protocol without dropping set options.
pub fn build_request(
    api: WireApi,
    model: &ModelConfig,
    history: &[Value],
    tools: &[ToolDefinition],
    choice: ToolChoice,
) -> Result<Value> {
    validate_provider_api(&model.provider, api)?;
    validate_model_settings(model, api)?;
    ensure!(!model.id.trim().is_empty(), "model ID must not be empty");
    ensure!(
        model.max_output_tokens > 0,
        "max_output_tokens must be positive"
    );
    ensure!(!history.is_empty(), "request history must not be empty");

    let mut seen_tools = BTreeSet::new();
    for tool in tools {
        ensure!(
            is_function_name(&tool.name),
            "invalid function tool name: {}",
            tool.name
        );
        ensure!(
            seen_tools.insert(&tool.name),
            "duplicate function tool name: {}",
            tool.name
        );
        ensure!(
            tool.parameters.is_object(),
            "tool {} parameters must be a JSON object",
            tool.name
        );
        ensure!(
            tool.parameters.get("type").and_then(Value::as_str) == Some("object"),
            "tool {} parameters must describe an object",
            tool.name
        );
    }
    if api == WireApi::Responses {
        for tool in tools {
            validate_strict_schema(&tool.parameters, &tool.name)?;
        }
    }
    if let ToolChoice::Required(ref name) = choice {
        ensure!(
            seen_tools.contains(name),
            "required tool is not present in tool definitions: {name}"
        );
    }

    match api {
        WireApi::Responses => build_responses_request(model, history, tools, choice),
        WireApi::ChatCompletions => build_chat_request(model, history, tools, choice),
    }
}

fn build_responses_request(
    model: &ModelConfig,
    history: &[Value],
    tools: &[ToolDefinition],
    choice: ToolChoice,
) -> Result<Value> {
    let mut request = Map::new();
    request.insert("model".into(), Value::String(model.id.clone()));
    request.insert("input".into(), Value::Array(history.to_vec()));
    request.insert(
        "max_output_tokens".into(),
        Value::from(model.max_output_tokens),
    );
    request.insert(
        "tools".into(),
        Value::Array(tools.iter().map(responses_tool).collect()),
    );
    request.insert("tool_choice".into(), responses_tool_choice(choice));
    request.insert("parallel_tool_calls".into(), Value::Bool(false));
    request.insert("store".into(), Value::Bool(false));
    request.insert("include".into(), json!(["reasoning.encrypted_content"]));
    if let Some(temperature) = model.temperature {
        request.insert("temperature".into(), json!(temperature));
    }
    if let Some(effort) = &model.reasoning_effort {
        request.insert("reasoning".into(), json!({"effort": effort}));
    }
    Ok(Value::Object(request))
}

fn build_chat_request(
    model: &ModelConfig,
    history: &[Value],
    tools: &[ToolDefinition],
    choice: ToolChoice,
) -> Result<Value> {
    if model.provider == "deepseek"
        && deepseek_thinking_enabled(model)
        && matches!(&choice, ToolChoice::Required(_))
    {
        bail!("DeepSeek thinking mode does not support required or named tool choice");
    }

    let mut request = Map::new();
    request.insert("model".into(), Value::String(model.id.clone()));
    // Chat history messages are passed through unchanged, including reasoning_content/tool_calls.
    request.insert("messages".into(), Value::Array(history.to_vec()));
    request.insert("max_tokens".into(), Value::from(model.max_output_tokens));
    request.insert(
        "tools".into(),
        Value::Array(tools.iter().map(chat_tool).collect()),
    );
    request.insert("tool_choice".into(), chat_tool_choice(choice));
    if let Some(temperature) = model.temperature {
        request.insert("temperature".into(), json!(temperature));
    }
    if let Some(effort) = &model.reasoning_effort {
        request.insert("reasoning_effort".into(), Value::String(effort.clone()));
    }
    if model.provider == "deepseek" {
        let thinking = deepseek_thinking_enabled(model);
        request.insert(
            "thinking".into(),
            json!({"type": if thinking { "enabled" } else { "disabled" }}),
        );
    }
    if let Some(seed) = model.seed {
        request.insert("seed".into(), Value::from(seed));
    }
    Ok(Value::Object(request))
}

fn responses_tool(tool: &ToolDefinition) -> Value {
    json!({
        "type": "function",
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.parameters,
        "strict": true
    })
}

fn chat_tool(tool: &ToolDefinition) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.parameters
        }
    })
}

fn validate_strict_schema(schema: &Value, tool_name: &str) -> Result<()> {
    if schema.get("type").and_then(Value::as_str) == Some("object") {
        ensure!(
            schema.get("additionalProperties") == Some(&Value::Bool(false)),
            "Responses strict schema for {tool_name} must set additionalProperties=false on every object"
        );
        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let required = schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<BTreeSet<_>>();
        for (name, property_schema) in &properties {
            ensure!(
                required.contains(name.as_str()),
                "Responses strict schema for {tool_name} must require property {name}"
            );
            validate_strict_schema(property_schema, tool_name)?;
        }
    }
    if let Some(items) = schema.get("items") {
        validate_strict_schema(items, tool_name)?;
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = schema.get(keyword).and_then(Value::as_array) {
            for branch in branches {
                validate_strict_schema(branch, tool_name)?;
            }
        }
    }
    if let Some(definitions) = schema.get("$defs").and_then(Value::as_object) {
        for definition in definitions.values() {
            validate_strict_schema(definition, tool_name)?;
        }
    }
    Ok(())
}

fn responses_tool_choice(choice: ToolChoice) -> Value {
    match choice {
        ToolChoice::Auto => Value::String("auto".into()),
        ToolChoice::None => Value::String("none".into()),
        ToolChoice::Required(name) => json!({"type": "function", "name": name}),
    }
}

fn chat_tool_choice(choice: ToolChoice) -> Value {
    match choice {
        ToolChoice::Auto => Value::String("auto".into()),
        ToolChoice::None => Value::String("none".into()),
        ToolChoice::Required(name) => json!({"type": "function", "function": {"name": name}}),
    }
}

fn is_function_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn deepseek_thinking_enabled(model: &ModelConfig) -> bool {
    model
        .thinking
        .unwrap_or(model.reasoning_effort.as_deref() != Some("none"))
}

/// Convert one model response into visible text, tool calls, replay history and normalized usage.
pub fn parse_response(
    api: WireApi,
    raw: Value,
    mut record: RequestRecord,
) -> std::result::Result<ModelReply, String> {
    if record.api != api {
        return Err("request record wire API does not match response parser".into());
    }
    let model = raw
        .get("model")
        .and_then(Value::as_str)
        .filter(|model| !model.trim().is_empty())
        .ok_or_else(|| "response is missing its API-returned model ID".to_owned())?;
    record.model = Some(model.to_owned());

    match api {
        WireApi::Responses => parse_responses(raw, record),
        WireApi::ChatCompletions => parse_chat_completion(raw, record),
    }
}

fn parse_responses(
    raw: Value,
    mut record: RequestRecord,
) -> std::result::Result<ModelReply, String> {
    let status = raw
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| "Responses API response is missing status".to_owned())?;
    let completed = status == "completed";

    let output = raw
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| "Responses API response is missing output items".to_owned())?;
    let mut text = String::new();
    let mut calls = Vec::new();
    let mut ids = BTreeSet::new();
    for item in output {
        match item.get("type").and_then(Value::as_str) {
            Some("function_call") if completed => {
                let call = parse_responses_call(item)?;
                ensure_unique_call_id(&mut ids, &call.id)?;
                calls.push(call);
            }
            Some("function_call") => {}
            Some("message") => append_responses_message_text(item, &mut text),
            // Keep reasoning and every other output item in replay history, but never text-render them.
            _ => {}
        }
    }
    record.usage = parse_responses_usage(raw.get("usage"))?;
    let finish_reason = status.to_owned();
    Ok(ModelReply {
        text,
        calls,
        history: output.clone(),
        raw,
        finish_reason: Some(finish_reason),
        record,
    })
}

fn append_responses_message_text(item: &Value, output: &mut String) {
    match item.get("content") {
        Some(Value::Array(parts)) => {
            for part in parts {
                match part.get("type").and_then(Value::as_str) {
                    Some("output_text") => {
                        if let Some(text) = part.get("text").and_then(Value::as_str) {
                            output.push_str(text);
                        }
                    }
                    Some("refusal") => {
                        if let Some(refusal) = part.get("refusal").and_then(Value::as_str) {
                            output.push_str(refusal);
                        }
                    }
                    _ => {}
                }
            }
        }
        Some(Value::String(text)) => output.push_str(text),
        _ => {}
    }
}

fn parse_responses_call(item: &Value) -> std::result::Result<FunctionCall, String> {
    let id = required_nonempty_string(item.get("call_id"), "Responses function_call.call_id")?;
    let name = required_nonempty_string(item.get("name"), "Responses function_call.name")?;
    let arguments = required_string(item.get("arguments"), "Responses function_call.arguments")?;
    validate_arguments(&arguments, "Responses function call")?;
    Ok(FunctionCall {
        id,
        name,
        arguments,
    })
}

fn parse_chat_completion(
    raw: Value,
    mut record: RequestRecord,
) -> std::result::Result<ModelReply, String> {
    let choices = raw
        .get("choices")
        .and_then(Value::as_array)
        .ok_or_else(|| "Chat Completions response is missing choices".to_owned())?;
    if choices.len() != 1 {
        return Err(format!(
            "expected exactly one chat choice, got {}",
            choices.len()
        ));
    }
    let choice = &choices[0];
    let finish_reason =
        required_nonempty_string(choice.get("finish_reason"), "chat finish_reason")?;
    let completed = finish_reason == "stop" || finish_reason == "tool_calls";

    let message = choice
        .get("message")
        .and_then(Value::as_object)
        .ok_or_else(|| "Chat Completions choice is missing assistant message".to_owned())?;
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return Err("Chat Completions message role is not assistant".into());
    }
    let mut text = String::new();
    append_chat_content(message.get("content"), &mut text)?;
    if let Some(refusal) = message.get("refusal").and_then(Value::as_str) {
        text.push_str(refusal);
    }
    let mut calls = Vec::new();
    let mut ids = BTreeSet::new();
    match (message.get("tool_calls"), completed) {
        (_, false) => {}
        (None | Some(Value::Null), true) => {}
        (Some(Value::Array(tool_calls)), true) => {
            for value in tool_calls {
                let call = parse_chat_call(value)?;
                ensure_unique_call_id(&mut ids, &call.id)?;
                calls.push(call);
            }
        }
        (Some(_), true) => {
            return Err("Chat Completions message tool_calls is not an array".into());
        }
    }
    record.usage = parse_chat_usage(raw.get("usage"))?;
    Ok(ModelReply {
        text,
        calls,
        // Preserve the complete assistant message, especially reasoning_content and tool_calls.
        history: vec![Value::Object(message.clone())],
        raw,
        finish_reason: Some(finish_reason),
        record,
    })
}

fn append_chat_content(
    content: Option<&Value>,
    output: &mut String,
) -> std::result::Result<(), String> {
    match content {
        None | Some(Value::Null) => Ok(()),
        Some(Value::String(text)) => {
            output.push_str(text);
            Ok(())
        }
        Some(Value::Array(parts)) => {
            for part in parts {
                if part.get("type").and_then(Value::as_str) == Some("text") {
                    let text = required_string(part.get("text"), "chat content text")?;
                    output.push_str(&text);
                }
            }
            Ok(())
        }
        Some(_) => Err("Chat Completions message content has an unsupported shape".into()),
    }
}

fn parse_chat_call(value: &Value) -> std::result::Result<FunctionCall, String> {
    if value.get("type").and_then(Value::as_str) != Some("function") {
        return Err("Chat Completions tool call type is not function".into());
    }
    let id = required_nonempty_string(value.get("id"), "chat tool_call.id")?;
    let function = value
        .get("function")
        .and_then(Value::as_object)
        .ok_or_else(|| "Chat Completions tool call is missing function".to_owned())?;
    let name = required_nonempty_string(function.get("name"), "chat function.name")?;
    let arguments = required_string(function.get("arguments"), "chat function.arguments")?;
    validate_arguments(&arguments, "Chat Completions function call")?;
    Ok(FunctionCall {
        id,
        name,
        arguments,
    })
}

fn validate_arguments(arguments: &str, context: &str) -> std::result::Result<(), String> {
    let parsed: Value = serde_json::from_str(arguments)
        .map_err(|_| format!("{context} arguments are not valid JSON"))?;
    if !parsed.is_object() {
        return Err(format!("{context} arguments must be a JSON object"));
    }
    Ok(())
}

fn ensure_unique_call_id(ids: &mut BTreeSet<String>, id: &str) -> std::result::Result<(), String> {
    if ids.insert(id.to_owned()) {
        Ok(())
    } else {
        Err(format!("duplicate function call ID: {id}"))
    }
}

fn required_string(value: Option<&Value>, field: &str) -> std::result::Result<String, String> {
    value
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("missing or invalid {field}"))
}

fn required_nonempty_string(
    value: Option<&Value>,
    field: &str,
) -> std::result::Result<String, String> {
    let text = required_string(value, field)?;
    if text.trim().is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    Ok(text)
}

fn parse_responses_usage(value: Option<&Value>) -> std::result::Result<TokenUsage, String> {
    let Some(usage) = optional_usage_object(value, "Responses API")? else {
        return Ok(TokenUsage::default());
    };
    Ok(TokenUsage {
        input: count(usage.get("input_tokens"), "Responses usage.input_tokens")?,
        cached_input: count(
            usage
                .get("input_tokens_details")
                .and_then(|details| details.get("cached_tokens")),
            "Responses usage.input_tokens_details.cached_tokens",
        )?,
        output: count(usage.get("output_tokens"), "Responses usage.output_tokens")?,
        reasoning: count(
            usage
                .get("output_tokens_details")
                .and_then(|details| details.get("reasoning_tokens")),
            "Responses usage.output_tokens_details.reasoning_tokens",
        )?,
    })
}

fn parse_chat_usage(value: Option<&Value>) -> std::result::Result<TokenUsage, String> {
    let Some(usage) = optional_usage_object(value, "Chat Completions")? else {
        return Ok(TokenUsage::default());
    };
    let cached_input = usage
        .get("prompt_cache_hit_tokens")
        .filter(|value| !value.is_null())
        .or_else(|| {
            usage
                .get("prompt_tokens_details")
                .and_then(|details| details.get("cached_tokens"))
        });
    Ok(TokenUsage {
        input: count(usage.get("prompt_tokens"), "chat usage.prompt_tokens")?,
        cached_input: count(cached_input, "chat usage cached input tokens")?,
        output: count(
            usage.get("completion_tokens"),
            "chat usage.completion_tokens",
        )?,
        reasoning: count(
            usage
                .get("completion_tokens_details")
                .and_then(|details| details.get("reasoning_tokens")),
            "chat usage.completion_tokens_details.reasoning_tokens",
        )?,
    })
}

fn optional_usage_object<'a>(
    value: Option<&'a Value>,
    api: &str,
) -> std::result::Result<Option<&'a Map<String, Value>>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(object)) => Ok(Some(object)),
        Some(_) => Err(format!("{api} usage must be an object or null")),
    }
}

fn count(value: Option<&Value>, field: &str) -> std::result::Result<Option<u64>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => number
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("{field} must be a nonnegative integer")),
        Some(_) => Err(format!("{field} must be a nonnegative integer")),
    }
}

/// Format one tool response in the output shape expected by the selected API.
pub fn tool_result(api: WireApi, call: &FunctionCall, response: &Value) -> Value {
    let content = response.to_string();
    match api {
        WireApi::Responses => json!({
            "type": "function_call_output",
            "call_id": call.id,
            "output": content
        }),
        WireApi::ChatCompletions => json!({
            "role": "tool",
            "tool_call_id": call.id,
            "content": content
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::estimate_cost;
    use crate::types::{AttemptRecord, Price};
    use rust_decimal::Decimal;

    fn model(provider: &str) -> ModelConfig {
        ModelConfig {
            id: match provider {
                "openai" => "gpt-example",
                "deepseek" => "deepseek-flash",
                _ => "nvidia/example-model",
            }
            .into(),
            provider: provider.into(),
            max_output_tokens: 1024,
            temperature: None,
            reasoning_effort: None,
            thinking: None,
            seed: None,
            price: None,
        }
    }

    fn tool() -> ToolDefinition {
        ToolDefinition {
            name: "get_offer".into(),
            description: "Fetch one offer.".into(),
            parameters: json!({
                "type": "object",
                "properties": {"offer_id": {"type": "string"}},
                "required": ["offer_id"],
                "additionalProperties": false
            }),
        }
    }

    fn record(api: WireApi) -> RequestRecord {
        RequestRecord {
            provider: match api {
                WireApi::Responses => "openai",
                WireApi::ChatCompletions => "deepseek",
            }
            .into(),
            requested_model: "requested-model".into(),
            model: None,
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
                request_id: None,
                retry_delay_ms: None,
            }],
        }
    }

    #[test]
    fn responses_request_is_stateless_and_uses_flat_strict_functions() {
        let mut model = model("openai");
        model.temperature = Some(0.3);
        model.reasoning_effort = Some("medium".into());
        let request = build_request(
            WireApi::Responses,
            &model,
            &[json!({"role": "user", "content": "Find an offer."})],
            &[tool()],
            ToolChoice::Auto,
        )
        .unwrap();
        assert_eq!(request["store"], false);
        assert_eq!(request["parallel_tool_calls"], false);
        assert_eq!(request["include"], json!(["reasoning.encrypted_content"]));
        assert_eq!(request["max_output_tokens"], 1024);
        assert_eq!(request["temperature"], 0.3);
        assert_eq!(request["reasoning"]["effort"], "medium");
        assert_eq!(request["tools"][0]["name"], "get_offer");
        assert_eq!(request["tools"][0]["strict"], true);
        assert!(request["tools"][0].get("function").is_none());

        let forced = build_request(
            WireApi::Responses,
            &model,
            &[json!({"role": "user", "content": "Find an offer."})],
            &[tool()],
            ToolChoice::Required("get_offer".into()),
        )
        .unwrap();
        assert_eq!(
            forced["tool_choice"],
            json!({"type":"function","name":"get_offer"})
        );
    }

    #[test]
    fn chat_request_replays_deepseek_reasoning_and_tool_calls_unchanged() {
        let mut model = model("deepseek");
        model.reasoning_effort = Some("high".into());
        model.thinking = Some(true);
        let assistant = json!({
            "role": "assistant",
            "content": null,
            "reasoning_content": "private reasoning stays in history",
            "tool_calls": [{"id":"call-1","type":"function","function":{"name":"get_offer","arguments":r#"{"offer_id":"A"}"#}}]
        });
        let request = build_request(
            WireApi::ChatCompletions,
            &model,
            &[
                assistant.clone(),
                json!({"role":"tool","tool_call_id":"call-1","content":"{}"}),
            ],
            &[tool()],
            ToolChoice::Auto,
        )
        .unwrap();
        assert_eq!(request["messages"][0], assistant);
        assert_eq!(request["max_tokens"], 1024);
        assert_eq!(request["reasoning_effort"], "high");
        assert_eq!(request["thinking"]["type"], "enabled");
        assert!(request.get("parallel_tool_calls").is_none());
        assert_eq!(request["tools"][0]["function"]["name"], "get_offer");
        assert!(request["tools"][0]["function"].get("strict").is_none());
    }

    #[test]
    fn request_codecs_reject_unsupported_or_ignored_settings() {
        let mut openai = model("openai");
        openai.seed = Some(7);
        assert!(
            build_request(
                WireApi::Responses,
                &openai,
                &[json!({"role":"user","content":"x"})],
                &[],
                ToolChoice::None
            )
            .is_err()
        );

        let mut deepseek = model("deepseek");
        deepseek.temperature = Some(0.2);
        assert!(
            build_request(
                WireApi::ChatCompletions,
                &deepseek,
                &[json!({"role":"user","content":"x"})],
                &[],
                ToolChoice::None
            )
            .is_err()
        );
        deepseek.temperature = None;
        assert!(
            build_request(
                WireApi::ChatCompletions,
                &deepseek,
                &[json!({"role":"user","content":"x"})],
                &[tool()],
                ToolChoice::Required("get_offer".into())
            )
            .is_err()
        );

        let mut nvidia = model("nvidia");
        nvidia.reasoning_effort = Some("high".into());
        assert!(
            build_request(
                WireApi::ChatCompletions,
                &nvidia,
                &[json!({"role":"user","content":"x"})],
                &[],
                ToolChoice::None
            )
            .is_err()
        );
    }

    #[test]
    fn responses_parser_preserves_every_output_item_but_hides_reasoning_from_text() {
        let raw = json!({
            "id":"resp-1",
            "model":"returned-model-id",
            "status":"completed",
            "output":[
                {"type":"reasoning","id":"rs_1","summary":[{"type":"summary_text","text":"private reasoning"}],"encrypted_content":"opaque"},
                {"type":"function_call","id":"fc_1","call_id":"call-1","name":"get_offer","arguments":r#"{"offer_id":"A"}"#,"status":"completed"},
                {"type":"message","role":"assistant","content":[{"type":"output_text","text":"Visible response."}]}
            ],
            "usage": {
                "input_tokens": 1000000,
                "input_tokens_details": {"cached_tokens": 400000},
                "output_tokens": 100000,
                "output_tokens_details": {"reasoning_tokens": 25000}
            }
        });
        let reply =
            parse_response(WireApi::Responses, raw.clone(), record(WireApi::Responses)).unwrap();
        assert_eq!(reply.text, "Visible response.");
        assert_eq!(reply.calls.len(), 1);
        assert_eq!(reply.calls[0].id, "call-1");
        assert_eq!(reply.calls[0].name, "get_offer");
        assert_eq!(reply.calls[0].arguments, r#"{"offer_id":"A"}"#);
        assert_eq!(reply.history, raw["output"].as_array().unwrap().clone());
        assert_eq!(reply.record.model.as_deref(), Some("returned-model-id"));
        assert_eq!(reply.record.usage.input, Some(1_000_000));
        assert_eq!(reply.record.usage.cached_input, Some(400_000));
        assert_eq!(reply.record.usage.output, Some(100_000));
        assert_eq!(reply.record.usage.reasoning, Some(25_000));
    }

    #[test]
    fn chat_parser_preserves_reasoning_content_without_using_it_as_final_text() {
        let raw = json!({
            "id":"chat-1",
            "model":"deepseek-v4-pro-actual",
            "choices":[{
                "index":0,
                "message":{
                    "role":"assistant",
                    "content":"Visible answer.",
                    "reasoning_content":"private chain of thought",
                    "tool_calls":[{"id":"call-2","type":"function","function":{"name":"get_offer","arguments":r#"{"offer_id":"B"}"#}}]
                },
                "finish_reason":"tool_calls"
            }],
            "usage":{
                "prompt_tokens":100,
                "completion_tokens":40,
                "prompt_tokens_details":{"cached_tokens":25},
                "prompt_cache_hit_tokens":25,
                "completion_tokens_details":{"reasoning_tokens":12}
            }
        });
        let reply = parse_response(
            WireApi::ChatCompletions,
            raw.clone(),
            record(WireApi::ChatCompletions),
        )
        .unwrap();
        assert_eq!(reply.text, "Visible answer.");
        assert_eq!(reply.history, vec![raw["choices"][0]["message"].clone()]);
        assert_eq!(
            reply.history[0]["reasoning_content"],
            "private chain of thought"
        );
        assert_eq!(
            reply.record.model.as_deref(),
            Some("deepseek-v4-pro-actual")
        );
        assert_eq!(reply.record.usage.input, Some(100));
        assert_eq!(reply.record.usage.cached_input, Some(25));
        assert_eq!(reply.record.usage.output, Some(40));
        assert_eq!(reply.record.usage.reasoning, Some(12));
    }

    #[test]
    fn parse_preserves_truncation_and_rejects_malformed_protocol() {
        let incomplete_responses = json!({
            "model":"actual",
            "status":"incomplete",
            "incomplete_details":{"reason":"max_output_tokens"},
            "output":[
                {"type":"function_call","call_id":"call-partial","name":"place_order","arguments":r#"{"offer_id":"A","packs":1,"authorized_total":59.5}"#},
                {"type":"message","role":"assistant","content":[{"type":"output_text","text":"Partial response"}]}
            ]
        });
        let incomplete = parse_response(
            WireApi::Responses,
            incomplete_responses.clone(),
            record(WireApi::Responses),
        )
        .unwrap();
        assert_eq!(incomplete.finish_reason.as_deref(), Some("incomplete"));
        assert_eq!(incomplete.text, "Partial response");
        assert!(incomplete.calls.is_empty());
        assert_eq!(
            incomplete.history,
            incomplete_responses["output"].as_array().unwrap().clone()
        );

        let failed = parse_response(
            WireApi::Responses,
            json!({"model":"actual","status":"failed","output":[]}),
            record(WireApi::Responses),
        )
        .unwrap();
        assert_eq!(failed.finish_reason.as_deref(), Some("failed"));

        let empty_responses = parse_response(
            WireApi::Responses,
            json!({"model":"actual","status":"completed","output":[]}),
            record(WireApi::Responses),
        )
        .unwrap();
        assert!(empty_responses.text.is_empty());
        assert!(empty_responses.calls.is_empty());
        assert_eq!(empty_responses.finish_reason.as_deref(), Some("completed"));
        assert!(
            parse_response(
                WireApi::Responses,
                json!({"status":"completed","output":[]}),
                record(WireApi::Responses)
            )
            .unwrap_err()
            .contains("model ID")
        );

        let malformed_call = json!({"model":"actual","status":"completed","output":[{"type":"function_call","call_id":"call-1","name":"get_offer","arguments":"not json"}]});
        assert!(
            parse_response(
                WireApi::Responses,
                malformed_call,
                record(WireApi::Responses)
            )
            .unwrap_err()
            .contains("valid JSON")
        );

        let chat_length = json!({"model":"actual","choices":[{"message":{"role":"assistant","content":"Partial answer","tool_calls":[{"id":"call-partial","type":"function","function":{"name":"get_offer","arguments":"not complete"}}]},"finish_reason":"length"}]});
        let truncated_chat = parse_response(
            WireApi::ChatCompletions,
            chat_length.clone(),
            record(WireApi::ChatCompletions),
        )
        .unwrap();
        assert_eq!(truncated_chat.finish_reason.as_deref(), Some("length"));
        assert_eq!(truncated_chat.text, "Partial answer");
        assert!(truncated_chat.calls.is_empty());
        assert_eq!(
            truncated_chat.history,
            vec![chat_length["choices"][0]["message"].clone()]
        );

        let filtered = parse_response(
            WireApi::ChatCompletions,
            json!({"model":"actual","choices":[{"message":{"role":"assistant","content":null},"finish_reason":"content_filter"}]}),
            record(WireApi::ChatCompletions),
        )
        .unwrap();
        assert_eq!(filtered.finish_reason.as_deref(), Some("content_filter"));
        assert!(filtered.text.is_empty());

        let empty_chat = parse_response(
            WireApi::ChatCompletions,
            json!({"model":"actual","choices":[{"message":{"role":"assistant","content":null},"finish_reason":"stop"}]}),
            record(WireApi::ChatCompletions),
        )
        .unwrap();
        assert!(empty_chat.text.is_empty());
        assert!(empty_chat.calls.is_empty());

        let chat_missing_call_fields = json!({"model":"actual","choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call-1","function":{"name":"get_offer","arguments":"{}"}}]},"finish_reason":"tool_calls"}]});
        assert!(
            parse_response(
                WireApi::ChatCompletions,
                chat_missing_call_fields,
                record(WireApi::ChatCompletions)
            )
            .is_err()
        );
    }

    #[test]
    fn cost_estimate_counts_cached_input_once_and_reasoning_inside_output_once() {
        let price = Price {
            input_per_million: Decimal::new(2, 0),
            cached_input_per_million: Some(Decimal::new(1, 0)),
            output_per_million: Decimal::new(10, 0),
        };
        let usage = TokenUsage {
            input: Some(1_000_000),
            cached_input: Some(400_000),
            output: Some(100_000),
            reasoning: Some(25_000),
        };
        assert_eq!(
            estimate_cost(Some(&price), &usage),
            Some(Decimal::new(26, 1))
        );
        assert_eq!(estimate_cost(None, &usage), None);
        assert_eq!(
            estimate_cost(
                Some(&price),
                &TokenUsage {
                    input: None,
                    ..usage.clone()
                }
            ),
            None
        );
        assert_eq!(
            estimate_cost(
                Some(&price),
                &TokenUsage {
                    output: None,
                    ..usage.clone()
                }
            ),
            None
        );
        assert_eq!(
            estimate_cost(
                Some(&price),
                &TokenUsage {
                    cached_input: Some(1_000_001),
                    ..usage
                }
            ),
            None
        );
    }

    #[test]
    fn tool_results_match_each_wire_protocol() {
        let call = FunctionCall {
            id: "call-1".into(),
            name: "get_offer".into(),
            arguments: "{}".into(),
        };
        let response = json!({"ok":true,"offer_id":"A"});
        let responses = tool_result(WireApi::Responses, &call, &response);
        assert_eq!(responses["type"], "function_call_output");
        assert_eq!(responses["call_id"], "call-1");
        assert_eq!(responses["output"], response.to_string());
        let chat = tool_result(WireApi::ChatCompletions, &call, &response);
        assert_eq!(chat["role"], "tool");
        assert_eq!(chat["tool_call_id"], "call-1");
        assert_eq!(chat["content"], response.to_string());
    }
}
