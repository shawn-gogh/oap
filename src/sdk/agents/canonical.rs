use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::managed_agents::registry::schema::ManagedAgentRow;

pub const CANONICAL_SPEC_VERSION: &str = "2026-07-16";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CanonicalAgentSpec {
    pub spec_version: String,
    pub identity: CanonicalIdentity,
    pub execution: CanonicalExecution,
    pub instructions: CanonicalInstructions,
    pub capabilities: CanonicalCapabilities,
    pub requirements: CanonicalRequirements,
    pub policies: CanonicalPolicies,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CanonicalIdentity {
    pub platform_agent_id: String,
    pub external_agent_id: Option<String>,
    pub source_provider: Option<String>,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CanonicalExecution {
    pub runtime: Option<String>,
    pub model: String,
    pub harness: String,
    pub max_runtime_minutes: i32,
    pub on_failure: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CanonicalInstructions {
    pub system: String,
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CanonicalCapabilities {
    pub tools: Vec<Value>,
    pub skill_ids: Vec<String>,
    pub rule_ids: Vec<String>,
    pub mcp_server_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CanonicalRequirements {
    pub vault_keys: Vec<String>,
    pub setup_commands: Vec<String>,
    pub network_access: Vec<String>,
    pub filesystem_access: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CanonicalPolicies {
    pub schedule: Option<CanonicalSchedule>,
    pub declared_side_effects: Vec<String>,
    pub approval_required: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CanonicalSchedule {
    pub cron: String,
    pub timezone: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NormalizationSeverity {
    Info,
    Warning,
    ApprovalRequired,
    Blocking,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NormalizationIssue {
    pub severity: NormalizationSeverity,
    pub code: String,
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NormalizationReport {
    pub spec: CanonicalAgentSpec,
    pub issues: Vec<NormalizationIssue>,
    pub can_import: bool,
    pub requires_approval: bool,
}

pub fn normalize_agent(agent: &ManagedAgentRow) -> NormalizationReport {
    let source = agent.config.get("source");
    let runtime = agent
        .config
        .get("runtime")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let tools = agent.tools.as_array().cloned().unwrap_or_default();
    let mut issues = Vec::new();

    if agent.model.trim().is_empty() {
        issues.push(issue(
            NormalizationSeverity::Blocking,
            "model_missing",
            "execution.model",
            "未声明执行模型。",
        ));
    }
    if !agent.tools.is_array() {
        issues.push(issue(
            NormalizationSeverity::Blocking,
            "tools_invalid",
            "capabilities.tools",
            "工具声明必须是数组。",
        ));
    }
    if runtime.is_none() {
        issues.push(issue(
            NormalizationSeverity::Info,
            "runtime_implicit",
            "execution.runtime",
            "未声明外部运行时，将使用平台内置执行路径。",
        ));
    }

    let raw = source.and_then(|value| value.get("raw"));
    for key in unhandled_high_risk_fields(raw) {
        issues.push(issue(
            NormalizationSeverity::ApprovalRequired,
            "unmapped_high_risk_field",
            &format!("source.raw.{key}"),
            "来源包含尚未映射的高风险声明，发布前必须人工确认。",
        ));
    }

    let spec = CanonicalAgentSpec {
        spec_version: CANONICAL_SPEC_VERSION.to_owned(),
        identity: CanonicalIdentity {
            platform_agent_id: agent.id.clone(),
            external_agent_id: source
                .and_then(|value| value.get("external_agent_id"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            source_provider: source
                .and_then(|value| value.get("provider"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            name: agent.name.clone(),
            description: agent.description.clone(),
        },
        execution: CanonicalExecution {
            runtime,
            model: agent.model.clone(),
            harness: agent.harness.clone(),
            max_runtime_minutes: agent.max_runtime_minutes,
            on_failure: agent.on_failure.clone(),
        },
        instructions: CanonicalInstructions {
            system: agent.system.clone(),
            prompt: agent.prompt.clone(),
        },
        capabilities: CanonicalCapabilities {
            tools,
            skill_ids: string_array(&agent.skill_ids),
            rule_ids: string_array(&agent.rule_ids),
            mcp_server_ids: string_array(
                agent.config.get("mcp_server_ids").unwrap_or(&Value::Null),
            ),
        },
        requirements: CanonicalRequirements {
            vault_keys: string_array(&agent.vault_keys),
            setup_commands: string_array(&agent.setup_commands),
            network_access: string_array(
                agent.config.get("network_access").unwrap_or(&Value::Null),
            ),
            filesystem_access: string_array(
                agent
                    .config
                    .get("filesystem_access")
                    .unwrap_or(&Value::Null),
            ),
        },
        policies: CanonicalPolicies {
            schedule: agent.cron.as_ref().map(|cron| CanonicalSchedule {
                cron: cron.clone(),
                timezone: agent.timezone.clone(),
            }),
            declared_side_effects: string_array(
                agent
                    .config
                    .get("declared_side_effects")
                    .unwrap_or(&Value::Null),
            ),
            approval_required: issues
                .iter()
                .any(|issue| issue.severity == NormalizationSeverity::ApprovalRequired),
        },
    };
    let can_import = !issues
        .iter()
        .any(|issue| issue.severity == NormalizationSeverity::Blocking);
    let requires_approval = issues
        .iter()
        .any(|issue| issue.severity == NormalizationSeverity::ApprovalRequired);
    NormalizationReport {
        spec,
        issues,
        can_import,
        requires_approval,
    }
}

fn issue(
    severity: NormalizationSeverity,
    code: &str,
    field: &str,
    message: &str,
) -> NormalizationIssue {
    NormalizationIssue {
        severity,
        code: code.to_owned(),
        field: field.to_owned(),
        message: message.to_owned(),
    }
}

fn string_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn unhandled_high_risk_fields(raw: Option<&Value>) -> Vec<&str> {
    const FIELDS: [&str; 8] = [
        "credentials",
        "secrets",
        "permissions",
        "network",
        "filesystem",
        "side_effects",
        "data_egress",
        "subagents",
    ];
    let Some(raw) = raw.and_then(Value::as_object) else {
        return Vec::new();
    };
    FIELDS
        .into_iter()
        .filter(|field| raw.contains_key(*field))
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn agent() -> ManagedAgentRow {
        ManagedAgentRow {
            id: "agent_1".to_owned(),
            name: "Reviewer".to_owned(),
            model: "deepseek-chat".to_owned(),
            system: "Review changes.".to_owned(),
            tools: json!([{ "type": "read" }]),
            cadence: None,
            interval_seconds: None,
            session_id: None,
            loop_id: None,
            created_at: 1,
            prompt: None,
            cron: None,
            timezone: "UTC".to_owned(),
            vault_keys: json!([]),
            setup_commands: json!([]),
            max_runtime_minutes: 30,
            on_failure: "stop".to_owned(),
            config: json!({
                "runtime": "local-opencode",
                "source": {
                    "provider": "opencode",
                    "external_agent_id": "reviewer",
                    "raw": { "permissions": ["write"] }
                }
            }),
            owner_id: Some("alice".to_owned()),
            status: "draft".to_owned(),
            description: None,
            harness: "claude_managed_agents".to_owned(),
            skill_ids: json!([]),
            rule_ids: json!([]),
        }
    }

    #[test]
    fn high_risk_unmapped_fields_require_approval() {
        let report = normalize_agent(&agent());

        assert!(report.can_import);
        assert!(report.requires_approval);
        assert_eq!(
            report.spec.identity.external_agent_id.as_deref(),
            Some("reviewer")
        );
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "unmapped_high_risk_field"));
    }

    #[test]
    fn invalid_model_and_tools_block_import() {
        let mut value = agent();
        value.model.clear();
        value.tools = json!({});

        let report = normalize_agent(&value);

        assert!(!report.can_import);
        assert_eq!(
            report
                .issues
                .iter()
                .filter(|issue| issue.severity == NormalizationSeverity::Blocking)
                .count(),
            2
        );
    }
}
