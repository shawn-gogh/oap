use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::{
    db::managed_agents::{audit, mcp_invocation_grants, session_control, sessions},
    errors::GatewayError,
    proxy::{auth::master_key::require_any_gateway_key, state::AppState},
};

mod approval;
mod catalog;
mod definitions;
mod expose_port;
mod factory;
mod selection;
mod session_management;
mod skill;
mod tools;

pub const PLATFORM_SESSION_MCP_ID: &str = "read_platform_session";
pub const SEND_PLATFORM_SESSION_MESSAGE_MCP_ID: &str = "send_platform_session_message";
pub const AGENT_MEMORY_MCP_ID: &str = "agent_memory";
pub const EDIT_AGENT_SKILL_MCP_ID: &str = "edit_agent_skill";
pub const PLATFORM_MCP_SERVER_NAME: &str = "platform";
pub const CREATE_MANAGED_AGENT_MCP_ID: &str = "create_managed_agent";
pub const LIST_SUB_AGENTS_MCP_ID: &str = "list_sub_agents";
pub const RUN_SUB_AGENT_MCP_ID: &str = "run_sub_agent";
pub const REQUEST_HUMAN_APPROVAL_MCP_ID: &str = "request_human_approval";
pub const CHECK_HUMAN_APPROVAL_MCP_ID: &str = "check_human_approval";
pub const EXPOSE_PORT_MCP_ID: &str = "expose_port";

pub use catalog::{platform_mcps, PlatformMcp};
pub use selection::selected_platform_mcp_ids;
pub(crate) use selection::sub_agent_ids;

pub fn platform_mcp_servers(
    state: &AppState,
    agent_id: &str,
    config: &Value,
    session_id: Option<&str>,
    inline_auth_token: Option<&str>,
) -> Result<Vec<Value>, GatewayError> {
    let ids = selected_platform_mcp_ids(config);
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut server = json!({
        "name": PLATFORM_MCP_SERVER_NAME,
        "type": "url",
        "url": platform_mcp_url(state, agent_id, session_id)?
    });
    if let Some(token) = inline_auth_token {
        server["authorization_token"] = Value::String(token.to_owned());
    }
    Ok(vec![server])
}

pub fn platform_mcp_toolsets(config: &Value) -> Vec<Value> {
    let ids = selected_platform_mcp_ids(config);
    if ids.is_empty() {
        return Vec::new();
    }
    vec![json!({
        "type": "mcp_toolset",
        "mcp_server_name": PLATFORM_MCP_SERVER_NAME,
        "default_config": {
            "enabled": false,
            "permission_policy": { "type": "always_allow" }
        },
        "configs": ids.into_iter().map(|id| json!({ "name": id, "enabled": true })).collect::<Vec<_>>()
    })]
}

pub fn platform_mcp_url(
    state: &AppState,
    agent_id: &str,
    session_id: Option<&str>,
) -> Result<String, GatewayError> {
    let base_url = proxy_base_url(state)?;
    let url = format!(
        "{}/mcp/platform/{}",
        base_url.trim_end_matches('/'),
        agent_id
    );
    Ok(match session_id {
        Some(session_id) if !session_id.trim().is_empty() => {
            format!("{url}?session_id={}", session_id.trim())
        }
        _ => url,
    })
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, GatewayError> {
    require_any_gateway_key(&headers, &state).await?;
    Ok(Json(json!({ "platform_mcps": platform_mcps() })))
}

pub async fn serve(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Query(query): Query<PlatformMcpQuery>,
    Json(request): Json<JsonRpcRequest>,
) -> Result<Json<Value>, GatewayError> {
    let session_id = query
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|session_id| !session_id.is_empty());
    let auth = crate::http::mcp_invocation_auth::authenticate_request(&state, &headers, session_id)
        .await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let active_grant = if let Some(session_id) = session_id {
        let session = sessions::repository::get(pool, session_id)
            .await?
            .ok_or_else(|| GatewayError::NotFound("session not found".to_owned()))?;
        if !auth.is_admin && session.owner_id.as_deref() != Some(auth.user_id.as_str()) {
            return Err(GatewayError::NotFound("session not found".to_owned()));
        }
        let Some(snapshot) = session_control::repository::active_turn(pool, session_id).await?
        else {
            return Ok(Json(rpc_error(
                request.id,
                -32001,
                "active invocation MCP grant is required",
            )));
        };
        let Some(invocation) = snapshot.invocations.first() else {
            return Ok(Json(rpc_error(
                request.id,
                -32001,
                "active invocation MCP grant is required",
            )));
        };
        let grant = mcp_invocation_grants::repository::active_for_invocation(
            pool,
            &invocation.id,
            PLATFORM_MCP_SERVER_NAME,
        )
        .await?;
        let Some(grant) = grant else {
            return Ok(Json(rpc_error(
                request.id,
                -32001,
                "active invocation MCP grant is required",
            )));
        };
        Some(grant)
    } else {
        None
    };
    let requested_tool = (request.method == "tools/call")
        .then(|| {
            request
                .params
                .as_ref()
                .and_then(|params| params.get("name"))
                .and_then(Value::as_str)
        })
        .flatten();
    if let (Some(grant), Some(tool_name)) = (active_grant.as_ref(), requested_tool) {
        if !grant.allows(tool_name) {
            return Ok(Json(rpc_error(
                request.id,
                -32602,
                "tool is not allowed by the invocation MCP grant",
            )));
        }
        if !mcp_invocation_grants::repository::mark_used(pool, &grant.id).await? {
            return Ok(Json(rpc_error(
                request.id,
                -32001,
                "active invocation MCP grant is required",
            )));
        }
        audit::record(
            pool,
            &auth.user_id,
            "mcp.tool_call",
            "mcp_invocation_grant",
            &grant.id,
            json!({
                "session_id": grant.session_id,
                "turn_id": grant.turn_id,
                "invocation_id": grant.invocation_id,
                "server_id": grant.server_id,
                "tool_names": [tool_name],
            }),
        )
        .await?;
    }
    let response = match request.method.as_str() {
        "initialize" => initialize_response(request.id),
        "tools/list" => {
            let mut tools = definitions::tool_defs();
            if let Some(grant) = active_grant.as_ref() {
                tools.retain(|tool| {
                    tool.get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|name| grant.allows(name))
                });
            }
            json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "result": { "tools": tools }
            })
        }
        "tools/call" => {
            let Some(params) = request.params else {
                return Ok(Json(rpc_error(request.id, -32602, "params are required")));
            };
            let result = call_tool(
                state.clone(),
                pool,
                &agent_id,
                query.session_id.as_deref(),
                params,
            )
            .await?;
            json!({ "jsonrpc": "2.0", "id": request.id, "result": result })
        }
        "notifications/initialized" => json!({
            "jsonrpc": "2.0",
            "id": request.id,
            "result": {}
        }),
        _ => rpc_error(request.id, -32601, "method not found"),
    };
    Ok(Json(response))
}

async fn call_tool(
    state: Arc<AppState>,
    pool: &PgPool,
    agent_id: &str,
    session_id: Option<&str>,
    params: Value,
) -> Result<Value, GatewayError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| GatewayError::InvalidJsonMessage("tool name is required".to_owned()))?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let payload = match name {
        PLATFORM_SESSION_MCP_ID => {
            session_management::read_platform_session(pool, arguments).await?
        }
        SEND_PLATFORM_SESSION_MESSAGE_MCP_ID => {
            session_management::send_platform_session_message(
                state.clone(),
                pool.clone(),
                arguments,
            )
            .await?
        }
        AGENT_MEMORY_MCP_ID => tools::agent_memory(pool, agent_id, arguments).await?,
        EDIT_AGENT_SKILL_MCP_ID => skill::edit_agent_skill(pool, agent_id, arguments).await?,
        CREATE_MANAGED_AGENT_MCP_ID => {
            factory::create_managed_agent(state.as_ref(), pool, arguments).await?
        }
        LIST_SUB_AGENTS_MCP_ID => tools::list_sub_agents(pool, agent_id).await?,
        RUN_SUB_AGENT_MCP_ID => {
            tools::run_sub_agent(state.clone(), pool.clone(), agent_id, arguments).await?
        }
        REQUEST_HUMAN_APPROVAL_MCP_ID => {
            approval::request_human_approval(state.as_ref(), pool, agent_id, session_id, arguments)
                .await?
        }
        CHECK_HUMAN_APPROVAL_MCP_ID => approval::check_human_approval(pool, arguments).await?,
        EXPOSE_PORT_MCP_ID => {
            expose_port::expose_port(state.as_ref(), pool, agent_id, session_id, arguments).await?
        }
        _ => {
            return Ok(json!({
                "isError": true,
                "content": [{ "type": "text", "text": format!("unknown tool: {name}") }]
            }))
        }
    };
    Ok(json!({
        "content": [{ "type": "text", "text": serde_json::to_string_pretty(&payload)? }]
    }))
}

pub(crate) fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str, GatewayError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| GatewayError::InvalidJsonMessage(format!("{field} is required")))
}

pub(super) fn public_base_url(state: &AppState) -> Result<String, GatewayError> {
    proxy_base_url(state)
}

fn proxy_base_url(state: &AppState) -> Result<String, GatewayError> {
    state.resolved_mcp_proxy_base_url().ok_or_else(|| {
        GatewayError::InvalidConfig(
            "mcp_servers.proxy_base_url is required for platform MCPs".to_owned(),
        )
    })
}

fn rpc_error(id: Option<Value>, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

fn initialize_response(id: Option<Value>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2025-06-18",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "litellm-platform", "version": env!("CARGO_PKG_VERSION") }
        }
    })
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct PlatformMcpQuery {
    pub session_id: Option<String>,
}
