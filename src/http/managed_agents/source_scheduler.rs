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
    let mut completed =
        expire_due_reviews_once(state.clone(), crate::db::managed_agents::now_ms()).await?;
    let due = sources::list_due_sources(&pool, 25).await?;
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

pub async fn expire_due_reviews_once(
    state: Arc<AppState>,
    now: i64,
) -> Result<usize, GatewayError> {
    let Some(pool) = state.db.as_ref().cloned() else {
        return Ok(0);
    };
    let due = governance::mark_due_for_review(&pool, now, 25).await?;
    for review in &due {
        let Some(agent) = repository::get(&pool, &review.agent_id).await? else {
            continue;
        };
        audit::record(
            &pool,
            "source-scheduler",
            "agent.governance.review_due",
            "agent",
            &agent.id,
            json!({
                "published_at": review.published_at,
                "review_due_at": review.review_due_at,
            }),
        )
        .await?;
        super::mattermost::notify_governance_event(
            &state,
            &pool,
            &agent,
            super::mattermost::GovernanceNotification::ReviewDue {
                review_due_at: review.review_due_at.unwrap_or(now),
            },
        )
        .await;
    }
    Ok(due.len())
}
