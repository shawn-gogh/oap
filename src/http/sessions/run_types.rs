use std::collections::BTreeMap;

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
    managed_agents::adapters::types::{ContinuationMode, InteractionProfileV1},
};

#[derive(Debug, Serialize)]
pub struct RunSnapshotV1 {
    pub schema_version: u16,
    pub turn: SessionTurnRow,
    pub interaction_profile: InteractionProfileV1,
    pub input: Value,
    pub result: Option<Value>,
    pub invocations: Vec<SessionInvocationRow>,
    pub operations: Vec<OperationV1>,
    pub progress: Option<RunProgressV1>,
    pub steps: Vec<RunStepV1>,
    pub pending_input_request: Option<PendingInputRequestV1>,
    pub pending_requests: Vec<InboxItemRow>,
    pub artifacts: Vec<ManagedArtifactRow>,
    pub latest_sequence: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunProgressV1 {
    pub schema_version: u16,
    pub mode: String,
    pub label: String,
    pub current: f64,
    pub total: Option<f64>,
    pub percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunStepV1 {
    pub schema_version: u16,
    pub id: String,
    pub invocation_id: Option<String>,
    pub label: String,
    pub status: String,
    pub index: Option<i64>,
    pub total: Option<i64>,
    pub metadata: Value,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperationV1 {
    pub schema_version: u16,
    pub id: String,
    pub session_id: String,
    pub turn_id: String,
    pub invocation_id: String,
    pub operation_key: String,
    pub operation_type: String,
    pub status: String,
    pub request_json: Value,
    pub result_json: Option<Value>,
    pub error_json: Option<Value>,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
}

impl From<SessionOperationRow> for OperationV1 {
    fn from(row: SessionOperationRow) -> Self {
        Self {
            schema_version: 1,
            id: row.id,
            session_id: row.session_id,
            turn_id: row.turn_id,
            invocation_id: row.invocation_id,
            operation_key: row.operation_key,
            operation_type: row.operation_type,
            status: row.status,
            request_json: row.request_json,
            result_json: row.result_json,
            error_json: row.error_json,
            created_at: row.created_at,
            updated_at: row.updated_at,
            completed_at: row.completed_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PendingInputRequestV1 {
    pub request_id: String,
    pub invocation_id: Option<String>,
    pub prompt: String,
    pub schema: Option<Value>,
    pub fields: Option<Value>,
    pub requested_at: i64,
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
        progress: Option<RunProgressV1>,
        steps: Vec<RunStepV1>,
        pending_input_request: Option<PendingInputRequestV1>,
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
            operations: operations.into_iter().map(OperationV1::from).collect(),
            progress,
            steps,
            pending_input_request,
            pending_requests,
            artifacts,
            latest_sequence,
        })
    }
}

pub fn canonical_progress(events: &[SessionControlEventRow]) -> Option<RunProgressV1> {
    events.iter().rev().find_map(|event| {
        if event.event_type != "invocation.progress" && event.event_type != "turn.progress" {
            return None;
        }
        let payload = &event.event_json;
        let current = payload
            .get("current")
            .and_then(Value::as_f64)
            .or_else(|| payload.get("percent").and_then(Value::as_f64))?;
        let total = payload.get("total").and_then(Value::as_f64);
        let percent = payload.get("percent").and_then(Value::as_f64).or_else(|| {
            total
                .filter(|total| *total > 0.0)
                .map(|total| current / total * 100.0)
        });
        Some(RunProgressV1 {
            schema_version: 1,
            mode: payload
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or(if total.is_some() { "steps" } else { "percent" })
                .to_owned(),
            label: payload
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("运行中")
                .to_owned(),
            current,
            total,
            percent,
        })
    })
}

pub fn canonical_steps(events: &[SessionControlEventRow]) -> Vec<RunStepV1> {
    let mut steps = BTreeMap::<String, RunStepV1>::new();
    for event in events {
        if !matches!(
            event.event_type.as_str(),
            "step.started" | "step.updated" | "step.completed" | "step.failed"
        ) {
            continue;
        }
        let payload = &event.event_json;
        let Some(id) = payload.get("id").and_then(Value::as_str) else {
            continue;
        };
        let step = steps.entry(id.to_owned()).or_insert_with(|| RunStepV1 {
            schema_version: 1,
            id: id.to_owned(),
            invocation_id: event.invocation_id.clone(),
            label: payload
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or(id)
                .to_owned(),
            status: "running".to_owned(),
            index: payload.get("index").and_then(Value::as_i64),
            total: payload.get("total").and_then(Value::as_i64),
            metadata: payload.get("metadata").cloned().unwrap_or_default(),
            started_at: Some(event.created_at),
            completed_at: None,
        });
        if let Some(label) = payload.get("label").and_then(Value::as_str) {
            step.label = label.to_owned();
        }
        step.status = match event.event_type.as_str() {
            "step.completed" => "completed",
            "step.failed" => "failed",
            _ => payload
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("running"),
        }
        .to_owned();
        if matches!(step.status.as_str(), "completed" | "failed" | "cancelled") {
            step.completed_at = Some(event.created_at);
        }
        if let Some(metadata) = payload.get("metadata") {
            step.metadata = metadata.clone();
        }
    }
    steps.into_values().collect()
}

#[derive(Debug, Deserialize)]
pub struct ResumeRunRequestV1 {
    pub request_id: String,
    pub input: Value,
    #[serde(default)]
    pub mode: Option<ContinuationMode>,
}

impl ResumeRunRequestV1 {
    pub fn mode(&self) -> ContinuationMode {
        self.mode.unwrap_or(ContinuationMode::Input)
    }
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

    use super::{ControlEventV1, ResumeRunRequestV1, RetryRunRequestV1};

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
                "progress",
                "steps",
                "pending_input_request",
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

    #[test]
    fn retry_request_preserves_an_explicit_idempotency_key() {
        let request: RetryRunRequestV1 = serde_json::from_value(json!({
            "request_id": " retry-request-1 ",
            "input": {"topic": "agents"}
        }))
        .unwrap();
        assert_eq!(request.request_id(), Some("retry-request-1"));
    }

    #[test]
    fn resume_request_defaults_to_input_and_accepts_typed_continuation() {
        let default_request: ResumeRunRequestV1 = serde_json::from_value(json!({
            "request_id": "request-1",
            "input": {"message": "continue"}
        }))
        .unwrap();
        assert_eq!(
            default_request.mode(),
            crate::managed_agents::adapters::types::ContinuationMode::Input
        );

        let file_request: ResumeRunRequestV1 = serde_json::from_value(json!({
            "request_id": "request-2",
            "mode": "file_upload",
            "input": {"artifact_id": "artifact-1"}
        }))
        .unwrap();
        assert_eq!(
            file_request.mode(),
            crate::managed_agents::adapters::types::ContinuationMode::FileUpload
        );
    }

    #[test]
    fn projects_latest_canonical_progress_and_step_state() {
        let events = vec![
            control_event(
                1,
                "invocation.progress",
                json!({"mode": "steps", "label": "Planning", "current": 1, "total": 4}),
            ),
            control_event(
                2,
                "step.started",
                json!({"id": "research", "label": "Research", "index": 1, "total": 2}),
            ),
            control_event(
                3,
                "step.completed",
                json!({"id": "research", "label": "Research", "index": 1, "total": 2}),
            ),
            control_event(
                4,
                "invocation.progress",
                json!({"mode": "steps", "label": "Writing", "current": 3, "total": 4}),
            ),
        ];

        let progress = super::canonical_progress(&events).unwrap();
        assert_eq!(progress.label, "Writing");
        assert_eq!(progress.current, 3.0);
        assert_eq!(progress.percent, Some(75.0));
        let steps = super::canonical_steps(&events);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].id, "research");
        assert_eq!(steps[0].status, "completed");
        assert_eq!(steps[0].completed_at, Some(3));
    }

    fn control_event(seq: i32, event_type: &str, event_json: Value) -> SessionControlEventRow {
        SessionControlEventRow {
            id: format!("event_{seq}"),
            session_id: "session_1".to_owned(),
            turn_id: Some("turn_1".to_owned()),
            invocation_id: Some("invocation_1".to_owned()),
            request_id: Some("request_1".to_owned()),
            seq,
            event_key: format!("event:{seq}"),
            event_type: event_type.to_owned(),
            event_json,
            created_at: i64::from(seq),
        }
    }
}
