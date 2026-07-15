use sqlx::PgPool;

use crate::{
    db::managed_agents::{id, now_ms},
    errors::GatewayError,
};

use super::schema::{ExposedAppRow, NewExposedApp};

/// Platform-managed allocation range. Agents are told which port to bind, so
/// the range only needs to avoid the runtime images' own service ports.
pub const PORT_RANGE_START: i32 = 30000;
pub const PORT_RANGE_END: i32 = 39999;
const ALLOCATION_ATTEMPTS: usize = 16;

/// Registers an exposed app. With `requested_port` the insert is direct; a
/// conflict on the partial unique index means the port is taken. Without it,
/// random ports from the platform range are tried until one inserts cleanly —
/// the unique constraint is the arbiter, so concurrent allocations never race.
pub async fn allocate(
    pool: &PgPool,
    input: NewExposedApp<'_>,
    requested_port: Option<i32>,
) -> Result<ExposedAppRow, GatewayError> {
    if let Some(port) = requested_port {
        return match insert(pool, &input, port).await {
            Err(GatewayError::Database(error)) if is_unique_violation(&error) => {
                Err(GatewayError::BadRequest(format!(
                    "port {port} is already exposed on {}",
                    input.container_key
                )))
            }
            other => other,
        };
    }

    for _ in 0..ALLOCATION_ATTEMPTS {
        let port = random_port();
        match insert(pool, &input, port).await {
            Err(GatewayError::Database(error)) if is_unique_violation(&error) => continue,
            other => return other,
        }
    }
    Err(GatewayError::BadRequest(format!(
        "no free port available on {} after {ALLOCATION_ATTEMPTS} attempts",
        input.container_key
    )))
}

async fn insert(
    pool: &PgPool,
    input: &NewExposedApp<'_>,
    port: i32,
) -> Result<ExposedAppRow, GatewayError> {
    sqlx::query_as::<_, ExposedAppRow>(
        r#"
        INSERT INTO "LiteLLM_ExposedAppsTable"
          (id, session_id, agent_id, owner_user_id, container_key, port, name, created_at, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING *
        "#,
    )
    .bind(id("app"))
    .bind(input.session_id)
    .bind(input.agent_id)
    .bind(input.owner_user_id)
    .bind(input.container_key)
    .bind(port)
    .bind(input.name)
    .bind(now_ms())
    .bind(input.expires_at)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

fn random_port() -> i32 {
    let span = (PORT_RANGE_END - PORT_RANGE_START + 1) as u128;
    PORT_RANGE_START + (uuid::Uuid::new_v4().as_u128() % span) as i32
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|db| db.code())
        .is_some_and(|code| code == "23505")
}

pub async fn get(pool: &PgPool, app_id: &str) -> Result<Option<ExposedAppRow>, GatewayError> {
    sqlx::query_as::<_, ExposedAppRow>(r#"SELECT * FROM "LiteLLM_ExposedAppsTable" WHERE id = $1"#)
        .bind(app_id)
        .fetch_optional(pool)
        .await
        .map_err(GatewayError::Database)
}

/// Active and unexpired — what the proxy is allowed to route to.
pub async fn get_routable(
    pool: &PgPool,
    app_id: &str,
) -> Result<Option<ExposedAppRow>, GatewayError> {
    sqlx::query_as::<_, ExposedAppRow>(
        r#"
        SELECT * FROM "LiteLLM_ExposedAppsTable"
        WHERE id = $1 AND status = 'active'
          AND (expires_at IS NULL OR expires_at > $2)
        "#,
    )
    .bind(app_id)
    .bind(now_ms())
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn list_for_session(
    pool: &PgPool,
    session_id: &str,
) -> Result<Vec<ExposedAppRow>, GatewayError> {
    sqlx::query_as::<_, ExposedAppRow>(
        r#"
        SELECT * FROM "LiteLLM_ExposedAppsTable"
        WHERE session_id = $1 AND status = 'active'
        ORDER BY created_at DESC
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn soft_delete(pool: &PgPool, app_id: &str) -> Result<bool, GatewayError> {
    let result = sqlx::query(
        r#"
        UPDATE "LiteLLM_ExposedAppsTable"
        SET status = 'deleted', deleted_at = $2
        WHERE id = $1 AND status = 'active'
        "#,
    )
    .bind(app_id)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected() > 0)
}

pub async fn soft_delete_for_session(
    pool: &PgPool,
    session_id: &str,
) -> Result<u64, GatewayError> {
    let result = sqlx::query(
        r#"
        UPDATE "LiteLLM_ExposedAppsTable"
        SET status = 'deleted', deleted_at = $2
        WHERE session_id = $1 AND status = 'active'
        "#,
    )
    .bind(session_id)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected())
}

/// Invalidates every previously issued share token for the app.
pub async fn bump_share_version(pool: &PgPool, app_id: &str) -> Result<Option<i32>, GatewayError> {
    sqlx::query_scalar::<_, i32>(
        r#"
        UPDATE "LiteLLM_ExposedAppsTable"
        SET share_version = share_version + 1
        WHERE id = $1 AND status = 'active'
        RETURNING share_version
        "#,
    )
    .bind(app_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn soft_delete_expired(pool: &PgPool, now: i64) -> Result<u64, GatewayError> {
    let result = sqlx::query(
        r#"
        UPDATE "LiteLLM_ExposedAppsTable"
        SET status = 'deleted', deleted_at = $1
        WHERE status = 'active' AND expires_at IS NOT NULL AND expires_at <= $1
        "#,
    )
    .bind(now)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected())
}
