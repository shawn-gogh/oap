use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct InboxItemRow {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub session_id: Option<String>,
    pub agent: Option<String>,
    pub body: Option<String>,
    pub args_json: Option<String>,
    pub status: String,
    pub feedback: Option<String>,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
    pub enforcement_owner: String,
    pub effect_handler: String,
    pub required_role: String,
    pub delivery_status: String,
    pub delivery_attempts: i32,
    pub last_delivery_error: Option<String>,
    pub expires_at: Option<i64>,
    pub escalation_role: Option<String>,
    pub escalate_at: Option<i64>,
    pub escalated_at: Option<i64>,
    pub decided_by: Option<String>,
    pub decision_scope: String,
    pub applied_at: Option<i64>,
}
