use serde_json::{json, Value};
use sqlx::PgPool;

use crate::{
    db::managed_agents::registry::{
        self,
        schema::{ManagedAgentRow, UpdateManagedAgent},
    },
    errors::GatewayError,
};

pub(crate) use crate::channels::secrets::load_secret;

use super::types::MattermostAgentConfig;

pub(crate) async fn load_agent(
    pool: &PgPool,
    agent_id: &str,
) -> Result<ManagedAgentRow, GatewayError> {
    registry::repository::get(pool, agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("agent not found".to_owned()))
}

pub(crate) fn mattermost_config(
    agent: &ManagedAgentRow,
) -> Result<MattermostAgentConfig, GatewayError> {
    serde_json::from_value(
        agent
            .config
            .get("mattermost")
            .cloned()
            .unwrap_or_else(|| json!({})),
    )
    .map_err(GatewayError::InvalidJson)
}

pub(crate) fn bot_token_key(agent_id: &str, config: &MattermostAgentConfig) -> String {
    config
        .bot_token_key
        .clone()
        .unwrap_or_else(|| format!("MATTERMOST_{agent_id}_BOT_TOKEN"))
}

pub(crate) fn webhook_token_key(agent_id: &str, config: &MattermostAgentConfig) -> String {
    config
        .webhook_token_key
        .clone()
        .unwrap_or_else(|| format!("MATTERMOST_{agent_id}_WEBHOOK_TOKEN"))
}

pub(crate) async fn update_mattermost_config(
    pool: &PgPool,
    agent: &ManagedAgentRow,
    patch: Value,
) -> Result<ManagedAgentRow, GatewayError> {
    let config = patched_mattermost_config(&agent.config, patch);
    registry::repository::update(
        pool,
        &agent.id,
        UpdateManagedAgent {
            name: None,
            model: None,
            tools: None,
            runtime: None,
            system: None,
            prompt: None,
            cron: None,
            timezone: None,
            vault_keys: None,
            setup_commands: None,
            max_runtime_minutes: None,
            on_failure: None,
            config: Some(config),
            owner_id: None,
            status: None,
            description: None,
            harness: None,
            skill_ids: None,
            rule_ids: None,
        },
    )
    .await?
    .ok_or_else(|| GatewayError::NotFound("agent not found".to_owned()))
}

fn patched_mattermost_config(config: &Value, patch: Value) -> Value {
    let mut root = config.as_object().cloned().unwrap_or_default();
    let mut mattermost = root
        .get("mattermost")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(patch) = patch.as_object() {
        for (key, value) in patch {
            mattermost.insert(key.clone(), value.clone());
        }
    }
    root.insert("mattermost".to_owned(), Value::Object(mattermost));
    Value::Object(root)
}
