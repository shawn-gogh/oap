use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};

use crate::{
    db::managed_agents::runs::repository,
    errors::GatewayError,
    http::agents::configured_agent_runs_value,
    proxy::{auth::master_key::authenticate, state::AppState},
};

use super::types::{ListRunsQuery, RunsResponse};

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Query(query): Query<ListRunsQuery>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    if let Some(runs) = configured_agent_runs_value(&state, &agent_id) {
        return Ok(Json(runs));
    }

    let Some(pool) = state.db.as_ref() else {
        return Err(GatewayError::MissingDatabase);
    };
    let agent = crate::db::managed_agents::registry::repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    super::super::assert_agent_use(&auth, &agent, pool).await?;
    Ok(Json(serde_json::to_value(RunsResponse {
        runs: repository::list(pool, &agent_id, query.limit.unwrap_or(10)).await?,
    })?))
}
