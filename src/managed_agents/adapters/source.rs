use std::{future::Future, pin::Pin};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub type SourceAdapterFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, SourceAdapterError>> + Send + 'a>>;

#[derive(Debug, thiserror::Error)]
pub enum SourceAdapterError {
    #[error("source request failed: {0}")]
    Request(reqwest::Error),
    #[error("source returned HTTP {status}: {body}")]
    Upstream { status: u16, body: String },
    #[error("source document could not be decoded: {0}")]
    Decode(serde_json::Error),
    #[error("source document is invalid: {0}")]
    InvalidDocument(String),
}

impl From<reqwest::Error> for SourceAdapterError {
    fn from(error: reqwest::Error) -> Self {
        Self::Request(error)
    }
}

impl From<serde_json::Error> for SourceAdapterError {
    fn from(error: serde_json::Error) -> Self {
        Self::Decode(error)
    }
}

#[derive(Debug, Clone)]
pub struct ImportedAgent {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub model: Option<String>,
    pub provider: String,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NegotiatedProtocolInterface {
    pub url: String,
    pub protocol_version: String,
    pub protocol_binding: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NegotiatedSourceProfile {
    pub protocol: String,
    pub protocol_version: String,
    pub protocol_binding: String,
    pub interface_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    pub document_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_version: Option<String>,
    pub selection_reason: String,
    pub advertised_interfaces: Vec<NegotiatedProtocolInterface>,
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub push_notifications: bool,
    #[serde(default)]
    pub extended_agent_card: bool,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub required_extensions: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SourceCapabilities {
    pub discover: bool,
    pub remote_import: bool,
    pub file_import: bool,
    pub bundle_import: bool,
    pub continuous_sync: bool,
    pub incremental_sync: bool,
    pub native_health: bool,
    pub remote_suspend: bool,
    pub remote_delete: bool,
    pub signed_webhooks: bool,
    pub runtime_contract: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportedInteractionContract {
    pub schema_version: u16,
    pub primary_surface: &'static str,
    pub execution_mode: &'static str,
    pub input_schema: Value,
    pub output_schema: Value,
    pub progress_mode: &'static str,
    pub continuation_modes: Vec<String>,
    pub accepted_input_types: Vec<String>,
    pub artifact_media_types: Vec<String>,
    pub supports_retry: bool,
    pub supports_checkpoint_resume: bool,
    pub supports_child_invocations: bool,
}

impl Default for ImportedInteractionContract {
    fn default() -> Self {
        Self {
            schema_version: 1,
            primary_surface: "conversation",
            execution_mode: "async_stream",
            input_schema: json!({"type": "object"}),
            output_schema: json!({}),
            progress_mode: "none",
            continuation_modes: Vec::new(),
            accepted_input_types: vec!["application/json".to_owned(), "text/plain".to_owned()],
            artifact_media_types: Vec::new(),
            supports_retry: true,
            supports_checkpoint_resume: false,
            supports_child_invocations: false,
        }
    }
}

pub trait SourceAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn api_spec(&self) -> &'static str;

    fn protocol_version(&self) -> &'static str {
        "unverified"
    }

    fn expose_runtime_harness(&self) -> bool {
        true
    }

    fn requires_session_workspace(&self) -> bool {
        false
    }

    fn interaction_contract(&self, _raw: &Value) -> ImportedInteractionContract {
        ImportedInteractionContract::default()
    }

    fn negotiate_protocol(
        &self,
        _endpoint: &str,
        _raw: &Value,
    ) -> Result<Option<NegotiatedSourceProfile>, SourceAdapterError> {
        Ok(None)
    }

    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
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
    ) -> SourceAdapterFuture<'a, Vec<ImportedAgent>>;

    fn default_model(&self, model: Option<&str>) -> String;
    fn system_prompt(&self, external_agent_id: &str) -> String;

    fn system_prompt_from_raw(&self, external_agent_id: &str, _raw: &Value) -> String {
        self.system_prompt(external_agent_id)
    }
}
