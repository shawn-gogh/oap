use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    db::managed_agents::{
        audit,
        eval_runs::gate::{self, EvalGateStatus},
        governance::{self, AgentGovernanceRow},
        inbox::{repository as inbox, schema::InboxItemRow},
        registry::{repository, revisions, schema::ManagedAgentRow},
    },
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

use super::registry::preflight::{run_preflight_with_smoke, PreflightReport};

#[derive(Debug, Serialize)]
pub struct GovernanceResponse {
    pub governance: AgentGovernanceRow,
    pub current_revision: i32,
    pub eval_gate: EvalGateStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preflight: Option<PreflightReport>,
}

#[derive(Debug, Deserialize)]
pub struct RollbackRequest {
    pub version: Option<i32>,
}

async fn owned_imported_agent(
    state: &AppState,
    headers: &HeaderMap,
    agent_id: &str,
) -> Result<
    (
        sqlx::PgPool,
        crate::proxy::auth::master_key::AuthContext,
        ManagedAgentRow,
        AgentGovernanceRow,
    ),
    GatewayError,
> {
    let auth = authenticate(headers, state).await?;
    let pool = state.db.clone().ok_or(GatewayError::MissingDatabase)?;
    let agent = repository::get(&pool, agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    super::assert_agent_access(&auth, &agent)?;
    let governance = governance::get(&pool, agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("governance not found".to_owned()))?;
    Ok((pool, auth, agent, governance))
}

async fn current_revision(pool: &sqlx::PgPool, agent_id: &str) -> Result<i32, GatewayError> {
    revisions::latest_version(pool, agent_id)
        .await?
        .ok_or_else(|| GatewayError::BadRequest("智能体还没有可用版本。".to_owned()))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<GovernanceResponse>, GatewayError> {
    let (pool, _, agent, governance) = owned_imported_agent(&state, &headers, &agent_id).await?;
    let revision = current_revision(&pool, &agent_id).await?;
    Ok(Json(GovernanceResponse {
        governance,
        current_revision: revision,
        eval_gate: gate::evaluate(&pool, &agent, revision).await?,
        preflight: None,
    }))
}

pub async fn test(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<GovernanceResponse>, GatewayError> {
    let (pool, auth, agent, _) = owned_imported_agent(&state, &headers, &agent_id).await?;
    let revision = current_revision(&pool, &agent_id).await?;
    // The explicit governance test exercises the real execution path (A2A
    // message/send smoke) on top of the passive checks — discovery-only
    // probes have green-lit agents whose sessions then failed 100%.
    let report = run_preflight_with_smoke(&state, &pool, &agent, Some(&auth.user_id)).await?;
    let detail = report
        .checks
        .iter()
        .filter(|check| check.verdict == "failed")
        .map(|check| format!("{}：{}", check.label, check.detail))
        .collect::<Vec<_>>()
        .join("；");
    let detail = if detail.is_empty() {
        format!("{} 项运行检查通过或无需阻断。", report.checks.len())
    } else {
        detail
    };
    let governance =
        governance::mark_tested(&pool, &agent_id, revision, report.can_activate, &detail).await?;
    audit::record(
        &pool,
        &auth.user_id,
        "agent.governance.test",
        "agent",
        &agent_id,
        json!({ "revision": revision, "healthy": report.can_activate }),
    )
    .await?;
    Ok(Json(GovernanceResponse {
        governance,
        current_revision: revision,
        eval_gate: gate::evaluate(&pool, &agent, revision).await?,
        preflight: Some(report),
    }))
}

pub async fn request_publish(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, GatewayError> {
    let (pool, auth, agent, governance) = owned_imported_agent(&state, &headers, &agent_id).await?;
    let revision = current_revision(&pool, &agent_id).await?;
    super::publish_gate::assert_runtime_ready(&governance, revision)?;
    let (eval_gate, warnings) =
        super::publish_gate::enforce(&pool, &auth.user_id, &agent, revision).await?;
    let approval = inbox::create_approval(
        &pool,
        "agent_publish",
        format!("发布外部智能体「{}」v{}", agent.name, revision),
        None,
        Some(agent_id.clone()),
        Some("运行检查已通过，等待管理员审批发布。".to_owned()),
        Some(json!({
            "action": "publish_agent",
            "agent_id": agent_id,
            "revision": revision,
            "base_revision": governance.published_revision.unwrap_or(0),
        })),
    )
    .await?;
    let base_revision = governance.published_revision.unwrap_or(0);
    let governance = governance::request_publish(&pool, &agent.id, &approval.id).await?;
    audit::record(
        &pool,
        &auth.user_id,
        "agent.governance.publish_requested",
        "agent",
        &agent.id,
        json!({ "revision": revision, "approval_id": approval.id }),
    )
    .await?;
    super::mattermost::notify_governance_event(
        &state,
        &pool,
        &agent,
        super::mattermost::GovernanceNotification::PublishRequested {
            approval_id: &approval.id,
            base_revision,
            revision,
        },
    )
    .await;
    Ok(Json(json!({
        "governance": governance,
        "approval": approval,
        "eval_gate": eval_gate,
        "warnings": warnings,
    })))
}

pub fn is_publish_approval(item: &InboxItemRow) -> bool {
    if item.kind == "agent_publish" || item.effect_handler == "agent_publish" {
        return true;
    }
    item.args_json
        .as_deref()
        .and_then(|value| serde_json::from_str::<Value>(value).ok())
        .and_then(|value| {
            value
                .get("action")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .as_deref()
        == Some("publish_agent")
}

pub async fn apply_publish_approval(
    pool: &sqlx::PgPool,
    item: &InboxItemRow,
    actor: &str,
) -> Result<(), GatewayError> {
    let arguments = item
        .args_json
        .as_deref()
        .and_then(|value| serde_json::from_str::<Value>(value).ok())
        .ok_or_else(|| GatewayError::BadRequest("发布审批参数无效。".to_owned()))?;
    let agent_id = arguments
        .get("agent_id")
        .and_then(Value::as_str)
        .ok_or_else(|| GatewayError::BadRequest("发布审批缺少智能体 ID。".to_owned()))?;
    let revision = arguments
        .get("revision")
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| GatewayError::BadRequest("发布审批版本无效。".to_owned()))?;
    let current = governance::get(pool, agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("governance not found".to_owned()))?;
    if current.publish_approval_id.as_deref() != Some(item.id.as_str())
        || current.tested_revision != Some(revision)
    {
        return Err(GatewayError::BadRequest("发布审批已过期。".to_owned()));
    }
    // SoD note: owner==importer only holds while owner_id is immutable post-import.
    if current.owner_id == actor
        && crate::db::managed_agents::settings::repository::enforce_separation_of_duties(pool)
            .await?
    {
        return Err(GatewayError::BadRequest(
            "职责分离已启用：不能审批自己导入的智能体。".to_owned(),
        ));
    }
    governance::publish(pool, agent_id, revision).await?;
    repository::set_status(pool, agent_id, "active")
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    audit::record(
        pool,
        actor,
        "agent.governance.published",
        "agent",
        agent_id,
        json!({ "revision": revision, "approval_id": item.id, "self_approval": false }),
    )
    .await?;
    Ok(())
}

pub async fn rollback(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Json(input): Json<RollbackRequest>,
) -> Result<Json<Value>, GatewayError> {
    let (pool, auth, _, governance) = owned_imported_agent(&state, &headers, &agent_id).await?;
    let target = input
        .version
        .or(governance.previous_published_revision)
        .ok_or_else(|| GatewayError::BadRequest("没有可回滚的已发布版本。".to_owned()))?;
    // Rollback may only target a revision that went through the full
    // test→approval→publish pipeline. Without this check, "rollback" to an
    // arbitrary (untested, unapproved) revision would mint it as published —
    // a one-call bypass of the admin publish approval.
    let published_targets = [
        governance.published_revision,
        governance.previous_published_revision,
    ];
    if !published_targets.contains(&Some(target)) {
        return Err(GatewayError::BadRequest(format!(
            "版本 {target} 不是已发布版本，不能作为回滚目标。"
        )));
    }
    let revision = revisions::get_version(&pool, &agent_id, target)
        .await?
        .ok_or_else(|| GatewayError::BadRequest(format!("版本 {target} 不存在。")))?;
    let snapshot: ManagedAgentRow = serde_json::from_value(revision.snapshot)
        .map_err(|error| GatewayError::InvalidConfig(error.to_string()))?;
    let restored = repository::restore_snapshot(&pool, &agent_id, &snapshot)
        .await?
        .ok_or_else(|| GatewayError::NotFound("not found".to_owned()))?;
    let restored_revision = revisions::record(&pool, &restored, Some(&auth.user_id)).await?;
    let governance = governance::mark_rolled_back(&pool, &agent_id, restored_revision).await?;
    audit::record(
        &pool,
        &auth.user_id,
        "agent.governance.rolled_back",
        "agent",
        &agent_id,
        json!({ "restored_from_revision": target, "new_revision": restored_revision }),
    )
    .await?;
    Ok(Json(json!({
        "agent": restored,
        "governance": governance,
        "restored_from_revision": target,
        "note": "配置已回滚。智能体保持当前运行状态，如需恢复运行请执行激活（将重新运行预检）。",
    })))
}
