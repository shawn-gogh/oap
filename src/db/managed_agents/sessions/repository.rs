use serde_json::Value;
use sqlx::PgPool;

use crate::{
    db::managed_agents::{id, now_ms},
    errors::GatewayError,
};

use super::schema::SessionRow;

#[derive(Debug)]
pub struct CreateRuntimeSession<'a> {
    pub runtime: &'a str,
    pub agent_id: &'a str,
    pub title: &'a str,
    pub timezone: Option<&'a str>,
    pub runtime_agent_ref_id: Option<&'a str>,
    pub environment: Value,
    pub provider_session_id: Option<&'a str>,
    pub provider_run_id: Option<&'a str>,
    pub owner_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
}

pub async fn create(
    pool: &PgPool,
    harness: &str,
    agent_id: Option<&str>,
    title: &str,
    timezone: Option<&str>,
    owner_id: Option<&str>,
    task_id: Option<&str>,
) -> Result<SessionRow, GatewayError> {
    let session_id = id("ses");
    sqlx::query_as::<_, SessionRow>(
        r#"
        INSERT INTO "LiteLLM_ManagedAgentSessionsTable"
          (id, harness, agent_id, title, created_at, tz, owner_id, task_id, attempt_number)
        VALUES (
          $1, $2, $3, $4, $5, $6, $7, $8,
          CASE
            WHEN $8::TEXT IS NULL THEN 1
            ELSE (SELECT current_attempt_number FROM "LiteLLM_ManagedAgentTasksTable" WHERE id = $8)
          END
        )
        RETURNING *
        "#,
    )
    .bind(session_id)
    .bind(harness)
    .bind(agent_id)
    .bind(title)
    .bind(now_ms())
    .bind(timezone)
    .bind(owner_id)
    .bind(task_id)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn create_runtime(
    pool: &PgPool,
    input: CreateRuntimeSession<'_>,
) -> Result<SessionRow, GatewayError> {
    let session_id = id("ses");
    sqlx::query_as::<_, SessionRow>(
        r#"
        INSERT INTO "LiteLLM_ManagedAgentSessionsTable" (
          id, harness, agent_id, title, created_at, tz, runtime,
          runtime_agent_ref_id, environment_json, provider_session_id,
          provider_run_id, status, owner_id, task_id, attempt_number
        )
        VALUES (
          $1, $2, $3, $4, $5, $6, $2, $7, $8, $9, $10, 'starting', $11, $12,
          CASE
            WHEN $12::TEXT IS NULL THEN 1
            ELSE (SELECT current_attempt_number FROM "LiteLLM_ManagedAgentTasksTable" WHERE id = $12)
          END
        )
        RETURNING *
        "#,
    )
    .bind(session_id)
    .bind(input.runtime)
    .bind(input.agent_id)
    .bind(input.title)
    .bind(now_ms())
    .bind(input.timezone)
    .bind(input.runtime_agent_ref_id)
    .bind(input.environment)
    .bind(input.provider_session_id)
    .bind(input.provider_run_id)
    .bind(input.owner_id)
    .bind(input.task_id)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn set_runtime_refs(
    pool: &PgPool,
    session_id: &str,
    runtime_agent_ref_id: &str,
    provider_session_id: Option<&str>,
    provider_run_id: Option<&str>,
    status: &str,
) -> Result<SessionRow, GatewayError> {
    sqlx::query_as::<_, SessionRow>(
        r#"
        UPDATE "LiteLLM_ManagedAgentSessionsTable"
        SET runtime_agent_ref_id = $2,
            provider_session_id = COALESCE($3, provider_session_id),
            provider_run_id = COALESCE($4, provider_run_id),
            status = CASE WHEN status IN ('cancelled', 'timed_out') THEN status ELSE $5 END,
            updated_at = $6
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(session_id)
    .bind(runtime_agent_ref_id)
    .bind(provider_session_id)
    .bind(provider_run_id)
    .bind(status)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn set_provider_run(
    pool: &PgPool,
    session_id: &str,
    provider_run_id: &str,
    status: &str,
) -> Result<(), GatewayError> {
    sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentSessionsTable"
        SET provider_run_id = $2,
            status = CASE WHEN status IN ('cancelled', 'timed_out') THEN status ELSE $3 END,
            updated_at = $4
        WHERE id = $1
        "#,
    )
    .bind(session_id)
    .bind(provider_run_id)
    .bind(status)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(())
}

const TERMINAL_STATUSES: &[&str] = &["cancelled", "timed_out", "completed", "failed", "error"];

pub async fn set_status(pool: &PgPool, session_id: &str, status: &str) -> Result<(), GatewayError> {
    sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentSessionsTable"
        SET status = $2,
            updated_at = $3
        WHERE id = $1
          AND (status NOT IN ('cancelled', 'timed_out') OR status = $2)
        "#,
    )
    .bind(session_id)
    .bind(status)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    // Session over — release its exposed app ports so the slots can be
    // reallocated; best-effort, the TTL sweeper covers any miss.
    if TERMINAL_STATUSES.contains(&status) {
        if let Err(error) =
            crate::db::managed_agents::exposed_apps::repository::soft_delete_for_session(
                pool, session_id,
            )
            .await
        {
            tracing::warn!(session_id, "failed to release exposed apps: {error}");
        }
    }
    Ok(())
}

pub async fn set_title(pool: &PgPool, session_id: &str, title: &str) -> Result<bool, GatewayError> {
    let result = sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentSessionsTable"
        SET title = $2,
            updated_at = $3
        WHERE id = $1
        "#,
    )
    .bind(session_id)
    .bind(title)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected() > 0)
}

/// `owner`: None lists everything (admin); Some(user) restricts to that
/// user's sessions. Legacy NULL-owner rows are only visible to admins.
pub async fn list(pool: &PgPool, owner: Option<&str>) -> Result<Vec<SessionRow>, GatewayError> {
    sqlx::query_as::<_, SessionRow>(
        r#"
        SELECT *
        FROM "LiteLLM_ManagedAgentSessionsTable"
        WHERE $1::TEXT IS NULL OR owner_id = $1
        ORDER BY COALESCE(updated_at, created_at) DESC
        "#,
    )
    .bind(owner)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn get(pool: &PgPool, session_id: &str) -> Result<Option<SessionRow>, GatewayError> {
    sqlx::query_as::<_, SessionRow>(
        r#"SELECT * FROM "LiteLLM_ManagedAgentSessionsTable" WHERE id = $1"#,
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn latest_for_task(
    pool: &PgPool,
    task_id: &str,
) -> Result<Option<SessionRow>, GatewayError> {
    sqlx::query_as::<_, SessionRow>(
        r#"
        SELECT *
        FROM "LiteLLM_ManagedAgentSessionsTable"
        WHERE task_id = $1
        ORDER BY attempt_number DESC, created_at DESC
        LIMIT 1
        "#,
    )
    .bind(task_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn list_for_task(pool: &PgPool, task_id: &str) -> Result<Vec<SessionRow>, GatewayError> {
    sqlx::query_as::<_, SessionRow>(
        r#"
        SELECT *
        FROM "LiteLLM_ManagedAgentSessionsTable"
        WHERE task_id = $1
        ORDER BY attempt_number DESC, created_at DESC
        "#,
    )
    .bind(task_id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

/// Looks up by the *provider-side* session id (e.g. opencode's own session
/// id), not LAP's own `id` — the two are distinct values assigned
/// independently at provisioning time. Used to resolve inbound callbacks from
/// a runtime wrapper, which only knows its own session id.
pub async fn get_by_provider_session_id(
    pool: &PgPool,
    provider_session_id: &str,
) -> Result<Option<SessionRow>, GatewayError> {
    sqlx::query_as::<_, SessionRow>(
        r#"SELECT * FROM "LiteLLM_ManagedAgentSessionsTable" WHERE provider_session_id = $1"#,
    )
    .bind(provider_session_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn delete(pool: &PgPool, session_id: &str) -> Result<bool, GatewayError> {
    let result = sqlx::query(r#"DELETE FROM "LiteLLM_ManagedAgentSessionsTable" WHERE id = $1"#)
        .bind(session_id)
        .execute(pool)
        .await
        .map_err(GatewayError::Database)?;
    Ok(result.rows_affected() > 0)
}

pub async fn set_workspace_bucket(
    pool: &PgPool,
    session_id: &str,
    bucket: &str,
) -> Result<(), GatewayError> {
    sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentSessionsTable"
        SET workspace_bucket = $2
        WHERE id = $1
        "#,
    )
    .bind(session_id)
    .bind(bucket)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(())
}

pub async fn touch(pool: &PgPool, session_id: &str) -> Result<(), GatewayError> {
    sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentSessionsTable"
        SET updated_at = $2
        WHERE id = $1
        "#,
    )
    .bind(session_id)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(())
}
