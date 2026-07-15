use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    db::managed_agents::{
        exposed_apps::{repository, schema::ExposedAppRow},
        now_ms,
    },
    errors::GatewayError,
    proxy::{
        auth::master_key::{authenticate, AuthContext},
        state::AppState,
    },
};

use super::share;

const DEFAULT_SHARE_TTL_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    session_id: Option<String>,
    /// The platform MCP URL binds a session id at agent-registration time, so
    /// apps exposed from later chats land under that original session.
    /// Querying by agent catches them regardless of which session registered
    /// the exposure.
    agent_id: Option<String>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let mut apps: Vec<ExposedAppRow> = Vec::new();
    if let Some(session_id) = query.session_id.as_deref().filter(|v| !v.is_empty()) {
        apps.extend(repository::list_for_session(pool, session_id).await?);
    }
    if let Some(agent_id) = query.agent_id.as_deref().filter(|v| !v.is_empty()) {
        for app in repository::list_for_agent(pool, agent_id).await? {
            if !apps.iter().any(|existing| existing.id == app.id) {
                apps.push(app);
            }
        }
    }
    if query.session_id.is_none() && query.agent_id.is_none() {
        return Err(GatewayError::BadRequest(
            "session_id or agent_id is required".to_owned(),
        ));
    }
    apps.retain(|app| is_owner(&auth, app));
    apps.sort_by_key(|app| std::cmp::Reverse(app.created_at));
    Ok(Json(json!({ "apps": apps })))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> Result<Json<Value>, GatewayError> {
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let app = require_owned_app(&state, pool, &headers, &app_id).await?;
    let deleted = repository::soft_delete(pool, &app.id).await?;
    Ok(Json(json!({ "deleted": deleted })))
}

#[derive(Debug, Default, Deserialize)]
pub struct ShareRequest {
    ttl_seconds: Option<i64>,
}

pub async fn create_share(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
    body: Option<Json<ShareRequest>>,
) -> Result<Json<Value>, GatewayError> {
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let app = require_owned_app(&state, pool, &headers, &app_id).await?;
    let master_key =
        share::require_master_key(state.config.general_settings.master_key.as_deref())?;

    let ttl_ms = body
        .and_then(|Json(request)| request.ttl_seconds)
        .map(|seconds| seconds.max(1) * 1000)
        .unwrap_or(DEFAULT_SHARE_TTL_MS);
    let expires_at = now_ms() + ttl_ms;
    let token = share::sign_token(master_key, &app.id, expires_at, app.share_version);
    Ok(Json(json!({
        "url": format!("/apps/{}/?token={token}", app.id),
        "expires_at": expires_at,
    })))
}

pub async fn revoke_share(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(app_id): Path<String>,
) -> Result<Json<Value>, GatewayError> {
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let app = require_owned_app(&state, pool, &headers, &app_id).await?;
    let version = repository::bump_share_version(pool, &app.id).await?;
    Ok(Json(json!({ "revoked": version.is_some() })))
}

async fn require_owned_app(
    state: &AppState,
    pool: &sqlx::PgPool,
    headers: &HeaderMap,
    app_id: &str,
) -> Result<ExposedAppRow, GatewayError> {
    let auth = authenticate(headers, state).await?;
    let app = repository::get(pool, app_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("exposed app not found: {app_id}")))?;
    if !is_owner(&auth, &app) {
        return Err(GatewayError::Forbidden);
    }
    Ok(app)
}

fn is_owner(auth: &AuthContext, app: &ExposedAppRow) -> bool {
    auth.is_admin || app.owner_user_id.as_deref() == Some(auth.user_id.as_str())
}
