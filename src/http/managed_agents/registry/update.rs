use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};

use crate::{
    db::managed_agents::registry::{
        repository,
        schema::{ManagedAgentRow, UpdateManagedAgent},
    },
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

pub async fn update(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Json(input): Json<UpdateManagedAgent>,
) -> Result<Json<ManagedAgentRow>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = super::super::db(&state, &headers).await?;
    let existing = repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    super::super::assert_agent_edit(&auth, &existing, pool).await?;
    if let Some(config) = input.config.as_ref() {
        crate::db::managed_agents::quotas::schema::AgentQuotaConfig::from_config(config)?;
    }
    let mut row = repository::update(pool, &agent_id, input)
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    if crate::db::managed_agents::governance::get(pool, &agent_id)
        .await?
        .is_some()
    {
        row = repository::set_status(pool, &agent_id, "draft")
            .await?
            .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
        crate::db::managed_agents::governance::mark_changed(pool, &agent_id).await?;
    }
    // Best-effort: a failed snapshot must not fail the update itself.
    let _ = crate::db::managed_agents::registry::revisions::record(pool, &row, Some(&auth.user_id))
        .await;
    Ok(Json(row))
}
