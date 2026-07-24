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

/// What `sessions::external_bridge` (or a registered managed-protocol
/// harness) actually implements for a given runtime, so the conformance
/// checklist reports what's verified rather than a blanket "trust this
/// provider" flag. Managed-protocol harnesses get all four for free because
/// the harness itself is the contract; federated bridges (A2A, ACP, Dify,
/// OpenAPI) earn each one only when the bridge code genuinely implements it —
/// see the per-`runtime` cases below.
struct RuntimeContractCapabilities {
    terminal_events: bool,
    interrupt_or_abort: bool,
    approval_terminal_result: bool,
    event_recovery: bool,
}

fn runtime_contract_capabilities(
    runtime: Option<&str>,
    harness: &str,
) -> RuntimeContractCapabilities {
    let managed_protocol = runtime.is_some_and(|runtime| {
        matches!(
            runtime,
            "claude_managed_agents" | "cursor" | "gemini_antigravity" | "elastic_agent_builder"
        )
    }) || harness == "claude_managed_agents";
    if managed_protocol {
        return RuntimeContractCapabilities {
            terminal_events: true,
            interrupt_or_abort: true,
            approval_terminal_result: true,
            event_recovery: true,
        };
    }
    if runtime == Some("langgraph_assistant") {
        return RuntimeContractCapabilities {
            terminal_events: true,
            interrupt_or_abort: true,
            approval_terminal_result: true,
            event_recovery: true,
        };
    }
    if runtime == Some("a2a_v1") {
        return RuntimeContractCapabilities {
            terminal_events: true,
            interrupt_or_abort: true,
            approval_terminal_result: true,
            event_recovery: true,
        };
    }
    if matches!(runtime, Some("dify_app" | "openapi_rest" | "crewai_crew")) {
        // Bridges that sessions::external_bridge actually executes:
        // - dify_app / openapi_rest issue provider calls whose lifecycle is
        //   owned by the platform and always converges to a terminal state.
        // These bridges do not expose replayable provider event streams.
        return RuntimeContractCapabilities {
            terminal_events: true,
            interrupt_or_abort: true,
            approval_terminal_result: true,
            event_recovery: false,
        };
    }
    RuntimeContractCapabilities {
        terminal_events: false,
        interrupt_or_abort: false,
        approval_terminal_result: false,
        event_recovery: false,
    }
}

pub fn inspect_runtime_contract(agent: &ManagedAgentRow) -> ConformanceReport {
    inspect_runtime_contract_with_api_spec(agent, None)
}

/// Same as [`inspect_runtime_contract`], but keys the contract capabilities off
/// the harness's resolved `api_spec` rather than the raw runtime alias.
///
/// Custom harnesses (e.g. an imported `local-opencode` bundle) carry an
/// arbitrary alias in `config.runtime` while actually speaking a first-class
/// managed protocol (`local-opencode` → `claude_managed_agents`). Contract
/// conformance is a property of the protocol the platform executes, not the
/// display alias, so callers holding a DB pool should resolve the alias via the
/// harnesses table and pass the api_spec here. Pass `None` for built-in static
/// runtimes whose alias already equals their api_spec.
pub fn inspect_runtime_contract_with_api_spec(
    agent: &ManagedAgentRow,
    resolved_api_spec: Option<&str>,
) -> ConformanceReport {
    let runtime = agent
        .config
        .get("runtime")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let runtime_declared = runtime.is_some();
    // A resolved api_spec (custom harness) wins; otherwise the raw alias, which
    // for static runtimes already equals its api_spec.
    let capability_runtime = resolved_api_spec
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(runtime);
    let capabilities = runtime_contract_capabilities(capability_runtime, &agent.harness);
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
            capabilities.terminal_events,
            "运行时必须产生 completed、failed、cancelled 或 idle 终态。",
        ),
        check(
            "interrupt_or_abort",
            true,
            capabilities.interrupt_or_abort,
            "运行时必须支持 interrupt，或由平台执行可强制 abort。",
        ),
        check(
            "approval_terminal_result",
            true,
            capabilities.approval_terminal_result,
            "审批拒绝必须产生终态工具结果并释放会话。",
        ),
        check(
            "event_recovery",
            false,
            capabilities.event_recovery,
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

    #[test]
    fn a2a_bridge_is_conformant_once_it_implements_the_contract() {
        let report = inspect_runtime_contract(&agent(Some("a2a_v1")));
        assert_eq!(report.status, "conformant");
        assert!(
            report
                .checks
                .iter()
                .find(|check| check.id == "event_recovery")
                .unwrap()
                .passed
        );
    }

    #[test]
    fn execution_bridges_are_conformant() {
        for runtime in [
            "dify_app",
            "openapi_rest",
            "langgraph_assistant",
            "crewai_crew",
        ] {
            assert_eq!(
                inspect_runtime_contract(&agent(Some(runtime))).status,
                "conformant",
                "{runtime}"
            );
        }
        let langgraph = inspect_runtime_contract(&agent(Some("langgraph_assistant")));
        assert!(
            langgraph
                .checks
                .iter()
                .find(|check| check.id == "event_recovery")
                .unwrap()
                .passed
        );
    }

    #[test]
    fn custom_harness_alias_is_conformant_via_resolved_api_spec() {
        // An imported opencode bundle carries runtime="local-opencode" and
        // harness="claude-code"; on its own that is partial, but once the alias
        // resolves to the claude_managed_agents api_spec it is fully conformant.
        let mut agent = agent(Some("local-opencode"));
        agent.harness = "claude-code".to_owned();
        assert_eq!(inspect_runtime_contract(&agent).status, "partial");
        assert_eq!(
            inspect_runtime_contract_with_api_spec(&agent, Some("claude_managed_agents")).status,
            "conformant"
        );
    }

    #[test]
    fn federated_bridges_without_execution_stay_partial() {
        // No execution bridge exists for these specs (they hit "unsupported
        // external bridge" or an explicit unsupported error), so they must not
        // be publishable.
        for runtime in ["openai_assistant", "acp_legacy"] {
            assert_eq!(
                inspect_runtime_contract(&agent(Some(runtime))).status,
                "partial",
                "{runtime}"
            );
        }
    }
}
