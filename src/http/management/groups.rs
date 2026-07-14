use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    db::managed_agents::{audit, groups, users},
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

#[derive(Debug, Deserialize)]
pub struct ListGroupsQuery {
    pub query: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGroupRequest {
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    pub user_id: String,
    #[serde(default)]
    pub member_role: Option<String>,
}

async fn admin_pool(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(sqlx::PgPool, String), GatewayError> {
    let auth = authenticate(headers, state).await?;
    if !auth.is_admin {
        return Err(GatewayError::Forbidden);
    }
    Ok((
        state.db.clone().ok_or(GatewayError::MissingDatabase)?,
        auth.user_id,
    ))
}

async fn managed_group(
    state: &AppState,
    headers: &HeaderMap,
    group_id: &str,
) -> Result<
    (
        sqlx::PgPool,
        crate::proxy::auth::master_key::AuthContext,
        groups::schema::GroupRow,
    ),
    GatewayError,
> {
    let auth = authenticate(headers, state).await?;
    let pool = state.db.clone().ok_or(GatewayError::MissingDatabase)?;
    let group = groups::repository::find(&pool, group_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("group {group_id}")))?;
    if !auth.is_admin && !groups::members::is_group_admin(&pool, group_id, &auth.user_id).await? {
        return Err(GatewayError::NotFound(format!("group {group_id}")));
    }
    Ok((pool, auth, group))
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ListGroupsQuery>,
) -> Result<Json<Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.clone().ok_or(GatewayError::MissingDatabase)?;
    let groups = if auth.is_admin {
        groups::repository::list(&pool, query.query.as_deref()).await?
    } else {
        groups::repository::list_administered_by(&pool, &auth.user_id).await?
    };
    Ok(Json(json!({ "groups": groups })))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<CreateGroupRequest>,
) -> Result<(StatusCode, Json<Value>), GatewayError> {
    let (pool, user_id) = admin_pool(&state, &headers).await?;
    if input.name.trim().is_empty() {
        return Err(GatewayError::BadRequest("用户组名称不能为空。".to_owned()));
    }
    if groups::repository::find_by_name(&pool, input.name.trim())
        .await?
        .is_some()
    {
        return Err(GatewayError::BadRequest("用户组名称已存在。".to_owned()));
    }
    let group =
        groups::repository::create(&pool, &input.name, input.description.as_deref(), &user_id)
            .await?;
    audit::record(
        &pool,
        &user_id,
        "group.create",
        "group",
        &group.id,
        json!({ "name": group.name }),
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(group).unwrap_or_default()),
    ))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    Json(input): Json<UpdateGroupRequest>,
) -> Result<Json<Value>, GatewayError> {
    let (pool, auth) = admin_pool(&state, &headers).await?;
    let group = groups::repository::update_status(&pool, &group_id, &input.status)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("group {group_id}")))?;
    audit::record(
        &pool,
        &auth,
        "group.status.update",
        "group",
        &group_id,
        json!({ "status": group.status }),
    )
    .await?;
    Ok(Json(serde_json::to_value(group).unwrap_or_default()))
}

pub async fn list_members(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
) -> Result<Json<Value>, GatewayError> {
    let (pool, _, _) = managed_group(&state, &headers, &group_id).await?;
    let members = groups::members::list(&pool, &group_id).await?;
    Ok(Json(json!({ "members": members })))
}

pub async fn add_member(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    Json(input): Json<AddMemberRequest>,
) -> Result<(StatusCode, Json<Value>), GatewayError> {
    let (pool, auth, group) = managed_group(&state, &headers, &group_id).await?;
    if !group.is_active() {
        return Err(GatewayError::NotFound(format!("group {group_id}")));
    }
    if !users::repository::find(&pool, input.user_id.trim())
        .await?
        .is_some_and(|user| user.is_active())
    {
        return Err(GatewayError::BadRequest(
            "需要选择一个启用中的用户。".to_owned(),
        ));
    }
    let member = groups::members::upsert(
        &pool,
        &group_id,
        input.user_id.trim(),
        input.member_role.as_deref().unwrap_or("member"),
        &auth.user_id,
    )
    .await?;
    audit::record(
        &pool,
        &auth.user_id,
        "group.member.upsert",
        "group_member",
        input.user_id.trim(),
        json!({
            "group_id": group_id,
            "member_role": member.member_role,
        }),
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(member).unwrap_or_default()),
    ))
}

pub async fn delete_member(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((group_id, user_id)): Path<(String, String)>,
) -> Result<Json<bool>, GatewayError> {
    let (pool, auth, _) = managed_group(&state, &headers, &group_id).await?;
    let deleted = groups::members::delete(&pool, &group_id, &user_id).await?;
    if deleted {
        audit::record(
            &pool,
            &auth.user_id,
            "group.member.delete",
            "group_member",
            &user_id,
            json!({ "group_id": group_id }),
        )
        .await?;
    }
    Ok(Json(deleted))
}

pub async fn list_agent_grants(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
) -> Result<Json<Value>, GatewayError> {
    let (pool, _, _) = managed_group(&state, &headers, &group_id).await?;
    let grants = groups::agent_grants::list_for_group(&pool, &group_id).await?;
    Ok(Json(json!({ "grants": grants })))
}

pub async fn delete_agent_grant(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((group_id, agent_id)): Path<(String, String)>,
) -> Result<Json<bool>, GatewayError> {
    let (pool, auth, _) = managed_group(&state, &headers, &group_id).await?;
    let deleted = groups::agent_grants::delete(&pool, &agent_id, &group_id).await?;
    if deleted {
        audit::record(
            &pool,
            &auth.user_id,
            "group.agent_grant.delete",
            "agent_group_grant",
            &agent_id,
            json!({ "group_id": group_id }),
        )
        .await?;
    }
    Ok(Json(deleted))
}
