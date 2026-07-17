use std::{sync::Arc, time::Duration};

use serde_json::json;

use crate::{
    db::managed_agents::{now_ms, session_control, sessions},
    errors::GatewayError,
    proxy::state::AppState,
};

const POLL_INTERVAL: Duration = Duration::from_secs(15);
const BATCH_SIZE: i64 = 100;
const UNKNOWN_STATE_GRACE_MS: i64 = 5 * 60 * 1_000;

pub fn spawn(state: Arc<AppState>) {
    if state.db.is_none() {
        return;
    }
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(POLL_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if let Err(error) = run_once(state.clone(), now_ms()).await {
                tracing::warn!(%error, "session recovery sweep failed");
            }
        }
    });
}

pub async fn run_once(state: Arc<AppState>, now: i64) -> Result<usize, GatewayError> {
    let Some(pool) = state.db.as_ref() else {
        return Ok(0);
    };
    crate::db::managed_agents::credential_leases::repository::expire_due(pool, now).await?;
    crate::db::managed_agents::mcp_invocation_grants::repository::expire_due(pool, now).await?;
    let candidates = session_control::repository::recovery_candidates(pool, BATCH_SIZE).await?;
    let mut reconciled = 0;
    for candidate in candidates {
        if let Some(terminal) = terminal_turn_status(&candidate.session_status) {
            let error = (terminal == "failed").then(|| {
                json!({
                    "code": "session_state_reconciled",
                    "message": "session reached a terminal state before its turn"
                })
            });
            session_control::repository::transition(pool, &candidate.turn_id, terminal, error)
                .await?;
            reconciled += 1;
            continue;
        }
        if !matches!(
            candidate.session_status.as_str(),
            "starting" | "running" | "busy"
        ) {
            continue;
        }
        let Some(runtime) = candidate.runtime.as_deref() else {
            continue;
        };
        if state.provider_consumers.is_running(&candidate.session_id) {
            continue;
        }
        let Some(row) = sessions::repository::get(pool, &candidate.session_id).await? else {
            continue;
        };
        if super::external_bridge::supports(runtime)
            || super::generic_chat::is_generic_chat(pool, runtime).await?
        {
            reconcile_unknown_state(&state, &candidate, now, "gateway runtime cannot resume")
                .await?;
            continue;
        }
        match super::runtime_events_api::ensure_provider_consumer(&state, pool, &row, runtime).await
        {
            Ok(()) => {
                reconciled += 1;
            }
            Err(error) => {
                tracing::warn!(
                    session_id = %candidate.session_id,
                    adapter_runtime = runtime,
                    %error,
                    "runtime session recovery was deferred"
                );
                reconcile_unknown_state(&state, &candidate, now, "provider state is unavailable")
                    .await?;
            }
        }
    }
    Ok(reconciled)
}

async fn reconcile_unknown_state(
    state: &AppState,
    candidate: &session_control::schema::TurnRecoveryCandidate,
    now: i64,
    detail: &str,
) -> Result<(), GatewayError> {
    let updated_at = candidate
        .session_updated_at
        .unwrap_or(candidate.turn_updated_at)
        .max(candidate.turn_updated_at);
    if now.saturating_sub(updated_at) < UNKNOWN_STATE_GRACE_MS {
        return Ok(());
    }
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    super::runtime_lifecycle::mark_session_error(
        state,
        pool,
        &candidate.session_id,
        format!("state_unknown: {detail}"),
    )
    .await
}

fn terminal_turn_status(session_status: &str) -> Option<&'static str> {
    match session_status {
        "idle" | "completed" => Some("completed"),
        "cancelled" => Some("cancelled"),
        "timed_out" => Some("timed_out"),
        "error" | "failed" => Some("failed"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::terminal_turn_status;

    #[test]
    fn maps_legacy_session_terminal_states_to_turn_states() {
        assert_eq!(terminal_turn_status("idle"), Some("completed"));
        assert_eq!(terminal_turn_status("error"), Some("failed"));
        assert_eq!(terminal_turn_status("cancelled"), Some("cancelled"));
        assert_eq!(terminal_turn_status("running"), None);
    }
}
