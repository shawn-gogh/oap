use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    db::managed_agents::{messages, sessions::schema::SessionRow},
    errors::GatewayError,
};

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub(super) title: Option<String>,
    pub(super) harness: Option<String>,
    pub(super) agent: Option<String>,
    pub(super) agent_id: Option<String>,
    pub(super) runtime: Option<String>,
    pub(super) model: Option<String>,
    pub(super) prompt: Option<String>,
    pub(super) environment: Option<Value>,
    pub(super) timezone: Option<String>,
    pub(super) tz: Option<String>,
    pub(super) task_id: Option<String>,
}

impl CreateSessionRequest {
    pub(super) fn has_runtime(&self) -> bool {
        self.runtime.is_some()
    }
}

#[derive(Debug)]
pub(super) struct ResolvedSession {
    pub(super) title: String,
    pub(super) harness: String,
    pub(super) agent_id: Option<String>,
    pub(super) timezone: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PromptRequest {
    request_id: Option<String>,
    model: Option<PromptModel>,
    parts: Option<Vec<PromptPart>>,
    input: Option<Value>,
}

impl PromptRequest {
    pub(super) fn prompt_text(&self) -> Result<String, GatewayError> {
        let text = self
            .parts
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter_map(|part| match part {
                PromptPart::Text { text } => Some(text.as_str()),
                PromptPart::Other => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        if text.trim().is_empty() {
            return Err(GatewayError::InvalidJsonMessage(
                "prompt text is required".to_owned(),
            ));
        }
        Ok(text)
    }

    pub(super) fn run_input(&self) -> Result<Value, GatewayError> {
        if let Some(input) = self.input.as_ref() {
            if !input.is_object() {
                return Err(GatewayError::InvalidJsonMessage(
                    "run input must be a JSON object".to_owned(),
                ));
            }
            if input.as_object().is_some_and(serde_json::Map::is_empty) {
                return Err(GatewayError::InvalidJsonMessage(
                    "run input must not be empty".to_owned(),
                ));
            }
            return Ok(input.clone());
        }
        Ok(serde_json::json!({"message": self.prompt_text()?}))
    }

    pub(super) fn execution_prompt(&self) -> Result<String, GatewayError> {
        let input = self.run_input()?;
        execution_prompt_for_input(&input)
    }

    pub(super) fn has_structured_input(&self) -> bool {
        self.input.is_some()
    }

    pub(super) fn model_id(&self) -> Option<&str> {
        self.model
            .as_ref()
            .map(|model| model.model_id.trim())
            .filter(|model_id| !model_id.is_empty())
    }

    pub(super) fn request_id(&self) -> Option<&str> {
        self.request_id
            .as_deref()
            .map(str::trim)
            .filter(|request_id| !request_id.is_empty())
    }
}

pub(super) fn execution_prompt_for_input(input: &Value) -> Result<String, GatewayError> {
    for field in ["message", "prompt", "request", "query", "input"] {
        if let Some(text) = input
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            return Ok(text.to_owned());
        }
    }
    serde_json::to_string_pretty(input).map_err(GatewayError::from)
}

#[derive(Debug, Deserialize)]
struct PromptModel {
    #[serde(rename = "modelID")]
    model_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PromptPart {
    Text {
        text: String,
    },
    #[serde(other)]
    Other,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::PromptRequest;

    #[test]
    fn model_id_trims_and_ignores_blank_values() {
        let blank: PromptRequest = serde_json::from_value(json!({
            "model": { "modelID": "  " },
            "parts": [{ "type": "text", "text": "hi" }]
        }))
        .unwrap();
        let trimmed: PromptRequest = serde_json::from_value(json!({
            "model": { "modelID": " cursor-model " },
            "parts": [{ "type": "text", "text": "hi" }]
        }))
        .unwrap();

        assert_eq!(blank.model_id(), None);
        assert_eq!(trimmed.model_id(), Some("cursor-model"));
    }

    #[test]
    fn structured_input_is_preserved_and_produces_execution_prompt() {
        let request: PromptRequest = serde_json::from_value(json!({
            "input": {"topic": "agents", "depth": 3}
        }))
        .unwrap();

        assert_eq!(
            request.run_input().unwrap(),
            json!({"topic": "agents", "depth": 3})
        );
        assert!(request.execution_prompt().unwrap().contains("\"depth\": 3"));
    }

    #[test]
    fn legacy_text_parts_map_to_message_input() {
        let request: PromptRequest = serde_json::from_value(json!({
            "parts": [{"type": "text", "text": "hello"}]
        }))
        .unwrap();

        assert_eq!(request.run_input().unwrap(), json!({"message": "hello"}));
    }
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    id: String,
    title: String,
    agent: String,
    agent_id: Option<String>,
    harness: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime_agent_ref_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_bucket: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    task_id: Option<String>,
    attempt_number: i32,
    status: String,
    environment: Value,
    time: SessionTime,
}

impl SessionResponse {
    pub(crate) fn id(&self) -> &str {
        &self.id
    }
}

impl From<SessionRow> for SessionResponse {
    fn from(row: SessionRow) -> Self {
        Self {
            id: row.id,
            title: row.title,
            agent: row.agent_id.clone().unwrap_or_else(|| row.harness.clone()),
            agent_id: row.agent_id,
            harness: row.harness,
            runtime: row.runtime,
            runtime_agent_ref_id: row.runtime_agent_ref_id,
            provider_session_id: row.provider_session_id,
            provider_run_id: row.provider_run_id,
            workspace_bucket: row.workspace_bucket,
            owner_id: row.owner_id,
            task_id: row.task_id,
            attempt_number: row.attempt_number,
            status: row.status,
            environment: row.environment_json,
            time: SessionTime {
                created: row.created_at,
                updated: row.updated_at,
            },
        }
    }
}

#[derive(Debug, Serialize)]
struct SessionTime {
    created: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    info: Value,
    parts: Value,
}

impl TryFrom<messages::schema::SessionMessageRow> for MessageResponse {
    type Error = GatewayError;

    fn try_from(row: messages::schema::SessionMessageRow) -> Result<Self, Self::Error> {
        Ok(Self {
            info: serde_json::from_str(&row.info_json)?,
            parts: serde_json::from_str(&row.parts_json)?,
        })
    }
}
