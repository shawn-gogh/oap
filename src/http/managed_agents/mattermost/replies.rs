use std::sync::Arc;

use sqlx::PgPool;
use tracing::warn;

use crate::{
    db::managed_agents::registry::schema::ManagedAgentRow,
    errors::GatewayError,
    http::sessions::{enqueue_prompt_text, runtime_event_stream_for_session},
    proxy::state::AppState,
};

use super::{
    config::{bot_token_key, load_secret},
    reply_lock::MattermostPromptLock,
    reply_storage::last_message_seq,
    reply_stream::{MattermostReply, MattermostReplyParams},
    types::{MattermostAgentConfig, MattermostIncomingMessage},
    web_api,
};

pub(super) fn spawn_mattermost_prompt(
    state: Arc<AppState>,
    pool: PgPool,
    agent: ManagedAgentRow,
    config: MattermostAgentConfig,
    message: MattermostIncomingMessage,
    session_id: String,
) {
    tokio::spawn(async move {
        if let Err(error) =
            run_mattermost_prompt(state, pool, agent, config, message, session_id).await
        {
            warn!("mattermost prompt failed: {error}");
        }
    });
}

async fn run_mattermost_prompt(
    state: Arc<AppState>,
    pool: PgPool,
    agent: ManagedAgentRow,
    config: MattermostAgentConfig,
    message: MattermostIncomingMessage,
    session_id: String,
) -> Result<(), GatewayError> {
    let bot_token = load_secret(&state, &bot_token_key(&agent.id, &config)).await?;
    let server_url = config.server_url.clone().unwrap_or_default();
    let _lock = MattermostPromptLock::acquire(&state.keyed_locks, &session_id).await;
    run_locked_prompt(
        &state,
        &pool,
        &agent.model,
        &message,
        &session_id,
        &bot_token,
        &server_url,
    )
    .await
}

async fn run_locked_prompt(
    state: &Arc<AppState>,
    pool: &PgPool,
    model: &str,
    message: &MattermostIncomingMessage,
    session_id: &str,
    bot_token: &str,
    server_url: &str,
) -> Result<(), GatewayError> {
    let baseline_seq = last_message_seq(pool, session_id).await?;
    let runtime_stream = runtime_event_stream_for_session(state, pool, session_id)
        .await
        .ok();
    let event_stream = state.agent_runs.event_stream();
    let placeholder = post_placeholder(state, bot_token, server_url, message).await;
    let mut reply = MattermostReply::new(MattermostReplyParams {
        state,
        pool,
        bot_token,
        server_url,
        channel_id: &message.channel_id,
        root_id: &message.root_id,
        post_id: placeholder,
        session_id,
        baseline_seq,
    });
    enqueue_or_report(state, pool, model, message, &mut reply, session_id).await?;
    let result = if let Some(stream) = runtime_stream {
        reply.run_runtime(stream).await
    } else {
        reply.run(event_stream.rx).await
    };
    if let Err(error) = &result {
        let text = format!("Agent run failed: {error}");
        if let Err(update_error) = reply.replace_text(&text).await {
            warn!("mattermost failure update failed: {update_error}");
        }
    }
    result
}

async fn post_placeholder(
    state: &AppState,
    bot_token: &str,
    server_url: &str,
    message: &MattermostIncomingMessage,
) -> Option<String> {
    match web_api::upsert_post(web_api::UpsertPostParams {
        client: &state.http,
        server_url,
        bot_token,
        channel_id: &message.channel_id,
        root_id: &message.root_id,
        post_id: None,
        text: "_Thinking..._",
    })
    .await
    {
        Ok(post_id) => Some(post_id),
        Err(error) => {
            warn!("mattermost placeholder failed: {error}");
            None
        }
    }
}

async fn enqueue_or_report(
    state: &Arc<AppState>,
    pool: &PgPool,
    model: &str,
    message: &MattermostIncomingMessage,
    reply: &mut MattermostReply<'_>,
    session_id: &str,
) -> Result<(), GatewayError> {
    let result = enqueue_prompt_text(
        state.clone(),
        pool.clone(),
        session_id,
        message.prompt.clone(),
        model.to_owned(),
    )
    .await;
    if let Err(error) = result {
        reply.replace_text(&error.to_string()).await?;
        return Err(error);
    }
    Ok(())
}
