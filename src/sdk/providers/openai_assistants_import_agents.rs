use serde_json::Value;

use crate::sdk::providers::import_agents::{
    ImportAgentsError, ImportAgentsFuture, ImportAgentsProvider, ImportProviderCapabilities,
    ImportedAgent,
};

pub static OPENAI_ASSISTANTS_IMPORT_AGENTS: OpenAiAssistantsImportAgents =
    OpenAiAssistantsImportAgents;

pub struct OpenAiAssistantsImportAgents;

impl ImportAgentsProvider for OpenAiAssistantsImportAgents {
    fn id(&self) -> &'static str {
        "openai_assistants"
    }

    fn name(&self) -> &'static str {
        "OpenAI Assistants (legacy)"
    }

    fn api_spec(&self) -> &'static str {
        "openai_assistant"
    }

    fn protocol_version(&self) -> &'static str {
        "assistants=v2"
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
            let url = assistants_url(endpoint);
            let mut agents = Vec::new();
            let mut after: Option<String> = None;
            for page_index in 0..100 {
                let mut request = http
                    .get(&url)
                    .bearer_auth(api_key)
                    .header("accept", "application/json")
                    .header("openai-beta", "assistants=v2")
                    .query(&[("order", "asc"), ("limit", "100")]);
                if let Some(cursor) = after.as_deref() {
                    request = request.query(&[("after", cursor)]);
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
                let page: Value = serde_json::from_str(&body)?;
                let (mut discovered, next) = parse_page(page)?;
                agents.append(&mut discovered);
                let Some(next) = next else {
                    break;
                };
                if page_index == 99 {
                    return Err(ImportAgentsError::InvalidDocument(
                        "OpenAI Assistants discovery exceeded 100 pages".to_owned(),
                    ));
                }
                if after.as_deref() == Some(next.as_str()) {
                    return Err(ImportAgentsError::InvalidDocument(
                        "OpenAI Assistants pagination cursor did not advance".to_owned(),
                    ));
                }
                after = Some(next);
            }
            Ok(agents)
        })
    }

    fn default_model(&self, model: Option<&str>) -> String {
        model.unwrap_or("openai-managed").to_owned()
    }

    fn system_prompt(&self, external_agent_id: &str) -> String {
        format!(
            "Use the operator-confirmed migration mapping for OpenAI assistant {external_agent_id}."
        )
    }

    fn system_prompt_from_raw(&self, external_agent_id: &str, raw: &Value) -> String {
        text_at(raw, "instructions")
            .map(str::to_owned)
            .unwrap_or_else(|| self.system_prompt(external_agent_id))
    }
}

fn assistants_url(endpoint: &str) -> String {
    let endpoint = endpoint.trim_end_matches('/');
    if endpoint.ends_with("/v1/assistants") {
        endpoint.to_owned()
    } else if endpoint.ends_with("/v1") {
        format!("{endpoint}/assistants")
    } else {
        format!("{endpoint}/v1/assistants")
    }
}

fn parse_page(page: Value) -> Result<(Vec<ImportedAgent>, Option<String>), ImportAgentsError> {
    let values = page.get("data").and_then(Value::as_array).ok_or_else(|| {
        ImportAgentsError::InvalidDocument(
            "OpenAI Assistants list response must contain a data array".to_owned(),
        )
    })?;
    let agents = values.iter().filter_map(parse_assistant).collect();
    let next = if page
        .get("has_more")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        page.get("last_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|cursor| !cursor.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| {
                ImportAgentsError::InvalidDocument(
                    "OpenAI Assistants response has_more without last_id".to_owned(),
                )
            })?
            .into()
    } else {
        None
    };
    Ok((agents, next))
}

fn parse_assistant(raw: &Value) -> Option<ImportedAgent> {
    let id = text_at(raw, "id")?;
    Some(ImportedAgent {
        id: id.to_owned(),
        name: text_at(raw, "name").unwrap_or(id).to_owned(),
        description: text_at(raw, "description").map(str::to_owned),
        model: text_at(raw, "model").map(str::to_owned),
        provider: "openai_assistants".to_owned(),
        raw: raw.clone(),
    })
}

fn text_at<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{assistants_url, parse_page};

    #[test]
    fn normalizes_assistants_endpoint() {
        assert_eq!(
            assistants_url("https://api.openai.com"),
            "https://api.openai.com/v1/assistants"
        );
        assert_eq!(
            assistants_url("https://gateway.test/v1"),
            "https://gateway.test/v1/assistants"
        );
    }

    #[test]
    fn parses_assistants_and_next_cursor() {
        let (agents, cursor) = parse_page(json!({
            "data": [{
                "id": "asst_1",
                "name": "Research",
                "model": "gpt-4.1",
                "instructions": "Find primary sources."
            }],
            "has_more": true,
            "last_id": "asst_1"
        }))
        .unwrap();

        assert_eq!(agents[0].id, "asst_1");
        assert_eq!(agents[0].model.as_deref(), Some("gpt-4.1"));
        assert_eq!(cursor.as_deref(), Some("asst_1"));
    }
}
