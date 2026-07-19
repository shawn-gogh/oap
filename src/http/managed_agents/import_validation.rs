use serde_json::{json, Value};

use crate::{
    http::managed_agents::import_types::ImportAgent,
    sdk::providers::import_agents::ImportAgentsProvider,
};

pub(super) fn import_issues(
    provider: &dyn ImportAgentsProvider,
    agent: &ImportAgent,
) -> Vec<Value> {
    let mut issues = identity_and_risk_issues(agent);
    if let Some(issue) = provider_issue(provider.id(), agent.raw.as_ref()) {
        issues.push(issue);
    }
    issues
}

fn identity_and_risk_issues(agent: &ImportAgent) -> Vec<Value> {
    let mut issues = Vec::new();
    if agent.external_id.trim().is_empty() {
        issues.push(json!({
            "severity": "blocking",
            "code": "identity_missing",
            "field": "identity.external_agent_id",
            "message": "来源智能体缺少稳定身份。"
        }));
    }
    if let Some(raw) = agent.raw.as_ref().and_then(Value::as_object) {
        for field in [
            "credentials",
            "secrets",
            "permissions",
            "network",
            "filesystem",
            "side_effects",
            "data_egress",
            "subagents",
        ] {
            if raw.contains_key(field) {
                issues.push(json!({
                    "severity": "approval_required",
                    "code": "unmapped_high_risk_field",
                    "field": format!("source.raw.{field}"),
                    "message": "高风险来源字段需要人工映射与审批。"
                }));
            }
        }
    }
    issues
}

fn provider_issue(provider_id: &str, raw: Option<&Value>) -> Option<Value> {
    let raw = raw.cloned().unwrap_or(Value::Null);
    match provider_id {
        "a2a" if raw.get("url").and_then(Value::as_str).is_none() => Some(json!({
            "severity": "blocking",
            "code": "a2a_runtime_url_missing",
            "field": "source.raw.url",
            "message": "A2A Agent Card 缺少运行端点 URL。"
        })),
        "dify"
            if raw
                .get("mode")
                .and_then(Value::as_str)
                .is_some_and(|mode| mode.contains("workflow")) =>
        {
            Some(json!({
                "severity": "approval_required",
                "code": "dify_workflow_mapping_required",
                "field": "execution.input_mapping",
                "message": "Dify 工作流必须确认输入映射后才能执行。"
            }))
        }
        "openapi" if raw.get("x-lap-runtime").is_none() => Some(json!({
            "severity": "approval_required",
            "code": "openapi_runtime_mapping_required",
            "field": "source.raw.x-lap-runtime",
            "message": "OpenAPI 来源可进入资产清单，但执行前必须确认请求和响应映射。"
        })),
        "acp" => Some(json!({
            "severity": "approval_required",
            "code": "acp_profile_pin_required",
            "field": "execution.compatibility_profile",
            "message": "ACP 实现差异较大，执行前必须固定兼容配置并通过一致性测试。"
        })),
        "langgraph" if raw.get("x-lap-runtime").is_none() => Some(json!({
            "severity": "approval_required",
            "code": "langgraph_input_mapping_required",
            "field": "source.raw.x-lap-runtime",
            "message": "LangGraph 来源可进入资产清单，但执行前必须确认输入与状态映射。"
        })),
        "crewai" if raw.get("x-lap-runtime").is_none() => Some(json!({
            "severity": "approval_required",
            "code": "crewai_kickoff_mapping_required",
            "field": "source.raw.x-lap-runtime",
            "message": "CrewAI 来源可进入资产清单，但执行前必须确认 kickoff 输入映射。"
        })),
        "openai_assistants" => Some(json!({
            "severity": "approval_required",
            "code": "openai_assistants_migration_required",
            "field": "execution.compatibility_profile",
            "message": "OpenAI Assistants 已进入迁移期，执行前必须确认目标运行时与兼容映射。"
        })),
        _ => None,
    }
}
