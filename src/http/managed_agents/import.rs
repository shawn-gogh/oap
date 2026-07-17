use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::db::managed_agents::harnesses;
use crate::{
    db::{
        credentials,
        managed_agents::{
            audit,
            governance::{self, ImportedSource},
            registry::{
                repository, revisions,
                schema::{CreateManagedAgent, UpdateManagedAgent},
            },
            sources::repository as source_repository,
        },
    },
    errors::GatewayError,
    http::managed_agents::import_types::{
        provider_error, CredentialMode, DiscoverAgentsRequest, DiscoverAgentsResponse,
        ExternalAgent, ImportAgent, ImportAgentsRequest, ImportAgentsResponse, ImportItemResult,
        ImportPreviewItem, ImportPreviewResponse, ImportProviderResponse,
    },
    proxy::{
        auth::master_key::{authenticate, require_any_gateway_key, AuthContext},
        credential_crypto,
        state::AppState,
    },
    sdk::providers::{
        a2a_import_agents::A2A_IMPORT_AGENTS,
        acp_import_agents::ACP_IMPORT_AGENTS,
        dify_import_agents::DIFY_IMPORT_AGENTS,
        elastic::import_agents::ELASTIC_IMPORT_AGENTS,
        import_agents::{ImportAgentsProvider, ImportedAgent},
        openapi_import_agents::OPENAPI_IMPORT_AGENTS,
        opencode_import_agents::OPENCODE_IMPORT_AGENTS,
    },
};

pub async fn discover(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(provider_id): Path<String>,
    Json(input): Json<DiscoverAgentsRequest>,
) -> Result<Json<DiscoverAgentsResponse>, GatewayError> {
    require_any_gateway_key(&headers, &state).await?;
    let provider = resolve_provider(&state, &provider_id).await?;
    let endpoint = normalize_endpoint(&input.endpoint)?;
    super::source_management::validate_connector_endpoint(&endpoint).await?;
    let agents = provider
        .discover(&state.http, &endpoint, input.api_key.trim())
        .await
        .map_err(provider_error)?
        .into_iter()
        .map(ExternalAgent::from)
        .collect();
    Ok(Json(DiscoverAgentsResponse { agents }))
}

pub async fn import(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(provider_id): Path<String>,
    Json(input): Json<ImportAgentsRequest>,
) -> Result<(StatusCode, Json<ImportAgentsResponse>), GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    if input.agents.is_empty() {
        return Err(GatewayError::InvalidJsonMessage(
            "at least one agent is required".to_owned(),
        ));
    }
    let provider = resolve_provider(&state, &provider_id).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let endpoint = normalize_endpoint(&input.endpoint)?;
    super::source_management::validate_connector_endpoint(&endpoint).await?;
    let owner_id = owner_id_for_import(&input, &auth);
    let api_key = input.api_key.as_deref().map(str::trim);
    let credential_mode = input.credential_mode;
    validate_credential_mode(&credential_mode, &auth)?;
    let runtime = runtime_for_import(&state, provider, &provider_id, &endpoint).await;
    // Materialize a connector for this platform as a by-product of the import
    // (reused if one already exists), so directly imported agents get the
    // connector-backed capabilities — webhook push, credential reuse — without
    // the user ever configuring a "connector". ensure_source links sources to
    // it automatically by owner/provider/endpoint.
    super::source_management::ensure_connector_for_import(
        &state, pool, &owner_id, provider, &endpoint, api_key,
    )
    .await?;

    let mut rows = Vec::with_capacity(input.agents.len());
    let mut results = Vec::with_capacity(input.agents.len());
    for agent in input.agents {
        // Normalize the identity key: it is the dedupe key in governance, so
        // whitespace variants must not mint distinct (or colliding) agents.
        let agent = ImportAgent {
            external_id: agent.external_id.trim().to_owned(),
            ..agent
        };
        // Enforce the same blocking rules preview reports: a broken identity
        // or unusable execution contract must not enter the registry, even
        // when the caller skips preview and posts to import directly.
        let issues = import_issues(provider, &agent);
        if !blocking_issues(&issues).is_empty() {
            results.push(ImportItemResult {
                external_id: agent.external_id,
                agent_id: None,
                status: "blocked",
                snapshot_id: None,
                issues: Value::Array(issues),
            });
            continue;
        }
        let external_agent_id = agent.external_id.clone();
        let source_hash = source_hash(&agent)?;
        let existing = governance::find_by_source(
            pool,
            &owner_id,
            provider.id(),
            &endpoint,
            &external_agent_id,
        )
        .await?;
        let unchanged = existing
            .as_ref()
            .is_some_and(|governance| governance.source_hash == source_hash);
        // Re-importing an already governed agent with a *changed* definition
        // must not overwrite it directly — that would bypass drift review.
        // Route the change through the same candidate-snapshot path source
        // sync uses; the operator then accepts or rejects it explicitly.
        if let Some(existing) = existing.as_ref().filter(|_| !unchanged) {
            let row = repository::get(pool, &existing.agent_id)
                .await?
                .ok_or_else(|| GatewayError::NotFound("imported agent not found".to_owned()))?;
            let source =
                source_repository::ensure_source(pool, existing, "federated", None).await?;
            let remote = ImportedAgent {
                id: external_agent_id.clone(),
                name: agent_name(&agent).to_owned(),
                description: agent.description.clone(),
                model: agent.model.clone(),
                provider: provider.id().to_owned(),
                raw: agent.raw.clone().unwrap_or_else(|| json!({})),
            };
            let snapshot = super::source_management::record_drift_candidate(
                pool,
                &row,
                provider,
                &source,
                &remote,
                &source_hash,
                &auth.user_id,
            )
            .await?;
            audit::record(
                pool,
                &auth.user_id,
                "agent.source.drift_candidate",
                "agent",
                &row.id,
                json!({
                    "provider": provider.id(),
                    "endpoint": endpoint,
                    "external_agent_id": external_agent_id,
                    "snapshot_id": snapshot.id,
                    "via": "import",
                }),
            )
            .await?;
            results.push(ImportItemResult {
                external_id: external_agent_id,
                agent_id: Some(row.id.clone()),
                status: "drift_pending",
                snapshot_id: Some(snapshot.id),
                issues: snapshot.normalization_issues,
            });
            rows.push(row);
            continue;
        }
        let create = create_input(
            &state,
            provider,
            &endpoint,
            &owner_id,
            &credential_mode,
            api_key,
            &runtime,
            agent,
        )
        .await?;
        let credential_name = create
            .config
            .as_ref()
            .and_then(|config| config.pointer("/source/credential_name"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        // Changed existing agents were diverted to drift review above, so
        // only "unchanged re-import" and "first import" reach this point.
        let row = match existing.as_ref() {
            Some(existing) => repository::get(pool, &existing.agent_id)
                .await?
                .ok_or_else(|| GatewayError::NotFound("imported agent not found".to_owned()))?,
            None => repository::create(pool, create).await?,
        };
        let revision = if unchanged {
            match revisions::latest_version(pool, &row.id).await? {
                Some(version) => version,
                None => revisions::record(pool, &row, Some(&auth.user_id)).await?,
            }
        } else {
            revisions::record(pool, &row, Some(&auth.user_id)).await?
        };
        let governance = governance::record_import(
            pool,
            ImportedSource {
                agent_id: &row.id,
                owner_id: &owner_id,
                provider: provider.id(),
                endpoint: &endpoint,
                external_agent_id: &external_agent_id,
                source_hash: &source_hash,
                credential_scope: if matches!(credential_mode, CredentialMode::Shared) {
                    "personal"
                } else {
                    "byo"
                },
                credential_name: credential_name.as_deref(),
            },
        )
        .await?;
        let source = source_repository::ensure_source(pool, &governance, "federated", None).await?;
        let raw_spec = row
            .config
            .pointer("/source/raw")
            .cloned()
            .unwrap_or_else(|| {
                row.config
                    .get("source")
                    .cloned()
                    .unwrap_or_else(|| json!({}))
            });
        let snapshot = source_repository::record_import_snapshot(
            pool,
            &source,
            &row,
            raw_spec,
            &source_hash,
            revision,
            &auth.user_id,
        )
        .await?;
        audit::record(
            pool,
            &auth.user_id,
            if unchanged {
                "agent.source.checked"
            } else {
                "agent.source.imported"
            },
            "agent",
            &row.id,
            json!({
                "provider": provider.id(),
                "endpoint": endpoint,
                "external_agent_id": external_agent_id,
                "source_version": governance.source_version,
                "revision": revision,
                "source_id": source.id,
                "snapshot_id": snapshot.id,
                "changed": !unchanged,
            }),
        )
        .await?;
        results.push(ImportItemResult {
            external_id: external_agent_id,
            agent_id: Some(row.id.clone()),
            status: if unchanged { "unchanged" } else { "imported" },
            snapshot_id: Some(snapshot.id),
            issues: snapshot.normalization_issues,
        });
        rows.push(row);
    }

    // Every requested agent was rejected — surface a hard failure instead of
    // a 201 that quietly imported nothing.
    if rows.is_empty() {
        let messages = results
            .iter()
            .flat_map(|result| result.issues.as_array().into_iter().flatten())
            .filter_map(|issue| issue.get("message").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("；");
        return Err(GatewayError::BadRequest(format!(
            "所有智能体均无法导入：{messages}"
        )));
    }
    Ok((
        StatusCode::CREATED,
        Json(ImportAgentsResponse {
            agents: rows,
            results,
        }),
    ))
}

pub async fn preview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(provider_id): Path<String>,
    Json(input): Json<ImportAgentsRequest>,
) -> Result<Json<ImportPreviewResponse>, GatewayError> {
    authenticate(&headers, &state).await?;
    let provider = resolve_provider(&state, &provider_id).await?;
    let endpoint = normalize_endpoint(&input.endpoint)?;
    super::source_management::validate_connector_endpoint(&endpoint).await?;
    let items = input
        .agents
        .into_iter()
        .map(|agent| preview_item(provider, &endpoint, agent))
        .collect();
    Ok(Json(ImportPreviewResponse { items }))
}

/// Issue detection shared by `preview` and `import`: the same rules that mark
/// a preview item non-importable must also be enforced at import time —
/// otherwise calling `import` directly (skipping preview) smuggles in agents
/// with a broken identity or an unusable execution contract.
fn import_issues(provider: &dyn ImportAgentsProvider, agent: &ImportAgent) -> Vec<Value> {
    let mut issues = Vec::new();
    if agent.external_id.trim().is_empty() {
        issues.push(json!({
            "severity": "blocking",
            "code": "identity_missing",
            "field": "identity.external_agent_id",
            "message": "来源智能体缺少稳定身份。"
        }));
    }
    if let Some(raw) = agent.raw.as_ref().and_then(Value::as_object) {
        for field in [
            "credentials",
            "secrets",
            "permissions",
            "network",
            "filesystem",
            "side_effects",
            "data_egress",
            "subagents",
        ] {
            if raw.contains_key(field) {
                issues.push(json!({
                    "severity": "approval_required",
                    "code": "unmapped_high_risk_field",
                    "field": format!("source.raw.{field}"),
                    "message": "高风险来源字段需要人工映射与审批。"
                }));
            }
        }
    }
    let raw = agent.raw.as_ref().cloned().unwrap_or(Value::Null);
    match provider.id() {
        "a2a" if raw.get("url").and_then(Value::as_str).is_none() => {
            issues.push(json!({
                "severity": "blocking",
                "code": "a2a_runtime_url_missing",
                "field": "source.raw.url",
                "message": "A2A Agent Card 缺少运行端点 URL。"
            }));
        }
        "dify"
            if raw
                .get("mode")
                .and_then(Value::as_str)
                .is_some_and(|mode| mode.contains("workflow")) =>
        {
            issues.push(json!({
                "severity": "approval_required",
                "code": "dify_workflow_mapping_required",
                "field": "execution.input_mapping",
                "message": "Dify 工作流必须确认输入映射后才能执行。"
            }));
        }
        "openapi" if raw.get("x-lap-runtime").is_none() => {
            issues.push(json!({
                "severity": "approval_required",
                "code": "openapi_runtime_mapping_required",
                "field": "source.raw.x-lap-runtime",
                "message": "OpenAPI 来源可进入资产清单，但执行前必须确认请求和响应映射。"
            }));
        }
        "acp" => {
            issues.push(json!({
                "severity": "approval_required",
                "code": "acp_profile_pin_required",
                "field": "execution.compatibility_profile",
                "message": "ACP 实现差异较大，执行前必须固定兼容配置并通过一致性测试。"
            }));
        }
        _ => {}
    }
    issues
}

fn blocking_issues(issues: &[Value]) -> Vec<&Value> {
    issues
        .iter()
        .filter(|issue| issue.get("severity").and_then(Value::as_str) == Some("blocking"))
        .collect()
}

fn preview_item(
    provider: &dyn ImportAgentsProvider,
    endpoint: &str,
    agent: ImportAgent,
) -> ImportPreviewItem {
    let issues = import_issues(provider, &agent);
    let can_import = blocking_issues(&issues).is_empty();
    ImportPreviewItem {
        external_id: agent.external_id.clone(),
        canonical_spec: json!({
            "spec_version": crate::sdk::agents::canonical::CANONICAL_SPEC_VERSION,
            "identity": {
                "external_agent_id": agent.external_id,
                "source_provider": provider.id(),
                "name": agent.name,
                "description": agent.description,
            },
            "execution": {
                "runtime_contract": provider.api_spec(),
                "model": provider.default_model(agent.model.as_deref()),
            },
            "source": { "endpoint": endpoint },
        }),
        issues: Value::Array(issues),
        can_import,
    }
}

pub(crate) fn update_from_import(input: CreateManagedAgent) -> UpdateManagedAgent {
    UpdateManagedAgent {
        name: Some(input.name),
        model: input.model,
        tools: input.tools,
        runtime: input.runtime,
        system: input.system,
        prompt: input.prompt,
        cron: input
            .schedule
            .as_ref()
            .map(|schedule| schedule.cron.clone()),
        timezone: input.schedule.and_then(|schedule| schedule.timezone),
        vault_keys: input.vault_keys,
        setup_commands: input.setup_commands,
        max_runtime_minutes: input.max_runtime_minutes,
        on_failure: input.on_failure,
        config: input.config,
        owner_id: None,
        status: Some("draft".to_owned()),
        description: input.description,
        harness: input.harness,
        skill_ids: input.skill_ids,
        rule_ids: input.rule_ids,
    }
}

pub(crate) fn source_hash(agent: &ImportAgent) -> Result<String, GatewayError> {
    let value = json!({
        "external_id": agent.external_id,
        "name": agent.name,
        "description": agent.description,
        "model": agent.model,
        "raw": agent.raw,
    });
    let bytes = serde_json::to_vec(&value)
        .map_err(|error| GatewayError::InvalidConfig(error.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(crate) fn import_runtime_providers() -> Vec<ImportProviderResponse> {
    provider_registry()
        .into_iter()
        .map(|provider| ImportProviderResponse {
            id: provider.id(),
            name: provider.name(),
            api_spec: provider.api_spec(),
            capabilities: provider.capabilities(),
            expose_runtime_harness: provider.expose_runtime_harness(),
        })
        .collect()
}

pub async fn list_providers(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<ImportProviderResponse>>, GatewayError> {
    require_any_gateway_key(&headers, &state).await?;
    Ok(Json(import_runtime_providers()))
}

fn provider_registry() -> Vec<&'static dyn ImportAgentsProvider> {
    vec![
        &A2A_IMPORT_AGENTS,
        &ACP_IMPORT_AGENTS,
        &DIFY_IMPORT_AGENTS,
        &OPENAPI_IMPORT_AGENTS,
        &ELASTIC_IMPORT_AGENTS,
        &OPENCODE_IMPORT_AGENTS,
    ]
}

pub(crate) fn provider_for_id(
    provider_id: &str,
) -> Result<&'static dyn ImportAgentsProvider, GatewayError> {
    provider_registry()
        .into_iter()
        .find(|provider| provider.id() == provider_id || provider.api_spec() == provider_id)
        .ok_or_else(|| GatewayError::NotFound(format!("import provider not found: {provider_id}")))
}

/// Resolve an import provider from the path id used by the UI.
///
/// The import dialog lists runtime harnesses and posts the chosen harness
/// **alias** (e.g. `local-opencode`). A custom harness's alias matches neither
/// an import provider id nor an api_spec, so fall back to looking the harness up
/// and matching the import provider by its api_spec (e.g. opencode's
/// `claude_managed_agents`). Built-in runtimes whose alias already equals their
/// api_spec (e.g. `elastic_agent_builder`) resolve on the first try.
async fn resolve_provider(
    state: &AppState,
    provider_id: &str,
) -> Result<&'static dyn ImportAgentsProvider, GatewayError> {
    if let Ok(provider) = provider_for_id(provider_id) {
        return Ok(provider);
    }
    if let Some(pool) = state.db.as_ref() {
        if let Some(harness) = harnesses::repository::get_by_alias(pool, provider_id).await? {
            return provider_for_id(&harness.api_spec);
        }
    }
    Err(GatewayError::NotFound(format!(
        "import provider not found: {provider_id}"
    )))
}

#[allow(clippy::too_many_arguments)]
async fn create_input(
    state: &AppState,
    provider: &dyn ImportAgentsProvider,
    endpoint: &str,
    owner_id: &str,
    credential_mode: &CredentialMode,
    api_key: Option<&str>,
    runtime: &str,
    agent: ImportAgent,
) -> Result<CreateManagedAgent, GatewayError> {
    let credential_name = credential_name_for_agent(
        state,
        provider,
        endpoint,
        owner_id,
        credential_mode,
        api_key,
        &agent,
    )
    .await?;
    let raw = agent.raw.clone().unwrap_or(Value::Null);
    let system = provider.system_prompt_from_raw(&agent.external_id, &raw);
    Ok(CreateManagedAgent {
        name: agent_name(&agent).to_owned(),
        owner_id: owner_id.to_owned(),
        description: agent.description.clone(),
        runtime: Some(runtime.to_owned()),
        harness: Some("claude-code".to_owned()),
        prompt: Some(system.clone()),
        tools: Some(json!([])),
        schedule: None,
        vault_keys: Some(json!([])),
        setup_commands: Some(json!([])),
        max_runtime_minutes: Some(30),
        on_failure: Some("pause_and_notify".to_owned()),
        config: Some(agent_config(
            provider,
            endpoint,
            &agent,
            credential_mode,
            credential_name,
            runtime,
        )),
        model: Some(provider.default_model(agent.model.as_deref())),
        system: Some(system),
        skill_ids: Some(json!([])),
        rule_ids: Some(json!([])),
    })
}

async fn credential_name_for_agent(
    state: &AppState,
    provider: &dyn ImportAgentsProvider,
    endpoint: &str,
    owner_id: &str,
    credential_mode: &CredentialMode,
    api_key: Option<&str>,
    agent: &ImportAgent,
) -> Result<Option<String>, GatewayError> {
    if !matches!(credential_mode, CredentialMode::Shared) {
        return Ok(None);
    }
    let api_key = shared_api_key(api_key)?;
    let credential_name = provider_credential_name(provider.id(), &agent.external_id);
    save_provider_credential(
        state,
        provider,
        &credential_name,
        endpoint,
        api_key,
        owner_id,
    )
    .await?;
    Ok(Some(credential_name))
}

fn shared_api_key(api_key: Option<&str>) -> Result<&str, GatewayError> {
    api_key.filter(|value| !value.is_empty()).ok_or_else(|| {
        GatewayError::InvalidJsonMessage("api_key is required for shared credentials".to_owned())
    })
}

fn owner_id_for_import(input: &ImportAgentsRequest, auth: &AuthContext) -> String {
    if auth.is_admin {
        if let Some(owner_id) = input
            .owner_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return owner_id.to_owned();
        }
    }
    auth.user_id.clone()
}

fn validate_credential_mode(
    credential_mode: &CredentialMode,
    auth: &AuthContext,
) -> Result<(), GatewayError> {
    if matches!(credential_mode, CredentialMode::Shared) && !auth.is_admin {
        return Err(GatewayError::Unauthorized);
    }
    Ok(())
}

fn agent_name(agent: &ImportAgent) -> &str {
    agent
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(agent.external_id.as_str())
}

/// Resolve which runtime an imported agent should default to.
///
/// Custom-harness runtimes (e.g. opencode) are registered at runtime with a
/// user-chosen alias rather than a static api_spec. Resolution order:
///   1. the path id the UI sent, if it is itself a registered harness alias
///      (the import dialog posts the picked harness's alias) — most precise;
///   2. a harness whose api_base matches the import endpoint;
///   3. the provider's api_spec (the built-in static runtimes, e.g. Elastic).
async fn runtime_for_import(
    state: &AppState,
    provider: &dyn ImportAgentsProvider,
    provider_id: &str,
    endpoint: &str,
) -> String {
    if let Some(pool) = state.db.as_ref() {
        if let Ok(Some(row)) = harnesses::repository::get_by_alias(pool, provider_id).await {
            return row.alias;
        }
        if let Ok(rows) = harnesses::repository::list(pool).await {
            if let Some(row) = rows
                .into_iter()
                .find(|row| row.api_base.trim_end_matches('/') == endpoint)
            {
                return row.alias;
            }
        }
    }
    provider.api_spec().to_owned()
}

fn agent_config(
    provider: &dyn ImportAgentsProvider,
    endpoint: &str,
    agent: &ImportAgent,
    credential_mode: &CredentialMode,
    credential_name: Option<String>,
    runtime: &str,
) -> Value {
    let mut config = json!({
        "runtime": runtime,
        "runtime_capabilities": {
            "session_workspace": provider.requires_session_workspace()
        },
        "source": source_config(provider, endpoint, agent, credential_mode, credential_name),
    });
    if provider.api_spec() == "elastic_agent_builder" {
        config["elastic_agent_id"] = agent.external_id.clone().into();
    }
    config
}

fn source_config(
    provider: &dyn ImportAgentsProvider,
    endpoint: &str,
    agent: &ImportAgent,
    credential_mode: &CredentialMode,
    credential_name: Option<String>,
) -> Value {
    json!({
        "kind": "external_agent",
        "provider": provider.id(),
        "provider_name": provider.name(),
        "api_spec": provider.api_spec(),
        "endpoint": endpoint,
        "external_agent_id": agent.external_id,
        "credential_mode": credential_mode.as_str(),
        "credential_name": credential_name,
        "raw": agent.raw.clone().unwrap_or_else(|| json!({}))
    })
}

fn credential_info(provider: &dyn ImportAgentsProvider) -> Value {
    json!({
        "custom_llm_provider": provider.id(),
        "source": "agent-import",
        "api_spec": provider.api_spec(),
    })
}

pub(crate) fn normalize_endpoint(endpoint: &str) -> Result<String, GatewayError> {
    let trimmed = endpoint.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(GatewayError::InvalidJsonMessage(
            "endpoint is required".to_owned(),
        ));
    }
    let url = reqwest::Url::parse(trimmed)
        .map_err(|_| GatewayError::InvalidJsonMessage("endpoint must be a valid URL".to_owned()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(GatewayError::InvalidJsonMessage(
            "endpoint must use http or https".to_owned(),
        ));
    }
    Ok(trimmed.to_owned())
}

fn provider_credential_name(provider_id: &str, external_agent_id: &str) -> String {
    format!("provider:{provider_id}:agent:{external_agent_id}")
}

async fn save_provider_credential(
    state: &AppState,
    provider: &dyn ImportAgentsProvider,
    credential_name: &str,
    endpoint: &str,
    api_key: &str,
    owner_id: &str,
) -> Result<(), GatewayError> {
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let key =
        credential_crypto::encryption_key(state.config.general_settings.master_key.as_deref())?;
    credentials::upsert_personal(
        pool,
        credential_name,
        owner_id,
        json!({
            "api_key": credential_crypto::encrypt_value(api_key, &key)?,
            "api_base": credential_crypto::encrypt_value(endpoint, &key)?,
        }),
        credential_info(provider),
        owner_id,
    )
    .await
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;
