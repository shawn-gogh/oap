use std::{sync::Arc, time::Duration};

use crate::{
    db::managed_agents::{exposed_apps, now_ms},
    errors::GatewayError,
    proxy::state::AppState,
};

const CLEANUP_INTERVAL: Duration = Duration::from_secs(600);

pub fn spawn(state: Arc<AppState>) {
    if state.db.is_none() {
        return;
    }
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(CLEANUP_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if let Err(error) = run_cleanup_once(&state).await {
                tracing::warn!("expired exposed apps cleanup failed: {error}");
            }
        }
    });
}

pub async fn run_cleanup_once(state: &AppState) -> Result<(), GatewayError> {
    let Some(pool) = state.db.as_ref() else {
        return Ok(());
    };
    let released = exposed_apps::repository::soft_delete_expired(pool, now_ms()).await?;
    if released > 0 {
        tracing::info!(released, "released expired exposed app ports");
    }
    Ok(())
}
