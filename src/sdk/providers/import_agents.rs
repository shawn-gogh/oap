use std::{future::Future, pin::Pin};

use serde::Serialize;
use serde_json::Value;

pub type ImportAgentsFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ImportAgentsError>> + Send + 'a>>;

#[derive(Debug)]
pub enum ImportAgentsError {
    Request(reqwest::Error),
    Upstream { status: u16, body: String },
    Decode(serde_json::Error),
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

pub trait ImportAgentsProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn api_spec(&self) -> &'static str;

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
