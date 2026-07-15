use std::{sync::Arc, time::Duration};

use crate::{
    db::managed_agents::{inbox::repository, now_ms},
    errors::GatewayError,
    proxy::state::AppState,
};

use super::{approvals::deliver_and_record, types::ApprovalScope};

const POLL_INTERVAL: Duration = Duration::from_secs(15);
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
                tracing::warn!(%error, "approval expiry sweep failed");
            }
        }
    });
}

pub async fn run_due_once(state: Arc<AppState>, now: i64) -> Result<usize, GatewayError> {
    let Some(pool) = state.db.as_ref() else {
        return Ok(0);
    };
    for id in repository::list_due_for_escalation(pool, now, BATCH_SIZE).await? {
        if !repository::mark_escalated(pool, &id).await? {
            continue;
        }
        if let Some(item) = repository::get(pool, &id).await? {
            crate::db::managed_agents::audit::record(
                pool,
                "system:approval-escalation",
                "approval.escalated",
                &item.kind,
                &id,
                serde_json::json!({ "escalation_role": item.escalation_role }),
            )
            .await?;
        }
    }
    let ids = repository::list_due_for_expiry(pool, now, BATCH_SIZE).await?;
    let mut expired = 0;
    for id in ids {
        let Some(item) = repository::expire(pool, &id).await? else {
            continue;
        };
        let delivery_status = deliver_and_record(&state, pool, &id, ApprovalScope::Once).await?;
        crate::db::managed_agents::audit::record(
            pool,
            "system:approval-timeout",
            "approval.expired",
            &item.kind,
            &id,
            serde_json::json!({ "delivery_status": delivery_status }),
        )
        .await?;
        expired += 1;
    }
    Ok(expired)
}
