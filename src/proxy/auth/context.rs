#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user_id: String,
    pub is_admin: bool,
    pub role: String,
}

impl AuthContext {
    pub fn admin() -> Self {
        Self::with_role("admin", "admin")
    }

    pub fn user(user_id: impl Into<String>) -> Self {
        Self::with_role(user_id, "user")
    }

    pub fn operator(user_id: impl Into<String>) -> Self {
        Self::with_role(user_id, "operator")
    }

    pub fn with_role(user_id: impl Into<String>, role: impl Into<String>) -> Self {
        let role = role.into();
        Self {
            user_id: user_id.into(),
            is_admin: role == "admin",
            role,
        }
    }

    pub fn can_import(&self) -> bool {
        self.is_admin || self.role == "importer"
    }

    pub fn can_approve(&self) -> bool {
        self.is_admin || self.role == "approver"
    }

    pub fn can_operate(&self) -> bool {
        self.is_admin || self.role == "operator"
    }
}
