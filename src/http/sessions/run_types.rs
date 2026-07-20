use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    db::managed_agents::{
        artifacts::schema::ManagedArtifactRow,
        inbox::schema::InboxItemRow,
        session_control::schema::{
            SessionInvocationRow, SessionOperationRow, SessionTurnRow, TurnSnapshot,
        },
    },
    errors::GatewayError,
    managed_agents::adapters::types::InteractionProfileV1,
};

#[derive(Debug, Serialize)]
pub struct RunSnapshotV1 {
    pub schema_version: u16,
    pub turn: SessionTurnRow,
    pub interaction_profile: InteractionProfileV1,
    pub input: Value,
    pub result: Option<Value>,
    pub invocations: Vec<SessionInvocationRow>,
    pub operations: Vec<SessionOperationRow>,
    pub pending_requests: Vec<InboxItemRow>,
    pub artifacts: Vec<ManagedArtifactRow>,
    pub latest_sequence: i32,
}

impl RunSnapshotV1 {
    pub fn from_parts(
        snapshot: TurnSnapshot,
        operations: Vec<SessionOperationRow>,
        pending_requests: Vec<InboxItemRow>,
        artifacts: Vec<ManagedArtifactRow>,
        latest_sequence: i32,
    ) -> Result<Self, GatewayError> {
        let interaction_profile = if snapshot.turn.interaction_profile_json == serde_json::json!({})
        {
            InteractionProfileV1::default()
        } else {
            serde_json::from_value(snapshot.turn.interaction_profile_json.clone())?
        };
        Ok(Self {
            schema_version: 1,
            input: snapshot.turn.input_json.clone(),
            result: snapshot.turn.result_json.clone(),
            turn: snapshot.turn,
            interaction_profile,
            invocations: snapshot.invocations,
            operations,
            pending_requests,
            artifacts,
            latest_sequence,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct ResumeRunRequestV1 {
    pub request_id: String,
    pub input: Value,
}

#[derive(Debug, Default, Deserialize)]
pub struct RetryRunRequestV1 {
    pub input: Option<Value>,
}
