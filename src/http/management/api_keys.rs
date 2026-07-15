use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    db::managed_agents::{api_keys::repository, audit},
    errors::GatewayError,
    proxy::{
        auth::master_key::{authenticate, evict_gateway_key_cache},
        state::AppState,
    },
};

#[derive(Debug, Deserialize)]
pub struct CreateGatewayApiKeyRequest {
    label: Option<String>,
    user_id: Option<String>,
    role: Option<String>,
}

/// Key management mints identities, so it is admin-only — otherwise any key
/// could create keys and the ownership model would be meaningless.
async fn require_admin(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<crate::proxy::auth::master_key::AuthContext, GatewayError> {
    let auth = authenticate(headers, state).await?;
    if auth.is_admin {
        Ok(auth)
    } else {
        Err(GatewayError::Forbidden)
    }
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, GatewayError> {
    require_admin(&headers, &state).await?;
    if let Some(pool) = &state.db {
        let keys = repository::list(pool).await?;
        return Ok(Json(json!({ "keys": keys })));
    }
    Ok(Json(json!({ "keys": state.api_keys.list() })))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<CreateGatewayApiKeyRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    let auth = require_admin(&headers, &state).await?;
    if let Some(pool) = &state.db {
        let created = repository::create(
            pool,
            request.label.as_deref(),
            request.user_id.as_deref(),
            request.role.as_deref(),
        )
        .await?;
        let mut body = serde_json::to_value(&created.row).unwrap_or_default();
        body["key"] = json!(created.key);
        audit::record(
            pool,
            &auth.user_id,
            "api_key.create",
            "api_key",
            &created.row.id,
            json!({
                "user_id": created.row.user_id,
                "role": created.row.role,
            }),
        )
        .await?;
        return Ok((StatusCode::CREATED, Json(body)));
    }
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(state.api_keys.create(request.label)).unwrap_or_default()),
    ))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, GatewayError> {
    let auth = require_admin(&headers, &state).await?;
    if let Some(pool) = &state.db {
        return Ok(match repository::delete(pool, &id).await? {
            Some(key_hash) => {
                evict_gateway_key_cache(&key_hash);
                audit::record(
                    pool,
                    &auth.user_id,
                    "api_key.delete",
                    "api_key",
                    &id,
                    json!({}),
                )
                .await?;
                StatusCode::NO_CONTENT
            }
            None => StatusCode::NOT_FOUND,
        });
    }
    if state.api_keys.delete(&id) {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Ok(StatusCode::NOT_FOUND)
    }
}
