pub mod audit_timeline;
pub mod byo_credentials;
pub mod catalog;
pub mod eval_runs;
pub mod evolution;
pub mod governance;
pub mod grants;
pub mod identity_mappings;
pub mod import;
pub mod import_files;
mod import_types;
mod import_validation;
pub mod improvements;
pub mod inbox;
pub mod mattermost;
pub mod memory;
pub mod metrics;
mod publish_gate;
pub(crate) mod quota_enforcement;
pub mod registry;
pub mod routes;
pub mod routines;
pub mod rules;
pub mod runs;
pub mod skills;
mod source_alerts;
pub mod source_management;
pub mod source_scheduler;
pub mod tasks;
pub mod tool_approvals;
pub mod workspace;

use axum::http::HeaderMap;
use sqlx::PgPool;

use crate::{
    db::managed_agents::registry::schema::ManagedAgentRow,
    errors::GatewayError,
    proxy::{
        auth::master_key::{require_any_gateway_key, AuthContext},
        state::AppState,
    },
};

pub async fn db<'a>(state: &'a AppState, headers: &HeaderMap) -> Result<&'a PgPool, GatewayError> {
    require_any_gateway_key(headers, state).await?;

    state.db.as_ref().ok_or(GatewayError::MissingDatabase)
}

/// Soft-delete marker written by `DELETE /api/agents/{id}`: the row survives
/// (list queries filter it out) until the retention sweep hard-deletes it.
pub(crate) fn agent_deleted_at(agent: &ManagedAgentRow) -> Option<i64> {
    agent
        .config
        .get("deleted_at")
        .and_then(|value| value.as_i64())
}

/// Owner-or-admin gate for mutating an agent (and its workspace). Legacy
/// agents without an owner are admin-only to mutate. NotFound rather than
/// Forbidden so agent ids aren't enumerable across users.
pub(crate) fn assert_agent_access(
    auth: &AuthContext,
    agent: &ManagedAgentRow,
) -> Result<(), GatewayError> {
    if auth.is_admin || agent.owner_id.as_deref() == Some(auth.user_id.as_str()) {
        Ok(())
    } else {
        Err(GatewayError::NotFound(format!("agent {}", agent.id)))
    }
}

pub(crate) fn require_importer(auth: &AuthContext) -> Result<(), GatewayError> {
    if auth.can_import() {
        Ok(())
    } else {
        Err(GatewayError::Forbidden)
    }
}

pub(crate) async fn authenticate_importer(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<AuthContext, GatewayError> {
    let auth = crate::proxy::auth::master_key::authenticate(headers, state).await?;
    require_importer(&auth)?;
    Ok(auth)
}

/// Like `assert_agent_access` but also satisfied by an 'edit' grant.
/// Use for config-level mutations shared users may be trusted with.
pub(crate) async fn assert_agent_edit(
    auth: &AuthContext,
    agent: &ManagedAgentRow,
    pool: &PgPool,
) -> Result<(), GatewayError> {
    if assert_agent_access(auth, agent).is_ok() {
        return Ok(());
    }
    let grant =
        crate::db::managed_agents::agent_grants::repository::find(pool, &agent.id, &auth.user_id)
            .await?;
    if grant.is_some_and(|g| g.permission == "edit")
        || crate::db::managed_agents::groups::agent_grants::has_permission(
            pool,
            &agent.id,
            &auth.user_id,
            Some("edit"),
        )
        .await?
    {
        Ok(())
    } else {
        Err(GatewayError::NotFound(format!("agent {}", agent.id)))
    }
}

/// Visibility/usage gate: owner, admin, or any grant ('use' or 'edit').
/// Required to see an agent or start sessions/runs on it.
/// Draft agents are configuration-only: they may be edited and chatted with
/// for testing, but runs and scheduled triggers are refused until the agent
/// passes preflight and is activated.
pub(crate) fn assert_agent_runnable(agent: &ManagedAgentRow) -> Result<(), GatewayError> {
    if agent.status == "draft" {
        return Err(GatewayError::BadRequest(format!(
            "agent {} 处于草稿状态：请先通过预检并激活（POST /api/agents/{}/activate）",
            agent.id, agent.id
        )));
    }
    if agent.status == "archived_pending_delete" {
        return Err(GatewayError::BadRequest(format!(
            "agent {} 已被软删除并处于归档挂起状态",
            agent.id
        )));
    }
    if agent.status == "paused" {
        return Err(GatewayError::BadRequest(format!(
            "agent {} 已暂停，恢复运行后才能创建新任务",
            agent.id
        )));
    }
    Ok(())
}

/// Session-interaction gate, checked when creating a session for an agent
/// and on every prompt send. Stricter than `assert_agent_runnable`'s draft
/// rule (drafts stay chattable for testing) but blocks:
/// - retired / soft-deleted agents (`archived_pending_delete`);
/// - paused agents — pause/emergency stop must hold for new interaction too,
///   not only for the sessions interrupted at the moment of stopping;
/// - imported agents whose governance is suspended or retired (emergency
///   stop / repeated failed health checks) — "暂停新工作" must include chat,
///   otherwise the emergency stop is a no-op for sessions.
pub(crate) async fn assert_agent_interactive(
    pool: &PgPool,
    agent: &ManagedAgentRow,
) -> Result<(), GatewayError> {
    if agent.status == "archived_pending_delete" {
        return Err(GatewayError::BadRequest(format!(
            "智能体 {} 已退役或被删除，不能再进行会话交互。",
            agent.id
        )));
    }
    if let Some(governance) = crate::db::managed_agents::governance::get(pool, &agent.id).await? {
        match governance.lifecycle_status.as_str() {
            "retired" => {
                return Err(GatewayError::BadRequest(format!(
                    "智能体 {} 已退役，不能再进行会话交互。",
                    agent.id
                )));
            }
            "suspended" => {
                return Err(GatewayError::BadRequest(format!(
                    "智能体 {} 处于暂停状态（紧急停止或健康检查失败）：请先通过治理面板的\"运行检查\"确认健康后再继续会话。",
                    agent.id
                )));
            }
            "review_due" => {
                return Err(GatewayError::BadRequest(format!(
                    "智能体 {} 的发布有效期已到：请重新运行治理检查并完成发布复审。",
                    agent.id
                )));
            }
            _ => {}
        }
    }
    if agent.status == "paused" {
        return Err(GatewayError::BadRequest(format!(
            "智能体 {} 已暂停，恢复运行后才能继续会话（POST /api/agents/{}/resume）。",
            agent.id, agent.id
        )));
    }
    Ok(())
}

pub(crate) async fn assert_agent_use(
    auth: &AuthContext,
    agent: &ManagedAgentRow,
    pool: &PgPool,
) -> Result<(), GatewayError> {
    if assert_agent_access(auth, agent).is_ok() {
        return Ok(());
    }
    // Legacy ownerless agents stay usable by everyone (pre-isolation
    // behavior); only mutation is admin-gated for them.
    if agent.owner_id.is_none() {
        return Ok(());
    }
    let grant =
        crate::db::managed_agents::agent_grants::repository::find(pool, &agent.id, &auth.user_id)
            .await?;
    if grant.is_some()
        || crate::db::managed_agents::groups::agent_grants::has_permission(
            pool,
            &agent.id,
            &auth.user_id,
            None,
        )
        .await?
    {
        Ok(())
    } else {
        Err(GatewayError::NotFound(format!("agent {}", agent.id)))
    }
}
