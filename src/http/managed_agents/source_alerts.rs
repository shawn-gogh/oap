use serde_json::Value;
use sqlx::PgPool;

use crate::{
    db::managed_agents::{
        governance,
        registry::{repository, schema::ManagedAgentRow},
    },
    errors::GatewayError,
    proxy::state::AppState,
};

use super::mattermost::{notify_governance_event, GovernanceNotification};

pub(crate) async fn pause_for_health_failures(
    state: &AppState,
    pool: &PgPool,
    agent: &ManagedAgentRow,
    consecutive_failures: i64,
) -> Result<(), GatewayError> {
    repository::set_status(pool, &agent.id, "paused").await?;
    if governance::get(pool, &agent.id).await?.is_none() {
        return Ok(());
    }
    let detail = format!("连续 {consecutive_failures} 次健康检查发现阻断项，运行已暂停。");
    governance::suspend(pool, &agent.id, &detail).await?;
    notify_governance_event(
        state,
        pool,
        agent,
        GovernanceNotification::HealthDegraded {
            consecutive_failures,
            detail: &detail,
        },
    )
    .await;
    Ok(())
}

pub(crate) async fn pause_for_high_risk_drift(
    state: &AppState,
    pool: &PgPool,
    agent: &ManagedAgentRow,
    snapshot_id: &str,
    findings: &[(String, String, Option<Value>, Option<Value>)],
) -> Result<(), GatewayError> {
    repository::set_status(pool, &agent.id, "paused").await?;
    governance::suspend(pool, &agent.id, "检测到高风险来源漂移，已暂停新工作。").await?;
    let highest_risk = if findings.iter().any(|(_, risk, _, _)| risk == "critical") {
        "critical"
    } else {
        "high"
    };
    let changed_fields = findings
        .iter()
        .filter(|(_, risk, _, _)| matches!(risk.as_str(), "high" | "critical"))
        .map(|(field, _, _, _)| field.clone())
        .collect::<Vec<_>>();
    notify_governance_event(
        state,
        pool,
        agent,
        GovernanceNotification::HighRiskDrift {
            snapshot_id,
            highest_risk,
            changed_fields: &changed_fields,
        },
    )
    .await;
    Ok(())
}
