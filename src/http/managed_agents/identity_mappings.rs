use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    db::managed_agents::{audit, identity_mappings, registry, users},
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

#[derive(Debug, Deserialize)]
pub struct ListIdentityMappingsQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct BindIdentityMappingRequest {
    pub user_id: String,
    pub agent_id: Option<String>,
}

async fn admin_pool(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(sqlx::PgPool, crate::proxy::auth::master_key::AuthContext), GatewayError> {
    let auth = authenticate(headers, state).await?;
    if !auth.is_admin {
        return Err(GatewayError::Forbidden);
    }
    Ok((state.db.clone().ok_or(GatewayError::MissingDatabase)?, auth))
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ListIdentityMappingsQuery>,
) -> Result<Json<Value>, GatewayError> {
    let (pool, _) = admin_pool(&state, &headers).await?;
    let status = query.status.as_deref().map(str::trim);
    if status.is_some_and(|status| !matches!(status, "pending" | "active" | "blocked")) {
        return Err(GatewayError::BadRequest(
            "身份映射状态必须是 pending、active 或 blocked。".to_owned(),
        ));
    }
    let mappings =
        identity_mappings::repository::list(&pool, status, query.limit.unwrap_or(100)).await?;
    Ok(Json(json!({ "mappings": mappings })))
}

pub async fn bind(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(mapping_id): Path<String>,
    Json(input): Json<BindIdentityMappingRequest>,
) -> Result<Json<Value>, GatewayError> {
    let (pool, auth) = admin_pool(&state, &headers).await?;
    let user_id = input.user_id.trim();
    if user_id.is_empty() {
        return Err(GatewayError::BadRequest("用户 ID 不能为空。".to_owned()));
    }
    if !users::repository::find(&pool, user_id)
        .await?
        .is_some_and(|user| user.is_active())
    {
        return Err(GatewayError::BadRequest(
            "身份映射必须绑定到启用中的用户。".to_owned(),
        ));
    }
    let agent_id = input
        .agent_id
        .as_deref()
        .map(str::trim)
        .filter(|agent_id| !agent_id.is_empty());
    if let Some(agent_id) = agent_id {
        if registry::repository::get(&pool, agent_id).await?.is_none() {
            return Err(GatewayError::BadRequest(
                "身份映射指定的智能体不存在。".to_owned(),
            ));
        }
    }
    let mapping =
        identity_mappings::repository::bind(&pool, &mapping_id, user_id, agent_id, &auth.user_id)
            .await?
            .ok_or_else(|| GatewayError::NotFound(format!("identity mapping {mapping_id}")))?;
    audit::record(
        &pool,
        &auth.user_id,
        "identity_mapping.bind",
        "external_identity_mapping",
        &mapping.id,
        json!({
            "platform_user_id": mapping.platform_user_id,
            "platform_agent_id": mapping.platform_agent_id,
            "issuer": mapping.issuer,
            "audience": mapping.audience,
        }),
    )
    .await?;
    Ok(Json(serde_json::to_value(mapping).unwrap_or_default()))
}

pub async fn block(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(mapping_id): Path<String>,
) -> Result<Json<Value>, GatewayError> {
    let (pool, auth) = admin_pool(&state, &headers).await?;
    let mapping = identity_mappings::repository::block(&pool, &mapping_id, &auth.user_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("identity mapping {mapping_id}")))?;
    audit::record(
        &pool,
        &auth.user_id,
        "identity_mapping.block",
        "external_identity_mapping",
        &mapping.id,
        json!({
            "issuer": mapping.issuer,
            "audience": mapping.audience,
        }),
    )
    .await?;
    Ok(Json(serde_json::to_value(mapping).unwrap_or_default()))
}
