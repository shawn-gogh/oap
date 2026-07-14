use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::{
    db::managed_agents::{id, now_ms},
    errors::GatewayError,
    proxy::auth::master_key::AuthContext,
};

const SESSION_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, sqlx::FromRow)]
struct WebSessionRow {
    user_id: String,
    is_admin: bool,
}

pub struct CreatedSession {
    pub token: String,
    pub expires_at: i64,
}

pub fn hash_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

pub async fn create(pool: &PgPool, auth: &AuthContext) -> Result<CreatedSession, GatewayError> {
    let now = now_ms();
    let expires_at = now + SESSION_TTL_MS;
    let token = format!(
        "ws_{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    sqlx::query(
        r#"
        INSERT INTO "LiteLLM_WebSessionsTable"
          (id, token_hash, user_id, is_admin, created_at, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(id("web_session"))
    .bind(hash_token(&token))
    .bind(&auth.user_id)
    .bind(auth.is_admin)
    .bind(now)
    .bind(expires_at)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(CreatedSession { token, expires_at })
}

pub async fn authenticate(pool: &PgPool, token: &str) -> Result<Option<AuthContext>, GatewayError> {
    let row = sqlx::query_as::<_, WebSessionRow>(
        r#"
        SELECT user_id, is_admin
        FROM "LiteLLM_WebSessionsTable"
        WHERE token_hash = $1 AND revoked_at IS NULL AND expires_at > $2
          AND (
            is_admin OR EXISTS(
              SELECT 1 FROM "LiteLLM_UsersTable" users
              WHERE users.id = "LiteLLM_WebSessionsTable".user_id AND users.status = 'active'
            )
          )
        "#,
    )
    .bind(hash_token(token))
    .bind(now_ms())
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(row.map(|row| AuthContext {
        user_id: row.user_id,
        is_admin: row.is_admin,
    }))
}

pub async fn revoke(pool: &PgPool, token: &str) -> Result<bool, GatewayError> {
    let result = sqlx::query(
        r#"
        UPDATE "LiteLLM_WebSessionsTable"
        SET revoked_at = $2
        WHERE token_hash = $1 AND revoked_at IS NULL
        "#,
    )
    .bind(hash_token(token))
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected() > 0)
}

pub async fn revoke_all_for_user(pool: &PgPool, user_id: &str) -> Result<u64, GatewayError> {
    let result = sqlx::query(
        r#"
        UPDATE "LiteLLM_WebSessionsTable"
        SET revoked_at = $2
        WHERE user_id = $1 AND revoked_at IS NULL
        "#,
    )
    .bind(user_id)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected())
}
