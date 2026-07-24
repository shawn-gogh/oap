use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;

use crate::{db::managed_agents::registry::schema::ManagedAgentRow, errors::GatewayError};

use super::{repository, schema::EvalRunRow};

#[derive(Debug, Clone, Serialize)]
pub struct EvalGateStatus {
    pub required: bool,
    pub passed: bool,
    pub state: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_run: Option<EvalGateRun>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalGateRun {
    pub id: String,
    pub agent_version: Option<i32>,
    pub status: String,
    pub total: i32,
    pub passed: i32,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

pub async fn evaluate(
    pool: &PgPool,
    agent: &ManagedAgentRow,
    revision: i32,
) -> Result<EvalGateStatus, GatewayError> {
    let definition = Definition::from_config(&agent.config);
    if !definition.has_cases {
        return Ok(EvalGateStatus {
            required: false,
            passed: true,
            state: "not_required".to_owned(),
            message: "当前版本未定义黄金用例，发布不会被评估门禁阻断。".to_owned(),
            latest_run: None,
        });
    }
    // The creation wizard fills a template suite in to release its own
    // "继续设计" button, so most agents carry cases nobody wrote. Asserting
    // "能够完成工作流程" passes every judge while testing nothing, and gating
    // on it would force a full re-run on every revision for zero signal.
    // Editing any case clears the marker and turns the gate on for real.
    if definition.generated {
        return Ok(EvalGateStatus {
            required: false,
            passed: true,
            state: "not_required".to_owned(),
            message: "当前黄金用例仍是向导生成的模板，未被视为有效评估定义；编辑任一用例后发布门禁才会生效。"
                .to_owned(),
            latest_run: None,
        });
    }
    if !definition.complete {
        return Ok(blocked(
            "invalid_definition",
            "黄金用例定义不完整：需要成功标准以及正常、边界、恢复、安全四类用例。",
            None,
        ));
    }
    let latest = repository::latest_for_revision(pool, &agent.id, revision).await?;
    Ok(match latest {
        None => blocked(
            "not_run",
            "当前版本尚未运行黄金用例评估，请先运行评估并等待全部通过。",
            None,
        ),
        Some(run) => status_from_run(run),
    })
}

fn status_from_run(run: EvalRunRow) -> EvalGateStatus {
    let passed = run.status == "completed" && run.total > 0 && run.passed == run.total;
    let state = if passed {
        "passed"
    } else if run.status == "running" {
        "running"
    } else if run.status == "failed" {
        "error"
    } else {
        "failed"
    };
    let message = match state {
        "passed" => format!("当前版本的 {} 项黄金用例已全部通过。", run.total),
        "running" => "当前版本的黄金用例评估仍在运行，请等待完成后再申请发布。".to_owned(),
        "error" => "当前版本的黄金用例评估执行失败，请修复后重新运行评估。".to_owned(),
        _ => format!(
            "当前版本的黄金用例仅通过 {}/{} 项，请修复后重新运行评估。",
            run.passed, run.total
        ),
    };
    EvalGateStatus {
        required: true,
        passed,
        state: state.to_owned(),
        message,
        latest_run: Some(run.into()),
    }
}

fn blocked(state: &str, message: &str, latest_run: Option<EvalGateRun>) -> EvalGateStatus {
    EvalGateStatus {
        required: true,
        passed: false,
        state: state.to_owned(),
        message: message.to_owned(),
        latest_run,
    }
}

impl From<EvalRunRow> for EvalGateRun {
    fn from(run: EvalRunRow) -> Self {
        Self {
            id: run.id,
            agent_version: run.agent_version,
            status: run.status,
            total: run.total,
            passed: run.passed,
            created_at: run.created_at,
            completed_at: run.completed_at,
        }
    }
}

struct Definition {
    has_cases: bool,
    complete: bool,
    /// Cases are still the wizard's untouched template (`evaluation.generated`).
    generated: bool,
}

impl Definition {
    fn from_config(config: &Value) -> Self {
        let Some(evaluation) = config.pointer("/design/evaluation") else {
            return Self {
                has_cases: false,
                complete: false,
                generated: false,
            };
        };
        let category_counts = [
            "normal_cases",
            "edge_cases",
            "recovery_cases",
            "safety_cases",
        ]
        .map(|key| non_empty_items(evaluation.get(key)));
        let has_cases = category_counts.iter().any(|count| *count > 0);
        let has_criteria = evaluation
            .get("success_criteria")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty());
        Self {
            has_cases,
            complete: has_criteria && category_counts.iter().all(|count| *count > 0),
            generated: evaluation
                .get("generated")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }
}

fn non_empty_items(value: Option<&Value>) -> usize {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter(|item| !item.trim().is_empty())
                .count()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::Definition;

    #[test]
    fn definition_requires_all_golden_case_categories() {
        let complete = Definition::from_config(&json!({
            "design": {
                "evaluation": {
                    "success_criteria": "准确回答",
                    "normal_cases": ["正常"],
                    "edge_cases": ["边界"],
                    "recovery_cases": ["恢复"],
                    "safety_cases": ["安全"]
                }
            }
        }));
        assert!(complete.has_cases);
        assert!(complete.complete);

        let partial = Definition::from_config(&json!({
            "design": { "evaluation": { "normal_cases": ["正常"] } }
        }));
        assert!(partial.has_cases);
        assert!(!partial.complete);

        let absent = Definition::from_config(&json!({ "design": { "evaluation": {} } }));
        assert!(!absent.has_cases);
    }

    #[test]
    fn generated_marker_is_read_and_defaults_to_false() {
        let generated = Definition::from_config(&json!({
            "design": {
                "evaluation": {
                    "success_criteria": "准确回答",
                    "normal_cases": ["正常"],
                    "edge_cases": ["边界"],
                    "recovery_cases": ["恢复"],
                    "safety_cases": ["安全"],
                    "generated": true
                }
            }
        }));
        assert!(generated.complete);
        assert!(generated.generated);

        // A hand-written suite (and every agent created before the marker
        // existed) must stay gated.
        let authored = Definition::from_config(&json!({
            "design": {
                "evaluation": {
                    "success_criteria": "准确回答",
                    "normal_cases": ["正常"],
                    "edge_cases": ["边界"],
                    "recovery_cases": ["恢复"],
                    "safety_cases": ["安全"]
                }
            }
        }));
        assert!(!authored.generated);
    }
}
