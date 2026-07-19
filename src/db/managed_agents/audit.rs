use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;

use crate::{
    db::managed_agents::{id, now_ms},
    errors::GatewayError,
};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AuditLogRow {
    pub id: String,
    pub actor_user_id: String,
    pub action: String,
    pub target_type: String,
    pub target_id: String,
    pub metadata: Value,
    pub created_at: i64,
}

pub async fn record(
    pool: &PgPool,
    actor_user_id: &str,
    action: &str,
    target_type: &str,
    target_id: &str,
    metadata: Value,
) -> Result<(), GatewayError> {
    sqlx::query(
        r#"
        INSERT INTO "LiteLLM_AuditLogsTable"
          (id, actor_user_id, action, target_type, target_id, metadata, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(id("audit"))
    .bind(actor_user_id)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(metadata)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(())
}

pub async fn list(pool: &PgPool, limit: i64) -> Result<Vec<AuditLogRow>, GatewayError> {
    sqlx::query_as::<_, AuditLogRow>(
        r#"SELECT * FROM "LiteLLM_AuditLogsTable" ORDER BY created_at DESC LIMIT $1"#,
    )
    .bind(limit.clamp(1, 200))
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn list_for_target(
    pool: &PgPool,
    target_type: &str,
    target_id: &str,
    limit: i64,
) -> Result<Vec<AuditLogRow>, GatewayError> {
    sqlx::query_as::<_, AuditLogRow>(
        r#"
        SELECT * FROM "LiteLLM_AuditLogsTable"
        WHERE target_type = $1 AND target_id = $2
        ORDER BY created_at DESC
        LIMIT $3
        "#,
    )
    .bind(target_type)
    .bind(target_id)
    .bind(limit.clamp(1, 200))
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}
