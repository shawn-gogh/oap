use serde_json::json;

use super::{types::McpCapabilityGrant, AdapterError, AdapterFuture, McpAdapter};

#[derive(Default)]
pub struct InvocationMcpAdapter;

impl McpAdapter for InvocationMcpAdapter {
    fn project_grant<'a>(
        &'a self,
        grant: &'a McpCapabilityGrant,
    ) -> AdapterFuture<'a, serde_json::Value> {
        Box::pin(async move {
            if grant.session_id.trim().is_empty()
                || grant.turn_id.trim().is_empty()
                || grant.invocation_id.trim().is_empty()
            {
                return Err(AdapterError::InvalidConfiguration(
                    "MCP grant requires session, turn, and invocation identifiers".to_owned(),
                ));
            }
            if grant.expires_at <= crate::db::managed_agents::now_ms() {
                return Err(AdapterError::InvalidConfiguration(
                    "MCP grant is expired".to_owned(),
                ));
            }
            let servers = grant
                .server_ids
                .iter()
                .map(|server_id| {
                    json!({
                        "server_id": server_id,
                        "allowed_tools": grant.tool_allowlist.get(server_id).cloned().unwrap_or_default(),
                        "allow_all": grant.allow_all_servers.iter().any(|id| id == server_id),
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!({
                "version": "1",
                "session_id": grant.session_id,
                "turn_id": grant.turn_id,
                "invocation_id": grant.invocation_id,
                "expires_at": grant.expires_at,
                "servers": servers,
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::InvocationMcpAdapter;
    use crate::managed_agents::adapters::{types::McpCapabilityGrant, McpAdapter};

    #[tokio::test]
    async fn projects_explicit_server_and_tool_scope() {
        let grant = McpCapabilityGrant {
            session_id: "ses_1".to_owned(),
            turn_id: "turn_1".to_owned(),
            invocation_id: "inv_1".to_owned(),
            server_ids: vec!["mcp_1".to_owned()],
            tool_allowlist: HashMap::from([("mcp_1".to_owned(), vec!["read".to_owned()])]),
            allow_all_servers: Vec::new(),
            expires_at: crate::db::managed_agents::now_ms() + 60_000,
        };
        let value = InvocationMcpAdapter
            .project_grant(&grant)
            .await
            .expect("projection");
        assert_eq!(value["servers"][0]["allowed_tools"][0], "read");
        assert_eq!(value["servers"][0]["allow_all"], false);
    }
}
