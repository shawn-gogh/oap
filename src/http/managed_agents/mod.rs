pub mod eval_runs;
pub mod evolution;
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
pub mod teams;
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
