use std::{sync::Arc, time::Duration};

use crate::{
    db::managed_agents::{memory, now_ms, registry},
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
        WHERE (config->>'deleted_at')::BIGINT < $1
        "#,
    )
    .bind(cutoff)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)?;

    for agent_id in due_agents {
        tracing::info!(agent_id = %agent_id, "permanently deleting soft-deleted agent");
        // Safety net for rows soft-deleted before delete started stamping, and
        // for any session created between the soft delete and this sweep: once
        // the agent row is gone the session's agent_id points at nothing.
        if let Some(agent) = registry::repository::get(pool, &agent_id).await? {
            let snapshot = serde_json::json!({
                "agent_id": agent.id,
                "name": agent.name,
                "model": agent.model,
                "harness": agent.harness,
                "runtime": agent.config.get("runtime").and_then(|value| value.as_str()),
                "deleted_at": crate::http::managed_agents::agent_deleted_at(&agent),
            });
            let _ = crate::db::managed_agents::sessions::repository::stamp_deleted_agent(
                pool, &agent_id, &snapshot,
            )
            .await;
        }
        if registry::repository::delete(pool, &agent_id).await? {
            let _ = memory::repository::delete_all(pool, &agent_id).await;
            let _ = crate::db::managed_agents::agent_grants::repository::delete_all_for_agent(
                pool, &agent_id,
            )
            .await;
            let _ = crate::db::managed_agents::groups::agent_grants::delete_all_for_agent(
                pool, &agent_id,
            )
            .await;
            // Per-user BYO keys are named after the agent id and can never be
            // reused once it's gone — purge them so encrypted keys don't
            // accumulate as orphans. (The shared provider credential is keyed
            // by provider+external id and survives re-imports, so it stays.)
            let _ = sqlx::query(
                r#"DELETE FROM "LiteLLM_CredentialsTable" WHERE credential_name = $1 AND scope = 'personal'"#,
            )
            .bind(crate::http::runtime_resolution::byo_credential_name(&agent_id))
            .execute(pool)
            .await;

            if let Some(storage) = &state.object_storage {
                let bucket =
                    crate::object_storage::ObjectStorageClient::agent_bucket_name(&agent_id);
                if storage.bucket_exists(&bucket).await {
                    let _ = storage.delete_bucket_recursive(&bucket).await;
                }
            }
        }
    }

    Ok(())
}
