//! Evolution sweeper: the scheduler of the self-improvement loop.
//!
//! Opt-in per agent via `config.design.auto_evolve: true`. On each sweep, for
//! every opted-in agent with an evaluation definition:
//! - if there is no fresh eval run, start one;
//! - if the latest completed run has failures and no improvement proposal is
//!   already pending, draft one (it lands in the inbox — a human still
//!   approves before anything changes).
//!
//! Spend guard: at most one eval per agent per EVAL_INTERVAL, and never more
//! than one pending proposal per agent.

use std::{sync::Arc, time::Duration};

use axum::{extract::State, http::HeaderMap, Json};
use serde_json::{json, Value};

use crate::{
    db::managed_agents::{eval_runs, inbox, now_ms, registry},
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

/// Admin-only manual trigger (the background loop runs every 30 minutes).
pub async fn sweep(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    if !auth.is_admin {
        return Err(GatewayError::Unauthorized);
    }
    let actions = sweep_once(state).await?;
    Ok(Json(json!({ "actions": actions })))
}

const POLL_INTERVAL: Duration = Duration::from_secs(30 * 60);
const EVAL_INTERVAL_MS: i64 = 24 * 60 * 60 * 1000;

pub fn spawn(state: Arc<AppState>) {
    if state.db.is_none() {
        return;
    }
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(POLL_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if let Err(error) = sweep_once(state.clone()).await {
                tracing::warn!("evolution sweep failed: {error}");
            }
        }
    });
}

pub async fn sweep_once(state: Arc<AppState>) -> Result<usize, crate::errors::GatewayError> {
    let Some(pool) = state.db.as_ref().cloned() else {
        return Ok(0);
    };
    let mut actions = 0;
    let agents = registry::repository::list(&pool, None).await?;
    let pending = inbox::repository::pending_approvals(&pool, None, None).await?;
    let now = now_ms();

    for agent in agents {
        if agent.config.pointer("/design/auto_evolve") != Some(&Value::Bool(true)) {
            continue;
        }
        if agent.config.pointer("/design/evaluation").is_none() {
            continue;
        }

        let runs = eval_runs::repository::list(&pool, &agent.id, 1).await?;
        let latest = runs.first();

        // A run is already in flight — wait for it.
        if latest.is_some_and(|run| run.status == "running") {
            continue;
        }

        let needs_eval = match latest {
            None => true,
            Some(run) => now - run.created_at > EVAL_INTERVAL_MS,
        };
        if needs_eval {
            match super::eval_runs::start_eval_run(state.clone(), &pool, agent, "evolution-sweep")
                .await
            {
                Ok(run) => {
                    actions += 1;
                    tracing::info!(run_id = %run.id, "evolution sweep started eval");
                }
                Err(error) => tracing::warn!(%error, "evolution sweep eval failed to start"),
            }
            continue;
        }

        // Latest run is completed and recent; propose only on failures, and
        // only if no proposal for this agent is already awaiting a decision.
        let has_failures =
            latest.is_some_and(|run| run.status == "completed" && run.passed < run.total);
        if !has_failures {
            continue;
        }
        let already_pending = pending.iter().any(|item| {
            item.args_json
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                .is_some_and(|args| {
                    args.get("type").and_then(Value::as_str) == Some("agent_improvement")
                        && args.get("agent_id").and_then(Value::as_str) == Some(agent.id.as_str())
                })
        });
        if already_pending {
            continue;
        }
        match super::improvements::propose(&state, &pool, &agent).await {
            Ok(item) => {
                actions += 1;
                tracing::info!(proposal_id = %item.id, agent_id = %agent.id, "evolution sweep filed improvement proposal");
            }
            Err(error) => {
                tracing::warn!(agent_id = %agent.id, %error, "evolution sweep proposal failed")
            }
        }
    }
    Ok(actions)
}
