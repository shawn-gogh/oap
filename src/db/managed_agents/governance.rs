use serde::Serialize;
use sqlx::PgPool;

use crate::{
    db::managed_agents::{now_ms, registry::schema::ManagedAgentRow},
    errors::GatewayError,
};

mod reviews;
pub use reviews::{mark_due_for_review, publish};

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
    pub published_at: Option<i64>,
    pub review_due_at: Option<i64>,
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

pub fn external_source_kind(agent: &ManagedAgentRow) -> Option<&str> {
    agent
        .config
        .pointer("/source/kind")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|kind| !kind.is_empty())
}

pub fn requires_governance(agent: &ManagedAgentRow) -> bool {
    matches!(
        external_source_kind(agent),
        Some("external_agent" | "opencode_agent_file" | "agent_bundle")
    )
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

/// Matches a live agent for "re-importing the same source updates it in
/// place" (see `persist_imported_agent`/`update_from_import`). Deliberately
/// excludes soft-deleted agents (`archived_pending_delete`, pending the
/// reaper's 7-day purge, see `registry::cleanup`) — otherwise re-importing a
/// source you just deleted silently resurrects the old agent under its old
/// id, with all its sessions/tasks/history intact, which contradicts what
/// "delete" told the user it did.
pub async fn find_by_source(
    pool: &PgPool,
    owner_id: &str,
    provider: &str,
    endpoint: &str,
    external_agent_id: &str,
) -> Result<Option<AgentGovernanceRow>, GatewayError> {
    sqlx::query_as::<_, AgentGovernanceRow>(
        r#"
        SELECT g.* FROM "LiteLLM_ManagedAgentGovernanceTable" g
        JOIN "LiteLLM_ManagedAgentsTable" a ON a.id = g.agent_id
        WHERE g.owner_id = $1 AND g.source_provider = $2
          AND g.source_endpoint = $3 AND g.external_agent_id = $4
          AND a.status != 'archived_pending_delete'
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
    // (owner_id, source_provider, source_endpoint, external_agent_id) is
    // unique — a soft-deleted agent's governance row still holds that
    // identity for the rest of its 7-day retention window (see
    // `registry::cleanup`), so importing the same source again to make a
    // *new* agent (see `find_by_source`, which excludes soft-deleted rows)
    // would otherwise collide on that constraint. Free the identity by
    // tombstoning the old row instead of blocking the re-import or silently
    // reviving the deleted agent under it.
    sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentGovernanceTable" g
        SET external_agent_id = g.external_agent_id || '#deleted:' || g.agent_id
        FROM "LiteLLM_ManagedAgentsTable" a
        WHERE a.id = g.agent_id
          AND g.agent_id != $1
          AND g.owner_id = $2 AND g.source_provider = $3
          AND g.source_endpoint = $4 AND g.external_agent_id = $5
          AND a.status = 'archived_pending_delete'
        "#,
    )
    .bind(source.agent_id)
    .bind(source.owner_id)
    .bind(source.provider)
    .bind(source.endpoint)
    .bind(source.external_agent_id)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    let governance = sqlx::query_as::<_, AgentGovernanceRow>(
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
          lifecycle_status = CASE
            WHEN "LiteLLM_ManagedAgentGovernanceTable".source_hash = EXCLUDED.source_hash
              THEN "LiteLLM_ManagedAgentGovernanceTable".lifecycle_status
            ELSE 'imported'
          END,
          runtime_health = CASE
            WHEN "LiteLLM_ManagedAgentGovernanceTable".source_hash = EXCLUDED.source_hash
              THEN "LiteLLM_ManagedAgentGovernanceTable".runtime_health
            ELSE 'unknown'
          END,
          health_detail = CASE
            WHEN "LiteLLM_ManagedAgentGovernanceTable".source_hash = EXCLUDED.source_hash
              THEN "LiteLLM_ManagedAgentGovernanceTable".health_detail
            ELSE NULL
          END,
          credential_scope = EXCLUDED.credential_scope,
          credential_name = EXCLUDED.credential_name,
          tested_revision = CASE
            WHEN "LiteLLM_ManagedAgentGovernanceTable".source_hash = EXCLUDED.source_hash
              THEN "LiteLLM_ManagedAgentGovernanceTable".tested_revision
            ELSE NULL
          END,
          publish_approval_id = CASE
            WHEN "LiteLLM_ManagedAgentGovernanceTable".source_hash = EXCLUDED.source_hash
              THEN "LiteLLM_ManagedAgentGovernanceTable".publish_approval_id
            ELSE NULL
          END,
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
    .map_err(GatewayError::Database)?;
    let management_mode =
        match crate::db::managed_agents::registry::repository::get(pool, &governance.agent_id)
            .await?
            .as_ref()
            .and_then(external_source_kind)
        {
            Some("external_agent") => "federated",
            _ => "managed",
        };
    crate::db::managed_agents::sources::repository::ensure_source(
        pool,
        &governance,
        management_mode,
        None,
    )
    .await?;
    Ok(governance)
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
            tested_revision = $5, last_health_at = $6, updated_at = $6,
            publish_approval_id = NULL
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

pub async fn suspend(
    pool: &PgPool,
    agent_id: &str,
    detail: &str,
) -> Result<AgentGovernanceRow, GatewayError> {
    sqlx::query_as::<_, AgentGovernanceRow>(
        r#"
        UPDATE "LiteLLM_ManagedAgentGovernanceTable"
        SET lifecycle_status = 'suspended', runtime_health = 'degraded',
            health_detail = $2, publish_approval_id = NULL, updated_at = $3
        WHERE agent_id = $1 RETURNING *
        "#,
    )
    .bind(agent_id)
    .bind(detail)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn retire(
    pool: &PgPool,
    agent_id: &str,
    detail: &str,
) -> Result<AgentGovernanceRow, GatewayError> {
    sqlx::query_as::<_, AgentGovernanceRow>(
        r#"
        UPDATE "LiteLLM_ManagedAgentGovernanceTable"
        SET lifecycle_status = 'retired', runtime_health = 'unreachable',
            health_detail = $2, tested_revision = NULL,
            publish_approval_id = NULL, updated_at = $3
        WHERE agent_id = $1 RETURNING *
        "#,
    )
    .bind(agent_id)
    .bind(detail)
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

pub async fn mark_rolled_back(
    pool: &PgPool,
    agent_id: &str,
    revision: i32,
) -> Result<AgentGovernanceRow, GatewayError> {
    sqlx::query_as::<_, AgentGovernanceRow>(
        r#"
        UPDATE "LiteLLM_ManagedAgentGovernanceTable"
        SET lifecycle_status = 'rolled_back', runtime_health = 'unknown',
            health_detail = '已回滚到先前发布的版本，建议重新运行检查确认当前健康状态。',
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
