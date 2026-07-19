use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    db::{
        managed_agents::{
            agent_grants, api_keys, audit, groups, registry, users::repository, web_sessions,
        },
        vault_keys,
    },
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

#[derive(Debug, Deserialize)]
pub struct ListUsersQuery {
    pub query: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub id: String,
    pub display_name: String,
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub status: Option<String>,
    pub display_name: Option<String>,
    pub email: Option<Option<String>>,
}

#[derive(Debug, Deserialize)]
pub struct DeactivateUserRequest {
    pub transfer_to: Option<String>,
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

pub async fn me(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let can_manage_groups = match &state.db {
        Some(pool) if !auth.is_admin => {
            groups::members::is_any_group_admin(pool, &auth.user_id).await?
        }
        _ => auth.is_admin,
    };
    let user = match &state.db {
        Some(pool) if !auth.is_admin => Some(repository::ensure(pool, &auth.user_id).await?),
        _ => None,
    };
    let display_name = user
        .as_ref()
        .map(|row| row.display_name.clone())
        .unwrap_or_else(|| "Administrator".to_owned());
    Ok(Json(json!({
        "id": auth.user_id,
        "is_admin": auth.is_admin,
        "role": auth.role,
        "can_manage_groups": can_manage_groups,
        "display_name": display_name,
        "email": user.and_then(|row| row.email),
    })))
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.clone().ok_or(GatewayError::MissingDatabase)?;
    if !auth.is_admin
        && !crate::db::managed_agents::groups::members::is_any_group_admin(&pool, &auth.user_id)
            .await?
    {
        return Err(GatewayError::Forbidden);
    }
    let users = repository::list(&pool, query.query.as_deref()).await?;
    Ok(Json(json!({ "users": users })))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<Value>), GatewayError> {
    let (pool, auth) = admin_pool(&state, &headers).await?;
    if input.id.trim().is_empty() || input.display_name.trim().is_empty() {
        return Err(GatewayError::BadRequest(
            "用户 ID 和显示名称不能为空。".to_owned(),
        ));
    }
    if repository::find(&pool, input.id.trim()).await?.is_some() {
        return Err(GatewayError::BadRequest("用户 ID 已存在。".to_owned()));
    }
    if let Some(email) = input
        .email
        .as_deref()
        .map(str::trim)
        .filter(|email| !email.is_empty())
    {
        if repository::find_by_email(&pool, email).await?.is_some() {
            return Err(GatewayError::BadRequest(
                "邮箱已被其他用户使用。".to_owned(),
            ));
        }
    }
    let user = repository::create(
        &pool,
        &input.id,
        &input.display_name,
        input.email.as_deref(),
    )
    .await?;
    audit::record(
        &pool,
        &auth.user_id,
        "user.create",
        "user",
        &user.id,
        json!({
            "display_name": user.display_name,
            "email": user.email,
        }),
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(user).unwrap_or_default()),
    ))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<UpdateUserRequest>,
) -> Result<Json<Value>, GatewayError> {
    let (pool, auth) = admin_pool(&state, &headers).await?;
    if input.status.is_none() && input.display_name.is_none() && input.email.is_none() {
        return Err(GatewayError::BadRequest(
            "请至少提交一项用户资料或状态变更。".to_owned(),
        ));
    }
    if input.status.as_deref() == Some("disabled") {
        return Err(GatewayError::BadRequest(
            "停用用户必须使用停用并清理操作。".to_owned(),
        ));
    }
    if input
        .status
        .as_deref()
        .is_some_and(|status| status != "active")
    {
        return Err(GatewayError::BadRequest("用户状态无效。".to_owned()));
    }
    if input
        .display_name
        .as_deref()
        .is_some_and(|display_name| display_name.trim().is_empty())
    {
        return Err(GatewayError::BadRequest("显示名称不能为空。".to_owned()));
    }
    if let Some(Some(email)) = input.email.as_ref() {
        if let Some(existing) = repository::find_by_email(&pool, email).await? {
            if existing.id != id {
                return Err(GatewayError::BadRequest(
                    "邮箱已被其他用户使用。".to_owned(),
                ));
            }
        }
    }
    let mut user = if input.display_name.is_some() || input.email.is_some() {
        repository::update_profile(
            &pool,
            &id,
            input.display_name.as_deref(),
            input.email.as_ref().map(|email| email.as_deref()),
        )
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("user {id}")))?
    } else {
        repository::find(&pool, &id)
            .await?
            .ok_or_else(|| GatewayError::NotFound(format!("user {id}")))?
    };
    if let Some(status) = input.status.as_deref() {
        user = repository::update_status(&pool, &id, status)
            .await?
            .ok_or_else(|| GatewayError::NotFound(format!("user {id}")))?;
    }
    audit::record(
        &pool,
        &auth.user_id,
        "user.update",
        "user",
        &id,
        json!({
            "status": user.status,
            "display_name": user.display_name,
            "email": user.email,
        }),
    )
    .await?;
    Ok(Json(serde_json::to_value(user).unwrap_or_default()))
}

pub async fn deactivate(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<DeactivateUserRequest>,
) -> Result<Json<Value>, GatewayError> {
    let (pool, auth) = admin_pool(&state, &headers).await?;
    let user = repository::find(&pool, &id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("user {id}")))?;
    let owned_agents = registry::repository::count_by_owner(&pool, &id).await?;
    let transfer_to = input
        .transfer_to
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if owned_agents > 0 && transfer_to.is_none() {
        return Err(GatewayError::BadRequest(format!(
            "该用户拥有 {owned_agents} 个智能体；请先选择接收智能体的启用用户。"
        )));
    }
    if let Some(target) = transfer_to {
        if target == id {
            return Err(GatewayError::BadRequest("不能转移给原用户。".to_owned()));
        }
        if !repository::find(&pool, target)
            .await?
            .is_some_and(|candidate| candidate.is_active())
        {
            return Err(GatewayError::BadRequest(
                "需要选择一个启用中的接收用户。".to_owned(),
            ));
        }
        registry::repository::transfer_owner(&pool, &id, target).await?;
    }
    let key_hashes = api_keys::repository::delete_all_for_user(&pool, &id).await?;
    for key_hash in &key_hashes {
        crate::proxy::auth::master_key::evict_gateway_key_cache(key_hash);
    }
    let group_memberships = groups::members::delete_all_for_user(&pool, &id).await?;
    let direct_grants = agent_grants::repository::delete_all_for_user(&pool, &id).await?;
    let revoked_sessions = web_sessions::revoke_all_for_user(&pool, &id).await?;
    let vault_key_count = vault_keys::delete_personal_vault_keys_for_user(&pool, &id).await?;
    let user = repository::update_status(&pool, &id, "disabled")
        .await?
        .unwrap_or(user);
    audit::record(
        &pool,
        &auth.user_id,
        "user.deactivate",
        "user",
        &id,
        json!({
            "transfer_to": transfer_to,
            "transferred_agents": owned_agents,
            "revoked_gateway_keys": key_hashes.len(),
            "removed_group_memberships": group_memberships,
            "removed_direct_grants": direct_grants,
            "revoked_web_sessions": revoked_sessions,
            "removed_personal_vault_keys": vault_key_count,
        }),
    )
    .await?;
    Ok(Json(serde_json::to_value(user).unwrap_or_default()))
}
