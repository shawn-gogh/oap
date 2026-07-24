use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SourceConnectorRow {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub provider: String,
    pub endpoint: String,
    pub credential_name: Option<String>,
    pub status: String,
    pub capabilities: Value,
    pub adapter_id: Option<String>,
    pub protocol: Option<String>,
    pub protocol_version: Option<String>,
    pub negotiated_profile: Value,
    pub last_test_detail: Option<String>,
    pub last_test_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateSourceConnector {
    pub name: String,
    pub provider: String,
    pub endpoint: String,
    pub credential_name: Option<String>,
    pub adapter_id: String,
    pub protocol: String,
    pub protocol_version: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateSourceConnector {
    pub name: Option<String>,
    pub endpoint: Option<String>,
    pub credential_name: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ManagedAgentSourceRow {
    pub id: String,
    pub agent_id: String,
    pub connector_id: Option<String>,
    pub management_mode: String,
    pub sync_state: String,
    pub missing_count: i32,
    pub current_snapshot_id: Option<String>,
    pub candidate_snapshot_id: Option<String>,
    pub last_synced_at: Option<i64>,
    pub next_sync_at: Option<i64>,
    pub lease_owner: Option<String>,
    pub lease_until: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AgentSourceSnapshotRow {
    pub id: String,
    pub source_id: String,
    pub version: i32,
    pub digest: String,
    pub raw_spec: Value,
    pub canonical_spec: Value,
    pub protocol_profile: Value,
    pub normalization_issues: Value,
    pub agent_revision: Option<i32>,
    pub created_by: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AgentSourceSyncRunRow {
    pub id: String,
    pub source_id: String,
    pub connector_id: Option<String>,
    pub status: String,
    pub trigger_kind: String,
    pub cursor_before: Option<String>,
    pub cursor_after: Option<String>,
    pub discovered_count: i32,
    pub changed_count: i32,
    pub missing_count: i32,
    pub error_detail: Option<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AgentDriftFindingRow {
    pub id: String,
    pub source_id: String,
    pub snapshot_id: String,
    pub field_path: String,
    pub risk: String,
    pub previous_value: Option<Value>,
    pub candidate_value: Option<Value>,
    pub resolution: String,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AgentHealthCheckRow {
    pub id: String,
    pub agent_id: String,
    pub source_id: Option<String>,
    pub check_kind: String,
    pub status: String,
    pub detail: Option<String>,
    pub latency_ms: Option<i64>,
    pub checked_at: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RuntimeConformanceRow {
    pub agent_id: String,
    pub contract_version: String,
    pub status: String,
    pub checks: Value,
    pub checked_revision: Option<i32>,
    pub checked_at: i64,
}

#[derive(Debug, Serialize)]
pub struct AgentSourceOverview {
    pub source: ManagedAgentSourceRow,
    pub current_snapshot: Option<AgentSourceSnapshotRow>,
    pub candidate_snapshot: Option<AgentSourceSnapshotRow>,
    pub drift_findings: Vec<AgentDriftFindingRow>,
    pub recent_sync_runs: Vec<AgentSourceSyncRunRow>,
    pub recent_health_checks: Vec<AgentHealthCheckRow>,
    pub conformance: Option<RuntimeConformanceRow>,
}
