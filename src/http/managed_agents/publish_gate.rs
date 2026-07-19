use serde_json::json;
use sqlx::PgPool;

use crate::{
    db::managed_agents::{
        audit,
        eval_runs::gate::{self, EvalGateStatus},
        governance::AgentGovernanceRow,
        registry::schema::ManagedAgentRow,
    },
    errors::GatewayError,
};

pub fn assert_runtime_ready(
    governance: &AgentGovernanceRow,
    revision: i32,
) -> Result<(), GatewayError> {
    if governance.runtime_health == "healthy"
        && governance.lifecycle_status == "tested"
        && governance.tested_revision == Some(revision)
    {
        return Ok(());
    }
    Err(GatewayError::BadRequest(
        "当前版本尚未通过运行测试，不能申请发布。".to_owned(),
    ))
}

pub async fn enforce(
    pool: &PgPool,
    actor: &str,
    agent: &ManagedAgentRow,
    revision: i32,
) -> Result<(EvalGateStatus, Vec<String>), GatewayError> {
    let status = gate::evaluate(pool, agent, revision).await?;
    if !status.passed {
        audit::record(
            pool,
            actor,
            "agent.governance.publish_blocked",
            "agent",
            &agent.id,
            json!({
                "revision": revision,
                "gate": "golden_eval",
                "state": status.state,
                "message": status.message,
            }),
        )
        .await?;
        return Err(GatewayError::BadRequest(status.message));
    }
    let warnings = if status.required {
        Vec::new()
    } else {
        vec![status.message.clone()]
    };
    Ok((status, warnings))
}
