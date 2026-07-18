use std::{sync::Arc, time::Duration};

use serde_json::json;

use crate::{
    db::managed_agents::{audit, governance, registry::repository, sources::repository as sources},
    errors::GatewayError,
    proxy::{auth::master_key::AuthContext, state::AppState},
};

const POLL_INTERVAL: Duration = Duration::from_secs(60);
const LEASE_MS: i64 = 60_000;

pub fn spawn(state: Arc<AppState>) {
    if state.db.is_none() {
        return;
    }
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(POLL_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if let Err(error) = run_due_once(state.clone()).await {
                tracing::warn!("agent source scheduler tick failed: {error}");
            }
        }
    });
}

pub async fn run_due_once(state: Arc<AppState>) -> Result<usize, GatewayError> {
    let Some(pool) = state.db.as_ref().cloned() else {
        return Ok(0);
    };
    let due = sources::list_due_sources(&pool, 25).await?;
    let mut completed = 0;
    for source in due {
        if !sources::acquire_sync_lease(&pool, &source.id, "source-scheduler", LEASE_MS).await? {
            continue;
        }
        let Some(agent) = repository::get(&pool, &source.agent_id).await? else {
            sources::mark_sync_state(&pool, &source.id, "detached", source.missing_count).await?;
            continue;
        };
        let Some(governance) = governance::get(&pool, &source.agent_id).await? else {
            sources::mark_sync_state(&pool, &source.id, "sync_error", source.missing_count).await?;
            continue;
        };
        let auth = AuthContext::operator(governance.owner_id.clone());
        let run = sources::start_sync_run(&pool, &source, "scheduled").await?;
        match super::source_management::reconcile_source(
            &state,
            &pool,
            &auth,
            &agent,
            &governance,
            &source,
        )
        .await
        {
            Ok(changed) => {
                sources::finish_sync_run(&pool, &run.id, "succeeded", i32::from(changed), 0, None)
                    .await?;
                audit::record(
                    &pool,
                    "source-scheduler",
                    "agent.source.synced",
                    "agent",
                    &agent.id,
                    json!({ "sync_run_id": run.id, "changed": changed }),
                )
                .await?;
                let (health, latency_ms) =
                    super::source_management::run_health_check(&state, &pool, &agent).await?;
                audit::record(
                    &pool,
                    "source-scheduler",
                    "agent.health.checked",
                    "agent",
                    &agent.id,
                    json!({ "healthy": health.can_activate, "latency_ms": latency_ms }),
                )
                .await?;
                completed += 1;
            }
            Err(error) => {
                sources::mark_sync_state(&pool, &source.id, "sync_error", source.missing_count)
                    .await?;
                sources::finish_sync_run(&pool, &run.id, "failed", 0, 0, Some(&error.to_string()))
                    .await?;
                tracing::warn!(agent_id = %agent.id, "source sync failed: {error}");
            }
        }
    }
    Ok(completed)
}
