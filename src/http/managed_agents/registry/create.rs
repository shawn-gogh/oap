use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};

use crate::{
    db::managed_agents::registry::{
        repository,
        schema::{CreateManagedAgent, ManagedAgentRow},
    },
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

pub async fn create(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(mut input): Json<CreateManagedAgent>,
) -> Result<(StatusCode, Json<ManagedAgentRow>), GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = super::super::db(&state, &headers).await?;
    // Ownership comes from the key, not the request body; only admins may
    // create agents on someone else's behalf.
    if !auth.is_admin || input.owner_id.trim().is_empty() {
        input.owner_id = auth.user_id.clone();
    }
    if let Some(config) = input.config.as_ref() {
        crate::db::managed_agents::quotas::schema::AgentQuotaConfig::from_config(config)?;
    }
    let row = repository::create(pool, input).await?;
    // Best-effort: a failed snapshot must not fail the create itself.
    let _ = crate::db::managed_agents::registry::revisions::record(pool, &row, Some(&auth.user_id))
        .await;
    Ok((StatusCode::CREATED, Json(row)))
}
