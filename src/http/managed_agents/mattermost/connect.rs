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
    config::{bot_token_key, load_agent, update_mattermost_config, webhook_token_key},
    web_api,
};

const VAULT_USER: &str = "default";

#[derive(Debug, Deserialize)]
pub struct ConnectRequest {
    pub server_url: String,
    pub bot_token: String,
    pub webhook_token: String,
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
    let bot_token = input.bot_token.trim();
    let webhook_token = input.webhook_token.trim();
    if bot_token.is_empty() || webhook_token.is_empty() {
        return Err(GatewayError::InvalidJsonMessage(
            "bot_token and webhook_token are required".to_owned(),
        ));
    }

    let bot_user_id = web_api::verify_bot_token(&state.http, &server_url, bot_token).await?;

    let agent = load_agent(pool, &agent_id).await?;
    let bot_key = bot_token_key(&agent.id, &Default::default());
    let webhook_key = webhook_token_key(&agent.id, &Default::default());
    crate::proxy::vault::save(pool, &state.config, VAULT_USER, &bot_key, bot_token).await?;
    crate::proxy::vault::save(pool, &state.config, VAULT_USER, &webhook_key, webhook_token).await?;

    let updated = update_mattermost_config(
        pool,
        &agent,
        json!({
            "server_url": server_url,
            "bot_token_key": bot_key,
            "webhook_token_key": webhook_key,
            "bot_user_id": bot_user_id,
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
