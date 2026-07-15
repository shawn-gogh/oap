use serde::Serialize;
use sqlx::PgPool;

use crate::{db::managed_agents::now_ms, errors::GatewayError};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AgentGovernanceRow {
    pub agent_id: String,
    pub owner_id: String,
    pub source_provider: String,
    pub source_endpoint: String,
    pub external_agent_id: String,
    pub source_version: i32,
    pub source_hash: String,
    pub lifecycle_status: String,
    pub runtime_health: String,
    pub health_detail: Option<String>,
    pub credential_scope: String,
    pub credential_name: Option<String>,
    pub tested_revision: Option<i32>,
    pub published_revision: Option<i32>,
    pub previous_published_revision: Option<i32>,
    pub publish_approval_id: Option<String>,
    pub last_health_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug)]
pub struct ImportedSource<'a> {
    pub agent_id: &'a str,
    pub owner_id: &'a str,
    pub provider: &'a str,
    pub endpoint: &'a str,
    pub external_agent_id: &'a str,
    pub source_hash: &'a str,
    pub credential_scope: &'a str,
    pub credential_name: Option<&'a str>,
}

pub async fn get(
    pool: &PgPool,
    agent_id: &str,
) -> Result<Option<AgentGovernanceRow>, GatewayError> {
    sqlx::query_as::<_, AgentGovernanceRow>(
        r#"SELECT * FROM "LiteLLM_ManagedAgentGovernanceTable" WHERE agent_id = $1"#,
    )
    .bind(agent_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn find_by_source(
    pool: &PgPool,
    owner_id: &str,
    provider: &str,
    endpoint: &str,
    external_agent_id: &str,
) -> Result<Option<AgentGovernanceRow>, GatewayError> {
    sqlx::query_as::<_, AgentGovernanceRow>(
        r#"
        SELECT * FROM "LiteLLM_ManagedAgentGovernanceTable"
        WHERE owner_id = $1 AND source_provider = $2
          AND source_endpoint = $3 AND external_agent_id = $4
        "#,
    )
    .bind(owner_id)
    .bind(provider)
    .bind(endpoint)
    .bind(external_agent_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn record_import(
    pool: &PgPool,
    source: ImportedSource<'_>,
) -> Result<AgentGovernanceRow, GatewayError> {
    let now = now_ms();
    sqlx::query_as::<_, AgentGovernanceRow>(
        r#"
        INSERT INTO "LiteLLM_ManagedAgentGovernanceTable" (
          agent_id, owner_id, source_provider, source_endpoint, external_agent_id,
          source_version, source_hash, lifecycle_status, runtime_health,
          credential_scope, credential_name, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, 1, $6, 'imported', 'unknown', $7, $8, $9, $9)
        ON CONFLICT (agent_id) DO UPDATE SET
          source_version = CASE
            WHEN "LiteLLM_ManagedAgentGovernanceTable".source_hash = EXCLUDED.source_hash
              THEN "LiteLLM_ManagedAgentGovernanceTable".source_version
            ELSE "LiteLLM_ManagedAgentGovernanceTable".source_version + 1
          END,
          source_hash = EXCLUDED.source_hash,
          lifecycle_status = 'imported',
          runtime_health = 'unknown',
          health_detail = NULL,
          credential_scope = EXCLUDED.credential_scope,
          credential_name = EXCLUDED.credential_name,
          tested_revision = NULL,
          publish_approval_id = NULL,
          updated_at = EXCLUDED.updated_at
        RETURNING *
        "#,
    )
    .bind(source.agent_id)
    .bind(source.owner_id)
    .bind(source.provider)
    .bind(source.endpoint)
    .bind(source.external_agent_id)
    .bind(source.source_hash)
    .bind(source.credential_scope)
    .bind(source.credential_name)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn mark_tested(
    pool: &PgPool,
    agent_id: &str,
    revision: i32,
    healthy: bool,
    detail: &str,
) -> Result<AgentGovernanceRow, GatewayError> {
    let health = if healthy { "healthy" } else { "unhealthy" };
    let lifecycle = if healthy { "tested" } else { "unhealthy" };
    sqlx::query_as::<_, AgentGovernanceRow>(
        r#"
        UPDATE "LiteLLM_ManagedAgentGovernanceTable"
        SET lifecycle_status = $2, runtime_health = $3, health_detail = $4,
            tested_revision = $5, last_health_at = $6, updated_at = $6
        WHERE agent_id = $1
        RETURNING *
        "#,
    )
    .bind(agent_id)
    .bind(lifecycle)
    .bind(health)
    .bind(detail)
    .bind(revision)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn request_publish(
    pool: &PgPool,
    agent_id: &str,
    approval_id: &str,
) -> Result<AgentGovernanceRow, GatewayError> {
    sqlx::query_as::<_, AgentGovernanceRow>(
        r#"
        UPDATE "LiteLLM_ManagedAgentGovernanceTable"
        SET lifecycle_status = 'pending_approval', publish_approval_id = $2, updated_at = $3
        WHERE agent_id = $1
        RETURNING *
        "#,
    )
    .bind(agent_id)
    .bind(approval_id)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn mark_changed(
    pool: &PgPool,
    agent_id: &str,
) -> Result<AgentGovernanceRow, GatewayError> {
    sqlx::query_as::<_, AgentGovernanceRow>(
        r#"
        UPDATE "LiteLLM_ManagedAgentGovernanceTable"
        SET lifecycle_status = 'imported', runtime_health = 'unknown',
            health_detail = NULL, tested_revision = NULL,
            publish_approval_id = NULL, updated_at = $2
        WHERE agent_id = $1
        RETURNING *
        "#,
    )
    .bind(agent_id)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn reject_publish(
    pool: &PgPool,
    agent_id: &str,
) -> Result<AgentGovernanceRow, GatewayError> {
    sqlx::query_as::<_, AgentGovernanceRow>(
        r#"
        UPDATE "LiteLLM_ManagedAgentGovernanceTable"
        SET lifecycle_status = CASE WHEN runtime_health = 'healthy' THEN 'tested' ELSE 'unhealthy' END,
            publish_approval_id = NULL, updated_at = $2
        WHERE agent_id = $1
        RETURNING *
        "#,
    )
    .bind(agent_id)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn publish(
    pool: &PgPool,
    agent_id: &str,
    revision: i32,
) -> Result<AgentGovernanceRow, GatewayError> {
    sqlx::query_as::<_, AgentGovernanceRow>(
        r#"
        UPDATE "LiteLLM_ManagedAgentGovernanceTable"
        SET lifecycle_status = 'published', runtime_health = 'healthy',
            previous_published_revision = published_revision,
            published_revision = $2, publish_approval_id = NULL, updated_at = $3
        WHERE agent_id = $1 AND lifecycle_status = 'pending_approval'
        RETURNING *
        "#,
    )
    .bind(agent_id)
    .bind(revision)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn mark_rolled_back(
    pool: &PgPool,
    agent_id: &str,
    revision: i32,
) -> Result<AgentGovernanceRow, GatewayError> {
    sqlx::query_as::<_, AgentGovernanceRow>(
        r#"
        UPDATE "LiteLLM_ManagedAgentGovernanceTable"
        SET lifecycle_status = 'rolled_back', runtime_health = 'healthy',
            previous_published_revision = published_revision,
            published_revision = $2, tested_revision = $2,
            publish_approval_id = NULL, updated_at = $3
        WHERE agent_id = $1
        RETURNING *
        "#,
    )
    .bind(agent_id)
    .bind(revision)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}
