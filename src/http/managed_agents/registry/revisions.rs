use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde_json::{json, Value};

use crate::{
    db::managed_agents::registry::{repository, revisions},
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
