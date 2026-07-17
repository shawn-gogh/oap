//! Per-user API keys for BYO imported agents.
//!
//! An agent imported with `credential_mode: "byo"` carries no platform-held
//! key: each user who wants to talk to it stores their own key here, as a
//! personal credential named `agent_byo:{agent_id}`. Session execution
//! (`runtime_resolution::imported_agent_credential`) resolves the key of the
//! session's owner, so users never share credentials.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    db::{
        credentials,
        managed_agents::registry::{repository, schema::ManagedAgentRow},
    },
    errors::GatewayError,
    http::runtime_resolution::byo_credential_name,
    proxy::{
        auth::master_key::{authenticate, AuthContext},
        credential_crypto,
        state::AppState,
    },
};

#[derive(Debug, Deserialize)]
pub struct SaveByoCredentialRequest {
    pub api_key: String,
}

#[derive(Debug, Serialize)]
pub struct ByoCredentialStatusResponse {
    pub configured: bool,
}

#[derive(Debug, Serialize)]
pub struct SaveByoCredentialResponse {
    pub ok: bool,
}

#[derive(Debug, Serialize)]
pub struct DeleteByoCredentialResponse {
    pub ok: bool,
}

#[derive(Debug, Serialize)]
pub struct ListByoConfiguredResponse {
    pub agent_ids: Vec<String>,
}

async fn byo_agent(
    state: &AppState,
    headers: &HeaderMap,
    agent_id: &str,
) -> Result<(sqlx::PgPool, AuthContext, ManagedAgentRow), GatewayError> {
    let auth = authenticate(headers, state).await?;
    let pool = state.db.clone().ok_or(GatewayError::MissingDatabase)?;
    let agent = repository::get(&pool, agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("agent {agent_id}")))?;
    super::assert_agent_use(&auth, &agent, &pool).await?;
    if agent
        .config
        .pointer("/source/credential_mode")
        .and_then(serde_json::Value::as_str)
        != Some("byo")
    {
        return Err(GatewayError::BadRequest(
            "该智能体不使用 BYO 凭据模式。".to_owned(),
        ));
    }
    Ok((pool, auth, agent))
}

/// GET /api/agents/{agent_id}/byo-credential — has the caller stored a key?
pub async fn status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<ByoCredentialStatusResponse>, GatewayError> {
    let (pool, auth, agent) = byo_agent(&state, &headers, &agent_id).await?;
    let configured =
        credentials::get_personal_by_name(&pool, &byo_credential_name(&agent.id), &auth.user_id)
            .await?
            .is_some();
    Ok(Json(ByoCredentialStatusResponse { configured }))
}

/// PUT /api/agents/{agent_id}/byo-credential — store (or replace) the
/// caller's own key for this agent.
pub async fn store(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Json(input): Json<SaveByoCredentialRequest>,
) -> Result<Json<SaveByoCredentialResponse>, GatewayError> {
    let (pool, auth, agent) = byo_agent(&state, &headers, &agent_id).await?;
    let api_key = input.api_key.trim();
    if api_key.is_empty() {
        return Err(GatewayError::InvalidJsonMessage(
            "api_key is required".to_owned(),
        ));
    }
    let endpoint = agent
        .config
        .pointer("/source/endpoint")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let key =
        credential_crypto::encryption_key(state.config.general_settings.master_key.as_deref())?;
    credentials::upsert_personal(
        &pool,
        &byo_credential_name(&agent.id),
        &auth.user_id,
        json!({
            "api_key": credential_crypto::encrypt_value(api_key, &key)?,
            "api_base": credential_crypto::encrypt_value(endpoint, &key)?,
        }),
        json!({ "kind": "agent_byo", "agent_id": agent.id }),
        &auth.user_id,
    )
    .await?;
    Ok(Json(SaveByoCredentialResponse { ok: true }))
}

/// DELETE /api/agents/{agent_id}/byo-credential — remove the caller's key.
pub async fn delete(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<DeleteByoCredentialResponse>, GatewayError> {
    let (pool, auth, agent) = byo_agent(&state, &headers, &agent_id).await?;
    let deleted = sqlx::query(
        r#"
        DELETE FROM "LiteLLM_CredentialsTable"
        WHERE credential_name = $1 AND scope = 'personal' AND owner_id = $2
        "#,
    )
    .bind(byo_credential_name(&agent.id))
    .bind(&auth.user_id)
    .execute(&pool)
    .await
    .map_err(GatewayError::Database)?
    .rows_affected()
        > 0;
    Ok(Json(DeleteByoCredentialResponse { ok: deleted }))
}

/// GET /api/agents/byo-credentials — agent ids the caller has keys for.
/// Lets the agents page show "已配置" without probing each agent.
pub async fn list_configured(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<ListByoConfiguredResponse>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let agent_ids = sqlx::query_scalar::<_, String>(
        r#"
        SELECT credential_name
        FROM "LiteLLM_CredentialsTable"
        WHERE credential_name LIKE 'agent_byo:%' AND scope = 'personal' AND owner_id = $1
        ORDER BY credential_name ASC
        "#,
    )
    .bind(&auth.user_id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)?
    .into_iter()
    .filter_map(|name| {
        name.strip_prefix("agent_byo:")
            .map(str::to_owned)
            .filter(|id| !id.is_empty())
    })
    .collect();
    Ok(Json(ListByoConfiguredResponse { agent_ids }))
}
