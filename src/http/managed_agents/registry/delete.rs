use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};

use crate::{
    db::managed_agents::{memory, registry},
    errors::GatewayError,
    proxy::state::AppState,
};

use super::types::DeleteResponse;

pub async fn delete(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<DeleteResponse>, GatewayError> {
    let auth =
        crate::proxy::auth::master_key::authenticate(&headers, &state).await?;
    let pool = super::super::db(&state, &headers).await?;
    let existing = registry::repository::get(pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    super::super::assert_agent_access(&auth, &existing)?;
    if !registry::repository::delete(pool, &agent_id).await? {
        return Err(GatewayError::NotFound("not found".to_owned()));
    }
    memory::repository::delete_all(pool, &agent_id).await?;
    // Best-effort workspace cleanup; a stuck bucket must not block deletion.
    if let Some(storage) = &state.object_storage {
        let bucket = crate::object_storage::ObjectStorageClient::agent_bucket_name(&agent_id);
        if storage.bucket_exists(&bucket).await {
            let _ = storage.delete_bucket_recursive(&bucket).await;
        }
    }
    Ok(Json(DeleteResponse { ok: true }))
}
