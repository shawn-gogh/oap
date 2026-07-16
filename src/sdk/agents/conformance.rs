use serde::{Deserialize, Serialize};

use crate::db::managed_agents::registry::schema::ManagedAgentRow;

pub const RUNTIME_CONTRACT_VERSION: &str = "lap-runtime-v1";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConformanceCheck {
    pub id: String,
    pub required: bool,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConformanceReport {
    pub contract_version: String,
    pub status: String,
    pub checks: Vec<ConformanceCheck>,
}

pub fn inspect_runtime_contract(agent: &ManagedAgentRow) -> ConformanceReport {
    let runtime = agent
        .config
        .get("runtime")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let runtime_declared = runtime.is_some();
    let managed_protocol = runtime.is_some_and(|runtime| {
        matches!(
            runtime,
            "claude_managed_agents" | "cursor" | "gemini_antigravity" | "elastic_agent_builder"
        ) || agent.harness == "claude_managed_agents"
    });
    let checks = vec![
        check(
            "stable_identity",
            true,
            !agent.id.trim().is_empty(),
            "平台智能体 ID 作为稳定身份。",
        ),
        check(
            "request_correlation",
            true,
            runtime_declared,
            "运行时会话必须通过平台 Session ID 关联。",
        ),
        check(
            "terminal_events",
            true,
            managed_protocol,
            "运行时必须产生 completed、failed、cancelled 或 idle 终态。",
        ),
        check(
            "interrupt_or_abort",
            true,
            managed_protocol,
            "运行时必须支持 interrupt，或由平台执行可强制 abort。",
        ),
        check(
            "approval_terminal_result",
            true,
            managed_protocol,
            "审批拒绝必须产生终态工具结果并释放会话。",
        ),
        check(
            "event_recovery",
            false,
            managed_protocol,
            "事件流应支持按会话恢复和幂等重放。",
        ),
    ];
    let required_passed = checks
        .iter()
        .filter(|check| check.required)
        .all(|check| check.passed);
    let any_required_passed = checks
        .iter()
        .filter(|check| check.required)
        .any(|check| check.passed);
    let status = if required_passed {
        "conformant"
    } else if any_required_passed {
        "partial"
    } else {
        "non_conformant"
    };
    ConformanceReport {
        contract_version: RUNTIME_CONTRACT_VERSION.to_owned(),
        status: status.to_owned(),
        checks,
    }
}

fn check(id: &str, required: bool, passed: bool, detail: &str) -> ConformanceCheck {
    ConformanceCheck {
        id: id.to_owned(),
        required,
        passed,
        detail: detail.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn agent(runtime: Option<&str>) -> ManagedAgentRow {
        ManagedAgentRow {
            id: "agent_1".to_owned(),
            name: "Agent".to_owned(),
            model: "deepseek-chat".to_owned(),
            system: String::new(),
            tools: json!([]),
            cadence: None,
            interval_seconds: None,
            session_id: None,
            loop_id: None,
            created_at: 0,
            prompt: None,
            cron: None,
            timezone: "UTC".to_owned(),
            vault_keys: json!([]),
            setup_commands: json!([]),
            max_runtime_minutes: 30,
            on_failure: "stop".to_owned(),
            config: runtime.map_or_else(|| json!({}), |value| json!({ "runtime": value })),
            owner_id: None,
            status: "draft".to_owned(),
            description: None,
            harness: runtime.unwrap_or("chat").to_owned(),
            skill_ids: json!([]),
            rule_ids: json!([]),
        }
    }

    #[test]
    fn known_managed_runtime_is_conformant() {
        assert_eq!(
            inspect_runtime_contract(&agent(Some("claude_managed_agents"))).status,
            "conformant"
        );
    }

    #[test]
    fn missing_runtime_is_non_conformant_for_external_execution() {
        assert_eq!(inspect_runtime_contract(&agent(None)).status, "partial");
    }
}
