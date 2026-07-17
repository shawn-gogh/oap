use serde_json::Value;

use crate::sdk::providers::import_agents::{
    ImportAgentsError, ImportAgentsFuture, ImportAgentsProvider, ImportProviderCapabilities,
    ImportedAgent,
};

pub static OPENAPI_IMPORT_AGENTS: OpenApiImportAgents = OpenApiImportAgents;

pub struct OpenApiImportAgents;

impl ImportAgentsProvider for OpenApiImportAgents {
    fn id(&self) -> &'static str {
        "openapi"
    }

    fn name(&self) -> &'static str {
        "OpenAPI / REST"
    }

    fn api_spec(&self) -> &'static str {
        "openapi_rest"
    }

    fn expose_runtime_harness(&self) -> bool {
        false
    }

    fn capabilities(&self) -> ImportProviderCapabilities {
        ImportProviderCapabilities {
            discover: true,
            remote_import: true,
            file_import: true,
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
            let spec_url = if endpoint.ends_with(".json") || endpoint.ends_with(".yaml") {
                endpoint.to_owned()
            } else {
                format!("{}/openapi.json", endpoint.trim_end_matches('/'))
            };
            let mut request = http.get(spec_url).header("accept", "application/json");
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
            let raw: Value = match serde_json::from_str(&body) {
                Ok(value) => value,
                Err(json_error) => serde_yaml::from_str(&body).map_err(|yaml_error| {
                    ImportAgentsError::InvalidDocument(format!(
                        "JSON: {json_error}; YAML: {yaml_error}"
                    ))
                })?,
            };
            Ok(parse_spec(endpoint, raw).into_iter().collect())
        })
    }

    fn default_model(&self, model: Option<&str>) -> String {
        model.unwrap_or("openapi-mapped").to_owned()
    }

    fn system_prompt(&self, external_agent_id: &str) -> String {
        format!(
            "Use the operator-confirmed OpenAPI runtime mapping for service {external_agent_id}."
        )
    }
}

fn parse_spec(endpoint: &str, raw: Value) -> Option<ImportedAgent> {
    let version = raw.get("openapi")?.as_str()?;
    if !version.starts_with("3.") {
        return None;
    }
    let info = raw.get("info")?;
    let name = info.get("title")?.as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    Some(ImportedAgent {
        id: endpoint.to_owned(),
        name: name.to_owned(),
        description: info
            .get("description")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        model: None,
        provider: "openapi".to_owned(),
        raw,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::parse_spec;

    #[test]
    fn accepts_openapi_three_and_preserves_spec() {
        let agent = parse_spec(
            "https://api.test",
            json!({"openapi": "3.1.0", "info": {"title": "Agent API"}, "paths": {}}),
        )
        .unwrap();
        assert_eq!(agent.id, "https://api.test");
        assert_eq!(agent.name, "Agent API");
    }

    #[test]
    fn rejects_swagger_two() {
        assert!(parse_spec(
            "https://api.test",
            json!({"swagger": "2.0", "info": {"title": "Legacy"}})
        )
        .is_none());
    }
}
