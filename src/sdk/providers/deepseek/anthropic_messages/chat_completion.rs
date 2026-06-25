//! Parse a DeepSeek (OpenAI Chat Completions) response into a normalized shape.
//!
//! The gateway buffers the full upstream body before this runs, so both the
//! non-streaming JSON form and the streaming SSE form are parsed here in one
//! pass — no incremental state machine is needed.

use serde_json::Value;

use crate::errors::GatewayError;

#[derive(Debug, Default, Clone)]
pub(super) struct ParsedChatCompletion {
    pub id: String,
    pub model: Option<String>,
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

#[derive(Debug, Default, Clone)]
pub(super) struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

pub(super) fn parse_chat_completion(
    body: &[u8],
    content_type: Option<&str>,
) -> Result<ParsedChatCompletion, GatewayError> {
    if content_type.unwrap_or_default().contains("text/event-stream") || looks_like_sse(body) {
        return Ok(parse_sse(body));
    }
    let raw: Value = serde_json::from_slice(body)?;
    Ok(parse_json(&raw))
}

fn looks_like_sse(body: &[u8]) -> bool {
    String::from_utf8_lossy(body)
        .lines()
        .any(|line| line.trim_start().starts_with("data:"))
}

fn parse_json(raw: &Value) -> ParsedChatCompletion {
    let mut parsed = ParsedChatCompletion {
        id: raw
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("chatcmpl_deepseek")
            .to_owned(),
        model: raw.get("model").and_then(Value::as_str).map(str::to_owned),
        ..Default::default()
    };

    if let Some(message) = raw
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| {
            parsed.finish_reason = choice
                .get("finish_reason")
                .and_then(Value::as_str)
                .map(str::to_owned);
            choice.get("message")
        })
    {
        if let Some(text) = message.get("content").and_then(Value::as_str) {
            parsed.text.push_str(text);
        }
        if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
            for call in tool_calls {
                parsed.tool_calls.push(ToolCall {
                    id: call.get("id").and_then(Value::as_str).unwrap_or("").to_owned(),
                    name: call
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    arguments: call
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                });
            }
        }
    }

    apply_usage(&mut parsed, raw);
    parsed
}

fn parse_sse(body: &[u8]) -> ParsedChatCompletion {
    let mut parsed = ParsedChatCompletion {
        id: "chatcmpl_deepseek".to_owned(),
        ..Default::default()
    };
    for line in String::from_utf8_lossy(body).lines() {
        let Some(data) = line.trim_start().strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(chunk) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        apply_chunk(&mut parsed, &chunk);
    }
    parsed
}

fn apply_chunk(parsed: &mut ParsedChatCompletion, chunk: &Value) {
    if let Some(id) = chunk.get("id").and_then(Value::as_str) {
        parsed.id = id.to_owned();
    }
    if parsed.model.is_none() {
        parsed.model = chunk.get("model").and_then(Value::as_str).map(str::to_owned);
    }
    if let Some(choice) = chunk
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
    {
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            parsed.finish_reason = Some(reason.to_owned());
        }
        if let Some(delta) = choice.get("delta") {
            if let Some(text) = delta.get("content").and_then(Value::as_str) {
                parsed.text.push_str(text);
            }
            if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
                for call in tool_calls {
                    apply_tool_call_delta(parsed, call);
                }
            }
        }
    }
    // Usage only appears (with stream_options.include_usage) on the final chunk,
    // where "choices" is typically empty.
    apply_usage(parsed, chunk);
}

fn apply_tool_call_delta(parsed: &mut ParsedChatCompletion, call: &Value) {
    let index = call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
    while parsed.tool_calls.len() <= index {
        parsed.tool_calls.push(ToolCall::default());
    }
    let slot = &mut parsed.tool_calls[index];
    if let Some(id) = call.get("id").and_then(Value::as_str) {
        if !id.is_empty() {
            slot.id = id.to_owned();
        }
    }
    if let Some(function) = call.get("function") {
        if let Some(name) = function.get("name").and_then(Value::as_str) {
            if !name.is_empty() {
                slot.name = name.to_owned();
            }
        }
        if let Some(arguments) = function.get("arguments").and_then(Value::as_str) {
            slot.arguments.push_str(arguments);
        }
    }
}

fn apply_usage(parsed: &mut ParsedChatCompletion, value: &Value) {
    let Some(usage) = value.get("usage").filter(|usage| !usage.is_null()) else {
        return;
    };
    if let Some(input) = usage.get("prompt_tokens").and_then(Value::as_i64) {
        parsed.input_tokens = input;
    }
    if let Some(output) = usage.get("completion_tokens").and_then(Value::as_i64) {
        parsed.output_tokens = output;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_text_json() {
        let body = json!({
            "id": "chatcmpl-1",
            "model": "deepseek-chat",
            "choices": [{
                "message": { "role": "assistant", "content": "hello" },
                "finish_reason": "stop"
            }],
            "usage": { "prompt_tokens": 7, "completion_tokens": 3 }
        });
        let parsed = parse_chat_completion(body.to_string().as_bytes(), None).unwrap();
        assert_eq!(parsed.text, "hello");
        assert_eq!(parsed.finish_reason.as_deref(), Some("stop"));
        assert_eq!(parsed.input_tokens, 7);
        assert_eq!(parsed.output_tokens, 3);
        assert!(parsed.tool_calls.is_empty());
    }

    #[test]
    fn parses_tool_calls_json() {
        let body = json!({
            "id": "chatcmpl-2",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": { "name": "get_weather", "arguments": "{\"city\":\"sf\"}" }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });
        let parsed = parse_chat_completion(body.to_string().as_bytes(), None).unwrap();
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].id, "call_1");
        assert_eq!(parsed.tool_calls[0].name, "get_weather");
        assert_eq!(parsed.tool_calls[0].arguments, "{\"city\":\"sf\"}");
        assert_eq!(parsed.finish_reason.as_deref(), Some("tool_calls"));
    }

    #[test]
    fn accumulates_streaming_text_and_tool_args() {
        let body = concat!(
            "data: {\"id\":\"c\",\"model\":\"deepseek-chat\",\"choices\":[{\"delta\":{\"content\":\"he\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"llo\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_9\",\"function\":{\"name\":\"f\",\"arguments\":\"{\\\"a\\\":\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"1}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":5}}\n\n",
            "data: [DONE]\n\n"
        );
        let parsed = parse_chat_completion(body.as_bytes(), Some("text/event-stream")).unwrap();
        assert_eq!(parsed.text, "hello");
        assert_eq!(parsed.id, "c");
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].id, "call_9");
        assert_eq!(parsed.tool_calls[0].name, "f");
        assert_eq!(parsed.tool_calls[0].arguments, "{\"a\":1}");
        assert_eq!(parsed.finish_reason.as_deref(), Some("tool_calls"));
        assert_eq!(parsed.input_tokens, 11);
        assert_eq!(parsed.output_tokens, 5);
    }
}
