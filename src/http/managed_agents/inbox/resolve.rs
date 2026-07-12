use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};

use crate::{
    db::managed_agents::inbox::repository,
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

use super::types::{OkResponse, ResolveRequest};

pub async fn resolve(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(item_id): Path<String>,
    Json(input): Json<ResolveRequest>,
) -> Result<Json<OkResponse>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = super::super::db(&state, &headers).await?;
    let item = repository::get(pool, &item_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("item not found".to_owned()))?;
    if !auth.is_admin && !repository::approval_scope_owned_by(pool, &item, &auth.user_id).await? {
        return Err(GatewayError::NotFound("item not found".to_owned()));
    }
    if !repository::resolve_issue(pool, &item_id, input.note).await? {
        return Err(GatewayError::NotFound(
            "item not found or already resolved".to_owned(),
        ));
    }
    Ok(Json(OkResponse { ok: true }))
}
