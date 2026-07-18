use sqlx::PgPool;
use tokio::sync::OwnedMutexGuard;

use crate::{
    db::managed_agents::{registry::schema::ManagedAgentRow, sessions::schema::SessionRow},
    errors::GatewayError,
    http::managed_agents::quota_enforcement,
    proxy::state::AppState,
};

use super::{
    storage::resolve_session_request,
    types::{CreateSessionRequest, ResolvedSession},
};

pub(super) async fn resolve_non_runtime_session(
    state: &AppState,
    pool: &PgPool,
    input: CreateSessionRequest,
) -> Result<(ResolvedSession, Option<OwnedMutexGuard<()>>), GatewayError> {
    let resolved = resolve_session_request(state, pool, input).await?;
    let quota = non_runtime_session(state, pool, &resolved).await?;
    Ok((resolved, quota))
}

async fn non_runtime_session(
    state: &AppState,
    pool: &PgPool,
    resolved: &ResolvedSession,
) -> Result<Option<OwnedMutexGuard<()>>, GatewayError> {
    quota_enforcement::lock_session_creation_for_id(state, pool, resolved.agent_id.as_deref()).await
}

pub(super) async fn runtime_session(
    state: &AppState,
    pool: &PgPool,
    agent_id: Option<&str>,
    agent: &ManagedAgentRow,
    has_prompt: bool,
) -> Result<Option<OwnedMutexGuard<()>>, GatewayError> {
    quota_enforcement::lock_session_creation(state, pool, agent_id, agent, has_prompt).await
}

pub(super) async fn prompt(
    state: &AppState,
    pool: &PgPool,
    row: &SessionRow,
) -> Result<Option<OwnedMutexGuard<()>>, GatewayError> {
    quota_enforcement::lock_prompt(state, pool, row.agent_id.as_deref()).await
}

pub(super) async fn finish_non_runtime(
    state: &AppState,
    pool: &PgPool,
    resolved: &ResolvedSession,
    row: &SessionRow,
    task_id: Option<&str>,
) -> Result<(), GatewayError> {
    if let Some(task_id) = task_id {
        crate::db::managed_agents::tasks::repository::mark_waiting_input(pool, task_id).await?;
    }
    state.agent_runs.track_run(&resolved.harness, &row.id);
    Ok(())
}
