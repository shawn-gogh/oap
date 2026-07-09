use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct EvalRunRow {
    pub id: String,
    pub agent_id: String,
    pub agent_version: Option<i32>,
    pub model: String,
    pub status: String,
    pub total: i32,
    pub passed: i32,
    pub results: Value,
    pub error: Option<String>,
    pub created_by: Option<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}
