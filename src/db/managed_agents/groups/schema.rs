use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct GroupRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl GroupRow {
    pub fn is_active(&self) -> bool {
        self.status == "active"
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct GroupMemberRow {
    pub group_id: String,
    pub user_id: String,
    pub member_role: String,
    pub added_by: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AgentGroupGrantRow {
    pub id: String,
    pub agent_id: String,
    pub group_id: String,
    pub permission: String,
    pub granted_by: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}
