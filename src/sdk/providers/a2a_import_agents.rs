use serde_json::Value;

use crate::sdk::providers::import_agents::{
    ImportAgentsError, ImportAgentsFuture, ImportAgentsProvider, ImportProviderCapabilities,
    ImportedAgent, ImportedInteractionContract,
};

pub static A2A_IMPORT_AGENTS: A2aImportAgents = A2aImportAgents;

pub struct A2aImportAgents;

impl ImportAgentsProvider for A2aImportAgents {
    fn id(&self) -> &'static str {
        "a2a"
    }

    fn name(&self) -> &'static str {
        "A2A"
    }

    fn api_spec(&self) -> &'static str {
        "a2a_v1"
    }

    fn expose_runtime_harness(&self) -> bool {
        false
    }

    fn interaction_contract(&self, raw: &Value) -> ImportedInteractionContract {
        let streaming = raw
            .pointer("/capabilities/streaming")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        ImportedInteractionContract {
            execution_mode: if streaming {
                "async_stream"
            } else {
                "async_poll"
            },
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"message": {"type": "string"}},
                "required": ["message"]
            }),
            progress_mode: "status",
            continuation_modes: vec![
                "input".to_owned(),
                "approval".to_owned(),
                "authentication".to_owned(),
            ],
            accepted_input_types: card_modes(
                raw,
                "defaultInputModes",
                &["application/json", "text/plain"],
            ),
            artifact_media_types: card_modes(raw, "defaultOutputModes", &["*/*"]),
            supports_checkpoint_resume: true,
            ..Default::default()
        }
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
            let url = format!(
                "{}/.well-known/agent-card.json",
                endpoint.trim_end_matches('/')
            );
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
            Ok(parse_agent_card(endpoint, raw).into_iter().collect())
        })
    }

    fn default_model(&self, model: Option<&str>) -> String {
        model.unwrap_or("a2a-remote").to_owned()
    }

    fn system_prompt(&self, external_agent_id: &str) -> String {
        format!("Route this request to the governed A2A agent {external_agent_id}.")
    }
}

fn card_modes(raw: &Value, field: &str, fallback: &[&str]) -> Vec<String> {
    raw.get(field)
        .and_then(Value::as_array)
        .map(|modes| {
            modes
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|mode| !mode.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|modes| !modes.is_empty())
        .unwrap_or_else(|| fallback.iter().map(|mode| (*mode).to_owned()).collect())
}

fn parse_agent_card(endpoint: &str, raw: Value) -> Option<ImportedAgent> {
    let name = raw.get("name")?.as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    let id = raw
        .get("id")
        .and_then(Value::as_str)
        .or_else(|| raw.get("url").and_then(Value::as_str))
        .unwrap_or(endpoint)
        .trim()
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
        provider: "a2a".to_owned(),
        raw,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{parse_agent_card, A2A_IMPORT_AGENTS};
    use crate::sdk::providers::import_agents::ImportAgentsProvider;

    #[test]
    fn parses_agent_card_with_stable_url_identity() {
        let agent = parse_agent_card(
            "https://example.test/agent",
            json!({
                "name": "Threat analyst",
                "description": "Assesses open-source intelligence",
                "url": "https://example.test/a2a",
                "version": "1.2.0",
                "skills": [{"id": "threat-assessment", "name": "Threat assessment"}]
            }),
        )
        .unwrap();
        assert_eq!(agent.id, "https://example.test/a2a");
        assert_eq!(agent.name, "Threat analyst");
    }

    #[test]
    fn interaction_contract_uses_agent_card_transport_evidence() {
        let contract = A2A_IMPORT_AGENTS.interaction_contract(&json!({
            "capabilities": {"streaming": true},
            "defaultInputModes": ["text/plain", "image/png"],
            "defaultOutputModes": ["application/json"]
        }));

        assert_eq!(contract.execution_mode, "async_stream");
        assert_eq!(contract.accepted_input_types, ["text/plain", "image/png"]);
        assert_eq!(contract.artifact_media_types, ["application/json"]);
    }
}
