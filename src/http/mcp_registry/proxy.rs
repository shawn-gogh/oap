mod credentials;
mod headers;
mod policy;

use std::{collections::HashSet, sync::Arc};

use axum::{
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::{HeaderMap, Method, StatusCode},
    response::Response,
};
use futures_util::TryStreamExt;
use serde::Deserialize;
use serde_json::json;

use crate::{
    db::{
        managed_agents::{audit, mcp_invocation_grants, session_control, sessions},
        mcp_servers::repository,
    },
    errors::GatewayError,
    proxy::{credential_crypto, state::AppState},
};

use super::substitute_vars;

/// `GET|POST|PUT|DELETE|PATCH /{mcp_server_name}/mcp`
///
/// Proxies MCP protocol traffic to the registered upstream server, injecting
/// the calling user's credential (personal vault key, falling back to the
/// server's own stored credential).
pub async fn dynamic_mcp(
    State(state): State<Arc<AppState>>,
    Path(server_name): Path<String>,
    Query(query): Query<McpProxyQuery>,
    headers: HeaderMap,
    method: Method,
    body: Bytes,
) -> Result<Response, GatewayError> {
    let session_id = query
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|session_id| !session_id.is_empty());
    let auth = crate::http::mcp_invocation_auth::authenticate_request(&state, &headers, session_id)
        .await?;

    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let server = repository::get_by_name(pool, &server_name)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("MCP server '{server_name}' not found")))?;
    let global_tools = policy::allowed_tools(&server.allowed_tools);
    let mut allowed_tools = global_tools.clone();
    let mut allow_all = global_tools.is_empty();
    let mcp_request = policy::parse_mcp_request(&body);
    let active_grant = if let Some(session_id) = session_id {
        let session = sessions::repository::get(pool, session_id)
            .await?
            .ok_or_else(|| GatewayError::NotFound("session not found".to_owned()))?;
        if !auth.is_admin && session.owner_id.as_deref() != Some(auth.user_id.as_str()) {
            return Err(GatewayError::NotFound("session not found".to_owned()));
        }
        let Some(snapshot) = session_control::repository::active_turn(pool, session_id).await?
        else {
            return Ok(policy::grant_required_response());
        };
        let Some(invocation) = snapshot.invocations.first() else {
            return Ok(policy::grant_required_response());
        };
        let Some(grant) = mcp_invocation_grants::repository::active_for_invocation(
            pool,
            &invocation.id,
            &server.server_id,
        )
        .await?
        else {
            return Ok(policy::grant_required_response());
        };
        if grant.allow_all {
            allowed_tools = global_tools.clone();
            allow_all = global_tools.is_empty();
        } else {
            let grant_tools = grant.tool_names();
            allowed_tools = if global_tools.is_empty() {
                grant_tools
            } else {
                grant_tools.intersection(&global_tools).cloned().collect()
            };
            allow_all = false;
        }
        Some(grant)
    } else {
        None
    };
    if let Some(response) = policy::reject_disallowed_call(&mcp_request, &allowed_tools, allow_all)
    {
        return Ok(response);
    }
    if policy::has_tool_call(&mcp_request) {
        if let Some(grant) = active_grant.as_ref() {
            if !mcp_invocation_grants::repository::mark_used(pool, &grant.id).await? {
                return Ok(policy::grant_required_response());
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
                    "tool_names": policy::tool_names(&mcp_request),
                }),
            )
            .await?;
        }
    }

    let base_url = server
        .url
        .as_deref()
        .filter(|u| !u.trim().is_empty())
        .ok_or_else(|| {
            GatewayError::InvalidJsonMessage("MCP server has no URL configured".to_owned())
        })?;
    let user_id = active_grant
        .as_ref()
        .map(|grant| grant.owner_id.clone())
        .unwrap_or_else(|| super::caller_user_id(&headers, &state));
    let enc_key =
        credential_crypto::encryption_key(state.config.general_settings.master_key.as_deref())?;
    let vars = credentials::resolve_variables(pool, &server, &user_id, &enc_key).await?;
    // Substitute ${VAR_NAME} in the URL (e.g. parameterized server IDs). Keep
    // the trailing slash intact: streamable-HTTP MCP servers live at `/mcp/`,
    // and stripping it makes the server redirect, dropping the Authorization
    // header on the way (a bogus "Malformed API Key" upstream failure).
    let target_url = substitute_vars(base_url, &vars);

    let mut req = headers::build_outbound_request(
        &state.http,
        method,
        &target_url,
        &headers,
        &server.static_headers,
        &vars,
    )?;
    if !headers::has_static_headers(&server.static_headers) {
        if let Some(cred) =
            credentials::resolve_auth_credential(&state, pool, &server, &user_id, &enc_key).await?
        {
            req = headers::apply_auth(req, server.auth_type.as_deref(), &cred);
        }
    }
    if !body.is_empty() {
        req = req.body(body);
    }

    let upstream = req.send().await.map_err(GatewayError::Upstream)?;
    response_from_upstream(upstream, &mcp_request, &allowed_tools, allow_all).await
}

#[derive(Debug, Default, Deserialize)]
pub struct McpProxyQuery {
    session_id: Option<String>,
}

async fn response_from_upstream(
    upstream: reqwest::Response,
    mcp_request: &policy::McpRequest,
    allowed_tools: &HashSet<String>,
    allow_all: bool,
) -> Result<Response, GatewayError> {
    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    if policy::should_filter_tools_list(mcp_request, status, allow_all) {
        let headers = headers::copy_response_headers(upstream.headers());
        let content_type = upstream
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let text = upstream.text().await.map_err(GatewayError::Upstream)?;
        let filtered =
            policy::filter_tools_list_payload(&text, &content_type, allowed_tools, allow_all);
        let mut response = Response::new(Body::from(filtered));
        *response.status_mut() = status;
        *response.headers_mut() = headers;
        return Ok(response);
    }

    let resp_headers = headers::copy_response_headers(upstream.headers());
    let stream = upstream.bytes_stream().map_err(std::io::Error::other);
    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = status;
    *response.headers_mut() = resp_headers;
    Ok(response)
}
