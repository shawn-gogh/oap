use serde_json::Value;
use sqlx::PgPool;

use crate::{
    agents::runs::AgentRunStatus,
    db::managed_agents::{id, runtime_events, sessions},
    errors::GatewayError,
    proxy::state::AppState,
    sdk::agents::AgentEvent,
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
