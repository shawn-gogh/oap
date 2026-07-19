use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde_json::{json, Value};

use crate::{
    db::managed_agents::registry::{repository, revision_diff, revisions},
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = super::super::db(&state, &headers).await?;
    let agent = repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    super::super::assert_agent_access(&auth, &agent)?;
    let rows = revisions::list(pool, &agent_id, 100).await?;
    Ok(Json(json!({ "revisions": rows })))
}

pub async fn diff(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((agent_id, from_version, to_version)): Path<(String, i32, i32)>,
) -> Result<Json<Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = super::super::db(&state, &headers).await?;
    let agent = repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    super::super::assert_agent_access(&auth, &agent)?;
    let previous = revision_snapshot(pool, &agent_id, from_version).await?;
    let candidate = revision_snapshot(pool, &agent_id, to_version).await?;
    let findings = revision_diff::compare(&previous, &candidate);
    Ok(Json(json!({
        "agent_id": agent_id,
        "from_version": from_version,
        "to_version": to_version,
        "highest_risk": revision_diff::highest_risk(&findings),
        "findings": findings,
    })))
}

async fn revision_snapshot(
    pool: &sqlx::PgPool,
    agent_id: &str,
    version: i32,
) -> Result<Value, GatewayError> {
    if version == 0 {
        return Ok(json!({}));
    }
    revisions::get_version(pool, agent_id, version)
        .await?
        .map(|revision| revision.snapshot)
        .ok_or_else(|| GatewayError::NotFound(format!("revision {version}")))
}
