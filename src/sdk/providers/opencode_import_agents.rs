//! One-click import of agents from a running opencode runtime.
//!
//! opencode (the LAP runtime template under `templates/opencode`) persists its
//! agents and exposes them over `GET /v1/agents`, returning
//! `{ "data": [ { id, name, description, model: { id }, system, ... } ] }`.
//! This provider discovers those agents so they can be re-created as LAP
//! managed agents that run on the opencode harness.
//!
//! It is a standalone file (not a `providers/<name>/` directory) so `build.rs`
//! — which only auto-wires provider directories — leaves it alone; it is opt-in
//! via the registry in `http/managed_agents/import.rs`.

use serde_json::Value;

use crate::sdk::providers::import_agents::{
    ImportAgentsError, ImportAgentsFuture, ImportAgentsProvider, ImportProviderCapabilities,
    ImportedAgent,
};

pub static OPENCODE_IMPORT_AGENTS: OpencodeImportAgents = OpencodeImportAgents;

pub struct OpencodeImportAgents;

impl ImportAgentsProvider for OpencodeImportAgents {
    fn id(&self) -> &'static str {
        "opencode"
    }

    fn name(&self) -> &'static str {
        "OpenCode"
    }

    /// opencode speaks the Anthropic Managed Agents surface, registered in LAP
    /// as the `claude_managed_agents` api_spec. The concrete runtime an imported
    /// agent defaults to is the harness alias resolved from the endpoint (see
    /// `import.rs`); this api_spec is only the fallback.
    fn api_spec(&self) -> &'static str {
        "claude_managed_agents"
    }

    fn capabilities(&self) -> ImportProviderCapabilities {
        ImportProviderCapabilities {
            discover: true,
            remote_import: true,
            file_import: true,
            bundle_import: true,
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
            let mut request = http
                .get(format!("{endpoint}/v1/agents"))
                .header("accept", "application/json");
            if !api_key.is_empty() {
                // opencode honors Anthropic-style auth headers when present.
                request = request.header("x-api-key", api_key);
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
            let values = raw
                .get("data")
                .and_then(Value::as_array)
                .cloned()
                .or_else(|| raw.as_array().cloned())
                .or_else(|| raw.get("agents").and_then(Value::as_array).cloned())
                .unwrap_or_default();
            Ok(values
                .into_iter()
                .filter_map(|raw| external_agent(self.id(), raw))
                .collect())
        })
    }

    fn default_model(&self, model: Option<&str>) -> String {
        // This deployment routes to DeepSeek. Keep an explicit DeepSeek model if
        // the opencode agent already used one; otherwise fall back to
        // deepseek-chat (opencode's Anthropic/OpenAI models can't be routed by
        // the DeepSeek-only gateway).
        match model.map(str::trim).filter(|value| !value.is_empty()) {
            Some(value) if value.contains("deepseek") => value.to_owned(),
            _ => "deepseek-chat".to_owned(),
        }
    }

    fn system_prompt(&self, external_agent_id: &str) -> String {
        format!("Imported opencode agent (external id: {external_agent_id}).")
    }

    fn system_prompt_from_raw(&self, external_agent_id: &str, raw: &Value) -> String {
        raw.get("system")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| self.system_prompt(external_agent_id))
    }
}

fn external_agent(provider: &str, raw: Value) -> Option<ImportedAgent> {
    let id = raw.get("id").and_then(Value::as_str)?.trim().to_owned();
    if id.is_empty() {
        return None;
    }
    let name = raw
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(id.as_str())
        .to_owned();
    let description = raw
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    // opencode serializes model as `{ "id": "..." }`; tolerate a bare string too.
    let model = raw
        .get("model")
        .and_then(|model| {
            model
                .get("id")
                .and_then(Value::as_str)
                .or_else(|| model.as_str())
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    Some(ImportedAgent {
        id,
        name,
        description,
        model,
        provider: provider.to_owned(),
        raw,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_opencode_agent_payload() {
        let raw = json!({
            "id": "agent_123",
            "name": "Code Reviewer",
            "description": "Reviews diffs",
            "model": { "id": "anthropic/claude-sonnet-4-5" },
            "system": "You are a meticulous reviewer."
        });
        let agent = external_agent("opencode", raw).unwrap();
        assert_eq!(agent.id, "agent_123");
        assert_eq!(agent.name, "Code Reviewer");
        assert_eq!(agent.description.as_deref(), Some("Reviews diffs"));
        assert_eq!(agent.model.as_deref(), Some("anthropic/claude-sonnet-4-5"));
    }

    #[test]
    fn falls_back_to_id_when_name_missing() {
        let agent = external_agent("opencode", json!({ "id": "a1" })).unwrap();
        assert_eq!(agent.name, "a1");
        assert!(agent.model.is_none());
    }

    #[test]
    fn skips_entries_without_id() {
        assert!(external_agent("opencode", json!({ "name": "no id" })).is_none());
    }

    #[test]
    fn default_model_maps_non_deepseek_to_deepseek_chat() {
        assert_eq!(
            OPENCODE_IMPORT_AGENTS.default_model(Some("anthropic/claude-sonnet-4-5")),
            "deepseek-chat"
        );
        assert_eq!(
            OPENCODE_IMPORT_AGENTS.default_model(Some("deepseek-reasoner")),
            "deepseek-reasoner"
        );
        assert_eq!(OPENCODE_IMPORT_AGENTS.default_model(None), "deepseek-chat");
    }

    #[test]
    fn system_prompt_from_raw_prefers_real_prompt() {
        let raw = json!({ "system": "You are a reviewer." });
        assert_eq!(
            OPENCODE_IMPORT_AGENTS.system_prompt_from_raw("a1", &raw),
            "You are a reviewer."
        );
        // Empty/absent system falls back to the placeholder.
        assert_eq!(
            OPENCODE_IMPORT_AGENTS.system_prompt_from_raw("a1", &json!({})),
            "Imported opencode agent (external id: a1)."
        );
    }

    #[test]
    fn declares_supported_import_modes() {
        let capabilities = OPENCODE_IMPORT_AGENTS.capabilities();

        assert!(capabilities.discover);
        assert!(capabilities.remote_import);
        assert!(capabilities.file_import);
        assert!(capabilities.bundle_import);
        assert!(capabilities.continuous_sync);
        assert_eq!(capabilities.runtime_contract, "claude_managed_agents");
    }
}
