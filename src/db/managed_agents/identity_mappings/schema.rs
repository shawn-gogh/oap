use serde::Serialize;
use serde_json::Value;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ExternalIdentityMappingRow {
    pub id: String,
    pub issuer: String,
    pub subject: String,
    pub audience: String,
    pub platform_user_id: Option<String>,
    pub platform_agent_id: Option<String>,
    pub status: String,
    pub claims_digest: String,
    pub evidence: Value,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_seen_at: i64,
    pub bound_by: Option<String>,
    pub bound_at: Option<i64>,
}
