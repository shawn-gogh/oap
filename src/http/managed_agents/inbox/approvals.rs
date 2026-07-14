use std::sync::Arc;

use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use sqlx::PgPool;

use crate::{
    db::managed_agents::inbox::{repository, schema::InboxItemRow},
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

use super::types::{
    AcceptRequest, ApprovalScope, ApprovalsResponse, DecisionResponse, RejectRequest,
};

pub async fn list_pending(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<Json<ApprovalsResponse>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = super::super::db(&state, &headers).await?;
    let session_id = query
        .get("session_id")
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty());
    let owner = (!auth.is_admin).then_some(auth.user_id.as_str());

    let mut approvals = repository::pending_approvals(pool, session_id, owner).await?;
    let transient = state.transient_approvals.list_pending(session_id);
    let mut transient_filtered = Vec::new();
    for item in transient {
        if auth.is_admin || repository::approval_scope_owned_by(pool, &item, &auth.user_id).await? {
            transient_filtered.push(item);
        }
    }
    approvals.extend(transient_filtered);

    Ok(Json(ApprovalsResponse { approvals }))
}

/// Approvals authorize agent side effects, so deciding one requires owning
/// the linked session or agent (admins pass). Unknown items and items the
/// caller can't see both surface as not-found.
async fn owned_approval(
    state: &crate::proxy::state::AppState,
    pool: &sqlx::PgPool,
    auth: &crate::proxy::auth::master_key::AuthContext,
    item_id: &str,
) -> Result<(InboxItemRow, bool), GatewayError> {
    // 1. Check transient approvals first
    if let Some(row) = state.transient_approvals.get_row(item_id) {
        if auth.is_admin || repository::approval_scope_owned_by(pool, &row, &auth.user_id).await? {
            return Ok((row, true));
        }
        return Err(GatewayError::NotFound("approval not found".to_owned()));
    }
    // 2. Check DB approvals
    let item = repository::get(pool, item_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("approval not found".to_owned()))?;
    if auth.is_admin || repository::approval_scope_owned_by(pool, &item, &auth.user_id).await? {
        return Ok((item, false));
    }
    Err(GatewayError::NotFound("approval not found".to_owned()))
}

pub async fn accept(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    Json(input): Json<AcceptRequest>,
) -> Result<Json<DecisionResponse>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = super::super::db(&state, &headers).await?.clone();
    let (existing, is_transient) = owned_approval(&state, &pool, &auth, &item_id).await?;

    if is_transient {
        if let Some(removed) = state.transient_approvals.remove(&item_id) {
            let mut row = removed.row;
            row.status = "accepted".to_owned();
            row.args_json = input.arguments.map(|v| v.to_string());

            crate::http::managed_agents::tool_approvals::reply_direct(
                &state,
                &pool,
                &row,
                input.scope,
                &removed.provider_session_id,
                &removed.runtime,
            )
            .await;

            publish_approval_reply(&state, &row);
        }
        return Ok(Json(DecisionResponse { ok: true, live: true }));
    }

    let publish_approval = crate::http::managed_agents::governance::is_publish_approval(&existing);
    if publish_approval && !auth.is_admin {
        return Err(GatewayError::NotFound("approval not found".to_owned()));
    }
    let live =
        repository::decide_approval(&pool, &item_id, "accept", None, input.arguments).await?;
    if publish_approval {
        crate::http::managed_agents::governance::apply_publish_approval(
            &pool,
            &existing,
            &auth.user_id,
        )
        .await?;
    } else if existing.kind == "tool_permission" {
        reply_tool_permission(&state, &pool, &item_id, input.scope).await;
        publish_approval_reply(&state, &existing);
    } else {
        if live {
            crate::http::managed_agents::improvements::apply_if_improvement(
                state.clone(),
                pool.clone(),
                &item_id,
            )
            .await;
        }
        resume_linked_session(state, pool, &item_id).await;
    }
    Ok(Json(DecisionResponse { ok: true, live }))
}

pub async fn reject(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    Json(input): Json<RejectRequest>,
) -> Result<Json<DecisionResponse>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = super::super::db(&state, &headers).await?.clone();
    let (existing, is_transient) = owned_approval(&state, &pool, &auth, &item_id).await?;

    if is_transient {
        if let Some(removed) = state.transient_approvals.remove(&item_id) {
            let mut row = removed.row;
            row.status = "rejected".to_owned();
            row.feedback = input.feedback;

            crate::http::managed_agents::tool_approvals::reply_direct(
                &state,
                &pool,
                &row,
                ApprovalScope::Once,
                &removed.provider_session_id,
                &removed.runtime,
            )
            .await;

            publish_approval_reply(&state, &row);
        }
        return Ok(Json(DecisionResponse { ok: true, live: true }));
    }

    let publish_approval = crate::http::managed_agents::governance::is_publish_approval(&existing);
    if publish_approval && !auth.is_admin {
        return Err(GatewayError::NotFound("approval not found".to_owned()));
    }
    let live = repository::decide_approval(&pool, &item_id, "reject", input.feedback, None).await?;
    if publish_approval {
        if let Some(agent_id) = existing.agent.as_deref() {
            crate::db::managed_agents::governance::reject_publish(&pool, agent_id).await?;
        }
    } else if existing.kind == "tool_permission" {
        reply_tool_permission(&state, &pool, &item_id, ApprovalScope::Once).await;
        publish_approval_reply(&state, &existing);
    } else {
        resume_linked_session(state, pool, &item_id).await;
    }
    Ok(Json(DecisionResponse { ok: true, live }))
}

/// tool_permission items resolve by replying to opencode's own permission
/// request, not by resuming the chat with a new prompt — the tool call is
/// paused at the protocol level, not waiting on the next user message.
async fn reply_tool_permission(
    state: &Arc<AppState>,
    pool: &sqlx::PgPool,
    item_id: &str,
    scope: ApprovalScope,
) {
    if let Ok(Some(item)) = repository::get(pool, item_id).await {
        crate::http::managed_agents::tool_approvals::reply(state, pool, &item, scope).await;
    }
}

fn publish_approval_reply(state: &Arc<AppState>, item: &InboxItemRow) {
    let Some(session_id) = item.session_id.as_deref() else {
        return;
    };
    state.local_session_events.publish(
        session_id,
        serde_json::json!({
            "type": "approval.replied",
            "approval": { "id": item.id }
        }),
    );
}

async fn resume_linked_session(state: Arc<AppState>, pool: PgPool, item_id: &str) {
    let Ok(Some(item)) = repository::get(&pool, item_id).await else {
        return;
    };
    let Some(session_id) = item.session_id.as_deref() else {
        return;
    };
    // Push the decision to any live SSE subscriber so every open tab clears
    // the approval immediately instead of waiting for the next poll.
    state.local_session_events.publish(
        session_id,
        serde_json::json!({
            "type": "approval.replied",
            "approval": { "id": item.id, "status": item.status }
        }),
    );
    let model = resume_model(&pool, session_id).await;
    if let Err(error) = crate::http::sessions::enqueue_prompt_text(
        state,
        pool.clone(),
        session_id,
        resume_prompt(&item),
        model,
    )
    .await
    {
        tracing::warn!(
            approval_id = %item.id,
            session_id,
            error = %error,
            "failed to resume session after approval decision"
        );
    }
}

/// Resumes with the model the session's agent is configured for — a
/// hardcoded model breaks deployments that don't route it. Falls back to the
/// gateway default only when the agent lookup fails.
async fn resume_model(pool: &PgPool, session_id: &str) -> String {
    crate::http::sessions::agent_model_for_session(pool, session_id)
        .await
        .unwrap_or_else(|| "claude-sonnet-4-6".to_owned())
}

fn resume_prompt(item: &InboxItemRow) -> String {
    match item.status.as_str() {
        "accepted" => format!(
            "Human approved approval {}.\nTitle: {}\nApproved arguments:\n{}\nContinue the session using this decision.",
            item.id,
            item.title,
            item.args_json.as_deref().unwrap_or("{}")
        ),
        "rejected" => format!(
            "Human rejected approval {}.\nTitle: {}\nFeedback: {}\nDo not proceed with the requested action; adjust based on this feedback.",
            item.id,
            item.title,
            item.feedback.as_deref().unwrap_or("No feedback provided.")
        ),
        _ => format!(
            "Human approval {} is still {}. Do not proceed until it is accepted or rejected.",
            item.id, item.status
        ),
    }
}
