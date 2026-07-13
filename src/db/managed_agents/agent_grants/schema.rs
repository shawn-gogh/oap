use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AgentGrantRow {
    pub id: String,
    pub agent_id: String,
    pub grantee_user_id: String,
    /// 'use' — can see the agent and start sessions on it.
    /// 'edit' — additionally can modify it (config, workspace, evals).
    pub permission: String,
    pub granted_by: Option<String>,
    pub created_at: i64,
}
