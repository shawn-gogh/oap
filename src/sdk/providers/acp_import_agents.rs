use serde_json::Value;

use crate::sdk::providers::import_agents::{
    ImportAgentsError, ImportAgentsFuture, ImportAgentsProvider, ImportProviderCapabilities,
    ImportedAgent,
};

pub static ACP_IMPORT_AGENTS: AcpImportAgents = AcpImportAgents;

pub struct AcpImportAgents;

impl ImportAgentsProvider for AcpImportAgents {
    fn id(&self) -> &'static str {
        "acp"
    }

    fn name(&self) -> &'static str {
        "ACP (Legacy)"
    }

    fn api_spec(&self) -> &'static str {
        "acp_legacy"
    }

    fn expose_runtime_harness(&self) -> bool {
        false
    }

    fn capabilities(&self) -> ImportProviderCapabilities {
        ImportProviderCapabilities {
            discover: true,
            remote_import: true,
            file_import: false,
            bundle_import: false,
            continuous_sync: true,
            incremental_sync: false,
            native_health: false,
            remote_suspend: false,
            remote_delete: false,
            signed_webhooks: false,
            runtime_contract: self.api_spec(),
        }
    }

    fn discover<'a>(
        &'a self,
        http: &'a reqwest::Client,
        endpoint: &'a str,
        api_key: &'a str,
    ) -> ImportAgentsFuture<'a, Vec<ImportedAgent>> {
        Box::pin(async move {
            let mut request = http
                .get(format!("{}/agents", endpoint.trim_end_matches('/')))
                .header("accept", "application/json");
            if !api_key.is_empty() {
                request = request.bearer_auth(api_key);
            }
            let response = request.send().await?;
            let status = response.status();
            let body = response.text().await?;
            if !status.is_success() {
                return Err(ImportAgentsError::Upstream {
                    status: status.as_u16(),
                    body,
                });
            }
            let raw: Value = serde_json::from_str(&body)?;
            Ok(parse_agents(raw))
        })
    }

    fn default_model(&self, model: Option<&str>) -> String {
        model.unwrap_or("acp-remote").to_owned()
    }

    fn system_prompt(&self, external_agent_id: &str) -> String {
        format!("Route this request through the pinned ACP compatibility profile for {external_agent_id}.")
    }
}

fn parse_agents(raw: Value) -> Vec<ImportedAgent> {
    raw.as_array()
        .or_else(|| raw.get("agents").and_then(Value::as_array))
        .or_else(|| raw.get("data").and_then(Value::as_array))
        .into_iter()
        .flatten()
        .filter_map(|value| {
            let id = value.get("id")?.as_str()?.trim();
            if id.is_empty() {
                return None;
            }
            Some(ImportedAgent {
                id: id.to_owned(),
                name: value
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .unwrap_or(id)
                    .to_owned(),
                description: value
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                model: value
                    .get("model")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                provider: "acp".to_owned(),
                raw: value.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::parse_agents;

    #[test]
    fn parses_common_acp_agent_collection() {
        let agents = parse_agents(json!({"agents": [{"id": "risk", "name": "Risk"}]}));
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].id, "risk");
    }
}
