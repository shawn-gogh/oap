use serde_json::Value;
use sqlx::PgPool;

use crate::{
    db::managed_agents::{id, now_ms},
    errors::GatewayError,
};

use super::schema::CredentialLeaseRow;

pub struct NewCredentialLease<'a> {
    pub owner_id: &'a str,
    pub session_id: &'a str,
    pub turn_id: &'a str,
    pub invocation_id: &'a str,
    pub credential_name: &'a str,
    pub adapter_id: &'a str,
    pub purpose: &'a str,
    pub ttl_ms: i64,
    pub metadata: Value,
}

pub async fn issue(
    pool: &PgPool,
    input: NewCredentialLease<'_>,
) -> Result<CredentialLeaseRow, GatewayError> {
    let now = now_ms();
    let expires_at = now.saturating_add(input.ttl_ms.max(1));
    sqlx::query_as::<_, CredentialLeaseRow>(
        r#"
        INSERT INTO "LiteLLM_AgentCredentialLeasesTable" (
          id, owner_id, session_id, turn_id, invocation_id, credential_name,
          adapter_id, purpose, issued_at, expires_at, metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        ON CONFLICT (invocation_id, credential_name) DO UPDATE SET
          expires_at = GREATEST("LiteLLM_AgentCredentialLeasesTable".expires_at, EXCLUDED.expires_at),
          metadata = "LiteLLM_AgentCredentialLeasesTable".metadata || EXCLUDED.metadata,
          revoked_at = NULL
        RETURNING *
        "#,
    )
    .bind(id("lease"))
    .bind(input.owner_id)
    .bind(input.session_id)
    .bind(input.turn_id)
    .bind(input.invocation_id)
    .bind(input.credential_name)
    .bind(input.adapter_id)
    .bind(input.purpose)
    .bind(now)
    .bind(expires_at)
    .bind(input.metadata)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn mark_resolved(
    pool: &PgPool,
    lease_id: &str,
    owner_id: &str,
    now: i64,
) -> Result<bool, GatewayError> {
    let result = sqlx::query(
        r#"
        UPDATE "LiteLLM_AgentCredentialLeasesTable"
        SET last_resolved_at = $3
        WHERE id = $1 AND owner_id = $2 AND revoked_at IS NULL AND expires_at > $3
        "#,
    )
    .bind(lease_id)
    .bind(owner_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected() == 1)
}

pub async fn get(
    pool: &PgPool,
    lease_id: &str,
) -> Result<Option<CredentialLeaseRow>, GatewayError> {
    sqlx::query_as::<_, CredentialLeaseRow>(
        r#"SELECT * FROM "LiteLLM_AgentCredentialLeasesTable" WHERE id = $1"#,
    )
    .bind(lease_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn revoke_for_turn(pool: &PgPool, turn_id: &str) -> Result<u64, GatewayError> {
    let result = sqlx::query(
        r#"
        UPDATE "LiteLLM_AgentCredentialLeasesTable"
        SET revoked_at = $2
        WHERE turn_id = $1 AND revoked_at IS NULL
        "#,
    )
    .bind(turn_id)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected())
}

pub async fn expire_due(pool: &PgPool, now: i64) -> Result<u64, GatewayError> {
    let result = sqlx::query(
        r#"
        UPDATE "LiteLLM_AgentCredentialLeasesTable"
        SET revoked_at = $1
        WHERE revoked_at IS NULL AND expires_at <= $1
        "#,
    )
    .bind(now)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected())
}
