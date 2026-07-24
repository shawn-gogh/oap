use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};

use crate::{db::managed_agents::registry, errors::GatewayError, proxy::state::AppState};

use super::types::DeleteResponse;

pub async fn delete(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<DeleteResponse>, GatewayError> {
    let auth = crate::proxy::auth::master_key::authenticate(&headers, &state).await?;
    let pool = super::super::db(&state, &headers).await?;
    let existing = registry::repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    super::super::assert_agent_access(&auth, &existing)?;

    // Stop the agent's live work before the row is soft-deleted. Rewriting the
    // row alone leaves remote runtimes executing: the deleted agent keeps
    // running tools and spending budget, and an in-flight prompt's completion
    // handler would resurrect the session afterwards. `retire` already does
    // this; delete has to hold the same guarantee.
    let interrupted = crate::http::managed_agents::source_management::interrupt_agent_sessions(
        &state,
        pool,
        &agent_id,
        "智能体已删除",
    )
    .await;
    let cancelled =
        crate::db::managed_agents::sources::repository::cancel_agent_work(pool, &agent_id).await?;

    let now = crate::db::managed_agents::now_ms();
    if !registry::repository::soft_delete(pool, &agent_id, now).await? {
        return Err(GatewayError::NotFound("not found".to_owned()));
    }
    crate::db::managed_agents::sources::repository::detach_source(pool, &agent_id).await?;

    // Sessions outlive the agent on purpose (they are the audit trail), so
    // freeze what they belonged to before the retention sweep removes the
    // agent row for good.
    let snapshot = serde_json::json!({
        "agent_id": existing.id,
        "name": existing.name,
        "model": existing.model,
        "harness": existing.harness,
        "runtime": existing.config.get("runtime").and_then(|value| value.as_str()),
        "deleted_at": now,
    });
    let stamped = crate::db::managed_agents::sessions::repository::stamp_deleted_agent(
        pool, &agent_id, &snapshot,
    )
    .await?;

    tracing::info!(
        agent_id = %agent_id,
        interrupted_sessions = interrupted,
        cancelled_work_items = cancelled,
        stamped_sessions = stamped,
        "soft-deleted agent and stopped its live work"
    );

    Ok(Json(DeleteResponse { ok: true }))
}
