use std::{net::IpAddr, sync::Arc, time::Instant};

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Sha256;

use crate::{
    db::{
        credentials,
        managed_agents::{
            audit, governance,
            registry::{repository, revisions, schema::UpdateManagedAgent},
            sources::{repository as sources, schema::*},
        },
    },
    errors::GatewayError,
    proxy::{
        auth::master_key::{authenticate, AuthContext},
        credential_crypto,
        state::AppState,
    },
    sdk::{
        agents::{
            canonical::{normalize_agent, CanonicalAgentSpec},
            conformance::inspect_runtime_contract,
        },
        providers::import_agents::{ImportAgentsProvider, ImportedAgent},
    },
};

use super::{
    import::{provider_for_id, source_hash},
    import_types::ImportAgent,
    registry::preflight::run_preflight,
};

const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const SYNC_LEASE_MS: i64 = 60_000;
const MISSING_THRESHOLD: i32 = 3;
/// Consecutive failed health-check runs required before an active agent is
/// auto-paused (mirrors MISSING_THRESHOLD's rationale for source sync).
const HEALTH_PAUSE_THRESHOLD: i64 = 3;

#[derive(Debug, Deserialize)]
pub struct DriftResolutionRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateConnectorRequest {
    pub name: String,
    pub provider: String,
    pub endpoint: String,
    pub credential_name: Option<String>,
    pub api_key: Option<String>,
    pub webhook_secret: Option<String>,
}

pub async fn list_connectors(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<SourceConnectorRow>>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    Ok(Json(
        sources::list_connectors(pool, (!auth.is_admin).then_some(auth.user_id.as_str())).await?,
    ))
}

pub async fn create_connector(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<CreateConnectorRequest>,
) -> Result<Json<SourceConnectorRow>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let provider = provider_for_id(input.provider.trim())?;
    let name = required(&input.name, "name")?;
    let endpoint = validate_connector_endpoint(&input.endpoint).await?;
    let mut credential_name = input
        .credential_name
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    if input
        .api_key
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        || input
            .webhook_secret
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    {
        let generated_name = credential_name.unwrap_or_else(|| {
            format!(
                "provider:{}:connector:{}",
                provider.id(),
                uuid::Uuid::new_v4().simple()
            )
        });
        store_connector_credential(
            &state,
            pool,
            &auth.user_id,
            &generated_name,
            &endpoint,
            input.api_key.as_deref(),
            input.webhook_secret.as_deref(),
        )
        .await?;
        credential_name = Some(generated_name);
    } else {
        validate_credential_reference(pool, &auth.user_id, credential_name.as_deref()).await?;
    }
    let capabilities = serde_json::to_value(provider.capabilities())
        .map_err(|error| GatewayError::InvalidConfig(error.to_string()))?;
    let connector = sources::create_connector(
        pool,
        &auth.user_id,
        CreateSourceConnector {
            name,
            provider: provider.id().to_owned(),
            endpoint,
            credential_name,
            adapter_id: provider.id().to_owned(),
            protocol: provider.api_spec().to_owned(),
            protocol_version: provider.protocol_version().to_owned(),
        },
        capabilities,
    )
    .await?;
    audit::record(
        pool,
        &auth.user_id,
        "agent.connector.created",
        "agent_source_connector",
        &connector.id,
        json!({ "provider": connector.provider, "endpoint": connector.endpoint }),
    )
    .await?;
    Ok(Json(connector))
}

pub async fn update_connector(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(connector_id): Path<String>,
    Json(mut input): Json<UpdateSourceConnector>,
) -> Result<Json<SourceConnectorRow>, GatewayError> {
    let (pool, auth, existing) = owned_connector(&state, &headers, &connector_id).await?;
    if let Some(endpoint) = input.endpoint.as_deref() {
        input.endpoint = Some(validate_connector_endpoint(endpoint).await?);
    }
    validate_credential_reference(&pool, &existing.owner_id, input.credential_name.as_deref())
        .await?;
    let connector = sources::update_connector(&pool, &connector_id, input)
        .await?
        .ok_or_else(|| GatewayError::NotFound("connector not found".to_owned()))?;
    audit::record(
        &pool,
        &auth.user_id,
        "agent.connector.updated",
        "agent_source_connector",
        &connector.id,
        json!({ "status": connector.status }),
    )
    .await?;
    Ok(Json(connector))
}

pub async fn delete_connector(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(connector_id): Path<String>,
) -> Result<Json<Value>, GatewayError> {
    let (pool, auth, _) = owned_connector(&state, &headers, &connector_id).await?;
    if !sources::delete_connector(&pool, &connector_id).await? {
        return Err(GatewayError::NotFound("connector not found".to_owned()));
    }
    audit::record(
        &pool,
        &auth.user_id,
        "agent.connector.deleted",
        "agent_source_connector",
        &connector_id,
        json!({}),
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn test_connector(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(connector_id): Path<String>,
) -> Result<Json<SourceConnectorRow>, GatewayError> {
    let (pool, auth, connector) = owned_connector(&state, &headers, &connector_id).await?;
    let tested = test_connector_inner(&state, &pool, &connector).await?;
    audit::record(
        &pool,
        &auth.user_id,
        "agent.connector.tested",
        "agent_source_connector",
        &connector_id,
        json!({ "status": tested.status, "detail": tested.last_test_detail }),
    )
    .await?;
    Ok(Json(tested))
}

pub async fn connector_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(connector_id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, GatewayError> {
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let connector = sources::get_connector(pool, &connector_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("connector not found".to_owned()))?;
    let event_id = header(&headers, "x-lap-event-id")?;
    let timestamp = header(&headers, "x-lap-timestamp")?;
    let timestamp_ms = timestamp
        .parse::<i64>()
        .map(|value| {
            if value < 10_000_000_000 {
                value.saturating_mul(1000)
            } else {
                value
            }
        })
        .map_err(|_| GatewayError::Unauthorized)?;
    if crate::db::managed_agents::now_ms().abs_diff(timestamp_ms) > 5 * 60 * 1000 {
        return Err(GatewayError::Unauthorized);
    }
    let signature = header(&headers, "x-lap-signature")?;
    let secret = connector_webhook_secret(&state, pool, &connector).await?;
    verify_webhook_signature(&secret, &timestamp, &body, &signature)?;
    let inserted = sources::accept_webhook_delivery(pool, &connector_id, &event_id).await?;
    if inserted {
        audit::record(
            pool,
            "connector-webhook",
            "agent.connector.webhook_received",
            "agent_source_connector",
            &connector_id,
            json!({ "event_id": event_id }),
        )
        .await?;
    }
    Ok(Json(json!({ "accepted": inserted, "replayed": !inserted })))
}

pub async fn get_source(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<AgentSourceOverview>, GatewayError> {
    let (pool, _, _) = editable_agent(&state, &headers, &agent_id).await?;
    let overview = sources::overview(&pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("agent source not found".to_owned()))?;
    Ok(Json(overview))
}

pub async fn normalize_source(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, GatewayError> {
    let (_, _, agent) = editable_agent(&state, &headers, &agent_id).await?;
    let report = normalize_agent(&agent);
    Ok(Json(serde_json::to_value(report).map_err(|error| {
        GatewayError::InvalidConfig(error.to_string())
    })?))
}

pub async fn sync_source(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<AgentSourceOverview>, GatewayError> {
    let (pool, auth, agent) = editable_agent(&state, &headers, &agent_id).await?;
    let governance = governance::get(&pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("governance not found".to_owned()))?;
    let source = sources::get_source_by_agent(&pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("agent source not found".to_owned()))?;
    let worker_id = format!("manual:{}", auth.user_id);
    if !sources::acquire_sync_lease(&pool, &source.id, &worker_id, SYNC_LEASE_MS).await? {
        return Err(GatewayError::BadRequest(
            "该来源正在同步，请稍后重试。".to_owned(),
        ));
    }
    let run = sources::start_sync_run(&pool, &source, "manual").await?;
    let result = reconcile_source(&state, &pool, &auth, &agent, &governance, &source).await;
    match result {
        Ok(changed) => {
            sources::finish_sync_run(&pool, &run.id, "succeeded", i32::from(changed), 0, None)
                .await?;
            audit::record(
                &pool,
                &auth.user_id,
                "agent.source.synced",
                "agent",
                &agent_id,
                json!({ "sync_run_id": run.id, "changed": changed }),
            )
            .await?;
        }
        Err(error) => {
            sources::mark_sync_state(&pool, &source.id, "sync_error", source.missing_count).await?;
            sources::finish_sync_run(&pool, &run.id, "failed", 0, 0, Some(&error.to_string()))
                .await?;
            return Err(error);
        }
    }
    Ok(Json(
        sources::overview(&pool, &agent_id)
            .await?
            .ok_or_else(|| GatewayError::NotFound("agent source not found".to_owned()))?,
    ))
}

pub async fn accept_drift(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Json(input): Json<DriftResolutionRequest>,
) -> Result<Json<AgentSourceOverview>, GatewayError> {
    let (pool, auth, agent) = editable_agent(&state, &headers, &agent_id).await?;
    let governance = governance::get(&pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("governance not found".to_owned()))?;
    let source = sources::get_source_by_agent(&pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("agent source not found".to_owned()))?;
    let snapshot = sources::get_snapshot(&pool, source.candidate_snapshot_id.as_deref())
        .await?
        .ok_or_else(|| GatewayError::BadRequest("当前没有待处理的来源变更。".to_owned()))?;
    let canonical: CanonicalAgentSpec = serde_json::from_value(snapshot.canonical_spec.clone())
        .map_err(|error| GatewayError::InvalidConfig(error.to_string()))?;
    let mut config = agent.config.clone();
    if let Some(source_config) = config.get_mut("source").and_then(Value::as_object_mut) {
        source_config.insert("raw".to_owned(), snapshot.raw_spec.clone());
    }
    let updated = repository::update(
        &pool,
        &agent_id,
        UpdateManagedAgent {
            name: Some(canonical.identity.name),
            model: Some(canonical.execution.model),
            tools: Some(Value::Array(canonical.capabilities.tools)),
            runtime: canonical.execution.runtime,
            system: Some(canonical.instructions.system),
            prompt: canonical.instructions.prompt,
            vault_keys: Some(json!(canonical.requirements.vault_keys)),
            setup_commands: Some(json!(canonical.requirements.setup_commands)),
            max_runtime_minutes: Some(canonical.execution.max_runtime_minutes),
            on_failure: Some(canonical.execution.on_failure),
            config: Some(config),
            status: Some("draft".to_owned()),
            description: canonical.identity.description,
            harness: Some(canonical.execution.harness),
            skill_ids: Some(json!(canonical.capabilities.skill_ids)),
            rule_ids: Some(json!(canonical.capabilities.rule_ids)),
            ..UpdateManagedAgent::default()
        },
    )
    .await?
    .ok_or_else(|| GatewayError::NotFound("agent not found".to_owned()))?;
    let revision = revisions::record(&pool, &updated, Some(&auth.user_id)).await?;
    governance::record_import(
        &pool,
        governance::ImportedSource {
            agent_id: &agent_id,
            owner_id: &governance.owner_id,
            provider: &governance.source_provider,
            endpoint: &governance.source_endpoint,
            external_agent_id: &governance.external_agent_id,
            source_hash: &snapshot.digest,
            credential_scope: &governance.credential_scope,
            credential_name: governance.credential_name.as_deref(),
        },
    )
    .await?;
    sources::resolve_candidate(&pool, &source.id, &snapshot.id, "accepted", Some(revision)).await?;
    audit::record(
        &pool,
        &auth.user_id,
        "agent.drift.accepted",
        "agent",
        &agent_id,
        json!({ "snapshot_id": snapshot.id, "revision": revision, "reason": input.reason }),
    )
    .await?;
    Ok(Json(
        sources::overview(&pool, &agent_id)
            .await?
            .ok_or_else(|| GatewayError::NotFound("agent source not found".to_owned()))?,
    ))
}

pub async fn reject_drift(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Json(input): Json<DriftResolutionRequest>,
) -> Result<Json<AgentSourceOverview>, GatewayError> {
    let (pool, auth, _) = editable_agent(&state, &headers, &agent_id).await?;
    let source = sources::get_source_by_agent(&pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("agent source not found".to_owned()))?;
    let snapshot_id = source
        .candidate_snapshot_id
        .clone()
        .ok_or_else(|| GatewayError::BadRequest("当前没有待处理的来源变更。".to_owned()))?;
    sources::resolve_candidate(&pool, &source.id, &snapshot_id, "rejected", None).await?;
    audit::record(
        &pool,
        &auth.user_id,
        "agent.drift.rejected",
        "agent",
        &agent_id,
        json!({ "snapshot_id": snapshot_id, "reason": input.reason }),
    )
    .await?;
    Ok(Json(
        sources::overview(&pool, &agent_id)
            .await?
            .ok_or_else(|| GatewayError::NotFound("agent source not found".to_owned()))?,
    ))
}

pub async fn check_conformance(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<RuntimeConformanceRow>, GatewayError> {
    let (pool, auth, agent) = editable_agent(&state, &headers, &agent_id).await?;
    let report = inspect_runtime_contract(&agent);
    let revision = revisions::latest_version(&pool, &agent_id).await?;
    let checks = serde_json::to_value(&report.checks)
        .map_err(|error| GatewayError::InvalidConfig(error.to_string()))?;
    let row =
        sources::upsert_conformance(&pool, &agent_id, &report.status, checks, revision).await?;
    let source = sources::get_source_by_agent(&pool, &agent_id).await?;
    sources::record_health(
        &pool,
        &agent_id,
        source.as_ref().map(|source| source.id.as_str()),
        "conformance",
        if report.status == "conformant" {
            "healthy"
        } else {
            "degraded"
        },
        Some(&report.status),
        None,
    )
    .await?;
    audit::record(
        &pool,
        &auth.user_id,
        "agent.runtime.conformance_checked",
        "agent",
        &agent_id,
        json!({ "status": report.status, "revision": revision }),
    )
    .await?;
    Ok(Json(row))
}

pub async fn check_health(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, GatewayError> {
    let (pool, auth, agent) = editable_agent(&state, &headers, &agent_id).await?;
    let (report, latency) = run_health_check(&state, &pool, &agent).await?;
    audit::record(
        &pool,
        &auth.user_id,
        "agent.health.checked",
        "agent",
        &agent_id,
        json!({ "healthy": report.can_activate, "latency_ms": latency }),
    )
    .await?;
    Ok(Json(json!({ "preflight": report, "latency_ms": latency })))
}

pub(crate) async fn run_health_check(
    state: &Arc<AppState>,
    pool: &sqlx::PgPool,
    agent: &crate::db::managed_agents::registry::schema::ManagedAgentRow,
) -> Result<(super::registry::preflight::PreflightReport, i64), GatewayError> {
    let started = Instant::now();
    let report = run_preflight(state, pool, agent).await?;
    let source = sources::get_source_by_agent(pool, &agent.id).await?;
    let latency = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    for check in &report.checks {
        sources::record_health(
            pool,
            &agent.id,
            source.as_ref().map(|source| source.id.as_str()),
            health_kind(check.id),
            if check.verdict == "failed" {
                "unhealthy"
            } else if check.verdict == "verified" {
                "healthy"
            } else {
                "unknown"
            },
            Some(&check.detail),
            Some(latency),
        )
        .await?;
    }
    // A per-run summary record, so consecutive-failure counting has a single
    // stable check kind to look at (per-check kinds vary between runs).
    sources::record_health(
        pool,
        &agent.id,
        source.as_ref().map(|source| source.id.as_str()),
        "preflight",
        if report.can_activate {
            "healthy"
        } else {
            "unhealthy"
        },
        None,
        Some(latency),
    )
    .await?;
    // Auto-pause only after HEALTH_PAUSE_THRESHOLD consecutive failed runs:
    // a single failure is often transient (flaky MCP smoke test, network
    // blip) and shouldn't take a production agent offline by itself.
    if !report.can_activate && agent.status == "active" {
        let recent =
            sources::recent_health_statuses(pool, &agent.id, "preflight", HEALTH_PAUSE_THRESHOLD)
                .await?;
        let consecutive_failures = recent.len() as i64 >= HEALTH_PAUSE_THRESHOLD
            && recent.iter().all(|status| status == "unhealthy");
        if consecutive_failures {
            repository::set_status(pool, &agent.id, "paused").await?;
            if governance::get(pool, &agent.id).await?.is_some() {
                governance::suspend(
                    pool,
                    &agent.id,
                    &format!("连续 {HEALTH_PAUSE_THRESHOLD} 次健康检查发现阻断项，运行已暂停。"),
                )
                .await?;
            }
        }
    }
    Ok((report, latency))
}

/// Interrupts every live runtime session of the agent (best effort) before
/// the DB-level status sweep. `cancel_agent_work` alone only rewrites rows:
/// without this, remote runtimes and A2A pollers keep executing after an
/// "emergency stop", and an in-flight prompt's completion handler would even
/// overwrite the swept 'cancelled' status and resurrect the session.
async fn interrupt_agent_sessions(
    state: &Arc<AppState>,
    pool: &sqlx::PgPool,
    agent_id: &str,
    reason: &str,
) -> u64 {
    let rows = match crate::db::managed_agents::sessions::repository::list_active_for_agent(
        pool, agent_id,
    )
    .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(agent_id, %error, "failed to list active sessions for interruption");
            return 0;
        }
    };
    let mut interrupted = 0;
    for row in rows {
        match crate::http::sessions::abort_session_internal(state, pool, &row, reason).await {
            Ok(()) => interrupted += 1,
            Err(error) => {
                tracing::warn!(agent_id, session_id = %row.id, %error, "failed to interrupt session during agent stop");
            }
        }
    }
    interrupted
}

pub async fn emergency_stop(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, GatewayError> {
    let (pool, auth, _) = editable_agent(&state, &headers, &agent_id).await?;
    repository::set_status(&pool, &agent_id, "paused")
        .await?
        .ok_or_else(|| GatewayError::NotFound("agent not found".to_owned()))?;
    let interrupted = interrupt_agent_sessions(&state, &pool, &agent_id, "智能体被紧急停止").await;
    let cancelled = sources::cancel_agent_work(&pool, &agent_id).await?;
    if governance::get(&pool, &agent_id).await?.is_some() {
        governance::suspend(&pool, &agent_id, "执行了紧急停止。所有能力令牌已撤销。").await?;
    }
    audit::record(
        &pool,
        &auth.user_id,
        "agent.emergency_stopped",
        "agent",
        &agent_id,
        json!({ "cancelled_work_items": cancelled, "interrupted_sessions": interrupted }),
    )
    .await?;
    Ok(Json(
        json!({ "ok": true, "cancelled_work_items": cancelled, "interrupted_sessions": interrupted }),
    ))
}

pub async fn retire(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, GatewayError> {
    let (pool, auth, _) = editable_agent(&state, &headers, &agent_id).await?;
    let interrupted = interrupt_agent_sessions(&state, &pool, &agent_id, "智能体已退役").await;
    let cancelled = sources::cancel_agent_work(&pool, &agent_id).await?;
    repository::set_status(&pool, &agent_id, "archived_pending_delete")
        .await?
        .ok_or_else(|| GatewayError::NotFound("agent not found".to_owned()))?;
    sources::detach_source(&pool, &agent_id).await?;
    if governance::get(&pool, &agent_id).await?.is_some() {
        governance::retire(&pool, &agent_id, "智能体已退役，来源证据保留。").await?;
    }
    audit::record(
        &pool,
        &auth.user_id,
        "agent.retired",
        "agent",
        &agent_id,
        json!({ "cancelled_work_items": cancelled, "interrupted_sessions": interrupted, "evidence_preserved": true }),
    )
    .await?;
    Ok(Json(
        json!({ "ok": true, "cancelled_work_items": cancelled }),
    ))
}

pub(crate) async fn reconcile_source(
    state: &AppState,
    pool: &sqlx::PgPool,
    auth: &AuthContext,
    agent: &crate::db::managed_agents::registry::schema::ManagedAgentRow,
    governance: &governance::AgentGovernanceRow,
    source: &ManagedAgentSourceRow,
) -> Result<bool, GatewayError> {
    // Sources imported directly (no connector) still know their provider,
    // endpoint and credential via governance — sync them by discovering
    // against the endpoint itself. Previously this arm silently marked
    // in_sync, which made drift detection a no-op for every directly
    // imported agent.
    let (provider, endpoint, api_key) = match source.connector_id.as_deref() {
        Some(connector_id) => {
            let connector = sources::get_connector(pool, connector_id)
                .await?
                .ok_or_else(|| GatewayError::NotFound("source connector not found".to_owned()))?;
            if connector.status == "disabled" {
                return Err(GatewayError::BadRequest("来源连接器已停用。".to_owned()));
            }
            let api_key = connector_api_key(state, pool, &connector).await?;
            (
                provider_for_id(&connector.provider)?,
                connector.endpoint.clone(),
                api_key,
            )
        }
        None => {
            let api_key = credential_api_key(
                state,
                pool,
                governance.credential_name.as_deref(),
                &governance.owner_id,
            )
            .await?;
            (
                provider_for_id(&governance.source_provider)?,
                governance.source_endpoint.clone(),
                api_key,
            )
        }
    };
    validate_connector_endpoint(&endpoint).await?;
    let discovered = tokio::time::timeout(
        CONNECT_TIMEOUT,
        provider.discover(&state.http, &endpoint, &api_key),
    )
    .await
    .map_err(|_| GatewayError::BadRequest("来源同步连接超时。".to_owned()))?
    .map_err(super::import_types::provider_error)?;
    let Some(remote) = discovered
        .into_iter()
        .find(|remote| remote.id == governance.external_agent_id)
    else {
        let missing_count = source.missing_count.saturating_add(1);
        let state_name = if missing_count >= MISSING_THRESHOLD {
            "missing"
        } else {
            "in_sync"
        };
        sources::mark_sync_state(pool, &source.id, state_name, missing_count).await?;
        if missing_count >= MISSING_THRESHOLD {
            repository::set_status(pool, &agent.id, "paused").await?;
            governance::suspend(pool, &agent.id, "外部来源连续三次未发现该智能体。").await?;
        }
        return Ok(false);
    };
    let imported = import_agent(&remote);
    let digest = source_hash(&imported)?;
    if digest == governance.source_hash {
        sources::mark_sync_state(pool, &source.id, "in_sync", 0).await?;
        return Ok(false);
    }
    let candidate = candidate_agent(agent, provider, &remote);
    let snapshot = sources::record_candidate_snapshot(
        pool,
        source,
        &candidate,
        remote.raw,
        &digest,
        &auth.user_id,
    )
    .await?;
    let previous = sources::get_snapshot(pool, source.current_snapshot_id.as_deref()).await?;
    let findings = drift_findings(
        previous.as_ref().map(|snapshot| &snapshot.canonical_spec),
        &snapshot.canonical_spec,
    );
    let high_risk = findings
        .iter()
        .any(|(_, risk, _, _)| matches!(risk.as_str(), "high" | "critical"));
    sources::replace_drift_findings(pool, &source.id, &snapshot.id, &findings).await?;
    if high_risk {
        repository::set_status(pool, &agent.id, "paused").await?;
        governance::suspend(pool, &agent.id, "检测到高风险来源漂移，已暂停新工作。").await?;
    }
    Ok(true)
}

fn candidate_agent(
    current: &crate::db::managed_agents::registry::schema::ManagedAgentRow,
    provider: &dyn ImportAgentsProvider,
    remote: &ImportedAgent,
) -> crate::db::managed_agents::registry::schema::ManagedAgentRow {
    let mut candidate = current.clone();
    candidate.name = remote.name.clone();
    candidate.description = remote.description.clone();
    candidate.model = provider.default_model(remote.model.as_deref());
    candidate.system = provider.system_prompt_from_raw(&remote.id, &remote.raw);
    if let Some(source) = candidate
        .config
        .get_mut("source")
        .and_then(Value::as_object_mut)
    {
        source.insert("raw".to_owned(), remote.raw.clone());
    }
    candidate
}

fn import_agent(remote: &ImportedAgent) -> ImportAgent {
    ImportAgent {
        external_id: remote.id.clone(),
        name: Some(remote.name.clone()),
        description: remote.description.clone(),
        model: remote.model.clone(),
        raw: Some(remote.raw.clone()),
    }
}

fn drift_findings(
    previous: Option<&Value>,
    candidate: &Value,
) -> Vec<(String, String, Option<Value>, Option<Value>)> {
    const FIELDS: [(&str, &str); 10] = [
        ("/execution/runtime", "critical"),
        ("/execution/model", "medium"),
        ("/instructions/system", "medium"),
        ("/capabilities/tools", "high"),
        ("/capabilities/mcp_server_ids", "critical"),
        ("/requirements/vault_keys", "critical"),
        ("/requirements/network_access", "critical"),
        ("/requirements/filesystem_access", "high"),
        ("/policies/declared_side_effects", "critical"),
        ("/policies/schedule", "high"),
    ];
    let mut findings = Vec::new();
    for (pointer, risk) in FIELDS {
        let before = previous.and_then(|value| value.pointer(pointer)).cloned();
        let after = candidate.pointer(pointer).cloned();
        if before != after {
            findings.push((
                pointer.trim_start_matches('/').replace('/', "."),
                risk.to_owned(),
                before,
                after,
            ));
        }
    }
    if findings.is_empty() && previous != Some(candidate) {
        findings.push((
            "canonical_spec".to_owned(),
            "low".to_owned(),
            previous.cloned(),
            Some(candidate.clone()),
        ));
    }
    findings
}

async fn test_connector_inner(
    state: &AppState,
    pool: &sqlx::PgPool,
    connector: &SourceConnectorRow,
) -> Result<SourceConnectorRow, GatewayError> {
    validate_connector_endpoint(&connector.endpoint).await?;
    let provider = provider_for_id(&connector.provider)?;
    let api_key = connector_api_key(state, pool, connector).await?;
    let started = Instant::now();
    let discovered = tokio::time::timeout(
        CONNECT_TIMEOUT,
        provider.discover(&state.http, &connector.endpoint, &api_key),
    )
    .await;
    let (status, detail) = match discovered {
        Ok(Ok(agents)) => (
            "healthy",
            format!(
                "连接成功，发现 {} 个智能体，耗时 {}ms。",
                agents.len(),
                started.elapsed().as_millis()
            ),
        ),
        Ok(Err(error)) => (
            "unreachable",
            format!("连接失败：{}", super::import_types::provider_error(error)),
        ),
        Err(_) => ("unreachable", "连接超时。".to_owned()),
    };
    sources::set_connector_test(pool, &connector.id, status, &detail).await
}

async fn connector_api_key(
    state: &AppState,
    pool: &sqlx::PgPool,
    connector: &SourceConnectorRow,
) -> Result<String, GatewayError> {
    credential_api_key(
        state,
        pool,
        connector.credential_name.as_deref(),
        &connector.owner_id,
    )
    .await
}

/// Decrypts a named personal credential's `api_key`, or `""` if the source
/// has no credential reference (e.g. BYO-mode imports never persist one).
/// Shared by connector "test connection" and the federated-source reachability
/// probe in `registry::preflight`.
pub(crate) async fn credential_api_key(
    state: &AppState,
    pool: &sqlx::PgPool,
    credential_name: Option<&str>,
    owner_id: &str,
) -> Result<String, GatewayError> {
    let Some(name) = credential_name else {
        return Ok(String::new());
    };
    let credential = credentials::get_personal_by_name(pool, name, owner_id)
        .await?
        .ok_or_else(|| GatewayError::BadRequest("凭据不存在或不属于当前属主。".to_owned()))?;
    let Some(encrypted) = credential
        .credential_values
        .get("api_key")
        .and_then(Value::as_str)
    else {
        return Ok(String::new());
    };
    let key =
        credential_crypto::encryption_key(state.config.general_settings.master_key.as_deref())?;
    credential_crypto::decrypt_value(encrypted, &key)
}

async fn connector_webhook_secret(
    state: &AppState,
    pool: &sqlx::PgPool,
    connector: &SourceConnectorRow,
) -> Result<String, GatewayError> {
    let name = connector
        .credential_name
        .as_deref()
        .ok_or(GatewayError::Unauthorized)?;
    let credential = credentials::get_personal_by_name(pool, name, &connector.owner_id)
        .await?
        .ok_or(GatewayError::Unauthorized)?;
    let encrypted = credential
        .credential_values
        .get("webhook_secret")
        .and_then(Value::as_str)
        .ok_or(GatewayError::Unauthorized)?;
    let key =
        credential_crypto::encryption_key(state.config.general_settings.master_key.as_deref())?;
    credential_crypto::decrypt_value(encrypted, &key)
}

fn verify_webhook_signature(
    secret: &str,
    timestamp: &str,
    body: &[u8],
    signature: &str,
) -> Result<(), GatewayError> {
    let provided = decode_hex(signature.trim().trim_start_matches("sha256="))
        .ok_or(GatewayError::Unauthorized)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|_| GatewayError::Unauthorized)?;
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);
    mac.verify_slice(&provided)
        .map_err(|_| GatewayError::Unauthorized)
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = char::from(pair[0]).to_digit(16)?;
            let low = char::from(pair[1]).to_digit(16)?;
            Some(((high << 4) | low) as u8)
        })
        .collect()
}

fn header(headers: &HeaderMap, name: &str) -> Result<String, GatewayError> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or(GatewayError::Unauthorized)
}

async fn validate_credential_reference(
    pool: &sqlx::PgPool,
    owner_id: &str,
    credential_name: Option<&str>,
) -> Result<(), GatewayError> {
    let Some(name) = credential_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return Ok(());
    };
    if credentials::get_personal_by_name(pool, name, owner_id)
        .await?
        .is_none()
    {
        return Err(GatewayError::BadRequest(
            "连接器只能引用当前属主的个人凭据。".to_owned(),
        ));
    }
    Ok(())
}

async fn store_connector_credential(
    state: &AppState,
    pool: &sqlx::PgPool,
    owner_id: &str,
    credential_name: &str,
    endpoint: &str,
    api_key: Option<&str>,
    webhook_secret: Option<&str>,
) -> Result<(), GatewayError> {
    let key =
        credential_crypto::encryption_key(state.config.general_settings.master_key.as_deref())?;
    let mut values = serde_json::Map::new();
    values.insert(
        "api_base".to_owned(),
        json!(credential_crypto::encrypt_value(endpoint, &key)?),
    );
    if let Some(api_key) = api_key.map(str::trim).filter(|value| !value.is_empty()) {
        values.insert(
            "api_key".to_owned(),
            json!(credential_crypto::encrypt_value(api_key, &key)?),
        );
    }
    if let Some(secret) = webhook_secret
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        values.insert(
            "webhook_secret".to_owned(),
            json!(credential_crypto::encrypt_value(secret, &key)?),
        );
    }
    credentials::upsert_personal(
        pool,
        credential_name,
        owner_id,
        Value::Object(values),
        json!({ "source": "agent-source-connector" }),
        owner_id,
    )
    .await
}

pub(crate) async fn validate_connector_endpoint(endpoint: &str) -> Result<String, GatewayError> {
    let endpoint = endpoint.trim().trim_end_matches('/');
    let url = reqwest::Url::parse(endpoint)
        .map_err(|_| GatewayError::InvalidJsonMessage("endpoint 必须是有效 URL。".to_owned()))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(GatewayError::InvalidJsonMessage(
            "endpoint 只能使用 http 或 https。".to_owned(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(GatewayError::InvalidJsonMessage(
            "endpoint 不允许在 URL 中携带凭据。".to_owned(),
        ));
    }
    let host = url.host_str().unwrap_or_default();
    if matches!(host, "localhost" | "metadata.google.internal") || host.ends_with(".localhost") {
        return Err(GatewayError::InvalidJsonMessage(
            "endpoint 不允许指向本机或云元数据服务。".to_owned(),
        ));
    }
    let port = url.port_or_known_default().unwrap_or(80);
    let addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| GatewayError::InvalidJsonMessage("endpoint 主机无法解析。".to_owned()))?;
    for address in addresses {
        if forbidden_address(address.ip()) {
            return Err(GatewayError::InvalidJsonMessage(
                "endpoint 解析到了本机、链路本地或元数据地址。".to_owned(),
            ));
        }
    }
    Ok(endpoint.to_owned())
}

fn forbidden_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            address.is_loopback()
                || address.is_link_local()
                || address.is_unspecified()
                || address.is_multicast()
                || address.octets() == [169, 254, 169, 254]
        }
        IpAddr::V6(address) => {
            address.is_loopback()
                || address.is_unspecified()
                || address.is_multicast()
                || (address.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

async fn editable_agent(
    state: &AppState,
    headers: &HeaderMap,
    agent_id: &str,
) -> Result<
    (
        sqlx::PgPool,
        AuthContext,
        crate::db::managed_agents::registry::schema::ManagedAgentRow,
    ),
    GatewayError,
> {
    let auth = authenticate(headers, state).await?;
    let pool = state.db.clone().ok_or(GatewayError::MissingDatabase)?;
    let agent = repository::get(&pool, agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("agent not found".to_owned()))?;
    super::assert_agent_edit(&auth, &agent, &pool).await?;
    Ok((pool, auth, agent))
}

async fn owned_connector(
    state: &AppState,
    headers: &HeaderMap,
    connector_id: &str,
) -> Result<(sqlx::PgPool, AuthContext, SourceConnectorRow), GatewayError> {
    let auth = authenticate(headers, state).await?;
    let pool = state.db.clone().ok_or(GatewayError::MissingDatabase)?;
    let connector = sources::get_connector(&pool, connector_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("connector not found".to_owned()))?;
    if !auth.is_admin && connector.owner_id != auth.user_id {
        return Err(GatewayError::NotFound("connector not found".to_owned()));
    }
    Ok((pool, auth, connector))
}

fn required(value: &str, field: &str) -> Result<String, GatewayError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(GatewayError::InvalidJsonMessage(format!(
            "{field} is required"
        )));
    }
    Ok(value.to_owned())
}

fn health_kind(id: &str) -> &'static str {
    match id {
        "runtime" => "runtime",
        "model" => "model",
        "tools" => "tool",
        "mcp_server" => "mcp",
        "vault_key" | "source_credential" => "credential",
        "execution_smoke" => "runtime",
        _ => "source",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_loopback_and_metadata_addresses() {
        assert!(forbidden_address("127.0.0.1".parse().unwrap()));
        assert!(forbidden_address("169.254.169.254".parse().unwrap()));
        assert!(!forbidden_address("10.0.0.8".parse().unwrap()));
    }

    #[test]
    fn classifies_sensitive_drift_as_high_risk() {
        let previous = json!({
            "execution": { "runtime": "a", "model": "m" },
            "capabilities": { "tools": [], "mcp_server_ids": [] },
            "instructions": { "system": "a" },
            "requirements": {
                "vault_keys": [], "network_access": [], "filesystem_access": []
            },
            "policies": { "declared_side_effects": [], "schedule": null }
        });
        let mut candidate = previous.clone();
        candidate["capabilities"]["tools"] = json!([{ "type": "bash" }]);

        let findings = drift_findings(Some(&previous), &candidate);

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].0, "capabilities.tools");
        assert_eq!(findings[0].1, "high");
    }

    #[test]
    fn verifies_signed_webhook_and_rejects_tampering() {
        let secret = "secret";
        let timestamp = "1721123456";
        let body = br#"{"event":"agent.updated"}"#;
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(timestamp.as_bytes());
        mac.update(b".");
        mac.update(body);
        let signature = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();

        assert!(verify_webhook_signature(secret, timestamp, body, &signature).is_ok());
        assert!(verify_webhook_signature(secret, timestamp, b"tampered", &signature).is_err());
    }
}
