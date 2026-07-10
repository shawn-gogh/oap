//! Agent sharing: grant management endpoints. Only the owner (or admin) may
//! manage grants; grantees get 'use' (see + run sessions) or 'edit'
//! (additionally modify config/workspace/evals).

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    db::managed_agents::{agent_grants, registry},
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

#[derive(Debug, Deserialize)]
pub struct CreateGrantRequest {
    pub user_id: String,
    #[serde(default)]
    pub permission: Option<String>,
}

async fn owned_agent(
    state: &AppState,
    headers: &HeaderMap,
    agent_id: &str,
) -> Result<(sqlx::PgPool, String), GatewayError> {
    let auth = authenticate(headers, state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let agent = registry::repository::get(pool, agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    super::assert_agent_access(&auth, &agent)?;
    Ok((pool.clone(), auth.user_id))
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, GatewayError> {
    let (pool, _) = owned_agent(&state, &headers, &agent_id).await?;
    let grants = agent_grants::repository::list_for_agent(&pool, &agent_id).await?;
    Ok(Json(json!({ "grants": grants })))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Json(input): Json<CreateGrantRequest>,
) -> Result<(StatusCode, Json<Value>), GatewayError> {
    let (pool, granted_by) = owned_agent(&state, &headers, &agent_id).await?;
    let grantee = input.user_id.trim();
    if grantee.is_empty() {
        return Err(GatewayError::InvalidConfig("user_id is required".to_owned()));
    }
    let grant = agent_grants::repository::upsert(
        &pool,
        &agent_id,
        grantee,
        input.permission.as_deref().unwrap_or("use"),
        &granted_by,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(serde_json::to_value(grant).unwrap_or_default())))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((agent_id, grantee)): Path<(String, String)>,
) -> Result<Json<bool>, GatewayError> {
    let (pool, _) = owned_agent(&state, &headers, &agent_id).await?;
    Ok(Json(
        agent_grants::repository::delete(&pool, &agent_id, &grantee).await?,
    ))
}
