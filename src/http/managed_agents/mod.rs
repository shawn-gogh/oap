pub mod eval_runs;
pub mod evolution;
pub mod grants;
pub mod import;
pub mod import_files;
mod import_types;
pub mod improvements;
pub mod inbox;
pub mod memory;
pub mod registry;
pub mod routes;
pub mod routines;
pub mod rules;
pub mod runs;
pub mod skills;
pub mod slack;
pub mod tasks;
pub mod teams;
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
