use serde_json::Value;

use crate::sdk::providers::import_agents::{
    ImportAgentsError, ImportAgentsFuture, ImportAgentsProvider, ImportProviderCapabilities,
    ImportedAgent, ImportedInteractionContract,
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

    fn interaction_contract(&self, _raw: &Value) -> ImportedInteractionContract {
        ImportedInteractionContract {
            primary_surface: "run",
            execution_mode: "blocking",
            progress_mode: "status",
            artifact_media_types: vec!["*/*".to_owned()],
            ..Default::default()
        }
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

/// Every POST operation in the spec, as `(path, summary)`, in document order.
///
/// Unlike a LangGraph state schema — which describes shapes but not which field
/// carries the user's turn — an OpenAPI document enumerates the callable routes
/// outright. The runtime path is therefore a closed set the operator can pick
/// from rather than free text they have to transcribe correctly, and no network
/// call is needed: the spec was captured whole at import.
///
/// POST-only because that is what `invoke_openapi` issues.
pub fn runtime_paths(spec: &Value) -> Vec<(String, Option<String>)> {
    let Some(paths) = spec.get("paths").and_then(Value::as_object) else {
        return Vec::new();
    };
    paths
        .iter()
        .filter_map(|(path, item)| {
            let operation = item.get("post")?;
            // Server-relative routes only: `invoke_openapi` rejects anything
            // that is not a site-absolute path, so offering one would just
            // reproduce an error the operator cannot act on.
            if !path.starts_with('/') || path.starts_with("//") {
                return None;
            }
            let summary = operation
                .get("summary")
                .or_else(|| operation.get("operationId"))
                .or_else(|| operation.get("description"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            Some((path.clone(), summary))
        })
        .collect()
}

/// Extract the request and successful-response JSON Schemas for the POST
/// operation selected by the operator-confirmed runtime path.
pub fn runtime_schemas(spec: &Value, path: &str) -> (Option<Value>, Option<Value>) {
    let operation = spec
        .pointer(&format!("/paths/{}", escape_pointer(path)))
        .and_then(|path_item| path_item.get("post"));
    let input = operation
        .and_then(|operation| operation.get("requestBody"))
        .and_then(|body| body.get("content"))
        .and_then(json_content_schema)
        .map(|schema| resolve_local_schema(spec, schema));
    let output = operation
        .and_then(|operation| operation.get("responses"))
        .and_then(Value::as_object)
        .and_then(|responses| {
            ["200", "201", "202", "default"]
                .iter()
                .find_map(|status| responses.get(*status))
                .or_else(|| {
                    responses
                        .iter()
                        .find(|(status, _)| status.starts_with('2'))
                        .map(|(_, response)| response)
                })
        })
        .and_then(|response| response.get("content"))
        .and_then(json_content_schema)
        .map(|schema| resolve_local_schema(spec, schema));
    (input, output)
}

fn json_content_schema(content: &Value) -> Option<&Value> {
    let content = content.as_object()?;
    content
        .get("application/json")
        .or_else(|| {
            content
                .iter()
                .find(|(media_type, _)| media_type.ends_with("+json"))
                .map(|(_, value)| value)
        })?
        .get("schema")
}

fn resolve_local_schema(spec: &Value, schema: &Value) -> Value {
    let Some(reference) = schema.get("$ref").and_then(Value::as_str) else {
        return schema.clone();
    };
    let Some(pointer) = reference.strip_prefix('#') else {
        return schema.clone();
    };
    spec.pointer(pointer)
        .cloned()
        .unwrap_or_else(|| schema.clone())
}

fn escape_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{parse_spec, runtime_schemas};

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

    #[test]
    fn extracts_confirmed_operation_schemas() {
        let spec = json!({
            "openapi": "3.1.0",
            "paths": {"/agents/run": {"post": {
                "requestBody": {"content": {"application/json": {
                    "schema": {"$ref": "#/components/schemas/AgentInput"}
                }}},
                "responses": {"200": {"content": {"application/json": {
                    "schema": {"type": "object", "required": ["answer"]}
                }}}}
            }}},
            "components": {"schemas": {"AgentInput": {
                "type": "object", "required": ["message"]
            }}}
        });

        let (input, output) = runtime_schemas(&spec, "/agents/run");
        assert_eq!(input.unwrap()["required"][0], "message");
        assert_eq!(output.unwrap()["required"][0], "answer");
    }

    #[test]
    fn runtime_paths_lists_only_callable_post_routes() {
        // Shaped after the crewai-native fixture, which mixes the executable
        // POST route with GET-only endpoints the bridge can never call.
        let spec = json!({
            "openapi": "3.1.0",
            "info": {"title": "Self-hosted CrewAI research crew"},
            "paths": {
                "/health": {"get": {"summary": "Health"}},
                "/api/v1/agents": {"get": {"summary": "List agents"}},
                "/api/v1/kickoffs": {"post": {"summary": "Kickoff"}}
            }
        });

        assert_eq!(
            super::runtime_paths(&spec),
            vec![("/api/v1/kickoffs".to_owned(), Some("Kickoff".to_owned()))]
        );
    }

    #[test]
    fn runtime_paths_falls_back_to_operation_id_and_skips_unusable_routes() {
        let spec = json!({
            "openapi": "3.1.0",
            "info": {"title": "API"},
            "paths": {
                "/run": {"post": {"operationId": "runAgent"}},
                "/bare": {"post": {}},
                // invoke_openapi rejects non-site-absolute paths, so offering
                // them would only reproduce an error the operator cannot fix.
                "https://elsewhere.test/run": {"post": {"summary": "Absolute"}},
                "//protocol-relative": {"post": {"summary": "Protocol relative"}}
            }
        });

        let paths = super::runtime_paths(&spec);

        assert_eq!(
            paths,
            vec![
                ("/bare".to_owned(), None),
                ("/run".to_owned(), Some("runAgent".to_owned())),
            ]
        );
    }

    #[test]
    fn runtime_paths_is_empty_without_a_paths_object() {
        assert!(super::runtime_paths(&json!({"openapi": "3.1.0"})).is_empty());
    }
}
