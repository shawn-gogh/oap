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
    managed_agents::adapters::source::{ImportedAgent, SourceAdapter, SourceAdapterError},
    proxy::{
        auth::master_key::{authenticate, AuthContext},
        credential_crypto,
        state::AppState,
    },
    sdk::agents::{
        canonical::{normalize_agent, CanonicalAgentSpec},
        conformance::inspect_runtime_contract_with_api_spec,
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
    let provider = provider_for_id(&state.agent_adapters, input.provider.trim())?;
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
    let descriptor = state
        .agent_adapters
        .source(provider.id())
        .ok_or_else(|| GatewayError::InvalidConfig("来源适配器描述缺失。".to_owned()))?;
    let connector = sources::create_connector(
        pool,
        &auth.user_id,
        CreateSourceConnector {
            name,
            provider: provider.id().to_owned(),
            endpoint,
            credential_name,
            adapter_id: provider.id().to_owned(),
            protocol: descriptor.protocol.to_string(),
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

#[derive(Debug, Deserialize)]
pub struct RuntimeMappingRequest {
    /// Required for `openapi` — a site-relative path (e.g. `/agents/run`).
    /// Ignored for `langgraph`/`crewai`, which have a fixed endpoint shape.
    pub path: Option<String>,
    /// JSON field the prompt is wrapped under in the outbound request body.
    /// Falls back to the bridge's own per-provider default (see
    /// `sessions::external_bridge::invoke_{openapi,langgraph,crewai}`) when
    /// omitted, so an empty `{}` is already valid for langgraph/crewai.
    pub input_field: Option<String>,
    /// `openapi` only: JSON field to read the answer from in the response body.
    pub output_field: Option<String>,
    /// `langgraph`/`crewai` only: JSON pointer (e.g. `/output`) to read the
    /// answer from in the response body.
    pub output_path: Option<String>,
    /// Canonical JSON Schemas captured from source introspection. They travel
    /// with the confirmed mapping so every Run snapshots the exact contract
    /// that was reviewed by the operator.
    pub input_schema: Option<Value>,
    pub output_schema: Option<Value>,
}

/// Persists the operator-confirmed request/response mapping the execution
/// bridges require before they'll run a session for `openapi`/`langgraph`/
/// `crewai` sources (see `sessions::external_bridge`). This is the write side
/// of the `unmapped_high_risk_field`/`*_mapping_required` preflight issues
/// (`import_validation.rs`) — before this endpoint existed there was no way
/// to actually clear that check, so those three providers were import-able
/// but permanently unable to execute.
pub async fn set_runtime_mapping(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Json(input): Json<RuntimeMappingRequest>,
) -> Result<Json<crate::db::managed_agents::registry::schema::ManagedAgentRow>, GatewayError> {
    let (pool, auth, agent) = editable_agent(&state, &headers, &agent_id).await?;
    let governance = governance::get(&pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("governance not found".to_owned()))?;
    if governance.source_provider == "openapi" {
        let path = input
            .path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| GatewayError::InvalidJsonMessage("path 不能为空。".to_owned()))?;
        if !path.starts_with('/') || path.starts_with("//") {
            return Err(GatewayError::InvalidJsonMessage(
                "path 必须是站内绝对路径。".to_owned(),
            ));
        }
    }
    let mut mapping = json!({});
    for (key, value) in [
        ("path", &input.path),
        ("input_field", &input.input_field),
        ("output_field", &input.output_field),
        ("output_path", &input.output_path),
    ] {
        if let Some(value) = value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            mapping[key] = json!(value);
        }
    }
    if let Some(schema) = input.input_schema.filter(|schema| !schema.is_null()) {
        mapping["input_schema"] = schema;
    }
    if let Some(schema) = input.output_schema.filter(|schema| !schema.is_null()) {
        mapping["output_schema"] = schema;
    }
    if governance.source_provider == "openapi" {
        if let (Some(raw), Some(path)) = (
            agent.config.pointer("/source/raw"),
            mapping.get("path").and_then(Value::as_str),
        ) {
            let (input_schema, output_schema) =
                crate::sdk::providers::openapi_import_agents::runtime_schemas(raw, path);
            if mapping.get("input_schema").is_none() {
                if let Some(schema) = input_schema {
                    mapping["input_schema"] = schema;
                }
            }
            if mapping.get("output_schema").is_none() {
                if let Some(schema) = output_schema {
                    mapping["output_schema"] = schema;
                }
            }
        }
    }
    let row = repository::set_source_runtime_mapping(&pool, &agent_id, &mapping)
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    audit::record(
        &pool,
        &auth.user_id,
        "agent.source.runtime_mapping_confirmed",
        "agent",
        &agent_id,
        mapping,
    )
    .await?;
    Ok(Json(row))
}

#[derive(Debug, serde::Serialize)]
pub struct RuntimeMappingSuggestion {
    pub input_field: Option<String>,
    pub output_path: Option<String>,
    /// Present when a schema was actually fetched, so the UI can show the
    /// raw JSON Schema for cases the guess couldn't resolve automatically.
    pub input_schema: Option<Value>,
    pub output_schema: Option<Value>,
    /// Why no suggestion could be made — e.g. provider unsupported, schema
    /// endpoint unreachable — shown as a hint, not an error, since manual
    /// entry always remains available.
    pub note: Option<String>,
    /// `openapi` only: the POST routes the stored spec declares, each with its
    /// field guesses. Turns `path` from free text into a pick list.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<RuntimePathSuggestion>,
}

#[derive(Debug, serde::Serialize)]
pub struct RuntimePathSuggestion {
    pub path: String,
    pub summary: Option<String>,
    pub input_field: Option<String>,
    pub output_field: Option<String>,
    pub input_schema: Option<Value>,
    pub output_schema: Option<Value>,
}

/// Builds per-route field guesses from the OpenAPI document captured at import.
///
/// Reads the stored spec rather than calling the source: unlike LangGraph's
/// `/schemas` endpoint, nothing has to be reachable for this to work.
fn openapi_path_suggestions(spec: &Value) -> Vec<RuntimePathSuggestion> {
    crate::sdk::providers::openapi_import_agents::runtime_paths(spec)
        .into_iter()
        .map(|(path, summary)| {
            let (input_schema, output_schema) =
                crate::sdk::providers::openapi_import_agents::runtime_schemas(spec, &path);
            RuntimePathSuggestion {
                input_field: input_schema.as_ref().and_then(|schema| {
                    crate::sdk::providers::langgraph_import_agents::guess_field_name(
                        schema,
                        &["input", "message", "messages", "text", "query", "topic"],
                    )
                }),
                output_field: output_schema.as_ref().and_then(|schema| {
                    crate::sdk::providers::langgraph_import_agents::guess_field_name(
                        schema,
                        &["output", "answer", "result", "response"],
                    )
                }),
                path,
                summary,
                input_schema,
                output_schema,
            }
        })
        .collect()
}

/// Best-effort mapping suggestion, fetched from the source's own schema
/// introspection endpoint where one exists (currently just LangGraph
/// Platform's `GET /assistants/{id}/schemas`) rather than making the operator
/// hand-derive field names. Never fails the request — schema-fetch problems
/// become a `note` so the confirmation dialog still opens with blank fields.
pub async fn suggest_runtime_mapping(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<RuntimeMappingSuggestion>, GatewayError> {
    let (pool, _auth, agent) = editable_agent(&state, &headers, &agent_id).await?;
    let governance = governance::get(&pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("governance not found".to_owned()))?;
    if governance.source_provider == "openapi" {
        let paths = agent
            .config
            .pointer("/source/raw")
            .map(openapi_path_suggestions)
            .unwrap_or_default();
        let note = paths
            .is_empty()
            .then(|| "来源规范中没有可用的 POST 路由，请手动确认。".to_owned());
        return Ok(Json(RuntimeMappingSuggestion {
            input_field: None,
            output_path: None,
            input_schema: None,
            output_schema: None,
            note,
            paths,
        }));
    }
    if governance.source_provider != "langgraph" {
        return Ok(Json(RuntimeMappingSuggestion {
            input_field: None,
            output_path: None,
            input_schema: None,
            output_schema: None,
            note: Some("该来源暂不支持自动获取输入/输出结构，请手动确认。".to_owned()),
            paths: Vec::new(),
        }));
    }
    let endpoint = validate_connector_endpoint(&governance.source_endpoint).await?;
    let api_key = credential_api_key(
        &state,
        &pool,
        governance.credential_name.as_deref(),
        &governance.owner_id,
    )
    .await?;
    match crate::sdk::providers::langgraph_import_agents::fetch_schemas(
        &state.http,
        &endpoint,
        &governance.external_agent_id,
        &api_key,
    )
    .await
    {
        Ok(schemas) => {
            let input_schema = schemas.get("input_schema").cloned();
            let output_schema = schemas.get("output_schema").cloned();
            let input_field = input_schema.as_ref().and_then(|s| {
                crate::sdk::providers::langgraph_import_agents::guess_field_name(
                    s,
                    &["input", "message", "messages", "text", "query"],
                )
            });
            let output_field = output_schema.as_ref().and_then(|s| {
                crate::sdk::providers::langgraph_import_agents::guess_field_name(
                    s,
                    &["output", "answer", "result", "response"],
                )
            });
            Ok(Json(RuntimeMappingSuggestion {
                input_field,
                output_path: output_field.map(|field| format!("/{field}")),
                input_schema,
                output_schema,
                note: None,
                paths: Vec::new(),
            }))
        }
        Err(error) => {
            let reason = match error {
                SourceAdapterError::Upstream { status, .. } => {
                    format!("来源返回 HTTP {status}")
                }
                SourceAdapterError::Request(_) => "无法连接来源".to_owned(),
                SourceAdapterError::Decode(_) => "来源返回的内容不是有效 JSON".to_owned(),
                SourceAdapterError::InvalidDocument(message) => message,
            };
            Ok(Json(RuntimeMappingSuggestion {
                input_field: None,
                output_path: None,
                input_schema: None,
                output_schema: None,
                note: Some(format!("自动获取失败（{reason}），请手动确认。")),
                paths: Vec::new(),
            }))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RuntimeMappingProbeRequest {
    /// Candidate input field to probe. Defaults to the same `input` the
    /// execution bridge falls back to, so a probe with no body still shows the
    /// operator what the default would do.
    pub input_field: Option<String>,
    /// `openapi` only, and required there: the bridge has no default path to
    /// fall back to, so there is nothing to probe until the operator names one.
    pub path: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct RuntimeMappingProbe {
    pub input_field: String,
    pub sentinel: String,
    /// The complete upstream response. The confirmation UI renders this as a
    /// clickable tree — it, not the hint lists below, is the authoritative
    /// place to pick `output_path` from, because a valid mapping may address a
    /// whole array (e.g. `/messages`) rather than a string leaf.
    pub response: Value,
    /// Pointers whose string leaf contains the sentinel. Non-empty proves the
    /// probed `input_field` actually reached the source; empty is the signal
    /// that a plausible-looking field name is silently being ignored.
    pub sentinel_paths: Vec<String>,
    /// String leaves, offered as ranked output candidates.
    ///
    /// Always RFC 6901 pointers. LangGraph's `output_path` takes them
    /// verbatim; OpenAPI's `output_field` is a *top-level field name*, so only
    /// depth-1 entries apply there and the leading `/` is dropped when filling
    /// the field.
    pub string_paths: Vec<String>,
    /// True when the response had more string leaves than `string_paths` shows.
    pub string_paths_truncated: bool,
}

/// Upper bound on hint entries: a probe against a graph that echoes a large
/// document must not turn into a multi-megabyte response of pointer strings.
const PROBE_PATH_LIMIT: usize = 200;

/// Executes one real run against the source with a candidate input mapping and
/// returns the whole response, so the operator confirms the output field from
/// an observed payload rather than guessing at one.
///
/// This is the "observed signing" half of the mapping contract: the platform
/// still cannot decide *which* field is safe to expose (see
/// `set_runtime_mapping`) — only a human knows that a field named `output`
/// holds internal reasoning while `answer` holds the reply — but it can at
/// least stop making them guess the payload's shape.
///
/// Operator-triggered only, and never called from import or the background
/// scheduler: each probe really executes the remote agent, with whatever side
/// effects and model spend that entails.
pub async fn probe_runtime_mapping(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Json(input): Json<RuntimeMappingProbeRequest>,
) -> Result<Json<RuntimeMappingProbe>, GatewayError> {
    let (pool, auth, agent) = editable_agent(&state, &headers, &agent_id).await?;
    let governance = governance::get(&pool, &agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("governance not found".to_owned()))?;
    let source = agent
        .config
        .get("source")
        .ok_or_else(|| GatewayError::InvalidConfig("导入智能体缺少来源配置。".to_owned()))?;
    let input_field = input
        .input_field
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("input");
    let credential = crate::http::agent_runtimes::RuntimeCredential {
        api_key: probe_api_key(&state, &pool, &governance, &agent_id, &auth).await?,
        api_base: governance.source_endpoint.clone(),
    };
    let sentinel = crate::db::managed_agents::id("lap-probe");
    // CrewAI is deliberately absent: its bridge is an async kickoff plus a
    // bounded polling loop tied to a session row, so a probe would have to
    // reimplement that rather than reuse it.
    let path = input
        .path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let response = match governance.source_provider.as_str() {
        "langgraph" => {
            crate::http::sessions::external_bridge::probe_langgraph(
                &state,
                source,
                &credential,
                input_field,
                &sentinel,
            )
            .await?
        }
        "openapi" => {
            let path = path.ok_or_else(|| {
                GatewayError::BadRequest("OpenAPI 来源试跑必须提供站内路径。".to_owned())
            })?;
            crate::http::sessions::external_bridge::probe_openapi(
                &state,
                source,
                &credential,
                path,
                input_field,
                &sentinel,
            )
            .await?
        }
        other => {
            return Err(GatewayError::BadRequest(format!(
                "映射试跑暂不支持 {other} 来源，目前仅支持 LangGraph 与 OpenAPI。"
            )));
        }
    };

    let mut sentinel_paths = Vec::new();
    let mut string_paths = Vec::new();
    collect_pointer_paths(
        &response,
        "",
        &sentinel,
        &mut sentinel_paths,
        &mut string_paths,
    );
    let string_paths_truncated = string_paths.len() > PROBE_PATH_LIMIT;
    string_paths.truncate(PROBE_PATH_LIMIT);
    sentinel_paths.truncate(PROBE_PATH_LIMIT);

    // The response itself is deliberately not audited: it can carry retrieved
    // documents or user data. Record only that a probe happened and what it
    // established.
    audit::record(
        &pool,
        &auth.user_id,
        "agent.source.runtime_mapping_probed",
        "agent",
        &agent_id,
        json!({
            "provider": governance.source_provider,
            "path": path,
            "input_field": input_field,
            "input_field_reached_source": !sentinel_paths.is_empty(),
        }),
    )
    .await?;

    Ok(Json(RuntimeMappingProbe {
        input_field: input_field.to_owned(),
        sentinel,
        response,
        sentinel_paths,
        string_paths,
        string_paths_truncated,
    }))
}

/// Resolves the key a probe should authenticate with, mirroring
/// `runtime_resolution::imported_agent_credential` but without a session: a
/// probe is an operator action, so a BYO source uses the *caller's* own key
/// rather than a session owner's.
async fn probe_api_key(
    state: &AppState,
    pool: &sqlx::PgPool,
    governance: &governance::AgentGovernanceRow,
    agent_id: &str,
    auth: &AuthContext,
) -> Result<String, GatewayError> {
    if governance.credential_scope == "byo" {
        let name = crate::http::runtime_resolution::byo_credential_name(agent_id);
        return credential_api_key(state, pool, Some(&name), &auth.user_id).await;
    }
    credential_api_key(
        state,
        pool,
        governance.credential_name.as_deref(),
        &governance.owner_id,
    )
    .await
}

/// Walks a JSON document collecting RFC 6901 pointers to every string leaf,
/// and separately those whose text contains `sentinel`.
fn collect_pointer_paths(
    value: &Value,
    prefix: &str,
    sentinel: &str,
    sentinel_paths: &mut Vec<String>,
    string_paths: &mut Vec<String>,
) {
    match value {
        Value::String(text) => {
            string_paths.push(prefix.to_owned());
            if text.contains(sentinel) {
                sentinel_paths.push(prefix.to_owned());
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_pointer_paths(
                    item,
                    &format!("{prefix}/{index}"),
                    sentinel,
                    sentinel_paths,
                    string_paths,
                );
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                collect_pointer_paths(
                    item,
                    &format!("{prefix}/{}", escape_pointer_token(key)),
                    sentinel,
                    sentinel_paths,
                    string_paths,
                );
            }
        }
        _ => {}
    }
}

/// RFC 6901 token escaping. `~` must be escaped before `/`, otherwise the `~1`
/// produced for a slash would itself be re-escaped into `~01`.
fn escape_pointer_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
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
    let confirmed_mapping = config.pointer("/source/raw/x-lap-runtime").cloned();
    let mut next_raw = snapshot.raw_spec.clone();
    if let (Some(mapping), Some(raw)) = (confirmed_mapping, next_raw.as_object_mut()) {
        raw.insert("x-lap-runtime".to_owned(), mapping);
    }
    if let Some(source_config) = config.get_mut("source").and_then(Value::as_object_mut) {
        source_config.insert("raw".to_owned(), next_raw);
    }
    let provider = provider_for_id(&state.agent_adapters, &governance.source_provider)?;
    config["interaction_profile"] =
        serde_json::to_value(provider.interaction_contract(&snapshot.raw_spec))
            .map_err(|error| GatewayError::InvalidConfig(error.to_string()))?;
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
    let api_spec =
        crate::db::managed_agents::governance::resolve_runtime_api_spec(&pool, &agent).await;
    let report = inspect_runtime_contract_with_api_spec(&agent, api_spec.as_deref());
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
    // Consecutive failures avoid pausing production work on a transient probe error.
    if !report.can_activate && agent.status == "active" {
        let recent =
            sources::recent_health_statuses(pool, &agent.id, "preflight", HEALTH_PAUSE_THRESHOLD)
                .await?;
        let consecutive_failures = recent.len() as i64 >= HEALTH_PAUSE_THRESHOLD
            && recent.iter().all(|status| status == "unhealthy");
        if consecutive_failures {
            super::source_alerts::pause_for_health_failures(
                state,
                pool,
                agent,
                HEALTH_PAUSE_THRESHOLD,
            )
            .await?;
        }
    }
    Ok((report, latency))
}

/// Interrupts every live runtime session of the agent (best effort) before
/// the DB-level status sweep. `cancel_agent_work` alone only rewrites rows:
/// without this, remote runtimes and A2A pollers keep executing after an
/// "emergency stop", and an in-flight prompt's completion handler would even
/// overwrite the swept 'cancelled' status and resurrect the session.
pub(crate) async fn interrupt_agent_sessions(
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
                provider_for_id(&state.agent_adapters, &connector.provider)?,
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
                provider_for_id(&state.agent_adapters, &governance.source_provider)?,
                governance.source_endpoint.clone(),
                api_key,
            )
        }
    };
    // Bundle/file-uploaded agents (`agent-bundle://…`, `opencode-file://…`)
    // have no live endpoint to sync against — the uploaded content already
    // *is* the source of truth, so there is no drift to detect. Without this,
    // every scheduled tick hit `validate_connector_endpoint`'s http(s)-only
    // check and permanently marked the source `sync_error`, a state no
    // amount of user action could ever clear.
    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        return Ok(false);
    }
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
    if let Some(connector_id) = source.connector_id.as_deref() {
        if let Some(profile) = provider
            .negotiate_protocol(&endpoint, &remote.raw)
            .map_err(super::import_types::provider_error)?
        {
            validate_connector_endpoint(&profile.interface_url).await?;
            let profile_json = serde_json::to_value(&profile)
                .map_err(|error| GatewayError::InvalidConfig(error.to_string()))?;
            sources::set_connector_negotiated_profile(
                pool,
                connector_id,
                &profile.protocol,
                &profile.protocol_version,
                profile_json,
            )
            .await?;
        }
    }
    let imported = import_agent(&remote);
    let digest = source_hash(&imported)?;
    if digest == governance.source_hash {
        sources::mark_sync_state(pool, &source.id, "in_sync", 0).await?;
        return Ok(false);
    }
    record_drift_candidate(
        state,
        agent,
        provider,
        source,
        &remote,
        &digest,
        &auth.user_id,
    )
    .await?;
    Ok(true)
}

/// Stores source changes for review and pauses new work on high-risk drift.
pub(crate) async fn record_drift_candidate(
    state: &AppState,
    agent: &crate::db::managed_agents::registry::schema::ManagedAgentRow,
    provider: &dyn SourceAdapter,
    source: &ManagedAgentSourceRow,
    remote: &ImportedAgent,
    digest: &str,
    actor: &str,
) -> Result<AgentSourceSnapshotRow, GatewayError> {
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let candidate = candidate_agent(agent, provider, remote);
    let snapshot = sources::record_candidate_snapshot(
        pool,
        source,
        &candidate,
        remote.raw.clone(),
        digest,
        actor,
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
        super::source_alerts::pause_for_high_risk_drift(
            state,
            pool,
            agent,
            &snapshot.id,
            &findings,
        )
        .await?;
    }
    Ok(snapshot)
}

fn candidate_agent(
    current: &crate::db::managed_agents::registry::schema::ManagedAgentRow,
    provider: &dyn SourceAdapter,
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
    let provider = provider_for_id(&state.agent_adapters, &connector.provider)?;
    let api_key = connector_api_key(state, pool, connector).await?;
    let started = Instant::now();
    let discovered = tokio::time::timeout(
        CONNECT_TIMEOUT,
        provider.discover(&state.http, &connector.endpoint, &api_key),
    )
    .await;
    let (status, detail) = match discovered {
        Ok(Ok(agents)) => {
            if let Some(profile) = agents
                .first()
                .map(|agent| provider.negotiate_protocol(&connector.endpoint, &agent.raw))
                .transpose()
                .map_err(super::import_types::provider_error)?
                .flatten()
            {
                validate_connector_endpoint(&profile.interface_url).await?;
                let profile_json = serde_json::to_value(&profile)
                    .map_err(|error| GatewayError::InvalidConfig(error.to_string()))?;
                sources::set_connector_negotiated_profile(
                    pool,
                    &connector.id,
                    &profile.protocol,
                    &profile.protocol_version,
                    profile_json,
                )
                .await?;
            }
            (
                "healthy",
                format!(
                    "连接成功，发现 {} 个智能体，耗时 {}ms。",
                    agents.len(),
                    started.elapsed().as_millis()
                ),
            )
        }
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

/// Silently materializes a source connector for a direct import, so the
/// "import an agent" dialog stays the single user-facing entry point: the
/// connector (scheduled sync grouping, webhook endpoint, shared credential)
/// is created as a by-product instead of being a concept users must learn
/// and configure up front. Reuses an existing connector for the same
/// owner/provider/endpoint; stores the import's api_key as the connector
/// credential only when the connector is newly created (never overwrites an
/// existing connector's credential).
pub(crate) async fn ensure_connector_for_import(
    state: &AppState,
    pool: &sqlx::PgPool,
    owner_id: &str,
    provider: &dyn SourceAdapter,
    endpoint: &str,
    api_key: Option<&str>,
) -> Result<SourceConnectorRow, GatewayError> {
    if let Some(existing) = sources::find_connector(pool, owner_id, provider.id(), endpoint).await?
    {
        return Ok(existing);
    }
    let api_key = api_key.map(str::trim).filter(|value| !value.is_empty());
    let credential_name = match api_key {
        Some(api_key) => {
            let name = format!(
                "provider:{}:connector:{}",
                provider.id(),
                uuid::Uuid::new_v4().simple()
            );
            store_connector_credential(state, pool, owner_id, &name, endpoint, Some(api_key), None)
                .await?;
            Some(name)
        }
        None => None,
    };
    let host = reqwest::Url::parse(endpoint)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| endpoint.to_owned());
    let capabilities = serde_json::to_value(provider.capabilities())
        .map_err(|error| GatewayError::InvalidConfig(error.to_string()))?;
    let descriptor = state
        .agent_adapters
        .source(provider.id())
        .ok_or_else(|| GatewayError::InvalidConfig("来源适配器描述缺失。".to_owned()))?;
    let connector = sources::create_connector(
        pool,
        owner_id,
        CreateSourceConnector {
            name: format!("{} · {host}", provider.name()),
            provider: provider.id().to_owned(),
            endpoint: endpoint.to_owned(),
            credential_name,
            adapter_id: provider.id().to_owned(),
            protocol: descriptor.protocol.to_string(),
            protocol_version: provider.protocol_version().to_owned(),
        },
        capabilities,
    )
    .await?;
    audit::record(
        pool,
        owner_id,
        "agent.connector.created",
        "agent_source_connector",
        &connector.id,
        json!({ "provider": connector.provider, "endpoint": connector.endpoint, "via": "import" }),
    )
    .await?;
    Ok(connector)
}

/// Discovery credential for an agent's federated source, resolved exactly the
/// way `reconcile_source` resolves it: the linked connector's credential when
/// one exists, otherwise the source's own credential reference (`""` for BYO
/// imports without a connector). Used by the preflight reachability probe so
/// preflight and sync can never disagree about whether the source is
/// reachable just because they authenticated differently.
pub(crate) async fn discovery_api_key_for_agent(
    state: &AppState,
    pool: &sqlx::PgPool,
    agent: &crate::db::managed_agents::registry::schema::ManagedAgentRow,
) -> Result<String, GatewayError> {
    if let Some(source) = sources::get_source_by_agent(pool, &agent.id).await? {
        if let Some(connector_id) = source.connector_id.as_deref() {
            if let Some(connector) = sources::get_connector(pool, connector_id).await? {
                if connector.credential_name.is_some() {
                    return connector_api_key(state, pool, &connector).await;
                }
            }
        }
    }
    let credential_name = agent
        .config
        .pointer("/source/credential_name")
        .and_then(Value::as_str);
    let owner_id = agent.owner_id.clone().unwrap_or_default();
    credential_api_key(state, pool, credential_name, &owner_id).await
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
    if !auth.can_operate() {
        super::assert_agent_edit(&auth, &agent, &pool).await?;
    }
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

    fn probe_paths(response: &Value, sentinel: &str) -> (Vec<String>, Vec<String>) {
        let mut sentinel_paths = Vec::new();
        let mut string_paths = Vec::new();
        collect_pointer_paths(
            response,
            "",
            sentinel,
            &mut sentinel_paths,
            &mut string_paths,
        );
        (sentinel_paths, string_paths)
    }

    #[test]
    fn probe_paths_locate_the_sentinel_and_offer_string_candidates() {
        // The MessagesState shape the LangGraph fixture actually returns.
        let response = json!({
            "messages": [
                { "type": "human", "content": "lap-probe-1" },
                { "type": "ai", "content": "Evidence assistant received the request." }
            ]
        });

        let (sentinel_paths, string_paths) = probe_paths(&response, "lap-probe-1");

        assert_eq!(sentinel_paths, vec!["/messages/0/content".to_owned()]);
        assert!(string_paths.contains(&"/messages/1/content".to_owned()));
    }

    #[test]
    fn probe_reports_no_sentinel_when_the_input_field_is_ignored() {
        // A graph that never saw the probed field echoes nothing back — the
        // signal that a plausible field name is being silently dropped.
        let response = json!({ "answer": "I received an empty question." });

        let (sentinel_paths, string_paths) = probe_paths(&response, "lap-probe-1");

        assert!(sentinel_paths.is_empty());
        assert_eq!(string_paths, vec!["/answer".to_owned()]);
    }

    #[test]
    fn probe_paths_are_valid_pointers_into_the_response() {
        let response = json!({
            "a/b": { "c~d": "lap-probe-1" },
            "nested": [[{ "deep": "lap-probe-1" }]],
            "ignored": [1, true, null]
        });

        let (sentinel_paths, string_paths) = probe_paths(&response, "lap-probe-1");

        // Every emitted pointer must round-trip through serde_json::pointer,
        // otherwise the UI would hand the operator a mapping that cannot read.
        for path in string_paths.iter().chain(sentinel_paths.iter()) {
            assert!(response.pointer(path).is_some(), "dangling pointer {path}");
        }
        assert!(sentinel_paths.contains(&"/a~1b/c~0d".to_owned()));
        assert!(sentinel_paths.contains(&"/nested/0/0/deep".to_owned()));
        // Non-string leaves are not output_path candidates.
        assert!(!string_paths.iter().any(|path| path.starts_with("/ignored")));
    }
}
