use std::{sync::Arc, time::Duration};

use crate::{
    db::managed_agents::{now_ms, tasks::repository},
    errors::GatewayError,
    proxy::state::AppState,
};

const POLL_INTERVAL: Duration = Duration::from_secs(30);
const BATCH_SIZE: i64 = 100;

pub fn spawn(state: Arc<AppState>) {
    if state.db.is_none() {
        return;
    }
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(POLL_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if let Err(error) = run_due_once(state.clone(), now_ms()).await {
                tracing::warn!("task timeout sweep failed: {error}");
            }
        }
    });
}

pub async fn run_due_once(state: Arc<AppState>, now: i64) -> Result<usize, GatewayError> {
    let Some(pool) = state.db.as_ref() else {
        return Ok(0);
    };
    let task_ids = repository::list_due_for_timeout(pool, now, BATCH_SIZE).await?;
    let mut timed_out = 0;
    for task_id in task_ids {
        let Some(cancellation) = repository::timeout(pool, &task_id, now).await? else {
            continue;
        };
        let interruption =
            super::terminate_task_execution(&state, pool, &cancellation, "timed out").await;
        tracing::info!(
            task_id = %task_id,
            agent_id = %cancellation.task.agent_id,
            interruption,
            "task timed out"
        );
        timed_out += 1;
    }
    Ok(timed_out)
}
