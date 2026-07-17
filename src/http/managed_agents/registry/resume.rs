use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};

use crate::{
    db::managed_agents::registry::repository,
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

use super::types::AgentStatusResponse;

pub async fn resume(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<AgentStatusResponse>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = super::super::db(&state, &headers).await?;
    let existing = repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    super::super::assert_agent_edit(&auth, &existing, pool).await?;
    if existing.status == "draft" {
        return Err(GatewayError::BadRequest(format!(
            "草稿智能体不能直接恢复运行：请先通过预检并激活（POST /api/agents/{agent_id}/activate）"
        )));
    }
    // Resume is only for plain user pauses. A governance-suspended or retired
    // agent (emergency stop, failed health checks) must go back through the
    // governance pipeline — activate runs preflight and the lifecycle gates;
    // resume here would mint an "active" agent whose sessions stay blocked.
    if let Some(governance) = crate::db::managed_agents::governance::get(pool, &agent_id).await? {
        match governance.lifecycle_status.as_str() {
            "suspended" => {
                return Err(GatewayError::BadRequest(format!(
                    "智能体处于治理挂起状态（紧急停止或健康检查失败），不能直接恢复：请先在治理面板运行检查确认健康，再通过激活恢复（POST /api/agents/{agent_id}/activate）"
                )));
            }
            "retired" => {
                return Err(GatewayError::BadRequest(
                    "智能体已退役，不能恢复运行。".to_owned(),
                ));
            }
            _ => {}
        }
    }
    repository::set_status(pool, &agent_id, "active")
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    Ok(Json(AgentStatusResponse {
        id: agent_id,
        status: "active".to_owned(),
    }))
}
