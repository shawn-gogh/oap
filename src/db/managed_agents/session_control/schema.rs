use serde::Serialize;
use serde_json::Value;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SessionTurnRow {
    pub id: String,
    pub session_id: String,
    pub request_id: String,
    pub status: String,
    pub model: Option<String>,
    pub input_json: Value,
    pub input_schema_json: Value,
    pub output_schema_json: Value,
    pub interaction_profile_json: Value,
    pub result_json: Option<Value>,
    pub trigger_type: String,
    pub retry_of_turn_id: Option<String>,
    pub attempt_number: i32,
    pub deadline_at: Option<i64>,
    pub error_json: Option<Value>,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SessionInvocationRow {
    pub id: String,
    pub session_id: String,
    pub turn_id: String,
    pub parent_invocation_id: Option<String>,
    pub agent_id: Option<String>,
    pub agent_revision: Option<i32>,
    pub runtime: Option<String>,
    pub protocol: String,
    pub protocol_version: String,
    pub adapter_id: String,
    pub role: String,
    pub status: String,
    pub remote_agent_id: Option<String>,
    pub remote_session_id: Option<String>,
    pub remote_context_id: Option<String>,
    pub remote_task_id: Option<String>,
    pub resume_cursor: Option<String>,
    pub metadata: Value,
    pub error_json: Option<Value>,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SessionControlEventRow {
    pub id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub invocation_id: Option<String>,
    pub request_id: Option<String>,
    pub seq: i32,
    pub event_key: String,
    pub event_type: String,
    pub event_json: Value,
    pub created_at: i64,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SessionOperationRow {
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

#[derive(Debug, Clone, Serialize)]
pub struct TurnSnapshot {
    pub turn: SessionTurnRow,
    pub invocations: Vec<SessionInvocationRow>,
}

#[derive(Debug, Clone, FromRow)]
pub struct TurnRecoveryCandidate {
    pub turn_id: String,
    pub session_id: String,
    pub turn_status: String,
    pub session_status: String,
    pub runtime: Option<String>,
    pub turn_updated_at: i64,
    pub deadline_at: Option<i64>,
    pub session_updated_at: Option<i64>,
}
