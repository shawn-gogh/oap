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

    let now = crate::db::managed_agents::now_ms();
    if !registry::repository::soft_delete(pool, &agent_id, now).await? {
        return Err(GatewayError::NotFound("not found".to_owned()));
    }
    crate::db::managed_agents::sources::repository::detach_source(pool, &agent_id).await?;

    Ok(Json(DeleteResponse { ok: true }))
}
