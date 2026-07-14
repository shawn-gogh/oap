//! Immutable config snapshots, one per agent create/update. The version
//! history is what makes single-variable iteration attributable: an eval or
//! experience-pool record can point at the exact config it ran against.

use serde_json::Value;
use sqlx::PgPool;

use crate::{
    db::managed_agents::{id, now_ms},
    errors::GatewayError,
};

use super::schema::ManagedAgentRow;

/// Records a new revision of the agent (version = last + 1). Best-effort by
/// design at call sites: a failed snapshot must not fail the mutation itself.
pub async fn record(
    pool: &PgPool,
    agent: &ManagedAgentRow,
    created_by: Option<&str>,
) -> Result<i32, GatewayError> {
    let snapshot: Value =
        serde_json::to_value(agent).map_err(|e| GatewayError::InvalidConfig(e.to_string()))?;
    let version: i32 = sqlx::query_scalar(
        r#"
        INSERT INTO "LiteLLM_ManagedAgentRevisionsTable"
          (id, agent_id, version, snapshot, created_by, created_at)
        SELECT $1, $2,
               COALESCE(MAX(version), 0) + 1,
               $3, $4, $5
        FROM "LiteLLM_ManagedAgentRevisionsTable"
        WHERE agent_id = $2
        RETURNING version
        "#,
    )
    .bind(id("rev"))
    .bind(&agent.id)
    .bind(&snapshot)
    .bind(created_by)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(version)
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct AgentRevisionRow {
    pub id: String,
    pub agent_id: String,
    pub version: i32,
    pub snapshot: Value,
    pub created_by: Option<String>,
    pub created_at: i64,
}

pub async fn list(
    pool: &PgPool,
    agent_id: &str,
    limit: i64,
) -> Result<Vec<AgentRevisionRow>, GatewayError> {
    sqlx::query_as::<_, AgentRevisionRow>(
        r#"
        SELECT * FROM "LiteLLM_ManagedAgentRevisionsTable"
        WHERE agent_id = $1
        ORDER BY version DESC
        LIMIT $2
        "#,
    )
    .bind(agent_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn latest_version(pool: &PgPool, agent_id: &str) -> Result<Option<i32>, GatewayError> {
    sqlx::query_scalar::<_, i32>(
        r#"
        SELECT version FROM "LiteLLM_ManagedAgentRevisionsTable"
        WHERE agent_id = $1 ORDER BY version DESC LIMIT 1
        "#,
    )
    .bind(agent_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn get_version(
    pool: &PgPool,
    agent_id: &str,
    version: i32,
) -> Result<Option<AgentRevisionRow>, GatewayError> {
    sqlx::query_as::<_, AgentRevisionRow>(
        r#"
        SELECT * FROM "LiteLLM_ManagedAgentRevisionsTable"
        WHERE agent_id = $1 AND version = $2
        "#,
    )
    .bind(agent_id)
    .bind(version)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}
