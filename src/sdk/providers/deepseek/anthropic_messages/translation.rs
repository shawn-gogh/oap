//! Translate between the Anthropic Messages API (inbound) and the DeepSeek
//! OpenAI-compatible Chat Completions API (upstream).

use serde_json::{json, Map, Value};

use crate::sdk::routing::Deployment;

use super::chat_completion::{ParsedChatCompletion, ToolCall};

// ---------------------------------------------------------------------------
// Request: Anthropic Messages -> Chat Completions
// ---------------------------------------------------------------------------

pub(super) fn anthropic_messages_to_chat_completions(body: Value, deployment: &Deployment) -> Value {
    let mut request = Map::new();
    request.insert(
        "model".to_owned(),
        Value::String(deployment.upstream_model.clone()),
    );
    request.insert("messages".to_owned(), Value::Array(build_messages(&body)));

    if let Some(tools) = build_tools(&body) {
        request.insert("tools".to_owned(), Value::Array(tools));
        if let Some(choice) = build_tool_choice(&body) {
            request.insert("tool_choice".to_owned(), choice);
        }
    }

    if let Some(max_tokens) = body.get("max_tokens").cloned() {
        request.insert("max_tokens".to_owned(), max_tokens);
    }
    for key in ["temperature", "top_p"] {
        if let Some(value) = body.get(key).cloned() {
            request.insert(key.to_owned(), value);
        }
    }
    if let Some(stop) = body.get("stop_sequences").cloned() {
        request.insert("stop".to_owned(), stop);
    }

    let stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    if stream {
        request.insert("stream".to_owned(), Value::Bool(true));
        // Ask DeepSeek to emit a final usage chunk so token counts survive.
        request.insert(
            "stream_options".to_owned(),
            json!({ "include_usage": true }),
        );
    }

    Value::Object(request)
}

fn build_messages(body: &Value) -> Vec<Value> {
    let mut messages = Vec::new();

    if let Some(system) = body.get("system") {
        let text = blocks_text(system);
        if !text.is_empty() {
            messages.push(json!({ "role": "system", "content": text }));
        }
    }

    let Some(items) = body.get("messages").and_then(Value::as_array) else {
        return messages;
    };
    for message in items {
        let role = message.get("role").and_then(Value::as_str).unwrap_or("user");
        let content = message.get("content").unwrap_or(&Value::Null);
        match role {
            "assistant" => push_assistant(&mut messages, content),
            _ => push_user(&mut messages, content),
        }
    }
    messages
}

fn push_user(messages: &mut Vec<Value>, content: &Value) {
    // tool_result blocks become standalone `tool` messages and must precede any
    // fresh user text so they line up with the preceding assistant tool_calls.
    if let Value::Array(blocks) = content {
        for block in blocks {
            if block.get("type").and_then(Value::as_str) == Some("tool_result") {
                let tool_use_id = block
                    .get("tool_use_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": tool_use_id,
                    "content": blocks_text(block.get("content").unwrap_or(&Value::Null)),
                }));
            }
        }
    }
    let text = blocks_text(content);
    if !text.is_empty() {
        messages.push(json!({ "role": "user", "content": text }));
    }
}

fn push_assistant(messages: &mut Vec<Value>, content: &Value) {
    let mut tool_calls = Vec::new();
    if let Value::Array(blocks) = content {
        for block in blocks {
            if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                let arguments = block
                    .get("input")
                    .map(|input| input.to_string())
                    .unwrap_or_else(|| "{}".to_owned());
                tool_calls.push(json!({
                    "id": block.get("id").and_then(Value::as_str).unwrap_or_default(),
                    "type": "function",
                    "function": {
                        "name": block.get("name").and_then(Value::as_str).unwrap_or_default(),
                        "arguments": arguments,
                    }
                }));
            }
        }
    }

    let text = blocks_text(content);
    let mut message = Map::new();
    message.insert("role".to_owned(), Value::String("assistant".to_owned()));
    // OpenAI accepts null content when tool_calls are present.
    message.insert(
        "content".to_owned(),
        if text.is_empty() && !tool_calls.is_empty() {
            Value::Null
        } else {
            Value::String(text)
        },
    );
    if !tool_calls.is_empty() {
        message.insert("tool_calls".to_owned(), Value::Array(tool_calls));
    }
    messages.push(Value::Object(message));
}

fn build_tools(body: &Value) -> Option<Vec<Value>> {
    let tools = body.get("tools").and_then(Value::as_array)?;
    let mapped: Vec<Value> = tools
        .iter()
        .filter_map(|tool| {
            let name = tool.get("name").and_then(Value::as_str)?;
            let mut function = Map::new();
            function.insert("name".to_owned(), Value::String(name.to_owned()));
            if let Some(description) = tool.get("description").cloned() {
                function.insert("description".to_owned(), description);
            }
            function.insert(
                "parameters".to_owned(),
                tool.get("input_schema")
                    .cloned()
                    .unwrap_or_else(|| json!({ "type": "object", "properties": {} })),
            );
            Some(json!({ "type": "function", "function": Value::Object(function) }))
        })
        .collect();
    (!mapped.is_empty()).then_some(mapped)
}

fn build_tool_choice(body: &Value) -> Option<Value> {
    let choice = body.get("tool_choice")?;
    match choice.get("type").and_then(Value::as_str) {
        Some("auto") => Some(Value::String("auto".to_owned())),
        Some("any") => Some(Value::String("required".to_owned())),
        Some("none") => Some(Value::String("none".to_owned())),
        Some("tool") => choice.get("name").and_then(Value::as_str).map(|name| {
            json!({ "type": "function", "function": { "name": name } })
        }),
        _ => None,
    }
}

/// Flatten Anthropic content (string, block, or array of blocks) to plain text.
fn blocks_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| match item {
                Value::String(text) => Some(text.clone()),
                Value::Object(map) if map.get("type").and_then(Value::as_str) == Some("text") => {
                    map.get("text").and_then(Value::as_str).map(str::to_owned)
                }
                _ => None,
            })
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(map) => map
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Response: Chat Completions -> Anthropic Messages
// ---------------------------------------------------------------------------

fn stop_reason(parsed: &ParsedChatCompletion) -> &'static str {
    match parsed.finish_reason.as_deref() {
        Some("length") => "max_tokens",
        Some("tool_calls") => "tool_use",
        _ if !parsed.tool_calls.is_empty() => "tool_use",
        _ => "end_turn",
    }
}

fn tool_input(call: &ToolCall) -> Value {
    serde_json::from_str(&call.arguments).unwrap_or_else(|_| json!({}))
}

fn content_blocks(parsed: &ParsedChatCompletion) -> Vec<Value> {
    let mut blocks = Vec::new();
    if !parsed.text.is_empty() {
        blocks.push(json!({ "type": "text", "text": parsed.text }));
    }
    for call in &parsed.tool_calls {
        blocks.push(json!({
            "type": "tool_use",
            "id": call.id,
            "name": call.name,
            "input": tool_input(call),
        }));
    }
    if blocks.is_empty() {
        blocks.push(json!({ "type": "text", "text": "" }));
    }
    blocks
}

fn model_name<'a>(parsed: &'a ParsedChatCompletion, deployment: &'a Deployment) -> &'a str {
    parsed
        .model
        .as_deref()
        .unwrap_or(deployment.upstream_model.as_str())
}

pub(super) fn chat_completion_to_anthropic_message(
    parsed: &ParsedChatCompletion,
    deployment: &Deployment,
) -> Value {
    json!({
        "id": parsed.id,
        "type": "message",
        "role": "assistant",
        "model": model_name(parsed, deployment),
        "content": content_blocks(parsed),
        "stop_reason": stop_reason(parsed),
        "stop_sequence": null,
        "usage": {
            "input_tokens": parsed.input_tokens,
            "output_tokens": parsed.output_tokens,
        }
    })
}

pub(super) fn chat_completion_to_anthropic_sse(
    parsed: &ParsedChatCompletion,
    deployment: &Deployment,
) -> String {
    let mut out = String::new();

    push_sse(
        &mut out,
        "message_start",
        json!({
            "type": "message_start",
            "message": {
                "id": parsed.id,
                "type": "message",
                "role": "assistant",
                "model": model_name(parsed, deployment),
                "content": [],
                "stop_reason": null,
                "stop_sequence": null,
                "usage": { "input_tokens": parsed.input_tokens, "output_tokens": 0 },
            }
        }),
    );

    let mut index = 0;
    if !parsed.text.is_empty() {
        push_sse(
            &mut out,
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": index,
                "content_block": { "type": "text", "text": "" }
            }),
        );
        push_sse(
            &mut out,
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": index,
                "delta": { "type": "text_delta", "text": parsed.text }
            }),
        );
        push_sse(
            &mut out,
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": index }),
        );
        index += 1;
    }

    for call in &parsed.tool_calls {
        push_sse(
            &mut out,
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": index,
                "content_block": {
                    "type": "tool_use",
                    "id": call.id,
                    "name": call.name,
                    "input": {}
                }
            }),
        );
        if !call.arguments.is_empty() {
            push_sse(
                &mut out,
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": index,
                    "delta": { "type": "input_json_delta", "partial_json": call.arguments }
                }),
            );
        }
        push_sse(
            &mut out,
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": index }),
        );
        index += 1;
    }

    push_sse(
        &mut out,
        "message_delta",
        json!({
            "type": "message_delta",
            "delta": { "stop_reason": stop_reason(parsed), "stop_sequence": null },
            "usage": {
                "input_tokens": parsed.input_tokens,
                "output_tokens": parsed.output_tokens,
            }
        }),
    );
    push_sse(&mut out, "message_stop", json!({ "type": "message_stop" }));
    out
}

fn push_sse(out: &mut String, event: &str, data: Value) {
    out.push_str("event: ");
    out.push_str(event);
    out.push_str("\ndata: ");
    out.push_str(&data.to_string());
    out.push_str("\n\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deployment() -> Deployment {
        Deployment {
            provider_id: "deepseek".to_owned(),
            upstream_model: "deepseek-chat".to_owned(),
            api_base: "https://api.deepseek.com".to_owned(),
            api_key: "sk-upstream".to_owned(),
        }
    }

    #[test]
    fn maps_system_and_messages() {
        let req = anthropic_messages_to_chat_completions(
            json!({
                "model": "deepseek-chat",
                "system": "be terse",
                "max_tokens": 100,
                "messages": [{ "role": "user", "content": "hi" }]
            }),
            &deployment(),
        );
        assert_eq!(req["model"], "deepseek-chat");
        assert_eq!(req["max_tokens"], 100);
        let messages = req["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "be terse");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"], "hi");
    }

    #[test]
    fn maps_tools_and_tool_choice() {
        let req = anthropic_messages_to_chat_completions(
            json!({
                "model": "deepseek-chat",
                "messages": [{ "role": "user", "content": "weather?" }],
                "tools": [{
                    "name": "get_weather",
                    "description": "Look up weather",
                    "input_schema": { "type": "object", "properties": { "city": { "type": "string" } } }
                }],
                "tool_choice": { "type": "any" }
            }),
            &deployment(),
        );
        let tool = &req["tools"][0];
        assert_eq!(tool["type"], "function");
        assert_eq!(tool["function"]["name"], "get_weather");
        assert_eq!(tool["function"]["parameters"]["type"], "object");
        assert_eq!(req["tool_choice"], "required");
    }

    #[test]
    fn maps_assistant_tool_use_and_tool_result() {
        let req = anthropic_messages_to_chat_completions(
            json!({
                "model": "deepseek-chat",
                "messages": [
                    { "role": "user", "content": "weather in sf?" },
                    { "role": "assistant", "content": [
                        { "type": "tool_use", "id": "call_1", "name": "get_weather", "input": { "city": "sf" } }
                    ]},
                    { "role": "user", "content": [
                        { "type": "tool_result", "tool_use_id": "call_1", "content": "sunny" }
                    ]}
                ]
            }),
            &deployment(),
        );
        let messages = req["messages"].as_array().unwrap();
        // user, assistant(tool_calls), tool
        let assistant = &messages[1];
        assert_eq!(assistant["role"], "assistant");
        assert!(assistant["content"].is_null());
        assert_eq!(assistant["tool_calls"][0]["id"], "call_1");
        assert_eq!(assistant["tool_calls"][0]["function"]["name"], "get_weather");
        assert_eq!(
            assistant["tool_calls"][0]["function"]["arguments"],
            "{\"city\":\"sf\"}"
        );
        let tool = &messages[2];
        assert_eq!(tool["role"], "tool");
        assert_eq!(tool["tool_call_id"], "call_1");
        assert_eq!(tool["content"], "sunny");
    }

    #[test]
    fn stream_request_includes_usage_option() {
        let req = anthropic_messages_to_chat_completions(
            json!({
                "model": "deepseek-chat",
                "stream": true,
                "messages": [{ "role": "user", "content": "hi" }]
            }),
            &deployment(),
        );
        assert_eq!(req["stream"], true);
        assert_eq!(req["stream_options"]["include_usage"], true);
    }

    #[test]
    fn response_to_anthropic_message_with_tool_use() {
        let parsed = ParsedChatCompletion {
            id: "chatcmpl-1".to_owned(),
            model: Some("deepseek-chat".to_owned()),
            text: String::new(),
            tool_calls: vec![ToolCall {
                id: "call_1".to_owned(),
                name: "get_weather".to_owned(),
                arguments: "{\"city\":\"sf\"}".to_owned(),
            }],
            finish_reason: Some("tool_calls".to_owned()),
            input_tokens: 9,
            output_tokens: 4,
        };
        let msg = chat_completion_to_anthropic_message(&parsed, &deployment());
        assert_eq!(msg["stop_reason"], "tool_use");
        assert_eq!(msg["content"][0]["type"], "tool_use");
        assert_eq!(msg["content"][0]["input"]["city"], "sf");
        assert_eq!(msg["usage"]["input_tokens"], 9);
    }

    #[test]
    fn response_to_sse_has_message_and_block_events() {
        let parsed = ParsedChatCompletion {
            id: "chatcmpl-1".to_owned(),
            model: Some("deepseek-chat".to_owned()),
            text: "hello".to_owned(),
            tool_calls: Vec::new(),
            finish_reason: Some("stop".to_owned()),
            input_tokens: 3,
            output_tokens: 2,
        };
        let sse = chat_completion_to_anthropic_sse(&parsed, &deployment());
        assert!(sse.contains("event: message_start"));
        assert!(sse.contains("event: content_block_delta"));
        assert!(sse.contains("\"text\":\"hello\""));
        assert!(sse.contains("event: message_stop"));
    }
}
