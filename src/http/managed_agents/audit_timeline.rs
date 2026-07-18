use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    db::managed_agents::{audit, registry},
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

#[derive(Debug, Deserialize)]
pub struct TimelineQuery {
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TimelineResponse {
    events: Vec<audit::AuditLogRow>,
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<TimelineResponse>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let agent = registry::repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("agent {agent_id}")))?;
    super::assert_agent_access(&auth, &agent)?;
    let events =
        audit::list_for_target(pool, "agent", &agent_id, query.limit.unwrap_or(100)).await?;
    Ok(Json(TimelineResponse { events }))
}
