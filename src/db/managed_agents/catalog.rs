use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;

use crate::errors::GatewayError;

#[derive(Debug, sqlx::FromRow)]
pub struct CatalogSourceRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: Option<String>,
    pub harness: String,
    pub tools: Value,
    pub config: Value,
    pub skill_ids: Value,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CatalogConsumerRow {
    pub agent_id: String,
    pub user_id: String,
    pub display_name: String,
    pub last_used_at: i64,
    pub session_count: i64,
}

pub async fn list_sources(pool: &PgPool) -> Result<Vec<CatalogSourceRow>, GatewayError> {
    sqlx::query_as::<_, CatalogSourceRow>(
        r#"
        SELECT
          agent.id, agent.name, agent.description, agent.owner_id, agent.harness,
          agent.tools, agent.config, agent.skill_ids
        FROM "LiteLLM_ManagedAgentsTable" agent
        LEFT JOIN "LiteLLM_ManagedAgentGovernanceTable" governance
          ON governance.agent_id = agent.id
        WHERE agent.status = 'active'
          AND (
            governance.agent_id IS NULL
            OR governance.lifecycle_status IN ('published', 'rolled_back')
          )
        ORDER BY agent.name, agent.id
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn list_consumers(
    pool: &PgPool,
    agent_ids: &[String],
) -> Result<Vec<CatalogConsumerRow>, GatewayError> {
    if agent_ids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as::<_, CatalogConsumerRow>(
        r#"
        SELECT
          session.agent_id,
          session.owner_id AS user_id,
          COALESCE(user_row.display_name, session.owner_id) AS display_name,
          MAX(COALESCE(session.updated_at, session.created_at)) AS last_used_at,
          COUNT(*) AS session_count
        FROM "LiteLLM_ManagedAgentSessionsTable" session
        LEFT JOIN "LiteLLM_UsersTable" user_row ON user_row.id = session.owner_id
        WHERE session.agent_id = ANY($1)
          AND session.owner_id IS NOT NULL
          AND session.title NOT LIKE 'agent-builder-%'
        GROUP BY session.agent_id, session.owner_id, user_row.display_name
        ORDER BY last_used_at DESC
        "#,
    )
    .bind(agent_ids)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}
