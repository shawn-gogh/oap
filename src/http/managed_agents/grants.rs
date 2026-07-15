//! Agent sharing: grant management endpoints. Only the owner (or admin) may
//! manage grants; grantees get 'use' (see + run sessions) or 'edit'
//! (additionally modify config/workspace/evals).

use std::{collections::HashSet, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    db::managed_agents::{agent_grants, audit, groups, now_ms, registry, users},
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

#[derive(Debug, Deserialize)]
pub struct CreateGrantRequest {
    pub user_id: String,
    #[serde(default)]
    pub permission: Option<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateBatchGrantRequest {
    pub user_ids: Vec<String>,
    #[serde(default)]
    pub permission: Option<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct GrantableUsersQuery {
    pub query: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateGroupGrantRequest {
    pub group_id: String,
    #[serde(default)]
    pub permission: Option<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateBatchGroupGrantRequest {
    pub group_ids: Vec<String>,
    #[serde(default)]
    pub permission: Option<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct GrantableGroupsQuery {
    pub query: Option<String>,
}

#[derive(Debug, Serialize)]
struct GrantUserSummary {
    id: String,
    display_name: String,
    email: Option<String>,
    status: String,
}

#[derive(Debug, Serialize)]
struct GrantGroupSummary {
    id: String,
    name: String,
    status: String,
    member_count: usize,
}

#[derive(Debug, Serialize)]
struct AgentGrantDetails {
    #[serde(flatten)]
    grant: agent_grants::schema::AgentGrantRow,
    user: Option<GrantUserSummary>,
    source: &'static str,
}

#[derive(Debug, Serialize)]
struct AgentGroupGrantDetails {
    #[serde(flatten)]
    grant: groups::schema::AgentGroupGrantRow,
    group: Option<GrantGroupSummary>,
    source: &'static str,
}

async fn owned_agent(
    state: &AppState,
    headers: &HeaderMap,
    agent_id: &str,
) -> Result<(sqlx::PgPool, String), GatewayError> {
    let auth = authenticate(headers, state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let agent = registry::repository::get(pool, agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    super::assert_agent_access(&auth, &agent)?;
    Ok((pool.clone(), auth.user_id))
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, GatewayError> {
    let (pool, _) = owned_agent(&state, &headers, &agent_id).await?;
    let grants = agent_grant_details(&pool, &agent_id).await?;
    Ok(Json(json!({ "grants": grants })))
}

async fn agent_grant_details(
    pool: &sqlx::PgPool,
    agent_id: &str,
) -> Result<Vec<AgentGrantDetails>, GatewayError> {
    let grants = agent_grants::repository::list_for_agent(pool, agent_id).await?;
    let mut details = Vec::with_capacity(grants.len());
    for grant in grants {
        let user = users::repository::find(pool, &grant.grantee_user_id)
            .await?
            .map(|user| GrantUserSummary {
                id: user.id,
                display_name: user.display_name,
                email: user.email,
                status: user.status,
            });
        details.push(AgentGrantDetails {
            grant,
            user,
            source: "direct",
        });
    }
    Ok(details)
}

async fn group_grant_details(
    pool: &sqlx::PgPool,
    agent_id: &str,
) -> Result<Vec<AgentGroupGrantDetails>, GatewayError> {
    let grants = groups::agent_grants::list_for_agent(pool, agent_id).await?;
    let mut details = Vec::with_capacity(grants.len());
    for grant in grants {
        let group = if let Some(group) = groups::repository::find(pool, &grant.group_id).await? {
            let member_count = groups::members::list(pool, &group.id).await?.len();
            Some(GrantGroupSummary {
                id: group.id,
                name: group.name,
                status: group.status,
                member_count,
            })
        } else {
            None
        };
        details.push(AgentGroupGrantDetails {
            grant,
            group,
            source: "group",
        });
    }
    Ok(details)
}

fn expires_at(value: Option<i64>) -> Result<Option<i64>, GatewayError> {
    if value.is_some_and(|timestamp| timestamp <= now_ms()) {
        return Err(GatewayError::BadRequest(
            "授权到期时间必须晚于当前时间。".to_owned(),
        ));
    }
    Ok(value)
}

fn normalized_ids(values: Vec<String>, label: &str) -> Result<Vec<String>, GatewayError> {
    let mut seen = HashSet::new();
    let ids = values
        .into_iter()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(value.clone()))
        .collect::<Vec<_>>();
    if ids.is_empty() || ids.len() > 100 {
        return Err(GatewayError::BadRequest(format!(
            "一次最多授权 100 个{label}。"
        )));
    }
    Ok(ids)
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Json(input): Json<CreateGrantRequest>,
) -> Result<(StatusCode, Json<Value>), GatewayError> {
    let (pool, granted_by) = owned_agent(&state, &headers, &agent_id).await?;
    let grantee = input.user_id.trim();
    if grantee.is_empty() {
        return Err(GatewayError::InvalidConfig(
            "user_id is required".to_owned(),
        ));
    }
    if !crate::db::managed_agents::users::repository::find(&pool, grantee)
        .await?
        .is_some_and(|user| user.is_active())
    {
        return Err(GatewayError::InvalidConfig(
            "an active user is required".to_owned(),
        ));
    }
    let grant = agent_grants::repository::upsert(
        &pool,
        &agent_id,
        grantee,
        input.permission.as_deref().unwrap_or("use"),
        expires_at(input.expires_at)?,
        &granted_by,
    )
    .await?;
    audit::record(
        &pool,
        &granted_by,
        "agent.user_grant.upsert",
        "agent_user_grant",
        grantee,
        json!({
            "agent_id": agent_id,
            "permission": grant.permission,
            "expires_at": grant.expires_at,
        }),
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(grant).unwrap_or_default()),
    ))
}

pub async fn create_batch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Json(input): Json<CreateBatchGrantRequest>,
) -> Result<(StatusCode, Json<Value>), GatewayError> {
    let (pool, granted_by) = owned_agent(&state, &headers, &agent_id).await?;
    let user_ids = normalized_ids(input.user_ids, "用户")?;
    let active_ids = users::repository::active_ids(&pool, &user_ids).await?;
    if active_ids.len() != user_ids.len() {
        return Err(GatewayError::BadRequest(
            "只能授权给启用中的用户。".to_owned(),
        ));
    }
    let grant_expiry = expires_at(input.expires_at)?;
    let grants = agent_grants::repository::upsert_many(
        &pool,
        &agent_id,
        &user_ids,
        input.permission.as_deref().unwrap_or("use"),
        grant_expiry,
        &granted_by,
    )
    .await?;
    audit::record(
        &pool,
        &granted_by,
        "agent.user_grant.batch_upsert",
        "agent_user_grant",
        &agent_id,
        json!({ "user_ids": user_ids, "permission": input.permission, "expires_at": grant_expiry }),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(json!({ "grants": grants }))))
}

pub async fn grantable_users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Query(query): Query<GrantableUsersQuery>,
) -> Result<Json<Value>, GatewayError> {
    let (pool, _) = owned_agent(&state, &headers, &agent_id).await?;
    let query = query.query.as_deref().unwrap_or("").trim();
    if query.chars().count() < 2 {
        return Ok(Json(json!({ "users": [] })));
    }
    let users = crate::db::managed_agents::users::repository::list(&pool, Some(query)).await?;
    Ok(Json(json!({ "users": users })))
}

pub async fn list_group_grants(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, GatewayError> {
    let (pool, _) = owned_agent(&state, &headers, &agent_id).await?;
    let grants = group_grant_details(&pool, &agent_id).await?;
    Ok(Json(json!({ "grants": grants })))
}

pub async fn create_group_grant(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Json(input): Json<CreateGroupGrantRequest>,
) -> Result<(StatusCode, Json<Value>), GatewayError> {
    let (pool, granted_by) = owned_agent(&state, &headers, &agent_id).await?;
    let group_id = input.group_id.trim();
    if !crate::db::managed_agents::groups::repository::find(&pool, group_id)
        .await?
        .is_some_and(|group| group.is_active())
    {
        return Err(GatewayError::InvalidConfig(
            "an active group is required".to_owned(),
        ));
    }
    let grant = crate::db::managed_agents::groups::agent_grants::upsert(
        &pool,
        &agent_id,
        group_id,
        input.permission.as_deref().unwrap_or("use"),
        expires_at(input.expires_at)?,
        &granted_by,
    )
    .await?;
    audit::record(
        &pool,
        &granted_by,
        "agent.group_grant.upsert",
        "agent_group_grant",
        group_id,
        json!({
            "agent_id": agent_id,
            "permission": grant.permission,
            "expires_at": grant.expires_at,
        }),
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(grant).unwrap_or_default()),
    ))
}

pub async fn create_batch_group_grant(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Json(input): Json<CreateBatchGroupGrantRequest>,
) -> Result<(StatusCode, Json<Value>), GatewayError> {
    let (pool, granted_by) = owned_agent(&state, &headers, &agent_id).await?;
    let group_ids = normalized_ids(input.group_ids, "用户组")?;
    let active_ids = groups::repository::active_ids(&pool, &group_ids).await?;
    if active_ids.len() != group_ids.len() {
        return Err(GatewayError::BadRequest(
            "只能授权给启用中的用户组。".to_owned(),
        ));
    }
    let grant_expiry = expires_at(input.expires_at)?;
    let grants = groups::agent_grants::upsert_many(
        &pool,
        &agent_id,
        &group_ids,
        input.permission.as_deref().unwrap_or("use"),
        grant_expiry,
        &granted_by,
    )
    .await?;
    audit::record(
        &pool,
        &granted_by,
        "agent.group_grant.batch_upsert",
        "agent_group_grant",
        &agent_id,
        json!({ "group_ids": group_ids, "permission": input.permission, "expires_at": grant_expiry }),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(json!({ "grants": grants }))))
}

pub async fn delete_group_grant(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((agent_id, group_id)): Path<(String, String)>,
) -> Result<Json<bool>, GatewayError> {
    let (pool, actor) = owned_agent(&state, &headers, &agent_id).await?;
    let deleted =
        crate::db::managed_agents::groups::agent_grants::delete(&pool, &agent_id, &group_id)
            .await?;
    if deleted {
        audit::record(
            &pool,
            &actor,
            "agent.group_grant.delete",
            "agent_group_grant",
            &group_id,
            json!({ "agent_id": agent_id }),
        )
        .await?;
    }
    Ok(Json(deleted))
}

pub async fn grantable_groups(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Query(query): Query<GrantableGroupsQuery>,
) -> Result<Json<Value>, GatewayError> {
    let (pool, _) = owned_agent(&state, &headers, &agent_id).await?;
    let query = query.query.as_deref().unwrap_or("").trim();
    if query.chars().count() < 2 {
        return Ok(Json(json!({ "groups": [] })));
    }
    let groups = crate::db::managed_agents::groups::repository::list(&pool, Some(query)).await?;
    Ok(Json(json!({ "groups": groups })))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((agent_id, grantee)): Path<(String, String)>,
) -> Result<Json<bool>, GatewayError> {
    let (pool, actor) = owned_agent(&state, &headers, &agent_id).await?;
    let deleted = agent_grants::repository::delete(&pool, &agent_id, &grantee).await?;
    if deleted {
        audit::record(
            &pool,
            &actor,
            "agent.user_grant.delete",
            "agent_user_grant",
            &grantee,
            json!({ "agent_id": agent_id }),
        )
        .await?;
    }
    Ok(Json(deleted))
}
