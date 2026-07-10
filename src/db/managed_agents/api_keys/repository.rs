//! Persistent gateway API keys. Only a sha256 of the key is stored; the
//! plaintext is returned exactly once from `create`.

use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::{db::managed_agents::now_ms, errors::GatewayError};

use super::schema::GatewayApiKeyRow;

pub fn hash_key(key: &str) -> String {
    format!("{:x}", Sha256::digest(key.as_bytes()))
}

pub struct CreatedKey {
    pub row: GatewayApiKeyRow,
    pub key: String,
}

pub async fn create(
    pool: &PgPool,
    label: Option<&str>,
    user_id: Option<&str>,
    role: Option<&str>,
) -> Result<CreatedKey, GatewayError> {
    let id = format!("key_{}", uuid::Uuid::new_v4().simple());
    let key = format!(
        "sk-{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let role = match role {
        Some("admin") => "admin",
        _ => "user",
    };
    let user_id = user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&id);
    crate::db::managed_agents::users::repository::ensure(pool, user_id).await?;
    let row = sqlx::query_as::<_, GatewayApiKeyRow>(
        r#"INSERT INTO "LiteLLM_GatewayApiKeysTable"
             (id, key_hash, label, user_id, role, created_at)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING *"#,
    )
    .bind(&id)
    .bind(hash_key(&key))
    .bind(label.map(str::trim).filter(|l| !l.is_empty()))
    .bind(user_id)
    .bind(role)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(CreatedKey { row, key })
}

pub async fn list(pool: &PgPool) -> Result<Vec<GatewayApiKeyRow>, GatewayError> {
    sqlx::query_as::<_, GatewayApiKeyRow>(
        r#"SELECT * FROM "LiteLLM_GatewayApiKeysTable" ORDER BY created_at DESC, id"#,
    )
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

/// Returns the deleted row's key_hash so the caller can evict it from the
/// in-process auth cache immediately (otherwise a revoked key can keep
/// authenticating for up to CACHE_TTL more seconds).
pub async fn delete(pool: &PgPool, id: &str) -> Result<Option<String>, GatewayError> {
    sqlx::query_scalar::<_, String>(
        r#"DELETE FROM "LiteLLM_GatewayApiKeysTable" WHERE id = $1 RETURNING key_hash"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

/// Looks a presented key up by hash and touches `last_used_at`.
pub async fn find_by_key(
    pool: &PgPool,
    presented: &str,
) -> Result<Option<GatewayApiKeyRow>, GatewayError> {
    let row = sqlx::query_as::<_, GatewayApiKeyRow>(
        r#"UPDATE "LiteLLM_GatewayApiKeysTable"
           SET last_used_at = $2
           WHERE key_hash = $1
           RETURNING *"#,
    )
    .bind(hash_key(presented))
    .bind(now_ms())
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(row)
}
