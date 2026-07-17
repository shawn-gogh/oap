use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct CloudEventReceiptRow {
    pub id: String,
    pub direction: String,
    pub session_id: String,
    pub cloud_event_id: String,
    pub cloud_event_source: String,
    pub cloud_event_type: String,
    pub subject: Option<String>,
    pub data_digest: String,
    pub canonical_event_key: String,
    pub actor_user_id: String,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub delivery_count: i32,
}
