use std::hash::{DefaultHasher, Hash, Hasher};

use serde_json::Value;
use sqlx::PgPool;

use crate::{
    agents::runs::AgentRunStatus,
    db::managed_agents::{id, runtime_events, session_control, sessions},
    errors::GatewayError,
    proxy::state::AppState,
    sdk::agents::{AgentEvent, AgentEventPayload},
};

pub(super) async fn emit_runtime_stage(
    state: &AppState,
    pool: &PgPool,
    session_id: &str,
    phase: &str,
) -> Result<(), GatewayError> {
    tracing::info!(session_id, phase, "runtime session stage changed");
    let event = serde_json::json!({
        "id": id("stage"),
        "type": "session.status",
        "status": { "type": "running", "phase": phase },
    });
    runtime_events::repository::append(pool, session_id, event.clone()).await?;
    state.local_session_events.publish(session_id, event);
    Ok(())
}

pub(super) async fn mark_session_error(
    state: &AppState,
    pool: &PgPool,
    session_id: &str,
    message: String,
) -> Result<(), GatewayError> {
    mark_session_status(state, pool, session_id, "error", Some(message)).await
}

pub(super) async fn mark_session_status(
    state: &AppState,
    pool: &PgPool,
    session_id: &str,
    status: &str,
    error: Option<String>,
) -> Result<(), GatewayError> {
    sessions::repository::set_status(pool, session_id, status).await?;
    if let Some(snapshot) = session_control::repository::active_turn(pool, session_id).await? {
        match status {
            "idle" => {
                session_control::repository::transition(pool, &snapshot.turn.id, "completed", None)
                    .await?;
            }
            "error" => {
                session_control::repository::transition(
                    pool,
                    &snapshot.turn.id,
                    "failed",
                    Some(serde_json::json!({
                        "code": "runtime_error",
                        "message": error.as_deref().unwrap_or("managed agent interaction failed")
                    })),
                )
                .await?;
            }
            _ => {}
        }
    }
    match status {
        "starting" | "running" | "busy" => {
            crate::db::managed_agents::tasks::repository::mark_running_for_session(
                pool, session_id,
            )
            .await?;
        }
        "idle" => {
            state
                .agent_runs
                .update_status(session_id, AgentRunStatus::Completed);
            crate::db::managed_agents::tasks::artifacts::capture_session_output(pool, session_id)
                .await?;
            crate::db::managed_agents::tasks::repository::mark_verifying_for_session(
                pool, session_id,
            )
            .await?;
        }
        "error" => {
            let message = error.unwrap_or_else(|| "managed agent interaction failed".to_owned());
            state.agent_runs.set_error(session_id, message.clone());
            crate::db::managed_agents::tasks::repository::fail_for_session(
                pool, session_id, &message,
            )
            .await?;
        }
        _ => {}
    }
    Ok(())
}

pub(super) async fn persist_send_response_events(
    pool: &PgPool,
    resolved: &crate::http::runtime_resolution::ResolvedRuntime,
    session_id: &str,
    raw: &Value,
) -> Result<(), GatewayError> {
    for event in resolved.adapter.events_from_send_response_raw(raw) {
        runtime_events::repository::append(pool, session_id, event).await?;
    }
    Ok(())
}

pub(super) async fn persist_runtime_event(
    pool: &PgPool,
    session_id: &str,
    event: &AgentEvent,
) -> Result<(), GatewayError> {
    let event_json = serde_json::to_value(event)
        .map_err(|error| GatewayError::SandboxError(error.to_string()))?;
    runtime_events::repository::append(pool, session_id, event_json).await?;
    if event_requests_input(event) {
        persist_input_request(pool, session_id, event).await?;
    }
    match event.payload() {
        AgentEventPayload::AgentToolUse(tool) => {
            persist_operation_request(pool, session_id, &tool).await?;
        }
        AgentEventPayload::AgentToolResult(result) => {
            persist_operation_result(pool, session_id, &result).await?;
        }
        _ => {}
    }
    if let AgentEventPayload::AgentMessage(message) = event.payload() {
        persist_turn_message(
            pool,
            session_id,
            serde_json::json!({"content": message.content}),
        )
        .await?;
        persist_turn_result(
            pool,
            session_id,
            serde_json::json!({
                "type": "message",
                "content": message.content,
            }),
        )
        .await?;
    } else if let Some(content) = assistant_response_content(event) {
        // Some runtimes emit the terminal assistant turn as `assistant_response`
        // (which maps to `Unknown`, not `AgentMessage`). Persist its text so the
        // transcript survives a reload, mirroring the `AgentMessage` branch.
        persist_turn_message(pool, session_id, serde_json::json!({ "content": content })).await?;
        persist_turn_result(
            pool,
            session_id,
            serde_json::json!({ "type": "message", "content": content }),
        )
        .await?;
    }
    Ok(())
}

/// Extracts assistant message content from an `assistant_response` event —
/// either an explicit `content` array or a bare `text` field — so the
/// non-`AgentMessage`-typed terminal turn still lands in the transcript.
fn assistant_response_content(event: &AgentEvent) -> Option<Value> {
    if event.event_type != "assistant_response" {
        return None;
    }
    if let Some(content) = event.data.get("content").filter(|value| !value.is_null()) {
        return Some(content.clone());
    }
    event
        .data
        .get("text")
        .and_then(Value::as_str)
        .map(|text| serde_json::json!([{ "type": "text", "text": text }]))
}

async fn persist_operation_request(
    pool: &PgPool,
    session_id: &str,
    tool: &crate::sdk::agents::AgentToolUseData,
) -> Result<(), GatewayError> {
    let Some(operation_key) = tool.id.as_deref() else {
        return Ok(());
    };
    let Some(snapshot) = session_control::repository::active_turn(pool, session_id).await? else {
        return Ok(());
    };
    let Some(invocation) = snapshot.invocations.first() else {
        return Ok(());
    };
    let operation = session_control::repository::request_operation(
        pool,
        session_id,
        &snapshot.turn.id,
        &invocation.id,
        operation_key,
        tool.name.as_deref().unwrap_or("tool"),
        serde_json::json!({
            "name": tool.name,
            "input": tool.input,
        }),
    )
    .await?;
    session_control::repository::append_event(
        pool,
        session_control::repository::NewControlEvent {
            session_id,
            turn_id: Some(&snapshot.turn.id),
            invocation_id: Some(&invocation.id),
            request_id: Some(&snapshot.turn.request_id),
            event_key: &format!("operation:{operation_key}:requested"),
            event_type: "operation.requested",
            event: serde_json::json!({"schema_version": 1, "operation": operation}),
        },
    )
    .await?;
    Ok(())
}

async fn persist_operation_result(
    pool: &PgPool,
    session_id: &str,
    result: &crate::sdk::agents::AgentToolResultData,
) -> Result<(), GatewayError> {
    let Some(operation_key) = result.tool_use_id.as_deref() else {
        return Ok(());
    };
    let Some(snapshot) = session_control::repository::active_turn(pool, session_id).await? else {
        return Ok(());
    };
    let Some(invocation) = snapshot.invocations.first() else {
        return Ok(());
    };
    if !session_control::repository::operations_for_turn(pool, &snapshot.turn.id)
        .await?
        .iter()
        .any(|operation| {
            operation.invocation_id == invocation.id && operation.operation_key == operation_key
        })
    {
        session_control::repository::request_operation(
            pool,
            session_id,
            &snapshot.turn.id,
            &invocation.id,
            operation_key,
            "tool",
            serde_json::json!({}),
        )
        .await?;
    }
    let failed = result.raw.get("error").is_some()
        || result.raw.get("status").and_then(Value::as_str) == Some("error");
    let status = if failed { "failed" } else { "completed" };
    let error = failed.then(|| {
        result
            .raw
            .get("error")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({"message": "tool operation failed"}))
    });
    let Some(operation) = session_control::repository::resolve_operation(
        pool,
        &invocation.id,
        operation_key,
        status,
        result.content.clone(),
        error,
    )
    .await?
    else {
        return Ok(());
    };
    session_control::repository::append_event(
        pool,
        session_control::repository::NewControlEvent {
            session_id,
            turn_id: Some(&snapshot.turn.id),
            invocation_id: Some(&invocation.id),
            request_id: Some(&snapshot.turn.request_id),
            event_key: &format!("operation:{operation_key}:{status}"),
            event_type: if failed {
                "operation.failed"
            } else {
                "operation.completed"
            },
            event: serde_json::json!({"schema_version": 1, "operation": operation}),
        },
    )
    .await?;
    Ok(())
}

async fn persist_input_request(
    pool: &PgPool,
    session_id: &str,
    event: &AgentEvent,
) -> Result<(), GatewayError> {
    let Some(snapshot) = session_control::repository::active_turn(pool, session_id).await? else {
        return Ok(());
    };
    if snapshot.turn.status == "running" {
        session_control::repository::transition(pool, &snapshot.turn.id, "waiting_input", None)
            .await?;
    }
    let request_id = event
        .data
        .get("request_id")
        .or_else(|| event.data.get("id"))
        .and_then(Value::as_str)
        .unwrap_or(&snapshot.turn.request_id);
    session_control::repository::append_event(
        pool,
        session_control::repository::NewControlEvent {
            session_id,
            turn_id: Some(&snapshot.turn.id),
            invocation_id: snapshot
                .invocations
                .first()
                .map(|invocation| invocation.id.as_str()),
            request_id: Some(request_id),
            event_key: &format!("turn:{}:input:{request_id}:requested", snapshot.turn.id),
            event_type: "input.requested",
            event: serde_json::json!({
                "schema_version": 1,
                "request_id": request_id,
                "prompt": event.data.get("prompt"),
                "schema": event.data.get("schema"),
                "fields": event.data.get("fields"),
            }),
        },
    )
    .await?;
    Ok(())
}

pub(super) async fn persist_text_result(
    pool: &PgPool,
    session_id: &str,
    text: &str,
) -> Result<(), GatewayError> {
    persist_text_message(pool, session_id, text).await?;
    persist_turn_result(
        pool,
        session_id,
        serde_json::json!({
            "type": "message",
            "content": [{"type": "text", "text": text}],
        }),
    )
    .await
}

pub(super) async fn persist_text_message(
    pool: &PgPool,
    session_id: &str,
    text: &str,
) -> Result<(), GatewayError> {
    persist_turn_message(
        pool,
        session_id,
        serde_json::json!({
            "content": [{"type": "text", "text": text}],
        }),
    )
    .await
}

async fn persist_turn_message(
    pool: &PgPool,
    session_id: &str,
    message: Value,
) -> Result<(), GatewayError> {
    let Some(snapshot) = session_control::repository::active_turn(pool, session_id).await? else {
        return Ok(());
    };
    let mut hasher = DefaultHasher::new();
    message.to_string().hash(&mut hasher);
    let message_key = hasher.finish();
    session_control::repository::append_event(
        pool,
        session_control::repository::NewControlEvent {
            session_id,
            turn_id: Some(&snapshot.turn.id),
            invocation_id: snapshot
                .invocations
                .first()
                .map(|invocation| invocation.id.as_str()),
            request_id: Some(&snapshot.turn.request_id),
            event_key: &format!("turn:{}:message:{message_key:016x}", snapshot.turn.id),
            event_type: "message.completed",
            event: serde_json::json!({
                "schema_version": 1,
                "message": message,
            }),
        },
    )
    .await?;
    Ok(())
}

pub(super) async fn persist_turn_result(
    pool: &PgPool,
    session_id: &str,
    result: Value,
) -> Result<(), GatewayError> {
    let Some(snapshot) = session_control::repository::active_turn(pool, session_id).await? else {
        return Ok(());
    };
    session_control::repository::set_turn_result(pool, &snapshot.turn.id, result.clone()).await?;
    session_control::repository::append_event(
        pool,
        session_control::repository::NewControlEvent {
            session_id,
            turn_id: Some(&snapshot.turn.id),
            invocation_id: snapshot
                .invocations
                .first()
                .map(|invocation| invocation.id.as_str()),
            request_id: Some(&snapshot.turn.request_id),
            event_key: &format!("turn:{}:result:completed", snapshot.turn.id),
            event_type: "result.completed",
            event: serde_json::json!({
                "schema_version": 1,
                "result": result,
            }),
        },
    )
    .await?;
    Ok(())
}

pub(super) fn update_agent_run_status(
    state: &AppState,
    session_id: &str,
    status: &str,
    raw: &Value,
) {
    match status {
        "idle" => state
            .agent_runs
            .update_status(session_id, AgentRunStatus::Completed),
        "error" => state
            .agent_runs
            .set_error(session_id, provider_error_message(raw)),
        _ => {}
    }
}

pub(super) fn provider_run_status(raw: &Value) -> &'static str {
    match raw.get("status").and_then(Value::as_str) {
        Some("completed") => "idle",
        Some("failed" | "cancelled" | "incomplete" | "budget_exceeded") => "error",
        _ => "running",
    }
}

pub(super) fn terminal_event_status(event: &AgentEvent) -> Option<&'static str> {
    match event.event_type.as_str() {
        "assistant_response" | "agent.message" => Some("idle"),
        "session.error" => Some("error"),
        _ => None,
    }
}

fn event_requests_input(event: &AgentEvent) -> bool {
    matches!(
        event.event_type.as_str(),
        "input.required" | "input_request.created" | "agent.input_required"
    )
}

pub(super) fn event_keeps_turn_running(event: &AgentEvent) -> bool {
    let event_type = event.event_type.as_str();
    event_type == "user.message"
        || event_type == "session.status_running"
        || event_type == "session.thread_status_running"
        || event_type == "thinking_back"
        || event_type == "agent.thinking"
        || event_type == "agent.reasoning"
        || event_type == "tool_call"
        || event_type == "tool_result"
        || event_type == "agent.tool_use"
        || event_type == "agent.tool_result"
        || event_type == "content_block_start"
        || event_type == "content_block_delta"
        || event_type == "message_delta"
        || event_type == "message.part.updated"
        || event_type == "message.part.delta"
}

pub(super) fn event_error_message(event: &AgentEvent) -> String {
    event
        .data
        .get("error")
        .and_then(|error| {
            error
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| error.as_str())
        })
        .unwrap_or("managed agent interaction failed")
        .to_owned()
}

fn provider_error_message(raw: &Value) -> String {
    raw.get("error")
        .and_then(|error| {
            error
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| error.as_str())
        })
        .unwrap_or("managed agent interaction failed")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Map};

    use crate::sdk::agents::AgentEvent;

    use super::{assistant_response_content, event_requests_input};

    #[test]
    fn assistant_response_content_reads_content_then_text() {
        let with_content = serde_json::from_value::<Map<String, serde_json::Value>>(json!({
            "content": [{ "type": "text", "text": "done" }]
        }))
        .unwrap();
        assert_eq!(
            assistant_response_content(&AgentEvent::new("assistant_response", with_content)),
            Some(json!([{ "type": "text", "text": "done" }]))
        );

        let with_text =
            serde_json::from_value::<Map<String, serde_json::Value>>(json!({ "text": "hi" }))
                .unwrap();
        assert_eq!(
            assistant_response_content(&AgentEvent::new("assistant_response", with_text)),
            Some(json!([{ "type": "text", "text": "hi" }]))
        );

        // Non-assistant_response events are left to the AgentMessage branch.
        assert_eq!(
            assistant_response_content(&AgentEvent::new("agent.message", Map::new())),
            None
        );
    }

    #[test]
    fn recognizes_provider_neutral_input_request_events() {
        for event_type in [
            "input.required",
            "input_request.created",
            "agent.input_required",
        ] {
            let data = serde_json::from_value::<Map<String, serde_json::Value>>(json!({
                "request_id": "request_1"
            }))
            .unwrap();
            assert!(event_requests_input(&AgentEvent::new(event_type, data)));
        }
        assert!(!event_requests_input(&AgentEvent::new(
            "agent.message",
            Map::new()
        )));
    }
}
