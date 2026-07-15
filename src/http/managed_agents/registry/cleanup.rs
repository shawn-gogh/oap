use std::{sync::Arc, time::Duration};

use crate::{
    db::managed_agents::{now_ms, memory, registry},
    errors::GatewayError,
    proxy::state::AppState,
};

const CLEANUP_INTERVAL: Duration = Duration::from_secs(3600); // Check every hour
const RETENTION_PERIOD_MS: i64 = 7 * 24 * 60 * 60 * 1000; // 7 days

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
                tracing::warn!("soft-deleted agents cleanup failed: {error}");
            }
        }
    });
}

pub async fn run_cleanup_once(state: &AppState) -> Result<(), GatewayError> {
    let Some(pool) = state.db.as_ref() else {
        return Ok(());
    };
    
    let cutoff = now_ms() - RETENTION_PERIOD_MS;
    
    let due_agents: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT id FROM "LiteLLM_ManagedAgentsTable"
        WHERE status = 'archived_pending_delete'
          AND (config->>'deleted_at')::BIGINT < $1
        "#
    )
    .bind(cutoff)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)?;
    
    for agent_id in due_agents {
        tracing::info!(agent_id = %agent_id, "permanently deleting soft-deleted agent");
        if registry::repository::delete(pool, &agent_id).await? {
            let _ = memory::repository::delete_all(pool, &agent_id).await;
            let _ = crate::db::managed_agents::agent_grants::repository::delete_all_for_agent(pool, &agent_id).await;
            let _ = crate::db::managed_agents::groups::agent_grants::delete_all_for_agent(pool, &agent_id).await;
            
            if let Some(storage) = &state.object_storage {
                let bucket = crate::object_storage::ObjectStorageClient::agent_bucket_name(&agent_id);
                if storage.bucket_exists(&bucket).await {
                    let _ = storage.delete_bucket_recursive(&bucket).await;
                }
            }
        }
    }
    
    Ok(())
}
