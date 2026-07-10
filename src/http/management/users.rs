use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    db::managed_agents::users::repository,
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
    pub status: String,
}

async fn admin_pool(state: &AppState, headers: &HeaderMap) -> Result<sqlx::PgPool, GatewayError> {
    let auth = authenticate(headers, state).await?;
    if !auth.is_admin {
        return Err(GatewayError::Forbidden);
    }
    state.db.clone().ok_or(GatewayError::MissingDatabase)
}

pub async fn me(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
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
        "display_name": display_name,
        "email": user.and_then(|row| row.email),
    })))
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<Value>, GatewayError> {
    let pool = admin_pool(&state, &headers).await?;
    let users = repository::list(&pool, query.query.as_deref()).await?;
    Ok(Json(json!({ "users": users })))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<Value>), GatewayError> {
    let pool = admin_pool(&state, &headers).await?;
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
    let pool = admin_pool(&state, &headers).await?;
    let user = repository::update_status(&pool, &id, &input.status)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("user {id}")))?;
    Ok(Json(serde_json::to_value(user).unwrap_or_default()))
}
