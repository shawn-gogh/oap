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
    pub adapter_id: String,
    pub protocol: String,
    pub protocol_version: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeDescriptor {
    pub adapter_id: String,
    pub protocol: String,
    pub protocol_version: String,
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
                adapter_id: entry.id.to_owned(),
                protocol: entry.id.to_owned(),
                protocol_version: entry.adapter.protocol_version().to_owned(),
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
        adapter_id: harness.api_spec.clone(),
        protocol: harness.api_spec,
        protocol_version: entry.adapter.protocol_version().to_owned(),
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
    if let Some(provider) = source
        .and_then(|value| value.get("provider"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        resolved.adapter_id = provider.to_owned();
    }
    if let Some(protocol) = source
        .and_then(|value| value.get("api_spec"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        resolved.protocol = protocol.to_owned();
    }
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

pub(crate) async fn describe_session_runtime(
    pool: &PgPool,
    session: &SessionRow,
) -> Result<RuntimeDescriptor, GatewayError> {
    let Some(runtime) = session.runtime.as_deref() else {
        return Ok(RuntimeDescriptor {
            adapter_id: "platform".to_owned(),
            protocol: "platform".to_owned(),
            protocol_version: "1".to_owned(),
        });
    };

    let registry = providers::runtime_registry();
    let (mut adapter_id, mut protocol, protocol_version) =
        if let Some(entry) = registry.entry_for_id(runtime) {
            (
                entry.id.to_owned(),
                entry.id.to_owned(),
                entry.adapter.protocol_version().to_owned(),
            )
        } else if let Some(harness) = harnesses::repository::get_by_alias(pool, runtime).await? {
            let version = registry
                .entry_for_id(&harness.api_spec)
                .map(|entry| entry.adapter.protocol_version())
                .unwrap_or("unverified");
            (
                harness.api_spec.clone(),
                harness.api_spec,
                version.to_owned(),
            )
        } else {
            (
                runtime.to_owned(),
                runtime.to_owned(),
                "unverified".to_owned(),
            )
        };

    if let Some(agent_id) = session.agent_id.as_deref() {
        if let Some(agent) =
            crate::db::managed_agents::registry::repository::get(pool, agent_id).await?
        {
            if let Some(source) = agent.config.get("source") {
                if let Some(provider) = source
                    .get("provider")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    adapter_id = provider.to_owned();
                }
                if let Some(api_spec) = source
                    .get("api_spec")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    protocol = api_spec.to_owned();
                }
            }
        }
    }

    Ok(RuntimeDescriptor {
        adapter_id,
        protocol,
        protocol_version,
    })
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
    let lease = issue_runtime_credential_lease(pool, &agent, session).await?;
    let resolved = resolve_runtime_for_agent(pool, state, runtime, &agent).await?;
    mark_lease_resolved(pool, lease.as_ref()).await?;
    Ok(resolved)
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

/// Personal-credential name holding a user's own API key for a BYO imported
/// agent. One row per (agent, user): every session runs with the key of its
/// own owner, never a key shared across users.
pub(crate) fn byo_credential_name(agent_id: &str) -> String {
    format!("agent_byo:{agent_id}")
}

pub(crate) async fn imported_agent_credential(
    pool: &PgPool,
    state: &AppState,
    agent: &ManagedAgentRow,
    session: &SessionRow,
) -> Result<RuntimeCredential, GatewayError> {
    let source = agent
        .config
        .get("source")
        .ok_or_else(|| GatewayError::InvalidConfig("导入智能体缺少来源配置。".to_owned()))?;
    let endpoint = source
        .get("endpoint")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| GatewayError::InvalidConfig("导入智能体缺少来源端点。".to_owned()))?;
    let credential_mode = source
        .get("credential_mode")
        .and_then(serde_json::Value::as_str);
    let mut credential = match credential_mode {
        Some("shared") => shared_imported_credential(pool, state, agent, session, source).await?,
        Some("byo") => byo_imported_credential(pool, state, agent, session).await?,
        other => {
            return Err(GatewayError::InvalidConfig(format!(
                "导入智能体的凭据模式无效：{}。",
                other.unwrap_or("<missing>")
            )));
        }
    };
    credential.api_base = endpoint.to_owned();
    Ok(credential)
}

async fn shared_imported_credential(
    pool: &PgPool,
    state: &AppState,
    agent: &ManagedAgentRow,
    session: &SessionRow,
    source: &serde_json::Value,
) -> Result<RuntimeCredential, GatewayError> {
    let lease = issue_runtime_credential_lease(pool, agent, session)
        .await?
        .ok_or_else(|| GatewayError::InvalidConfig("当前调用没有可用的凭据租约。".to_owned()))?;
    let credential_name = source
        .get("credential_name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| GatewayError::InvalidConfig("导入智能体缺少凭据引用。".to_owned()))?;
    let owner_id = agent
        .owner_id
        .as_deref()
        .ok_or_else(|| GatewayError::InvalidConfig("导入智能体缺少凭据属主。".to_owned()))?;
    let row = credentials::get_personal_by_name(pool, credential_name, owner_id)
        .await?
        .ok_or_else(|| GatewayError::InvalidConfig("导入智能体的运行凭据不存在。".to_owned()))?;
    let credential = credential_from_row(state, row)?;
    mark_lease_resolved(pool, Some(&lease)).await?;
    Ok(credential)
}

/// BYO: the session owner brings their own key, stored as a personal
/// credential (`agent_byo:{agent_id}`) via the byo-credential API.
async fn byo_imported_credential(
    pool: &PgPool,
    state: &AppState,
    agent: &ManagedAgentRow,
    session: &SessionRow,
) -> Result<RuntimeCredential, GatewayError> {
    let user_id = session
        .owner_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            GatewayError::InvalidConfig(
                "该导入智能体使用 BYO 凭据，但当前会话没有属主，无法解析个人密钥。".to_owned(),
            )
        })?;
    let row = credentials::get_personal_by_name(pool, &byo_credential_name(&agent.id), user_id)
        .await?
        .ok_or_else(|| {
            GatewayError::InvalidConfig(
                "该导入智能体使用 BYO 凭据：请先在智能体页面为你的账户配置 API Key。".to_owned(),
            )
        })?;
    credential_from_row(state, row)
}

const RUNTIME_CREDENTIAL_LEASE_TTL_MS: i64 = 5 * 60 * 1_000;

async fn issue_runtime_credential_lease(
    pool: &PgPool,
    agent: &ManagedAgentRow,
    session: &SessionRow,
) -> Result<
    Option<crate::db::managed_agents::credential_leases::schema::CredentialLeaseRow>,
    GatewayError,
> {
    let Some(source) = agent.config.get("source") else {
        return Ok(None);
    };
    if source
        .get("credential_mode")
        .and_then(serde_json::Value::as_str)
        != Some("shared")
    {
        return Ok(None);
    }
    let credential_name = source
        .get("credential_name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| GatewayError::InvalidConfig("导入智能体缺少凭据引用。".to_owned()))?;
    let owner_id = agent
        .owner_id
        .as_deref()
        .ok_or_else(|| GatewayError::InvalidConfig("导入智能体缺少凭据属主。".to_owned()))?;
    let snapshot =
        crate::db::managed_agents::session_control::repository::active_turn(pool, &session.id)
            .await?
            .ok_or_else(|| {
                GatewayError::InvalidConfig("当前会话没有活动调用，无法签发凭据租约。".to_owned())
            })?;
    let invocation = snapshot
        .invocations
        .iter()
        .find(|invocation| invocation.role == "primary")
        .ok_or_else(|| GatewayError::InvalidConfig("当前 Turn 缺少主 Invocation。".to_owned()))?;
    let lease = crate::db::managed_agents::credential_leases::repository::issue(
        pool,
        crate::db::managed_agents::credential_leases::repository::NewCredentialLease {
            owner_id,
            session_id: &session.id,
            turn_id: &snapshot.turn.id,
            invocation_id: &invocation.id,
            credential_name,
            adapter_id: &invocation.adapter_id,
            purpose: "agent_runtime",
            ttl_ms: RUNTIME_CREDENTIAL_LEASE_TTL_MS,
            metadata: serde_json::json!({"protocol": invocation.protocol}),
        },
    )
    .await?;
    Ok(Some(lease))
}

async fn mark_lease_resolved(
    pool: &PgPool,
    lease: Option<&crate::db::managed_agents::credential_leases::schema::CredentialLeaseRow>,
) -> Result<(), GatewayError> {
    let Some(lease) = lease else {
        return Ok(());
    };
    if !crate::db::managed_agents::credential_leases::repository::mark_resolved(
        pool,
        &lease.id,
        &lease.owner_id,
        crate::db::managed_agents::now_ms(),
    )
    .await?
    {
        return Err(GatewayError::InvalidConfig(
            "凭据租约已过期或已撤销。".to_owned(),
        ));
    }
    Ok(())
}
