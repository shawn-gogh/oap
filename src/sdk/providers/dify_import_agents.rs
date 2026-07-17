use serde_json::Value;

use crate::sdk::providers::import_agents::{
    ImportAgentsError, ImportAgentsFuture, ImportAgentsProvider, ImportProviderCapabilities,
    ImportedAgent,
};

pub static DIFY_IMPORT_AGENTS: DifyImportAgents = DifyImportAgents;

pub struct DifyImportAgents;

impl ImportAgentsProvider for DifyImportAgents {
    fn id(&self) -> &'static str {
        "dify"
    }

    fn name(&self) -> &'static str {
        "Dify"
    }

    fn api_spec(&self) -> &'static str {
        "dify_app"
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
            native_health: true,
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
            let response = http
                .get(format!("{}/info", endpoint.trim_end_matches('/')))
                .bearer_auth(api_key)
                .header("accept", "application/json")
                .send()
                .await?;
            let status = response.status();
            let body = response.text().await?;
            if !status.is_success() {
                return Err(ImportAgentsError::Upstream {
                    status: status.as_u16(),
                    body,
                });
            }
            let raw: Value = serde_json::from_str(&body)?;
            Ok(parse_app(endpoint, raw).into_iter().collect())
        })
    }

    fn default_model(&self, model: Option<&str>) -> String {
        model.unwrap_or("dify-managed").to_owned()
    }

    fn system_prompt(&self, external_agent_id: &str) -> String {
        format!("Route this request to the governed Dify application {external_agent_id}.")
    }
}

fn parse_app(endpoint: &str, raw: Value) -> Option<ImportedAgent> {
    let name = raw.get("name")?.as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    let id = raw
        .get("id")
        .or_else(|| raw.get("app_id"))
        .and_then(Value::as_str)
        .unwrap_or(endpoint)
        .to_owned();
    Some(ImportedAgent {
        id,
        name: name.to_owned(),
        description: raw
            .get("description")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        model: None,
        provider: "dify".to_owned(),
        raw,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::parse_app;

    #[test]
    fn uses_endpoint_when_dify_hides_app_id() {
        let agent = parse_app(
            "https://dify.test/v1",
            json!({"name": "Research", "description": "OSINT workflow"}),
        )
        .unwrap();
        assert_eq!(agent.id, "https://dify.test/v1");
        assert_eq!(agent.name, "Research");
    }
}
