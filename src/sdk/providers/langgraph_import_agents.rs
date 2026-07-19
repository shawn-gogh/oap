use serde_json::{json, Value};

use crate::sdk::providers::import_agents::{
    ImportAgentsError, ImportAgentsFuture, ImportAgentsProvider, ImportProviderCapabilities,
    ImportedAgent,
};

pub static LANGGRAPH_IMPORT_AGENTS: LangGraphImportAgents = LangGraphImportAgents;

pub struct LangGraphImportAgents;

impl ImportAgentsProvider for LangGraphImportAgents {
    fn id(&self) -> &'static str {
        "langgraph"
    }

    fn name(&self) -> &'static str {
        "LangGraph / LangSmith"
    }

    fn api_spec(&self) -> &'static str {
        "langgraph_assistant"
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
            incremental_sync: true,
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
            let url = format!("{}/assistants/search", endpoint.trim_end_matches('/'));
            let mut request = http
                .post(url)
                .header("accept", "application/json")
                .json(&json!({"limit": 1000, "offset": 0}));
            if !api_key.is_empty() {
                request = request.bearer_auth(api_key).header("x-api-key", api_key);
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
            parse_assistants(raw)
        })
    }

    fn default_model(&self, model: Option<&str>) -> String {
        model.unwrap_or("langgraph-managed").to_owned()
    }

    fn system_prompt(&self, external_agent_id: &str) -> String {
        format!(
            "Use the operator-confirmed LangGraph execution mapping for assistant {external_agent_id}."
        )
    }
}

fn parse_assistants(raw: Value) -> Result<Vec<ImportedAgent>, ImportAgentsError> {
    let values = raw.as_array().ok_or_else(|| {
        ImportAgentsError::InvalidDocument(
            "LangGraph assistant search response must be an array".to_owned(),
        )
    })?;
    Ok(values.iter().filter_map(parse_assistant).collect())
}

fn parse_assistant(raw: &Value) -> Option<ImportedAgent> {
    let id = text_at(raw, "/assistant_id")?;
    let name = text_at(raw, "/name")
        .or_else(|| text_at(raw, "/graph_id"))
        .unwrap_or(id);
    let model = [
        "/context/model",
        "/context/model_name",
        "/config/configurable/model",
        "/config/configurable/model_name",
    ]
    .into_iter()
    .find_map(|path| text_at(raw, path))
    .map(str::to_owned);
    Some(ImportedAgent {
        id: id.to_owned(),
        name: name.to_owned(),
        description: text_at(raw, "/description").map(str::to_owned),
        model,
        provider: "langgraph".to_owned(),
        raw: raw.clone(),
    })
}

fn text_at<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::parse_assistants;

    #[test]
    fn parses_assistant_identity_and_model() {
        let agents = parse_assistants(json!([{
            "assistant_id": "assistant-1",
            "graph_id": "research",
            "name": "Research",
            "description": "Find evidence",
            "config": {"configurable": {"model": "openai/gpt-4.1"}}
        }]))
        .unwrap();

        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].id, "assistant-1");
        assert_eq!(agents[0].name, "Research");
        assert_eq!(agents[0].model.as_deref(), Some("openai/gpt-4.1"));
    }

    #[test]
    fn skips_assistants_without_stable_identity() {
        let agents = parse_assistants(json!([
            {"name": "Missing ID"},
            {"assistant_id": "kept", "graph_id": "fallback-name"}
        ]))
        .unwrap();

        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "fallback-name");
    }
}
