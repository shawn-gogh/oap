use sqlx::PgPool;

use crate::{db::managed_agents::now_ms, errors::GatewayError};

use super::{artifacts, repository, schema::TaskAcceptanceCheckRow};

pub async fn list(
    pool: &PgPool,
    task_id: &str,
) -> Result<Vec<TaskAcceptanceCheckRow>, GatewayError> {
    sqlx::query_as::<_, TaskAcceptanceCheckRow>(
        r#"
        SELECT *
        FROM "LiteLLM_ManagedAgentTaskAcceptanceChecksTable" checks
        WHERE task_id = $1 AND attempt_number = (
          SELECT current_attempt_number FROM "LiteLLM_ManagedAgentTasksTable" WHERE id = $1
        )
        ORDER BY criterion_index ASC
        "#,
    )
    .bind(task_id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn list_all(
    pool: &PgPool,
    task_id: &str,
) -> Result<Vec<TaskAcceptanceCheckRow>, GatewayError> {
    sqlx::query_as::<_, TaskAcceptanceCheckRow>(
        r#"
        SELECT *
        FROM "LiteLLM_ManagedAgentTaskAcceptanceChecksTable"
        WHERE task_id = $1
        ORDER BY attempt_number DESC, criterion_index ASC
        "#,
    )
    .bind(task_id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn record(
    pool: &PgPool,
    task_id: &str,
    criterion_index: i32,
    criterion: Option<&str>,
    verdict: &str,
    evidence: Option<&str>,
    checked_by: &str,
) -> Result<TaskAcceptanceCheckRow, GatewayError> {
    sqlx::query_as::<_, TaskAcceptanceCheckRow>(
        r#"
        INSERT INTO "LiteLLM_ManagedAgentTaskAcceptanceChecksTable" (
          id, task_id, attempt_number, criterion_index, criterion, verdict,
          evidence, checked_by, checked_at
        )
        SELECT $1, $2, current_attempt_number, $3, $4, $5, $6, $7, $8
        FROM "LiteLLM_ManagedAgentTasksTable"
        WHERE id = $2
        ON CONFLICT (task_id, attempt_number, criterion_index) DO UPDATE
        SET verdict = EXCLUDED.verdict,
            evidence = EXCLUDED.evidence,
            checked_by = EXCLUDED.checked_by,
            checked_at = EXCLUDED.checked_at
        RETURNING *
        "#,
    )
    .bind(crate::db::managed_agents::id("acceptance"))
    .bind(task_id)
    .bind(criterion_index)
    .bind(criterion.unwrap_or("Manual acceptance"))
    .bind(verdict)
    .bind(evidence)
    .bind(checked_by)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn reconcile(pool: &PgPool, task_id: &str) -> Result<(), GatewayError> {
    let attempt_number = sqlx::query_scalar::<_, i32>(
        r#"
        SELECT current_attempt_number
        FROM "LiteLLM_ManagedAgentTasksTable"
        WHERE id = $1
        "#,
    )
    .bind(task_id)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)?;
    let checks = list(pool, task_id).await?;
    if checks.iter().any(|check| check.verdict == "failed") {
        repository::fail(pool, task_id, "one or more acceptance checks failed").await?;
        return Ok(());
    }
    if !checks.is_empty()
        && checks.iter().all(|check| check.verdict == "passed")
        && artifacts::count_for_attempt(pool, task_id, attempt_number).await? > 0
    {
        repository::succeed(pool, task_id).await?;
    }
    Ok(())
}
