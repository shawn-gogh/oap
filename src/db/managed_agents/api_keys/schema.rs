use serde::Serialize;

pub const ROLE_ADMIN: &str = "admin";
pub const ROLE_USER: &str = "user";
pub const ROLE_IMPORTER: &str = "importer";
pub const ROLE_APPROVER: &str = "approver";
pub const ROLE_OPERATOR: &str = "operator";

pub fn valid_role(role: &str) -> bool {
    matches!(
        role,
        ROLE_ADMIN | ROLE_USER | ROLE_IMPORTER | ROLE_APPROVER | ROLE_OPERATOR
    )
}

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
        self.role == ROLE_ADMIN
    }
}
