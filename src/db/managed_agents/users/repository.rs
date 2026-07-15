use sqlx::PgPool;

use crate::{db::managed_agents::now_ms, errors::GatewayError};

use super::schema::UserRow;

pub async fn ensure(pool: &PgPool, id: &str) -> Result<UserRow, GatewayError> {
    let id = id.trim();
    let now = now_ms();
    sqlx::query_as::<_, UserRow>(
        r#"
        INSERT INTO "LiteLLM_UsersTable" (id, display_name, status, created_at, updated_at)
        VALUES ($1, $1, 'active', $2, $2)
        ON CONFLICT (id) DO UPDATE SET updated_at = "LiteLLM_UsersTable".updated_at
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn find(pool: &PgPool, id: &str) -> Result<Option<UserRow>, GatewayError> {
    sqlx::query_as::<_, UserRow>(r#"SELECT * FROM "LiteLLM_UsersTable" WHERE id = $1"#)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(GatewayError::Database)
}

pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<UserRow>, GatewayError> {
    sqlx::query_as::<_, UserRow>(
        r#"SELECT * FROM "LiteLLM_UsersTable" WHERE LOWER(email) = LOWER($1)"#,
    )
    .bind(email.trim())
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn create(
    pool: &PgPool,
    id: &str,
    display_name: &str,
    email: Option<&str>,
) -> Result<UserRow, GatewayError> {
    let now = now_ms();
    sqlx::query_as::<_, UserRow>(
        r#"
        INSERT INTO "LiteLLM_UsersTable" (id, display_name, email, status, created_at, updated_at)
        VALUES ($1, $2, $3, 'active', $4, $4)
        RETURNING *
        "#,
    )
    .bind(id.trim())
    .bind(display_name.trim())
    .bind(email.map(str::trim).filter(|value| !value.is_empty()))
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn list(pool: &PgPool, query: Option<&str>) -> Result<Vec<UserRow>, GatewayError> {
    let query = query.unwrap_or("").trim();
    sqlx::query_as::<_, UserRow>(
        r#"
        SELECT * FROM "LiteLLM_UsersTable"
        WHERE $1 = '' OR id ILIKE '%' || $1 || '%'
           OR display_name ILIKE '%' || $1 || '%'
           OR COALESCE(email, '') ILIKE '%' || $1 || '%'
        ORDER BY display_name, id
        LIMIT 100
        "#,
    )
    .bind(query)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn update_status(
    pool: &PgPool,
    id: &str,
    status: &str,
) -> Result<Option<UserRow>, GatewayError> {
    let status = match status {
        "disabled" => "disabled",
        _ => "active",
    };
    sqlx::query_as::<_, UserRow>(
        r#"UPDATE "LiteLLM_UsersTable" SET status = $2, updated_at = $3 WHERE id = $1 RETURNING *"#,
    )
    .bind(id)
    .bind(status)
    .bind(now_ms())
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn update_profile(
    pool: &PgPool,
    id: &str,
    display_name: Option<&str>,
    email: Option<Option<&str>>,
) -> Result<Option<UserRow>, GatewayError> {
    let email = email.map(|value| value.map(str::trim).filter(|value| !value.is_empty()));
    sqlx::query_as::<_, UserRow>(
        r#"
        UPDATE "LiteLLM_UsersTable"
        SET display_name = COALESCE($2, display_name),
            email = CASE WHEN $3 THEN $4 ELSE email END,
            updated_at = $5
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(
        display_name
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    )
    .bind(email.is_some())
    .bind(email.flatten())
    .bind(now_ms())
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn active_ids(pool: &PgPool, ids: &[String]) -> Result<Vec<String>, GatewayError> {
    sqlx::query_scalar::<_, String>(
        r#"SELECT id FROM "LiteLLM_UsersTable" WHERE id = ANY($1) AND status = 'active'"#,
    )
    .bind(ids)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}
