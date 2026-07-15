use std::sync::Arc;

use sqlx::PgPool;

use crate::{
    db::{
        credentials,
        managed_agents::{
            harnesses, registry::schema::ManagedAgentRow, sessions::schema::SessionRow,
        },
    },
    errors::GatewayError,
    http::agent_runtimes::{load_credential, RuntimeCredential},
    proxy::{credential_crypto, state::AppState},
    sdk::{
        agents::AgentRuntime,
        providers::{self, base::runtime::RuntimeAdapter},
    },
};

pub(crate) struct ResolvedRuntime {
    pub alias: String,
    pub agent_runtime: AgentRuntime,
    pub credential: RuntimeCredential,
    pub adapter: Arc<dyn RuntimeAdapter>,
    /// True when this runtime came from a DB-registered custom harness (e.g.
    /// opencode) rather than a built-in static runtime. Custom harnesses speak
    /// an api_spec like `claude_managed_agents` but don't implement
    /// Anthropic-specific infrastructure such as credential vaults, so
    /// provisioning must take a different path for them.
    pub is_custom_harness: bool,
}

pub(crate) async fn resolve_runtime(
    pool: &PgPool,
    state: &AppState,
    alias: &str,
) -> Result<ResolvedRuntime, GatewayError> {
    // 1. Try static registry first.
    {
        let registry = providers::runtime_registry();
        if let Some(entry) = registry.entry_for_id(alias) {
            let credential = load_credential(state, alias).await?;
            return Ok(ResolvedRuntime {
                alias: alias.to_owned(),
                agent_runtime: entry.runtime,
                credential,
                adapter: entry.adapter.clone(),
                is_custom_harness: false,
            });
        }
    }

    // 2. Custom harness: DB lookup
    let harness = harnesses::repository::get_by_alias(pool, alias)
        .await?
        .ok_or_else(|| GatewayError::InvalidJsonMessage(format!("unsupported runtime: {alias}")))?;

    let registry = providers::runtime_registry();
    let entry = registry.entry_for_id(&harness.api_spec).ok_or_else(|| {
        GatewayError::InvalidConfig(format!("unknown api_spec: {}", harness.api_spec))
    })?;

    let credential = harness_credential(pool, state, alias).await?;

    Ok(ResolvedRuntime {
        alias: alias.to_owned(),
        agent_runtime: entry.runtime,
        credential,
        adapter: entry.adapter.clone(),
        is_custom_harness: true,
    })
}

/// Resolves the runtime and, for imported agents, replaces its shared/global
/// credential with the credential stored under that agent owner's scope.
pub(crate) async fn resolve_runtime_for_agent(
    pool: &PgPool,
    state: &AppState,
    alias: &str,
    agent: &ManagedAgentRow,
) -> Result<ResolvedRuntime, GatewayError> {
    let mut resolved = resolve_runtime(pool, state, alias).await?;
    let source = agent.config.get("source");
    let credential_name = source
        .and_then(|value| value.get("credential_name"))
        .and_then(serde_json::Value::as_str);
    let credential_mode = source
        .and_then(|value| value.get("credential_mode"))
        .and_then(serde_json::Value::as_str);
    if credential_mode != Some("shared") {
        return Ok(resolved);
    }
    let credential_name = credential_name.ok_or_else(|| {
        GatewayError::InvalidConfig("导入智能体缺少隔离运行凭据名称。".to_owned())
    })?;
    let owner_id = agent
        .owner_id
        .as_deref()
        .ok_or_else(|| GatewayError::InvalidConfig("导入智能体缺少凭据属主。".to_owned()))?;
    let row = credentials::get_personal_by_name(pool, credential_name, owner_id)
        .await?
        .ok_or_else(|| {
            GatewayError::InvalidConfig("导入智能体的隔离运行凭据不存在。".to_owned())
        })?;
    resolved.credential = credential_from_row(state, row)?;
    Ok(resolved)
}

pub(crate) async fn resolve_runtime_for_session(
    pool: &PgPool,
    state: &AppState,
    runtime: &str,
    session: &SessionRow,
) -> Result<ResolvedRuntime, GatewayError> {
    let Some(agent_id) = session.agent_id.as_deref() else {
        return resolve_runtime(pool, state, runtime).await;
    };
    let Some(agent) = crate::db::managed_agents::registry::repository::get(pool, agent_id).await?
    else {
        return resolve_runtime(pool, state, runtime).await;
    };
    resolve_runtime_for_agent(pool, state, runtime, &agent).await
}

/// Loads and decrypts a DB-registered harness's api_base/api_key. Shared by
/// runtime resolution and specs that don't go through the SDK adapter (e.g.
/// generic_chat, whose "runtime" is the gateway itself).
pub(crate) async fn harness_credential(
    pool: &PgPool,
    state: &AppState,
    alias: &str,
) -> Result<RuntimeCredential, GatewayError> {
    let cred_name = harness_credential_name(alias);
    let row = credentials::get_by_name(pool, &cred_name)
        .await?
        .ok_or_else(|| {
            GatewayError::InvalidJsonMessage(format!("no credential for harness: {alias}"))
        })?;

    let key =
        credential_crypto::encryption_key(state.config.general_settings.master_key.as_deref())?;
    let values = row.credential_values.as_object().ok_or_else(|| {
        GatewayError::InvalidConfig("harness credential_values must be an object".to_owned())
    })?;
    let api_key = decrypt_field(values, "api_key", &key)?;
    let api_base = decrypt_field(values, "api_base", &key)?;
    Ok(RuntimeCredential { api_key, api_base })
}

pub(crate) fn harness_credential_name(alias: &str) -> String {
    format!("runtime-harness:{alias}")
}

fn decrypt_field(
    values: &serde_json::Map<String, serde_json::Value>,
    field: &str,
    key: &str,
) -> Result<String, GatewayError> {
    let enc = values.get(field).and_then(|v| v.as_str()).ok_or_else(|| {
        GatewayError::InvalidConfig(format!("harness credential missing field: {field}"))
    })?;
    credential_crypto::decrypt_value(enc, key)
}

fn credential_from_row(
    state: &AppState,
    row: credentials::CredentialRow,
) -> Result<RuntimeCredential, GatewayError> {
    let key =
        credential_crypto::encryption_key(state.config.general_settings.master_key.as_deref())?;
    let values = row.credential_values.as_object().ok_or_else(|| {
        GatewayError::InvalidConfig("credential_values must be an object".to_owned())
    })?;
    Ok(RuntimeCredential {
        api_key: decrypt_field(values, "api_key", &key)?,
        api_base: decrypt_field(values, "api_base", &key)?,
    })
}
