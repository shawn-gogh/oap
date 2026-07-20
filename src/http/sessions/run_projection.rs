use sqlx::PgPool;

use crate::{
    db::managed_agents::{artifacts, inbox, session_control},
    errors::GatewayError,
};

use super::run_types::RunSnapshotV1;

pub async fn load(
    pool: &PgPool,
    session_id: &str,
    turn_id: &str,
) -> Result<RunSnapshotV1, GatewayError> {
    let snapshot = session_control::repository::get_turn(pool, session_id, turn_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("turn {turn_id} not found")))?;
    let operations = session_control::repository::operations_for_turn(pool, turn_id).await?;
    let pending_requests = inbox::repository::pending_approvals(pool, Some(session_id), None)
        .await?
        .into_iter()
        .filter(|item| item.turn_id.as_deref().is_none_or(|id| id == turn_id))
        .collect();
    let artifacts = artifacts::repository::list(pool, session_id, Some(turn_id)).await?;
    let latest_sequence =
        session_control::repository::latest_event_sequence(pool, session_id).await?;
    RunSnapshotV1::from_parts(
        snapshot,
        operations,
        pending_requests,
        artifacts,
        latest_sequence,
    )
}
