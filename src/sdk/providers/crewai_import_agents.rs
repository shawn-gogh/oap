use serde_json::Value;

use crate::sdk::providers::import_agents::{
    ImportAgentsError, ImportAgentsFuture, ImportAgentsProvider, ImportProviderCapabilities,
    ImportedAgent,
};

pub static CREWAI_IMPORT_AGENTS: CrewAiImportAgents = CrewAiImportAgents;

pub struct CrewAiImportAgents;

impl ImportAgentsProvider for CrewAiImportAgents {
    fn id(&self) -> &'static str {
        "crewai"
    }

    fn name(&self) -> &'static str {
        "CrewAI AMP"
    }

    fn api_spec(&self) -> &'static str {
        "crewai_crew"
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
            let url = inputs_url(endpoint);
            let mut request = http.get(url).header("accept", "application/json");
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
            Ok(vec![parse_deployment(endpoint, raw)])
        })
    }

    fn default_model(&self, model: Option<&str>) -> String {
        model.unwrap_or("crewai-managed").to_owned()
    }

    fn system_prompt(&self, external_agent_id: &str) -> String {
        format!(
            "Use the operator-confirmed CrewAI kickoff mapping for deployment {external_agent_id}."
        )
    }
}

fn inputs_url(endpoint: &str) -> String {
    let endpoint = endpoint.trim_end_matches('/');
    if endpoint.ends_with("/inputs") {
        endpoint.to_owned()
    } else {
        format!("{endpoint}/inputs")
    }
}

fn parse_deployment(endpoint: &str, raw: Value) -> ImportedAgent {
    let id = first_text(&raw, &["id", "crew_id", "deployment_id"]).unwrap_or(endpoint);
    let name = first_text(&raw, &["name", "crew_name", "title"])
        .unwrap_or_else(|| endpoint_name(endpoint));
    ImportedAgent {
        id: id.to_owned(),
        name: name.to_owned(),
        description: first_text(&raw, &["description"]).map(str::to_owned),
        model: first_text(&raw, &["model", "model_name"]).map(str::to_owned),
        provider: "crewai".to_owned(),
        raw,
    }
}

fn first_text<'a>(value: &'a Value, fields: &[&str]) -> Option<&'a str> {
    fields.iter().find_map(|field| {
        value
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
    })
}

fn endpoint_name(endpoint: &str) -> &str {
    endpoint
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("CrewAI deployment")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{inputs_url, parse_deployment};

    #[test]
    fn normalizes_inputs_endpoint() {
        assert_eq!(
            inputs_url("https://crew.test/deployment"),
            "https://crew.test/deployment/inputs"
        );
        assert_eq!(
            inputs_url("https://crew.test/deployment/inputs/"),
            "https://crew.test/deployment/inputs"
        );
    }

    #[test]
    fn parses_deployment_metadata_when_available() {
        let agent = parse_deployment(
            "https://crew.test/my-crew",
            json!({
                "crew_id": "crew-1",
                "name": "Research Crew",
                "description": "Find evidence",
                "inputs": [{"name": "topic"}]
            }),
        );

        assert_eq!(agent.id, "crew-1");
        assert_eq!(agent.name, "Research Crew");
    }

    #[test]
    fn uses_endpoint_as_stable_deployment_identity() {
        let endpoint = "https://crew.test/deployments/research";
        let agent = parse_deployment(endpoint, json!([{"name": "topic"}]));

        assert_eq!(agent.id, endpoint);
        assert_eq!(agent.name, "research");
    }
}
