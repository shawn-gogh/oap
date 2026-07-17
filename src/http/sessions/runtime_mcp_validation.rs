use serde_json::Value;
use sqlx::PgPool;

use crate::{errors::GatewayError, proxy::state::AppState};

/// Registered MCP servers attached to an agent store the *raw* upstream URL,
/// which may carry `${VAR}` placeholders resolved per-user at call time (e.g.
/// Composio's `${COMPOSIO_USER_ID}` / `${COMPOSIO_MCP_SERVER_ID}`). The
/// managed-agents runtime (Anthropic) calls the MCP URL directly and rejects an
/// unresolved `${...}` URL as an invalid URI. So any templated entry is
/// rewritten to route through this gateway's own MCP proxy
/// (`{proxy_base}/{name}/mcp`), which resolves the caller's vault variables,
/// injects the server's static headers, and forwards upstream — the same path
/// tool discovery already uses. The proxy requires a gateway key, tracked on
/// the entry as `authorization_token` so runtime session provisioning can seed
/// the provider's MCP credential vault. Provider adapters strip this field from
/// the agent definition body. `name` is the server id; the dynamic proxy
/// resolves it by id, name, or alias.
///
/// v0: on-behalf-of identity is the default owner. Per-user identity over this
/// path is tracked separately (signed-token auth) — see issue.
pub(super) async fn rewrite_registered_mcp_servers(
    state: &AppState,
    pool: &PgPool,
    servers: &mut [Value],
    session_id: Option<&str>,
    mcp_auth_token: Option<&str>,
) -> Result<(), GatewayError> {
    for server in servers.iter_mut() {
        let Some(obj) = server.as_object_mut() else {
            continue;
        };
        let source_url = obj.get("url").and_then(Value::as_str).unwrap_or_default();
        if source_url.contains("/mcp/platform/") {
            continue;
        }
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .filter(|n| !n.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| {
                GatewayError::InvalidConfig(
                    "managed runtime MCP entries require a registered server name".to_owned(),
                )
            })?;
        if crate::db::mcp_servers::repository::get_by_name(pool, &name)
            .await?
            .is_none()
        {
            return Err(GatewayError::InvalidConfig(format!(
                "managed runtime MCP server {name} must be registered before use"
            )));
        }
        let base = state.resolved_mcp_proxy_base_url().ok_or_else(|| {
            GatewayError::InvalidConfig(
                "mcp_servers.proxy_base_url is required to proxy MCP servers with variables"
                    .to_owned(),
            )
        })?;
        let mut proxy_url =
            reqwest::Url::parse(&format!("{}/{}/mcp", base.trim_end_matches('/'), name)).map_err(
                |error| GatewayError::InvalidConfig(format!("invalid MCP proxy URL: {error}")),
            )?;
        if let Some(session_id) = session_id
            .map(str::trim)
            .filter(|session_id| !session_id.is_empty())
        {
            proxy_url
                .query_pairs_mut()
                .append_pair("session_id", session_id);
        }
        obj.insert("url".to_owned(), Value::String(proxy_url.to_string()));
        match mcp_auth_token {
            Some(token) => {
                obj.insert(
                    "authorization_token".to_owned(),
                    Value::String(token.to_owned()),
                );
            }
            None => {
                obj.remove("authorization_token");
            }
        }
    }
    Ok(())
}

pub(super) fn validate_runtime_mcp_servers(
    agent_id: &str,
    servers: &[Value],
) -> Result<(), GatewayError> {
    for (index, server) in servers.iter().enumerate() {
        let Some(server) = server.as_object() else {
            return Err(GatewayError::InvalidConfig(format!(
                "{agent_id} config.mcp_servers.{index} must be an object"
            )));
        };
        let server_type = server.get("type").and_then(Value::as_str).unwrap_or("url");
        if server_type != "url" {
            continue;
        }
        let Some(url) = server
            .get("url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|url| !url.is_empty())
        else {
            return Err(GatewayError::InvalidConfig(format!(
                "{agent_id} config.mcp_servers.{index}.url is required"
            )));
        };
        // Reject unresolved `${VAR}` placeholders: reqwest::Url::parse tolerates
        // them (percent-encoding), but the managed-agents runtime rejects such a
        // URL as an invalid URI. Templated registered servers must have been
        // rewritten to the gateway proxy by `rewrite_registered_mcp_servers`.
        if url.contains("${") {
            return Err(GatewayError::InvalidConfig(format!(
                "{agent_id} config.mcp_servers.{index}.url contains unresolved variables; \
                 it must be proxied through the gateway or fully resolved"
            )));
        }
        let parsed = reqwest::Url::parse(url).map_err(|_| {
            GatewayError::InvalidConfig(format!(
                "{agent_id} config.mcp_servers.{index}.url must be an absolute http(s) URL"
            ))
        })?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return Err(GatewayError::InvalidConfig(format!(
                "{agent_id} config.mcp_servers.{index}.url must be an absolute http(s) URL"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::validate_runtime_mcp_servers;

    #[test]
    fn validates_runtime_mcp_server_urls() {
        let err = validate_runtime_mcp_servers(
            "agent_1",
            &[json!({
                "name": "gmail",
                "type": "url",
                "url": "gmail"
            })],
        )
        .unwrap_err()
        .to_string();

        assert!(
            err.contains("agent_1 config.mcp_servers.0.url must be an absolute http(s) URL"),
            "got: {err}"
        );

        validate_runtime_mcp_servers(
            "agent_1",
            &[json!({
                "name": "gmail",
                "type": "url",
                "url": "https://mcp.composio.dev/gmail"
            })],
        )
        .unwrap();
    }
}
