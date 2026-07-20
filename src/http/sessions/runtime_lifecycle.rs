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
    if let AgentEventPayload::AgentMessage(message) = event.payload() {
        persist_turn_result(
            pool,
            session_id,
            serde_json::json!({
                "type": "message",
                "content": message.content,
            }),
        )
        .await?;
    }
    Ok(())
}

pub(super) async fn persist_text_result(
    pool: &PgPool,
    session_id: &str,
    text: &str,
) -> Result<(), GatewayError> {
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
