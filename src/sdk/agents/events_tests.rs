use serde_json::json;

use super::{parse_sse, AgentEventKind, AgentEventPayload};

#[test]
fn serde_roundtrip_keeps_flat_event_shape() {
    let events = parse_sse(
        "data: {\"type\":\"agent.message\",\"content\":[{\"type\":\"text\",\"text\":\"hello\"}]}\n\n",
    )
    .unwrap();

    assert_eq!(
        serde_json::to_value(&events[0]).unwrap(),
        json!({
            "type": "agent.message",
            "content": [{ "type": "text", "text": "hello" }]
        })
    );
}

#[test]
fn sse_event_name_supplies_missing_type() {
    let events = parse_sse(
        "event: agent.message\n\
         data: {\"content\":[{\"type\":\"text\",\"text\":\"hello\"}]}\n\n",
    )
    .unwrap();

    assert_eq!(events[0].kind(), AgentEventKind::AgentMessage);
}

#[test]
fn payload_type_wins_over_sse_event_name() {
    let events = parse_sse(
        "event: interaction_update\n\
         data: {\"type\":\"agent.message\",\"content\":[{\"type\":\"text\",\"text\":\"hello\"}]}\n\n",
    )
    .unwrap();

    assert_eq!(events[0].kind(), AgentEventKind::AgentMessage);
}

#[test]
fn unknown_events_preserve_fields() {
    let events = parse_sse("data: {\"type\":\"runtime.future\",\"x\":1}\n\n").unwrap();

    assert_eq!(
        events[0].payload(),
        AgentEventPayload::Unknown {
            event_type: "runtime.future".to_owned(),
            data: [("x".to_owned(), json!(1))].into_iter().collect(),
        }
    );
}

#[test]
fn typed_payload_keeps_extra_fields() {
    let events = parse_sse(
        "data: {\"type\":\"agent.tool_use\",\"id\":\"call_1\",\"name\":\"edit\",\"input\":{\"path\":\"README.md\"},\"processed_at\":\"now\"}\n\n",
    )
    .unwrap();

    let AgentEventPayload::AgentToolUse(payload) = events[0].payload() else {
        panic!("expected tool use payload");
    };
    assert_eq!(payload.id.as_deref(), Some("call_1"));
    assert_eq!(payload.name.as_deref(), Some("edit"));
    assert_eq!(payload.raw["processed_at"], json!("now"));
}

#[test]
fn sse_split_utf8_boundary() {
    let mut parser = super::SseParser::default();
    // A 3-byte Chinese character '哈' has UTF-8 bytes: [0xe5, 0x93, 0x88]
    // Let's send the first 2 bytes in chunk 1, and the 3rd byte in chunk 2.
    // The SSE format: data: {"type":"agent.message","content":[{"type":"text","text":"哈"}]}\n\n
    // Let's split it exactly in the middle of '哈'.
    let first_part = "data: {\"type\":\"agent.message\",\"content\":[{\"type\":\"text\",\"text\":\"".as_bytes();
    let utf8_char = [0xe5, 0x93, 0x88];
    let last_part = "\"}]}\n\n".as_bytes();
    
    let mut chunk1 = first_part.to_vec();
    chunk1.extend_from_slice(&utf8_char[..2]); // first 2 bytes of '哈'
    
    let mut chunk2 = vec![utf8_char[2]]; // 3rd byte of '哈'
    chunk2.extend_from_slice(last_part);
    
    let mut events = parser.push(&chunk1).unwrap();
    assert!(events.is_empty());
    
    events.extend(parser.push(&chunk2).unwrap());
    assert_eq!(events.len(), 1);
    
    assert_eq!(
        serde_json::to_value(&events[0]).unwrap(),
        json!({
            "type": "agent.message",
            "content": [{ "type": "text", "text": "哈" }]
        })
    );
}
