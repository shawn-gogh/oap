use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    db::managed_agents::registry::schema::ManagedAgentRow,
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

use super::{
    config::{
        bot_token_key, load_agent, load_secret, mattermost_config, update_mattermost_config,
        webhook_token_key,
    },
    web_api,
};

const VAULT_USER: &str = "default";

#[derive(Debug, Deserialize)]
pub struct ConnectRequest {
    pub server_url: String,
    #[serde(default)]
    pub bot_token: String,
    #[serde(default)]
    pub webhook_token: String,
    pub notification_channel_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConnectResponse {
    pub status: String,
    pub bot_user_id: String,
    pub agent: ManagedAgentRow,
}

/// POST /api/agents/{agent_id}/mattermost/connect — the whole "install"
/// step, since Mattermost self-hosted bots have no OAuth app-registration
/// flow to drive (unlike Slack): an admin creates a bot account + Outgoing
/// Webhook in their own Mattermost server first and pastes the resulting
/// credentials here. This verifies the bot token against `/users/me`,
/// stores both secrets in the vault, and records the connection in
/// `agent.config["mattermost"]`.
pub(crate) async fn connect(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Json(input): Json<ConnectRequest>,
) -> Result<Json<ConnectResponse>, GatewayError> {
    authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let server_url = input.server_url.trim().trim_end_matches('/').to_owned();
    if server_url.is_empty() {
        return Err(GatewayError::InvalidJsonMessage(
            "server_url is required".to_owned(),
        ));
    }
    let agent = load_agent(pool, &agent_id).await?;
    let existing = mattermost_config(&agent)?;
    let bot_key = bot_token_key(&agent.id, &existing);
    let webhook_key = webhook_token_key(&agent.id, &existing);
    let bot_token = resolve_token(&state, &bot_key, &input.bot_token, "bot_token").await?;
    let _webhook_token =
        resolve_token(&state, &webhook_key, &input.webhook_token, "webhook_token").await?;
    let bot_user_id = web_api::verify_bot_token(&state.http, &server_url, &bot_token).await?;
    save_supplied_token(&state, pool, &bot_key, &input.bot_token).await?;
    save_supplied_token(&state, pool, &webhook_key, &input.webhook_token).await?;
    let notification_channel_id = match input.notification_channel_id.as_deref() {
        Some(value) => Some(value.trim()).filter(|value| !value.is_empty()),
        None => existing.notification_channel_id.as_deref(),
    };

    let updated = update_mattermost_config(
        pool,
        &agent,
        json!({
            "server_url": server_url,
            "bot_token_key": bot_key,
            "webhook_token_key": webhook_key,
            "bot_user_id": bot_user_id,
            "notification_channel_id": notification_channel_id,
            "status": "connected",
            "connected_at": crate::db::managed_agents::now_ms(),
        }),
    )
    .await?;

    Ok(Json(ConnectResponse {
        status: "connected".to_owned(),
        bot_user_id,
        agent: updated,
    }))
}

async fn resolve_token(
    state: &AppState,
    key: &str,
    supplied: &str,
    field: &str,
) -> Result<String, GatewayError> {
    let supplied = supplied.trim();
    if !supplied.is_empty() {
        return Ok(supplied.to_owned());
    }
    load_secret(state, key).await.map_err(|_| {
        GatewayError::InvalidJsonMessage(format!(
            "{field} is required when Mattermost is not already connected"
        ))
    })
}

async fn save_supplied_token(
    state: &AppState,
    pool: &sqlx::PgPool,
    key: &str,
    supplied: &str,
) -> Result<(), GatewayError> {
    let supplied = supplied.trim();
    if !supplied.is_empty() {
        crate::proxy::vault::save(pool, &state.config, VAULT_USER, key, supplied).await?;
    }
    Ok(())
}
