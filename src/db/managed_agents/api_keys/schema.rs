use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct GatewayApiKeyRow {
    pub id: String,
    #[serde(skip_serializing)]
    pub key_hash: String,
    pub label: Option<String>,
    pub user_id: String,
    pub role: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

impl GatewayApiKeyRow {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}
