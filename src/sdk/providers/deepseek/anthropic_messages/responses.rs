//! Translate between the OpenAI Responses API (inbound, used by opencode/Codex)
//! and DeepSeek's Chat Completions API (upstream).
//!
//! This lives alongside the Anthropic Messages translation because a single
//! `deepseek` provider serves both public surfaces from one chat-completions
//! upstream; the module name reflects the registration home, not the surface.

use serde_json::{json, Map, Value};

use crate::sdk::routing::Deployment;

use super::chat_completion::{ParsedChatCompletion, ToolCall};

// ---------------------------------------------------------------------------
// Request: OpenAI Responses -> Chat Completions
// ---------------------------------------------------------------------------

pub(super) fn openai_responses_to_chat_completions(body: Value, deployment: &Deployment) -> Value {
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

    if let Some(max_tokens) = body
        .get("max_output_tokens")
        .filter(|v| !v.is_null())
        .cloned()
    {
        request.insert("max_tokens".to_owned(), max_tokens);
    }
    for key in ["temperature", "top_p"] {
        if let Some(value) = body.get(key).filter(|v| !v.is_null()).cloned() {
            request.insert(key.to_owned(), value);
        }
    }

    let stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    if stream {
        request.insert("stream".to_owned(), Value::Bool(true));
        request.insert("stream_options".to_owned(), json!({ "include_usage": true }));
    }

    Value::Object(request)
}

fn build_messages(body: &Value) -> Vec<Value> {
    let mut messages = Vec::new();

    // `instructions` is the Responses-API system prompt (opencode usually puts
    // the system text as the first input item instead, but handle both).
    if let Some(instructions) = body.get("instructions").and_then(Value::as_str) {
        if !instructions.is_empty() {
            messages.push(json!({ "role": "system", "content": instructions }));
        }
    }

    let input = body.get("input");
    match input {
        Some(Value::String(text)) => {
            messages.push(json!({ "role": "user", "content": text }));
        }
        Some(Value::Array(items)) => {
            for item in items {
                push_input_item(&mut messages, item);
            }
        }
        _ => {}
    }
    messages
}

fn push_input_item(messages: &mut Vec<Value>, item: &Value) {
    match item.get("type").and_then(Value::as_str) {
        Some("function_call") => {
            let call_id = item.get("call_id").and_then(Value::as_str).unwrap_or_default();
            let name = item.get("name").and_then(Value::as_str).unwrap_or_default();
            let arguments = item.get("arguments").and_then(Value::as_str).unwrap_or("{}");

            let tool_call = json!({
                "id": call_id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": arguments,
                }
            });

            let mut merged = false;
            if let Some(last_msg) = messages.last_mut() {
                if last_msg.get("role").and_then(Value::as_str) == Some("assistant") {
                    if let Some(obj) = last_msg.as_object_mut() {
                        if let Some(tool_calls) = obj.get_mut("tool_calls").and_then(Value::as_array_mut) {
                            tool_calls.push(tool_call.clone());
                            merged = true;
                        } else {
                            obj.insert("tool_calls".to_owned(), Value::Array(vec![tool_call.clone()]));
                            merged = true;
                        }
                    }
                }
            }

            if !merged {
                messages.push(json!({
                    "role": "assistant",
                    "content": Value::Null,
                    "tool_calls": [tool_call]
                }));
            }
        }
        Some("function_call_output") => {
            messages.push(json!({
                "role": "tool",
                "tool_call_id": item.get("call_id").and_then(Value::as_str).unwrap_or_default(),
                "content": output_text(item.get("output").unwrap_or(&Value::Null)),
            }));
        }
        // "message" or a bare { role, content } item.
        _ => {
            if let Some(role) = item.get("role").and_then(Value::as_str) {
                let content = input_content_text(item.get("content").unwrap_or(&Value::Null));
                
                let mut merged = false;
                if role == "assistant" {
                    if let Some(last_msg) = messages.last_mut() {
                        if last_msg.get("role").and_then(Value::as_str) == Some("assistant") {
                            if let Some(obj) = last_msg.as_object_mut() {
                                if !obj.contains_key("tool_calls") {
                                    let old_content = obj.get("content").and_then(Value::as_str).unwrap_or("");
                                    let new_content = if old_content.is_empty() {
                                        content.clone()
                                    } else {
                                        format!("{old_content}\n{content}")
                                    };
                                    obj.insert("content".to_owned(), Value::String(new_content));
                                    merged = true;
                                }
                            }
                        }
                    }
                }

                if !merged {
                    messages.push(json!({ "role": role, "content": content }));
                }
            }
        }
    }
}

/// Responses content parts use `input_text` / `output_text`; flatten to a string.
fn input_content_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| match part {
                Value::String(text) => Some(text.clone()),
                Value::Object(map) => map.get("text").and_then(Value::as_str).map(str::to_owned),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn output_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        _ => input_content_text(value),
    }
}

fn build_tools(body: &Value) -> Option<Vec<Value>> {
    let tools = body.get("tools").and_then(Value::as_array)?;
    let mapped: Vec<Value> = tools
        .iter()
        .filter_map(|tool| {
            // Responses tools are flat: { type: "function", name, description, parameters }.
            let name = tool.get("name").and_then(Value::as_str)?;
            let mut function = Map::new();
            function.insert("name".to_owned(), Value::String(name.to_owned()));
            if let Some(description) = tool.get("description").cloned() {
                function.insert("description".to_owned(), description);
            }
            function.insert(
                "parameters".to_owned(),
                tool.get("parameters")
                    .cloned()
                    .unwrap_or_else(|| json!({ "type": "object", "properties": {} })),
            );
            Some(json!({ "type": "function", "function": Value::Object(function) }))
        })
        .collect();
    (!mapped.is_empty()).then_some(mapped)
}

fn build_tool_choice(body: &Value) -> Option<Value> {
    match body.get("tool_choice") {
        Some(Value::String(s)) => match s.as_str() {
            "auto" => Some(Value::String("auto".to_owned())),
            "none" => Some(Value::String("none".to_owned())),
            "required" => Some(Value::String("required".to_owned())),
            _ => None,
        },
        Some(Value::Object(map)) => map
            .get("name")
            .and_then(Value::as_str)
            .map(|name| json!({ "type": "function", "function": { "name": name } })),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Response: Chat Completions -> OpenAI Responses
// ---------------------------------------------------------------------------

fn resp_id(parsed: &ParsedChatCompletion) -> String {
    format!("resp_{}", parsed.id)
}

fn model_name<'a>(parsed: &'a ParsedChatCompletion, deployment: &'a Deployment) -> &'a str {
    parsed
        .model
        .as_deref()
        .unwrap_or(deployment.upstream_model.as_str())
}

fn message_item(parsed: &ParsedChatCompletion) -> Option<Value> {
    (!parsed.text.is_empty()).then(|| {
        json!({
            "type": "message",
            "id": format!("msg_{}", parsed.id),
            "role": "assistant",
            "status": "completed",
            "content": [{ "type": "output_text", "text": parsed.text, "annotations": [] }],
        })
    })
}

fn function_call_item(call: &ToolCall) -> Value {
    json!({
        "type": "function_call",
        "id": format!("fc_{}", call.id),
        "call_id": call.id,
        "name": call.name,
        "arguments": call.arguments,
        "status": "completed",
    })
}

fn output_items(parsed: &ParsedChatCompletion) -> Vec<Value> {
    let mut items = Vec::new();
    if let Some(message) = message_item(parsed) {
        items.push(message);
    }
    for call in &parsed.tool_calls {
        items.push(function_call_item(call));
    }
    items
}

fn usage(parsed: &ParsedChatCompletion) -> Value {
    json!({
        "input_tokens": parsed.input_tokens,
        "output_tokens": parsed.output_tokens,
        "total_tokens": parsed.input_tokens + parsed.output_tokens,
    })
}

fn response_object(parsed: &ParsedChatCompletion, deployment: &Deployment, status: &str) -> Value {
    json!({
        "id": resp_id(parsed),
        "object": "response",
        "created_at": 0,
        "status": status,
        "model": model_name(parsed, deployment),
        "output": output_items(parsed),
        "usage": usage(parsed),
    })
}

pub(super) fn chat_completion_to_openai_response(
    parsed: &ParsedChatCompletion,
    deployment: &Deployment,
) -> Value {
    response_object(parsed, deployment, "completed")
}

pub(super) fn chat_completion_to_openai_response_sse(
    parsed: &ParsedChatCompletion,
    deployment: &Deployment,
) -> String {
    let mut out = String::new();
    let mut seq = 0;

    let mut created = response_object(parsed, deployment, "in_progress");
    created["output"] = json!([]);
    push(&mut out, &mut seq, "response.created", json!({ "response": created }));

    let mut output_index = 0;

    if let Some(message) = message_item(parsed) {
        let item_id = message["id"].as_str().unwrap_or("msg").to_owned();
        let mut in_progress = message.clone();
        in_progress["status"] = json!("in_progress");
        in_progress["content"] = json!([]);
        push(&mut out, &mut seq, "response.output_item.added", json!({
            "output_index": output_index, "item": in_progress
        }));
        push(&mut out, &mut seq, "response.content_part.added", json!({
            "item_id": item_id, "output_index": output_index, "content_index": 0,
            "part": { "type": "output_text", "text": "", "annotations": [] }
        }));
        push(&mut out, &mut seq, "response.output_text.delta", json!({
            "item_id": item_id, "output_index": output_index, "content_index": 0,
            "delta": parsed.text
        }));
        push(&mut out, &mut seq, "response.output_text.done", json!({
            "item_id": item_id, "output_index": output_index, "content_index": 0,
            "text": parsed.text
        }));
        push(&mut out, &mut seq, "response.content_part.done", json!({
            "item_id": item_id, "output_index": output_index, "content_index": 0,
            "part": { "type": "output_text", "text": parsed.text, "annotations": [] }
        }));
        push(&mut out, &mut seq, "response.output_item.done", json!({
            "output_index": output_index, "item": message
        }));
        output_index += 1;
    }

    for call in &parsed.tool_calls {
        let item = function_call_item(call);
        let item_id = item["id"].as_str().unwrap_or("fc").to_owned();
        let mut added = item.clone();
        added["arguments"] = json!("");
        added["status"] = json!("in_progress");
        push(&mut out, &mut seq, "response.output_item.added", json!({
            "output_index": output_index, "item": added
        }));
        if !call.arguments.is_empty() {
            push(&mut out, &mut seq, "response.function_call_arguments.delta", json!({
                "item_id": item_id, "output_index": output_index, "delta": call.arguments
            }));
        }
        push(&mut out, &mut seq, "response.function_call_arguments.done", json!({
            "item_id": item_id, "output_index": output_index, "arguments": call.arguments
        }));
        push(&mut out, &mut seq, "response.output_item.done", json!({
            "output_index": output_index, "item": item
        }));
        output_index += 1;
    }

    push(&mut out, &mut seq, "response.completed", json!({
        "response": response_object(parsed, deployment, "completed")
    }));
    out
}

fn push(out: &mut String, seq: &mut i64, event: &str, mut data: Value) {
    if let Some(obj) = data.as_object_mut() {
        obj.insert("type".to_owned(), Value::String(event.to_owned()));
        obj.insert("sequence_number".to_owned(), json!(*seq));
    }
    *seq += 1;
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
    fn maps_responses_input_and_tools() {
        let req = openai_responses_to_chat_completions(
            json!({
                "model": "deepseek-chat",
                "stream": true,
                "tool_choice": "auto",
                "input": [
                    { "role": "system", "content": "be terse" },
                    { "role": "user", "content": [{ "type": "input_text", "text": "hi" }] }
                ],
                "tools": [{ "type": "function", "name": "bash", "description": "run", "parameters": { "type": "object" } }]
            }),
            &deployment(),
        );
        let msgs = req["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[1]["content"], "hi");
        assert_eq!(req["tools"][0]["function"]["name"], "bash");
        assert_eq!(req["tool_choice"], "auto");
        assert_eq!(req["stream_options"]["include_usage"], true);
    }

    #[test]
    fn maps_function_call_and_output() {
        let req = openai_responses_to_chat_completions(
            json!({
                "input": [
                    { "type": "function_call", "call_id": "c1", "name": "bash", "arguments": "{\"command\":\"ls\"}" },
                    { "type": "function_call_output", "call_id": "c1", "output": "file.txt" }
                ]
            }),
            &deployment(),
        );
        let msgs = req["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["tool_calls"][0]["id"], "c1");
        assert_eq!(msgs[0]["tool_calls"][0]["function"]["name"], "bash");
        assert_eq!(msgs[1]["role"], "tool");
        assert_eq!(msgs[1]["tool_call_id"], "c1");
        assert_eq!(msgs[1]["content"], "file.txt");
    }

    #[test]
    fn groups_consecutive_function_calls() {
        let req = openai_responses_to_chat_completions(
            json!({
                "input": [
                    { "role": "assistant", "content": "Let me call some tools." },
                    { "type": "function_call", "call_id": "c1", "name": "bash", "arguments": "{\"command\":\"ls\"}" },
                    { "type": "function_call", "call_id": "c2", "name": "read", "arguments": "{\"path\":\"a.txt\"}" },
                    { "type": "function_call_output", "call_id": "c1", "output": "ls output" },
                    { "type": "function_call_output", "call_id": "c2", "output": "read output" }
                ]
            }),
            &deployment(),
        );
        let msgs = req["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0]["role"], "assistant");
        assert_eq!(msgs[0]["content"], "Let me call some tools.");
        assert_eq!(msgs[0]["tool_calls"][0]["id"], "c1");
        assert_eq!(msgs[0]["tool_calls"][1]["id"], "c2");
        assert_eq!(msgs[1]["role"], "tool");
        assert_eq!(msgs[1]["tool_call_id"], "c1");
        assert_eq!(msgs[2]["role"], "tool");
        assert_eq!(msgs[2]["tool_call_id"], "c2");
    }

    #[test]
    fn non_stream_response_has_output_message() {
        let parsed = ParsedChatCompletion {
            id: "x".to_owned(),
            model: Some("deepseek-chat".to_owned()),
            text: "hello".to_owned(),
            tool_calls: Vec::new(),
            finish_reason: Some("stop".to_owned()),
            input_tokens: 3,
            output_tokens: 2,
        };
        let resp = chat_completion_to_openai_response(&parsed, &deployment());
        assert_eq!(resp["object"], "response");
        assert_eq!(resp["status"], "completed");
        assert_eq!(resp["output"][0]["type"], "message");
        assert_eq!(resp["output"][0]["content"][0]["text"], "hello");
        assert_eq!(resp["usage"]["total_tokens"], 5);
    }

    #[test]
    fn stream_response_emits_created_and_completed() {
        let parsed = ParsedChatCompletion {
            id: "x".to_owned(),
            model: Some("deepseek-chat".to_owned()),
            text: "hi".to_owned(),
            tool_calls: vec![ToolCall {
                id: "c1".to_owned(),
                name: "bash".to_owned(),
                arguments: "{}".to_owned(),
            }],
            finish_reason: Some("tool_calls".to_owned()),
            input_tokens: 1,
            output_tokens: 1,
        };
        let sse = chat_completion_to_openai_response_sse(&parsed, &deployment());
        assert!(sse.contains("event: response.created"));
        assert!(sse.contains("event: response.output_text.delta"));
        assert!(sse.contains("event: response.function_call_arguments.done"));
        assert!(sse.contains("event: response.completed"));
        assert!(sse.contains("\"sequence_number\""));
    }
}
