use serde::Serialize;
use serde_json::Value;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct CredentialLeaseRow {
    pub id: String,
    pub owner_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub invocation_id: String,
    pub credential_name: String,
    pub adapter_id: String,
    pub purpose: String,
    pub issued_at: i64,
    pub expires_at: i64,
    pub last_resolved_at: Option<i64>,
    pub revoked_at: Option<i64>,
    pub metadata: Value,
}
