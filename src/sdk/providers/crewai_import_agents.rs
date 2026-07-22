use serde_json::{json, Map, Value};

use crate::sdk::providers::import_agents::{
    ImportAgentsError, ImportAgentsFuture, ImportAgentsProvider, ImportProviderCapabilities,
    ImportedAgent, ImportedInteractionContract,
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

    fn interaction_contract(&self, raw: &Value) -> ImportedInteractionContract {
        ImportedInteractionContract {
            primary_surface: "run",
            execution_mode: "async_poll",
            input_schema: crew_input_schema(raw),
            progress_mode: "steps",
            artifact_media_types: vec!["application/json".to_owned(), "text/plain".to_owned()],
            supports_child_invocations: true,
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
                if status == reqwest::StatusCode::NOT_FOUND
                    && exposes_openapi_document(http, endpoint, api_key).await
                {
                    return Err(ImportAgentsError::InvalidDocument(
                        "检测到自托管 CrewAI HTTP 应用，但 CrewAI OSS 不定义统一的远程发现/执行协议；请改用 OpenAPI / REST 来源导入该服务并确认运行映射。CrewAI 来源仅用于具备 /inputs、/kickoff、/status/{id} 合同的 AMP 部署。"
                            .to_owned(),
                    ));
                }
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

async fn exposes_openapi_document(http: &reqwest::Client, endpoint: &str, api_key: &str) -> bool {
    let mut request = http
        .get(format!("{}/openapi.json", endpoint.trim_end_matches('/')))
        .header("accept", "application/json");
    if !api_key.is_empty() {
        request = request.bearer_auth(api_key);
    }
    let Ok(response) = request.send().await else {
        return false;
    };
    if !response.status().is_success() {
        return false;
    }
    response
        .json::<Value>()
        .await
        .ok()
        .and_then(|document| {
            document
                .get("openapi")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .is_some_and(|version| version.starts_with("3."))
}

fn crew_input_schema(raw: &Value) -> Value {
    let inputs = raw
        .get("inputs")
        .and_then(Value::as_array)
        .or_else(|| raw.as_array());
    let Some(inputs) = inputs else {
        return json!({"type": "object"});
    };
    let mut properties = Map::new();
    let mut required = Vec::new();
    for input in inputs {
        let Some(name) = input
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
        else {
            continue;
        };
        let schema = input
            .get("schema")
            .cloned()
            .unwrap_or_else(|| json!({"type": "string"}));
        properties.insert(name.to_owned(), schema);
        if input.get("required").and_then(Value::as_bool) != Some(false) {
            required.push(Value::String(name.to_owned()));
        }
    }
    json!({"type": "object", "properties": properties, "required": required})
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

    use super::{crew_input_schema, inputs_url, parse_deployment};

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

    #[test]
    fn converts_discovered_inputs_to_json_schema() {
        let schema = crew_input_schema(&json!({
            "inputs": [
                {"name": "topic"},
                {"name": "limit", "schema": {"type": "integer"}, "required": false}
            ]
        }));

        assert_eq!(schema["properties"]["topic"]["type"], "string");
        assert_eq!(schema["properties"]["limit"]["type"], "integer");
        assert_eq!(schema["required"], json!(["topic"]));
    }
}
