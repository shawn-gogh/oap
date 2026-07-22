use std::{future::Future, pin::Pin};

use serde::Serialize;
use serde_json::{json, Value};

pub type ImportAgentsFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ImportAgentsError>> + Send + 'a>>;

#[derive(Debug)]
pub enum ImportAgentsError {
    Request(reqwest::Error),
    Upstream { status: u16, body: String },
    Decode(serde_json::Error),
    InvalidDocument(String),
}

impl From<reqwest::Error> for ImportAgentsError {
    fn from(error: reqwest::Error) -> Self {
        Self::Request(error)
    }
}

impl From<serde_json::Error> for ImportAgentsError {
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

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ImportProviderCapabilities {
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

/// Provider-neutral interaction contract persisted with every imported agent.
/// Provider modules translate their discovery payload into this shape once;
/// session execution can then snapshot it without knowing which provider was
/// used to import the agent.
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

pub trait ImportAgentsProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn api_spec(&self) -> &'static str;

    /// Connector-level protocol version before discovery negotiation. A
    /// provider may override this only when the transport contract itself is
    /// pinned; otherwise evidence from discovery must populate the negotiated
    /// profile instead of inventing a version.
    fn protocol_version(&self) -> &'static str {
        "unverified"
    }

    /// Whether this source protocol is also a user-selectable general runtime.
    /// Per-agent bridges such as A2A and Dify stay out of the runtime dropdown.
    fn expose_runtime_harness(&self) -> bool {
        true
    }

    fn requires_session_workspace(&self) -> bool {
        false
    }

    /// Normalize provider discovery evidence into the canonical interaction
    /// shape used by Run. Providers override this only for capabilities they
    /// can substantiate from their protocol or discovery response.
    fn interaction_contract(&self, _raw: &Value) -> ImportedInteractionContract {
        ImportedInteractionContract::default()
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
    ) -> ImportAgentsFuture<'a, Vec<ImportedAgent>>;

    fn default_model(&self, model: Option<&str>) -> String;
    fn system_prompt(&self, external_agent_id: &str) -> String;

    /// System prompt for an imported agent, with access to its raw discovery
    /// payload. Defaults to [`Self::system_prompt`]; providers that carry the
    /// real upstream prompt in `raw` (e.g. opencode) override this to preserve
    /// it instead of emitting a placeholder.
    fn system_prompt_from_raw(&self, external_agent_id: &str, _raw: &Value) -> String {
        self.system_prompt(external_agent_id)
    }
}
