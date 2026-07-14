use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    db::managed_agents::audit,
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

#[derive(Debug, Deserialize)]
pub struct ListAuditQuery {
    pub limit: Option<i64>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ListAuditQuery>,
) -> Result<Json<Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    if !auth.is_admin {
        return Err(GatewayError::Forbidden);
    }
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let logs = audit::list(pool, query.limit.unwrap_or(100)).await?;
    Ok(Json(json!({ "logs": logs })))
}
