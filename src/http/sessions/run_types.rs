use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    db::managed_agents::{
        artifacts::schema::ManagedArtifactRow,
        inbox::schema::InboxItemRow,
        session_control::schema::{
            SessionControlEventRow, SessionInvocationRow, SessionOperationRow, SessionTurnRow,
            TurnSnapshot,
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

#[derive(Debug, Serialize)]
pub struct ControlEventV1 {
    pub schema_version: u16,
    #[serde(rename = "type")]
    pub event_type: String,
    pub sequence: i32,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub invocation_id: Option<String>,
    pub request_id: Option<String>,
    pub occurred_at: i64,
    pub payload: Value,
}

impl From<SessionControlEventRow> for ControlEventV1 {
    fn from(event: SessionControlEventRow) -> Self {
        Self {
            schema_version: 1,
            event_type: event.event_type,
            sequence: event.seq,
            session_id: event.session_id,
            turn_id: event.turn_id,
            invocation_id: event.invocation_id,
            request_id: event.request_id,
            occurred_at: event.created_at,
            payload: event.event_json,
        }
    }
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
    request_id: Option<String>,
}

impl RetryRunRequestV1 {
    pub fn request_id(&self) -> Option<&str> {
        self.request_id
            .as_deref()
            .map(str::trim)
            .filter(|request_id| !request_id.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use crate::{
        db::managed_agents::session_control::schema::SessionControlEventRow,
        managed_agents::adapters::types::InteractionProfileV1,
    };

    use super::ControlEventV1;

    #[test]
    fn interaction_profile_fixture_matches_v1_contract() {
        let fixture =
            include_str!("../../../tests/fixtures/run_contract/interaction-profile-v1.json");
        let profile: InteractionProfileV1 = serde_json::from_str(fixture).unwrap();
        assert_eq!(profile.schema_version, 1);
        assert!(profile.supports_retry);
    }

    #[test]
    fn run_snapshot_fixtures_keep_required_projection_fields() {
        for fixture in [
            include_str!("../../../tests/fixtures/run_contract/run-snapshot-running-v1.json"),
            include_str!("../../../tests/fixtures/run_contract/run-snapshot-waiting-input-v1.json"),
            include_str!("../../../tests/fixtures/run_contract/run-snapshot-completed-v1.json"),
        ] {
            let snapshot: Value = serde_json::from_str(fixture).unwrap();
            for field in [
                "schema_version",
                "turn",
                "interaction_profile",
                "input",
                "invocations",
                "operations",
                "pending_requests",
                "artifacts",
                "latest_sequence",
            ] {
                assert!(snapshot.get(field).is_some(), "fixture missing {field}");
            }
        }
    }

    #[test]
    fn control_event_projection_is_versioned_and_provider_neutral() {
        let event = ControlEventV1::from(SessionControlEventRow {
            id: "event_1".to_owned(),
            session_id: "session_1".to_owned(),
            turn_id: Some("turn_1".to_owned()),
            invocation_id: Some("invocation_1".to_owned()),
            request_id: Some("request_1".to_owned()),
            seq: 7,
            event_key: "turn:turn_1:result:completed".to_owned(),
            event_type: "result.completed".to_owned(),
            event_json: json!({"result": {"score": 0.9}}),
            created_at: 1_720_000_000_000,
        });
        let value = serde_json::to_value(event).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["sequence"], 7);
        assert_eq!(value["type"], "result.completed");
        assert_eq!(value["payload"]["result"]["score"], 0.9);
        assert!(value.get("provider").is_none());
    }
}
