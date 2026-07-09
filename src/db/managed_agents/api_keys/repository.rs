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
    let row = sqlx::query_as::<_, GatewayApiKeyRow>(
        r#"INSERT INTO "LiteLLM_GatewayApiKeysTable"
             (id, key_hash, label, user_id, role, created_at)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING *"#,
    )
    .bind(&id)
    .bind(hash_key(&key))
    .bind(label.map(str::trim).filter(|l| !l.is_empty()))
    .bind(user_id.map(str::trim).filter(|u| !u.is_empty()).unwrap_or(&id))
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

pub async fn delete(pool: &PgPool, id: &str) -> Result<bool, GatewayError> {
    let result = sqlx::query(r#"DELETE FROM "LiteLLM_GatewayApiKeysTable" WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await
        .map_err(GatewayError::Database)?;
    Ok(result.rows_affected() > 0)
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
