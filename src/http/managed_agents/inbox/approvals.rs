use std::sync::Arc;

use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use sqlx::PgPool;

use crate::{
    db::managed_agents::{
        inbox::{repository, schema::InboxItemRow},
        runtime_events,
    },
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

use super::types::{
    AcceptRequest, ApprovalScope, ApprovalView, ApprovalsResponse, DecisionResponse, RejectRequest,
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
    let candidates = repository::pending_approvals(pool, session_id, None).await?;
    let mut approvals = Vec::new();
    for item in candidates {
        let allowed = can_decide(pool, &auth, &item).await?;
        let owned = repository::approval_scope_owned_by(pool, &item, &auth.user_id).await?;
        if allowed || owned {
            approvals.push(ApprovalView {
                item,
                can_decide: allowed,
            });
        }
    }

    Ok(Json(ApprovalsResponse { approvals }))
}

async fn owned_approval(
    pool: &sqlx::PgPool,
    auth: &crate::proxy::auth::master_key::AuthContext,
    item_id: &str,
) -> Result<InboxItemRow, GatewayError> {
    let item = repository::get(pool, item_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("approval not found".to_owned()))?;
    if can_decide(pool, auth, &item).await? {
        return Ok(item);
    }
    Err(GatewayError::NotFound("approval not found".to_owned()))
}

pub async fn can_decide(
    pool: &sqlx::PgPool,
    auth: &crate::proxy::auth::master_key::AuthContext,
    item: &InboxItemRow,
) -> Result<bool, GatewayError> {
    if auth.is_admin {
        return Ok(true);
    }
    if role_allows(pool, auth, item, &item.required_role).await? {
        return Ok(true);
    }
    if item.escalated_at.is_some() {
        if let Some(role) = item.escalation_role.as_deref() {
            return role_allows(pool, auth, item, role).await;
        }
    }
    Ok(false)
}

async fn role_allows(
    pool: &sqlx::PgPool,
    auth: &crate::proxy::auth::master_key::AuthContext,
    item: &InboxItemRow,
    role: &str,
) -> Result<bool, GatewayError> {
    match role {
        "admin" | "security" => Ok(false),
        "group_admin" => {
            crate::db::managed_agents::groups::members::is_any_group_admin(pool, &auth.user_id)
                .await
        }
        _ => repository::approval_scope_owned_by(pool, item, &auth.user_id).await,
    }
}

pub async fn accept(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    Json(input): Json<AcceptRequest>,
) -> Result<Json<DecisionResponse>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = super::super::db(&state, &headers).await?.clone();
    let existing = owned_approval(&pool, &auth, &item_id).await?;
    if existing.status != "pending" {
        return Ok(Json(DecisionResponse {
            ok: true,
            live: false,
            delivery_status: existing.delivery_status,
        }));
    }
    let live = repository::decide_approval(
        &pool,
        &item_id,
        "accept",
        None,
        input.arguments,
        &auth.user_id,
        match input.scope {
            ApprovalScope::Once => "once",
            ApprovalScope::Session => "session",
        },
    )
    .await?;
    let delivery_status = deliver_and_record(&state, &pool, &item_id, input.scope).await?;
    crate::db::managed_agents::audit::record(
        &pool,
        &auth.user_id,
        "approval.accepted",
        &existing.kind,
        &item_id,
        serde_json::json!({ "delivery_status": delivery_status }),
    )
    .await?;
    Ok(Json(DecisionResponse {
        ok: true,
        live,
        delivery_status,
    }))
}

pub async fn reject(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    Json(input): Json<RejectRequest>,
) -> Result<Json<DecisionResponse>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = super::super::db(&state, &headers).await?.clone();
    let existing = owned_approval(&pool, &auth, &item_id).await?;
    if existing.status != "pending" {
        return Ok(Json(DecisionResponse {
            ok: true,
            live: false,
            delivery_status: existing.delivery_status,
        }));
    }
    let live = repository::decide_approval(
        &pool,
        &item_id,
        "reject",
        input.feedback,
        None,
        &auth.user_id,
        "once",
    )
    .await?;
    let delivery_status = deliver_and_record(&state, &pool, &item_id, ApprovalScope::Once).await?;
    crate::db::managed_agents::audit::record(
        &pool,
        &auth.user_id,
        "approval.rejected",
        &existing.kind,
        &item_id,
        serde_json::json!({ "delivery_status": delivery_status }),
    )
    .await?;
    Ok(Json(DecisionResponse {
        ok: true,
        live,
        delivery_status,
    }))
}

pub async fn retry(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(item_id): Path<String>,
) -> Result<Json<DecisionResponse>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = super::super::db(&state, &headers).await?.clone();
    let item = owned_approval(&pool, &auth, &item_id).await?;
    if item.delivery_status != "delivery_failed" {
        return Err(GatewayError::BadRequest(
            "only failed approval delivery can be retried".to_owned(),
        ));
    }
    let scope = if item.decision_scope == "session" {
        ApprovalScope::Session
    } else {
        ApprovalScope::Once
    };
    let delivery_status = deliver_and_record(&state, &pool, &item_id, scope).await?;
    crate::db::managed_agents::audit::record(
        &pool,
        &auth.user_id,
        "approval.delivery_retried",
        &item.kind,
        &item_id,
        serde_json::json!({ "delivery_status": delivery_status }),
    )
    .await?;
    Ok(Json(DecisionResponse {
        ok: true,
        live: false,
        delivery_status,
    }))
}

pub(crate) async fn deliver_and_record(
    state: &Arc<AppState>,
    pool: &sqlx::PgPool,
    item_id: &str,
    scope: ApprovalScope,
) -> Result<String, GatewayError> {
    let item = repository::get(pool, item_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("approval not found".to_owned()))?;
    let result = deliver(state, pool, &item, scope).await;
    match result {
        Ok(()) => {
            repository::mark_delivery_applied(pool, item_id).await?;
            publish_approval_reply(state, pool, &item).await?;
            Ok("applied".to_owned())
        }
        Err(error) => {
            repository::mark_delivery_failed(pool, item_id, &error.to_string()).await?;
            Ok("delivery_failed".to_owned())
        }
    }
}

async fn deliver(
    state: &Arc<AppState>,
    pool: &PgPool,
    item: &InboxItemRow,
    scope: ApprovalScope,
) -> Result<(), GatewayError> {
    match item.effect_handler.as_str() {
        "runtime_permission" => {
            crate::http::managed_agents::tool_approvals::reply(state, pool, item, scope).await
        }
        "agent_publish" => {
            if item.status == "accepted" {
                let actor = item.decided_by.as_deref().unwrap_or("approval-system");
                crate::http::managed_agents::governance::apply_publish_approval(pool, item, actor)
                    .await
            } else if let Some(agent_id) = item.agent.as_deref() {
                crate::db::managed_agents::governance::reject_publish(pool, agent_id).await?;
                Ok(())
            } else {
                Err(GatewayError::BadRequest(
                    "publish approval missing agent".to_owned(),
                ))
            }
        }
        "agent_change" => {
            if item.status == "accepted" {
                crate::http::managed_agents::improvements::apply_if_improvement(
                    state.clone(),
                    pool.clone(),
                    &item.id,
                )
                .await?;
            }
            Ok(())
        }
        "resume_session" => resume_linked_session(state.clone(), pool.clone(), item).await,
        "platform_action" if item.status != "accepted" => Ok(()),
        handler => Err(GatewayError::InvalidConfig(format!(
            "approval effect handler is not implemented: {handler}"
        ))),
    }
}

async fn publish_approval_reply(
    state: &Arc<AppState>,
    pool: &PgPool,
    item: &InboxItemRow,
) -> Result<(), GatewayError> {
    let Some(session_id) = item.session_id.as_deref() else {
        return Ok(());
    };
    let reply = serde_json::json!({
        "id": format!("approval_reply_{}", item.id),
        "type": "approval.replied",
        "approval": { "id": item.id, "status": item.status }
    });
    runtime_events::repository::append(pool, session_id, reply.clone()).await?;
    state.local_session_events.publish(session_id, reply);

    if item.status == "rejected" {
        let message = item
            .feedback
            .as_deref()
            .map(str::trim)
            .filter(|feedback| !feedback.is_empty())
            .map(|feedback| format!("用户拒绝了该操作：{feedback}"))
            .unwrap_or_else(|| "用户拒绝了该操作".to_owned());
        let result = serde_json::json!({
            "id": format!("approval_rejected_{}", item.id),
            "type": "agent.tool_result",
            "tool_use_id": format!("approval_{}", item.id),
            "name": item.title,
            "status": "rejected",
            "input": item.args_json.as_deref().and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok()),
            "error": { "message": message },
            "error_message": message,
        });
        runtime_events::repository::append(pool, session_id, result.clone()).await?;
        state.local_session_events.publish(session_id, result);
    }
    Ok(())
}

async fn resume_linked_session(
    state: Arc<AppState>,
    pool: PgPool,
    item: &InboxItemRow,
) -> Result<(), GatewayError> {
    let Some(session_id) = item.session_id.as_deref() else {
        return Ok(());
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
    crate::http::sessions::enqueue_prompt_text(state, pool, session_id, resume_prompt(item), model)
        .await?;
    Ok(())
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
        "expired" => format!(
            "Human approval {} expired without a decision. Do not perform the requested action; continue only with safe alternatives.",
            item.id
        ),
        _ => format!(
            "Human approval {} is still {}. Do not proceed until it is accepted or rejected.",
            item.id, item.status
        ),
    }
}
