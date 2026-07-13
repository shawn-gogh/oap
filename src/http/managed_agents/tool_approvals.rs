//! Bridges opencode's native permission gate (`Permission.Service.ask`) into
//! LAP's inbox, so a human decision genuinely blocks tool execution instead
//! of merely being suggested to the model.
//!
//! Flow: the opencode wrapper (templates/opencode/src/app.mjs) sees a
//! `permission.asked` event on its /event pump and POSTs here to file a
//! pending inbox item (kind = "tool_permission"). A human accepts/rejects it
//! through the normal `/api/approvals/{id}/accept|reject` endpoints
//! (inbox/approvals.rs), which — for this kind — call `reply` below to POST
//! opencode's own `/permission/{requestID}/reply`, unblocking (or permanently
//! denying) the paused tool call. If nobody ever answers, the tool call stays
//! blocked: fail-closed by construction, not by anything we implement.

use std::sync::Arc;

use axum::{extract::State, http::HeaderMap, Json};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;

use crate::{
    db::managed_agents::{inbox, registry, sessions},
    errors::GatewayError,
    http::runtime_resolution::resolve_runtime,
    proxy::{auth::master_key::authenticate, state::AppState},
};

#[derive(Debug, Deserialize)]
pub struct PermissionAskedRequest {
    /// The *provider-side* (opencode) session id — the wrapper only knows
    /// its own id, not LAP's. Resolved to LAP's session below.
    pub session_id: String,
    pub request_id: String,
    pub permission: String,
    #[serde(default)]
    pub patterns: Vec<String>,
    #[serde(default)]
    pub metadata: Value,
}

/// POST /api/tool-approvals — called by the opencode wrapper when opencode
/// pauses a tool call awaiting permission. Authenticated the same way as
/// other internal agent endpoints (gateway/master key), since the wrapper
/// calls back through `LITELLM_API_KEY`, not a user session.
pub async fn asked(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<PermissionAskedRequest>,
) -> Result<Json<Value>, GatewayError> {
    authenticate(&headers, &state).await?;
    let pool = super::db(&state, &headers).await?;

    let session = sessions::repository::get_by_provider_session_id(pool, &input.session_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("session {}", input.session_id)))?;
    let agent_name = match session.agent_id.as_deref() {
        Some(agent_id) => registry::repository::get(pool, agent_id)
            .await?
            .map(|agent| agent.name),
        None => None,
    };

    let title = format!("工具权限请求：{}", input.permission);
    let body = if input.patterns.is_empty() {
        None
    } else {
        Some(input.patterns.join(", "))
    };
    let args = serde_json::json!({
        "request_id": input.request_id,
        "permission": input.permission,
        "patterns": input.patterns,
        "metadata": input.metadata,
    });

    let item = inbox::repository::create_approval(
        pool,
        "tool_permission",
        title,
        Some(session.id.clone()),
        agent_name,
        body,
        Some(args),
    )
    .await?;

    state.local_session_events.publish(
        &session.id,
        serde_json::json!({
            "type": "approval.asked",
            "approval": {
                "id": item.id,
                "title": item.title,
                "session_id": item.session_id,
                "args_json": item.args_json,
                "created_at": item.created_at,
            }
        }),
    );

    Ok(Json(serde_json::json!({ "id": item.id })))
}

/// Replies to opencode's own permission request so the paused tool call
/// resumes (or is permanently denied). Fire-and-forget: a failure here just
/// leaves the tool call blocked in opencode until it times out on its own —
/// safer than silently letting it through.
pub async fn reply(state: &Arc<AppState>, pool: &PgPool, item: &inbox::schema::InboxItemRow) {
    let Some(session_id) = item.session_id.as_deref() else {
        return;
    };
    let Ok(Some(session)) = sessions::repository::get(pool, session_id).await else {
        tracing::warn!(item_id = %item.id, "tool_permission reply: session not found");
        return;
    };
    let (Some(runtime), Some(provider_session_id)) = (
        session.runtime.as_deref(),
        session.provider_session_id.as_deref(),
    ) else {
        tracing::warn!(item_id = %item.id, "tool_permission reply: session missing runtime/provider_session_id");
        return;
    };
    let resolved = match resolve_runtime(pool, state, runtime).await {
        Ok(resolved) => resolved,
        Err(error) => {
            tracing::warn!(item_id = %item.id, %error, "tool_permission reply: runtime resolve failed");
            return;
        }
    };
    let Some(request_id) = item
        .args_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .and_then(|value| {
            value
                .get("request_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
    else {
        tracing::warn!(item_id = %item.id, "tool_permission reply: missing request_id");
        return;
    };
    // opencode's ReplyBody: "once" (allow this call only) | "always" (allow
    // this pattern for the rest of the session) | "reject". LAP only ever
    // sends "once"/"reject" — never "always" — so a single human decision
    // can't silently grant standing trust for future, unreviewed calls.
    let reply_value = if item.status == "accepted" {
        "once"
    } else {
        "reject"
    };
    let base = resolved.credential.api_base.trim_end_matches('/');
    let url = format!("{base}/v1/sessions/{provider_session_id}/permissions/{request_id}/reply");
    let body = serde_json::json!({ "reply": reply_value, "message": item.feedback });
    if let Err(error) = state.http.post(&url).json(&body).send().await {
        tracing::warn!(item_id = %item.id, %error, "tool_permission reply: request failed");
    }
}
