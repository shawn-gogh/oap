use sqlx::PgPool;

use crate::{
    db::managed_agents::{id, now_ms},
    errors::GatewayError,
};

use super::schema::GroupRow;

pub async fn create(
    pool: &PgPool,
    name: &str,
    description: Option<&str>,
    created_by: &str,
) -> Result<GroupRow, GatewayError> {
    let now = now_ms();
    sqlx::query_as::<_, GroupRow>(
        r#"
        INSERT INTO "LiteLLM_GroupsTable"
          (id, name, description, status, created_by, created_at, updated_at)
        VALUES ($1, $2, $3, 'active', $4, $5, $5)
        RETURNING *
        "#,
    )
    .bind(id("group"))
    .bind(name.trim())
    .bind(description.map(str::trim).filter(|value| !value.is_empty()))
    .bind(created_by)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn list(pool: &PgPool, query: Option<&str>) -> Result<Vec<GroupRow>, GatewayError> {
    let query = query.unwrap_or("").trim();
    sqlx::query_as::<_, GroupRow>(
        r#"
        SELECT * FROM "LiteLLM_GroupsTable"
        WHERE $1 = '' OR name ILIKE '%' || $1 || '%' OR id ILIKE '%' || $1 || '%'
        ORDER BY name, id
        LIMIT 100
        "#,
    )
    .bind(query)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn find(pool: &PgPool, id: &str) -> Result<Option<GroupRow>, GatewayError> {
    sqlx::query_as::<_, GroupRow>(r#"SELECT * FROM "LiteLLM_GroupsTable" WHERE id = $1"#)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(GatewayError::Database)
}

pub async fn find_by_name(pool: &PgPool, name: &str) -> Result<Option<GroupRow>, GatewayError> {
    sqlx::query_as::<_, GroupRow>(r#"SELECT * FROM "LiteLLM_GroupsTable" WHERE name = $1"#)
        .bind(name.trim())
        .fetch_optional(pool)
        .await
        .map_err(GatewayError::Database)
}

pub async fn update_status(
    pool: &PgPool,
    id: &str,
    status: &str,
) -> Result<Option<GroupRow>, GatewayError> {
    let status = match status {
        "disabled" => "disabled",
        _ => "active",
    };
    sqlx::query_as::<_, GroupRow>(
        r#"UPDATE "LiteLLM_GroupsTable" SET status = $2, updated_at = $3 WHERE id = $1 RETURNING *"#,
    )
    .bind(id)
    .bind(status)
    .bind(now_ms())
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}
