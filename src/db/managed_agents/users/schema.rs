use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct UserRow {
    pub id: String,
    pub display_name: String,
    pub email: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl UserRow {
    pub fn is_active(&self) -> bool {
        self.status == "active"
    }
}
