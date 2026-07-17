use serde_json::Value;
use sqlx::PgPool;

use crate::{
    db::managed_agents::{id, now_ms},
    errors::GatewayError,
};

use super::schema::ExternalIdentityMappingRow;

pub async fn observe(
    pool: &PgPool,
    issuer: &str,
    subject: &str,
    audience: &str,
    claims_digest: &str,
    evidence: Value,
) -> Result<ExternalIdentityMappingRow, GatewayError> {
    let now = now_ms();
    sqlx::query_as::<_, ExternalIdentityMappingRow>(
        r#"
        INSERT INTO "LiteLLM_ExternalIdentityMappingsTable" (
          id, issuer, subject, audience, status, claims_digest, evidence,
          created_at, updated_at, last_seen_at
        )
        VALUES ($1, $2, $3, $4, 'pending', $5, $6, $7, $7, $7)
        ON CONFLICT (issuer, subject, audience) DO UPDATE SET
          claims_digest = EXCLUDED.claims_digest,
          evidence = "LiteLLM_ExternalIdentityMappingsTable".evidence || EXCLUDED.evidence,
          updated_at = EXCLUDED.updated_at,
          last_seen_at = EXCLUDED.last_seen_at
        RETURNING *
        "#,
    )
    .bind(id("identity"))
    .bind(issuer)
    .bind(subject)
    .bind(audience)
    .bind(claims_digest)
    .bind(evidence)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn find_external(
    pool: &PgPool,
    issuer: &str,
    subject: &str,
    audience: &str,
) -> Result<Option<ExternalIdentityMappingRow>, GatewayError> {
    sqlx::query_as::<_, ExternalIdentityMappingRow>(
        r#"
        SELECT * FROM "LiteLLM_ExternalIdentityMappingsTable"
        WHERE issuer = $1 AND subject = $2 AND audience = $3
        "#,
    )
    .bind(issuer)
    .bind(subject)
    .bind(audience)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn get(
    pool: &PgPool,
    mapping_id: &str,
) -> Result<Option<ExternalIdentityMappingRow>, GatewayError> {
    sqlx::query_as::<_, ExternalIdentityMappingRow>(
        r#"SELECT * FROM "LiteLLM_ExternalIdentityMappingsTable" WHERE id = $1"#,
    )
    .bind(mapping_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn list(
    pool: &PgPool,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<ExternalIdentityMappingRow>, GatewayError> {
    sqlx::query_as::<_, ExternalIdentityMappingRow>(
        r#"
        SELECT * FROM "LiteLLM_ExternalIdentityMappingsTable"
        WHERE $1::TEXT IS NULL OR status = $1
        ORDER BY last_seen_at DESC, id
        LIMIT $2
        "#,
    )
    .bind(status)
    .bind(limit.clamp(1, 200))
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn bind(
    pool: &PgPool,
    mapping_id: &str,
    user_id: &str,
    agent_id: Option<&str>,
    actor_id: &str,
) -> Result<Option<ExternalIdentityMappingRow>, GatewayError> {
    let now = now_ms();
    sqlx::query_as::<_, ExternalIdentityMappingRow>(
        r#"
        UPDATE "LiteLLM_ExternalIdentityMappingsTable"
        SET platform_user_id = $2,
            platform_agent_id = $3,
            status = 'active',
            bound_by = $4,
            bound_at = $5,
            updated_at = $5
        WHERE id = $1
          AND EXISTS (
            SELECT 1 FROM "LiteLLM_UsersTable"
            WHERE id = $2 AND status = 'active'
          )
          AND ($3::TEXT IS NULL OR EXISTS (
            SELECT 1 FROM "LiteLLM_ManagedAgentsTable" WHERE id = $3
          ))
        RETURNING *
        "#,
    )
    .bind(mapping_id)
    .bind(user_id)
    .bind(agent_id)
    .bind(actor_id)
    .bind(now)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn block(
    pool: &PgPool,
    mapping_id: &str,
    actor_id: &str,
) -> Result<Option<ExternalIdentityMappingRow>, GatewayError> {
    let now = now_ms();
    sqlx::query_as::<_, ExternalIdentityMappingRow>(
        r#"
        UPDATE "LiteLLM_ExternalIdentityMappingsTable"
        SET platform_user_id = NULL,
            platform_agent_id = NULL,
            status = 'blocked',
            bound_by = $2,
            bound_at = $3,
            updated_at = $3
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(mapping_id)
    .bind(actor_id)
    .bind(now)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}
