use serde_json::Value;

use crate::sdk::providers::import_agents::{
    ImportAgentsError, ImportAgentsFuture, ImportAgentsProvider, ImportProviderCapabilities,
    ImportedAgent, ImportedInteractionContract,
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

    fn interaction_contract(&self, raw: &Value) -> ImportedInteractionContract {
        let mode = raw.get("mode").and_then(Value::as_str).unwrap_or("chat");
        let workflow = mode.contains("workflow");
        let workflow_surface = workflow || mode == "advanced-chat";
        ImportedInteractionContract {
            primary_surface: if workflow { "run" } else { "conversation" },
            execution_mode: "async_stream",
            input_schema: input_schema(raw),
            output_schema: if workflow {
                serde_json::json!({"type": "object"})
            } else {
                serde_json::json!({"type": "string"})
            },
            progress_mode: if workflow_surface { "steps" } else { "status" },
            continuation_modes: if workflow_surface {
                vec![
                    "input".to_owned(),
                    "approval".to_owned(),
                    "file_upload".to_owned(),
                    "choice".to_owned(),
                ]
            } else {
                Vec::new()
            },
            artifact_media_types: vec![
                "text/plain".to_owned(),
                "application/json".to_owned(),
                "image/*".to_owned(),
                "audio/*".to_owned(),
                "video/*".to_owned(),
                "application/pdf".to_owned(),
            ],
            supports_checkpoint_resume: workflow_surface,
            supports_child_invocations: workflow_surface,
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
            let mut raw: Value = serde_json::from_str(&body)?;
            if let Ok(response) = http
                .get(format!("{}/parameters", endpoint.trim_end_matches('/')))
                .bearer_auth(api_key)
                .header("accept", "application/json")
                .send()
                .await
            {
                if response.status().is_success() {
                    if let Ok(parameters) = response.json::<Value>().await {
                        raw["parameters"] = parameters;
                    }
                }
            }
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

fn input_schema(raw: &Value) -> Value {
    let forms = raw
        .pointer("/parameters/user_input_form")
        .and_then(Value::as_array);
    let Some(forms) = forms else {
        return serde_json::json!({
            "type": "object",
            "properties": {"message": {"type": "string"}},
            "required": ["message"]
        });
    };
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for wrapper in forms {
        let Some(field) = wrapper
            .as_object()
            .and_then(|wrapper| wrapper.values().next())
        else {
            continue;
        };
        let Some(variable) = field.get("variable").and_then(Value::as_str) else {
            continue;
        };
        let kind = wrapper
            .as_object()
            .and_then(|wrapper| wrapper.keys().next())
            .map(String::as_str)
            .unwrap_or("text-input");
        let mut schema = match kind {
            "number" => serde_json::json!({"type": "number"}),
            "checkbox" => serde_json::json!({"type": "boolean"}),
            "file" => serde_json::json!({"type": "object"}),
            "file-list" => serde_json::json!({"type": "array", "items": {"type": "object"}}),
            "select" => serde_json::json!({
                "type": "string",
                "enum": field.get("options").cloned().unwrap_or_else(|| serde_json::json!([]))
            }),
            _ => serde_json::json!({"type": "string"}),
        };
        if let Some(label) = field.get("label").and_then(Value::as_str) {
            schema["title"] = Value::String(label.to_owned());
        }
        if let Some(max_length) = field.get("max_length").and_then(Value::as_u64) {
            schema["maxLength"] = Value::Number(max_length.into());
        }
        if field.get("required").and_then(Value::as_bool) == Some(true) {
            required.push(Value::String(variable.to_owned()));
        }
        properties.insert(variable.to_owned(), schema);
    }
    if properties.is_empty() {
        return serde_json::json!({"type": "object"});
    }
    serde_json::json!({"type": "object", "properties": properties, "required": required})
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

    use super::{input_schema, parse_app, DifyImportAgents};
    use crate::sdk::providers::import_agents::ImportAgentsProvider;

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

    #[test]
    fn derives_workflow_input_schema_and_run_contract() {
        let raw = json!({
            "mode": "workflow",
            "parameters": {"user_input_form": [
                {"paragraph": {"variable": "topic", "label": "Topic", "required": true}},
                {"select": {"variable": "tone", "options": ["brief", "detailed"]}}
            ]}
        });
        let schema = input_schema(&raw);
        assert_eq!(schema["required"], json!(["topic"]));
        assert_eq!(
            schema["properties"]["tone"]["enum"],
            json!(["brief", "detailed"])
        );
        let contract = DifyImportAgents.interaction_contract(&raw);
        assert_eq!(contract.primary_surface, "run");
        assert_eq!(contract.execution_mode, "async_stream");
        assert_eq!(contract.progress_mode, "steps");
        assert!(contract.supports_checkpoint_resume);
        assert!(contract.supports_child_invocations);
    }
}
