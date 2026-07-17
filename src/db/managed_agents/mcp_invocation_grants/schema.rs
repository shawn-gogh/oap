use std::collections::HashSet;

use serde::Serialize;
use serde_json::Value;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct McpInvocationGrantRow {
    pub id: String,
    pub owner_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub invocation_id: String,
    pub server_id: String,
    pub allowed_tools: Value,
    pub allow_all: bool,
    pub issued_at: i64,
    pub expires_at: i64,
    pub last_used_at: Option<i64>,
    pub use_count: i32,
    pub revoked_at: Option<i64>,
    pub metadata: Value,
}

impl McpInvocationGrantRow {
    pub fn tool_names(&self) -> HashSet<String> {
        self.allowed_tools
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    }

    pub fn allows(&self, tool_name: &str) -> bool {
        self.allow_all || self.tool_names().contains(tool_name)
    }
}
