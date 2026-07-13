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

pub async fn pause(
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
    repository::set_status(pool, &agent_id, "paused")
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    Ok(Json(AgentStatusResponse {
        id: agent_id,
        status: "paused".to_owned(),
    }))
}
