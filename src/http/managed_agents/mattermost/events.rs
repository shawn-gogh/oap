use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Form,
};

use crate::{
    db::managed_agents::{mattermost, registry::schema::ManagedAgentRow},
    errors::GatewayError,
    http::sessions::create_runtime_session_for_agent,
    proxy::state::AppState,
};

use super::{
    config::{load_agent, load_secret, mattermost_config, webhook_token_key},
    message::{incoming_message, OutgoingWebhookPayload},
    replies::spawn_mattermost_prompt,
    signature,
    types::{MattermostAgentConfig, MattermostIncomingMessage},
};

/// POST /api/agents/{agent_id}/mattermost/events — Mattermost Outgoing
/// Webhook receiver. Always returns 200 so Mattermost doesn't retry a
/// message we've already accepted; errors are logged, not surfaced to the
/// channel (there is no equivalent of Slack's url_verification handshake —
/// Mattermost validates the endpoint by simply expecting a 200).
pub(crate) async fn events(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
    Form(payload): Form<OutgoingWebhookPayload>,
) -> Result<StatusCode, GatewayError> {
    let pool = state
        .db
        .as_ref()
        .ok_or(GatewayError::MissingDatabase)?
        .clone();

    let agent = load_agent(&pool, &agent_id).await?;
    let config = mattermost_config(&agent)?;
    let expected_token = load_secret(&state, &webhook_token_key(&agent.id, &config)).await?;
    signature::verify(&payload.token, &expected_token)?;

    let message = incoming_message(&payload);

    // The bot's own posts (our replies) would otherwise re-trigger this same
    // webhook and the agent would answer itself forever.
    if config.bot_user_id.is_some() && message.user_id == config.bot_user_id {
        return Ok(StatusCode::OK);
    }

    let event_key = message.post_id.clone();
    if !mattermost::repository::record_event(&pool, &agent.id, &event_key).await? {
        return Ok(StatusCode::OK);
    }

    handle_message(state, pool, agent, config, message).await?;
    Ok(StatusCode::OK)
}

async fn handle_message(
    state: Arc<AppState>,
    pool: sqlx::PgPool,
    agent: ManagedAgentRow,
    config: MattermostAgentConfig,
    message: MattermostIncomingMessage,
) -> Result<(), GatewayError> {
    let session_id = match message.requires_existing_thread {
        true => {
            match mattermost::repository::get(
                &pool,
                &agent.id,
                &message.channel_id,
                &message.root_id,
            )
            .await?
            {
                Some(row) => row.session_id,
                None => return Ok(()),
            }
        }
        false => {
            let session_id = create_runtime_session_for_agent(
                state.clone(),
                &pool,
                agent.id.clone(),
                agent_runtime(&agent),
                format!("Mattermost {} {}", message.channel_id, message.root_id),
                message.prompt.clone(),
                serde_json::json!({
                    "source": "mattermost",
                    "channel_id": message.channel_id,
                    "root_id": message.root_id,
                    "team_id": message.team_id,
                    "user_id": message.user_id,
                }),
            )
            .await?;
            mattermost::repository::upsert(
                &pool,
                &agent.id,
                &message.channel_id,
                &message.root_id,
                &session_id,
            )
            .await?
            .session_id
        }
    };
    spawn_mattermost_prompt(state, pool, agent, config, message, session_id);
    Ok(())
}

fn agent_runtime(agent: &ManagedAgentRow) -> String {
    agent
        .config
        .get("runtime")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|runtime| !runtime.is_empty())
        .unwrap_or(crate::sdk::agents::CLAUDE_MANAGED_AGENTS)
        .to_owned()
}
