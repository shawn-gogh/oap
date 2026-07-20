//! Generic external chat runtime ("generic_chat" api_spec).
//!
//! Onboards a third-party agent that only exposes an OpenAI-compatible
//! `/chat/completions` endpoint — no managed-agents contract, nothing to
//! deploy on their side. The gateway itself plays the runtime: sessions and
//! events live entirely in Postgres, and each prompt is one HTTP round trip
//! to the external endpoint. No tools, no streaming, no workspace — by
//! design the zero-friction tier of external-agent onboarding.

use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;

use crate::{
    db::managed_agents::{harnesses, messages, registry, runtime_events, sessions},
    errors::GatewayError,
    proxy::state::AppState,
};

use super::{runtime_lifecycle, storage::persist_message};

pub(crate) const GENERIC_CHAT_SPEC: &str = "generic_chat";

/// True when the session's runtime alias is a DB harness registered with the
/// generic_chat api_spec.
pub(super) async fn is_generic_chat(pool: &PgPool, alias: &str) -> Result<bool, GatewayError> {
    Ok(harnesses::repository::get_by_alias(pool, alias)
        .await?
        .is_some_and(|harness| harness.api_spec == GENERIC_CHAT_SPEC))
}

/// Runs one prompt round trip against the external chat endpoint and records
/// everything locally (messages + provider-format runtime events), so the
/// existing chat UI replay/poll path works unchanged.
/// Persists a runtime event and pushes it to any live SSE subscriber, so
/// generic_chat sessions stream in real time instead of relying on polling.
async fn append_event(state: &AppState, pool: &PgPool, session_id: &str, event: Value) {
    let _ = runtime_events::repository::append(pool, session_id, event.clone()).await;
    state.local_session_events.publish(session_id, event);
}

pub(super) async fn execute_prompt(
    state: Arc<AppState>,
    pool: &PgPool,
    row: &crate::db::managed_agents::sessions::schema::SessionRow,
    prompt: &str,
) -> Result<(), GatewayError> {
    let alias = row.runtime.as_deref().unwrap_or_default().to_owned();
    sessions::repository::set_status(pool, &row.id, "running").await?;
    append_event(
        &state,
        pool,
        &row.id,
        json!({
            "type": "user.message",
            "content": [{ "type": "text", "text": prompt }],
        }),
    )
    .await;

    let result = chat_round_trip(&state, pool, row, &alias, prompt).await;
    match result {
        Ok(reply) => {
            persist_message(pool, &row.id, "assistant", &reply, Some("stop")).await?;
            runtime_lifecycle::persist_text_result(pool, &row.id, &reply).await?;
            append_event(
                &state,
                pool,
                &row.id,
                json!({
                    "type": "agent.message",
                    "content": [{ "type": "text", "text": reply }],
                }),
            )
            .await;
            append_event(
                &state,
                pool,
                &row.id,
                json!({
                    "type": "session.status_idle",
                    "stop_reason": { "type": "end_turn" },
                }),
            )
            .await;
            sessions::repository::set_status(pool, &row.id, "idle").await?;
            crate::db::managed_agents::tasks::artifacts::capture_session_output(pool, &row.id)
                .await?;
            crate::db::managed_agents::tasks::repository::mark_verifying_for_session(pool, &row.id)
                .await?;
            Ok(())
        }
        Err(error) => {
            let message = error.to_string();
            append_event(
                &state,
                pool,
                &row.id,
                json!({
                    "type": "session.error",
                    "error": { "message": message },
                }),
            )
            .await;
            let _ = sessions::repository::set_status(pool, &row.id, "error").await;
            let _ = crate::db::managed_agents::tasks::repository::fail_for_session(
                pool, &row.id, &message,
            )
            .await;
            Err(error)
        }
    }
}

async fn chat_round_trip(
    state: &AppState,
    pool: &PgPool,
    row: &crate::db::managed_agents::sessions::schema::SessionRow,
    alias: &str,
    prompt: &str,
) -> Result<String, GatewayError> {
    let credential =
        crate::http::runtime_resolution::harness_credential(pool, state, alias).await?;

    let (system, model) = match row.agent_id.as_deref() {
        Some(agent_id) => registry::repository::get(pool, agent_id)
            .await?
            .map(|agent| (agent.system, agent.model))
            .unwrap_or_default(),
        None => (
            row.environment_json
                .get("temporary_system")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            row.environment_json
                .get("temporary_model")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        ),
    };

    let mut chat_messages = Vec::new();
    if !system.trim().is_empty() {
        chat_messages.push(json!({ "role": "system", "content": system }));
    }
    chat_messages.extend(history_messages(pool, &row.id).await);
    // History already contains the just-persisted user prompt; append it
    // explicitly only if parsing produced nothing (defensive).
    if !chat_messages
        .iter()
        .any(|m| m.get("role").and_then(Value::as_str) == Some("user"))
    {
        chat_messages.push(json!({ "role": "user", "content": prompt }));
    }

    let url = format!(
        "{}/chat/completions",
        credential.api_base.trim_end_matches('/')
    );
    let mut request = state.http.post(&url).json(&json!({
        "model": model,
        "messages": chat_messages,
    }));
    if !credential.api_key.trim().is_empty() {
        request = request.bearer_auth(credential.api_key.trim());
    }
    let response = request.send().await.map_err(GatewayError::Upstream)?;
    let status = response.status();
    let body: Value = response.json().await.map_err(GatewayError::Upstream)?;
    if !status.is_success() {
        return Err(GatewayError::SandboxError(format!(
            "external chat endpoint returned {status}: {}",
            body.to_string().chars().take(300).collect::<String>()
        )));
    }
    let reply = body
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if reply.is_empty() {
        return Err(GatewayError::SandboxError(
            "external chat endpoint returned an empty reply".to_owned(),
        ));
    }
    Ok(reply)
}

/// Best-effort reconstruction of the conversation as chat messages from the
/// stored message rows (info JSON carries the role, parts JSON the text).
async fn history_messages(pool: &PgPool, session_id: &str) -> Vec<Value> {
    let Ok(rows) = messages::repository::list(pool, session_id).await else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let info: Value = serde_json::from_str(&row.info_json).ok()?;
            let parts: Value = serde_json::from_str(&row.parts_json).ok()?;
            let role = info.get("role").and_then(Value::as_str)?;
            if role != "user" && role != "assistant" {
                return None;
            }
            let text = parts
                .as_array()?
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("");
            if text.trim().is_empty() {
                return None;
            }
            Some(json!({ "role": role, "content": text }))
        })
        .collect()
}
