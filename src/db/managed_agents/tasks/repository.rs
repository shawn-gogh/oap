use sqlx::PgPool;

use crate::{
    db::managed_agents::{id, now_ms},
    errors::GatewayError,
};

use super::schema::{AgentTaskRow, NewTask, TaskCancellation};

pub async fn create(pool: &PgPool, task: NewTask<'_>) -> Result<AgentTaskRow, GatewayError> {
    let mut tx = pool.begin().await.map_err(GatewayError::Database)?;
    let row = sqlx::query_as::<_, AgentTaskRow>(
        r#"
        INSERT INTO "LiteLLM_ManagedAgentTasksTable" (
          id, agent_id, application_version, source, source_id, title,
          input_json, status, created_by, created_at, deadline_at
        )
        VALUES (
          $1, $2, $3, $4, $5, $6, $7, 'queued', $8, $9,
          $9 + COALESCE(
            (SELECT GREATEST(max_runtime_minutes, 1)::BIGINT * 60000
             FROM "LiteLLM_ManagedAgentsTable" WHERE id = $2),
            1800000
          )
        )
        RETURNING *
        "#,
    )
    .bind(id("task"))
    .bind(task.agent_id)
    .bind(task.application_version)
    .bind(task.source)
    .bind(task.source_id)
    .bind(task.title)
    .bind(task.input)
    .bind(task.created_by)
    .bind(now_ms())
    .fetch_one(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;
    for (index, criterion) in task.completion_criteria.iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO "LiteLLM_ManagedAgentTaskAcceptanceChecksTable"
              (id, task_id, criterion_index, criterion)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(id("acceptance"))
        .bind(&row.id)
        .bind(index as i32)
        .bind(criterion)
        .execute(tx.as_mut())
        .await
        .map_err(GatewayError::Database)?;
    }
    tx.commit().await.map_err(GatewayError::Database)?;
    Ok(row)
}

pub async fn list(
    pool: &PgPool,
    agent_id: &str,
    limit: i64,
) -> Result<Vec<AgentTaskRow>, GatewayError> {
    sqlx::query_as::<_, AgentTaskRow>(
        r#"
        SELECT *
        FROM "LiteLLM_ManagedAgentTasksTable"
        WHERE agent_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(agent_id)
    .bind(limit.clamp(1, 100))
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn get(
    pool: &PgPool,
    agent_id: &str,
    task_id: &str,
) -> Result<Option<AgentTaskRow>, GatewayError> {
    sqlx::query_as::<_, AgentTaskRow>(
        r#"
        SELECT *
        FROM "LiteLLM_ManagedAgentTasksTable"
        WHERE agent_id = $1 AND id = $2
        "#,
    )
    .bind(agent_id)
    .bind(task_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn merge_input(
    pool: &PgPool,
    task_id: &str,
    input: serde_json::Value,
) -> Result<AgentTaskRow, GatewayError> {
    sqlx::query_as::<_, AgentTaskRow>(
        r#"
        UPDATE "LiteLLM_ManagedAgentTasksTable"
        SET input_json = input_json || $2
        WHERE id = $1 AND status = 'waiting_input'
        RETURNING *
        "#,
    )
    .bind(task_id)
    .bind(input)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)?
    .ok_or_else(|| {
        GatewayError::BadRequest(
            "task input can only be updated from waiting_input status".to_owned(),
        )
    })
}

pub async fn prepare_retry(
    pool: &PgPool,
    task_id: &str,
    max_attempts: i32,
) -> Result<AgentTaskRow, GatewayError> {
    let mut tx = pool.begin().await.map_err(GatewayError::Database)?;
    let task = sqlx::query_as::<_, AgentTaskRow>(
        r#"
        SELECT *
        FROM "LiteLLM_ManagedAgentTasksTable"
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(task_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?
    .ok_or_else(|| GatewayError::NotFound("task not found".to_owned()))?;
    if task.status != "failed" {
        return Err(GatewayError::BadRequest(format!(
            "task can only retry from failed status, current status is {}",
            task.status
        )));
    }
    if task.current_attempt_number >= max_attempts {
        return Err(GatewayError::BadRequest(format!(
            "task reached the maximum of {max_attempts} attempts"
        )));
    }
    let previous_attempt = task.current_attempt_number;
    let criteria = sqlx::query_as::<_, (i32, String)>(
        r#"
        SELECT criterion_index, criterion
        FROM "LiteLLM_ManagedAgentTaskAcceptanceChecksTable"
        WHERE task_id = $1 AND attempt_number = $2
        ORDER BY criterion_index
        "#,
    )
    .bind(task_id)
    .bind(previous_attempt)
    .fetch_all(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;
    let task = sqlx::query_as::<_, AgentTaskRow>(
        r#"
        UPDATE "LiteLLM_ManagedAgentTasksTable"
        SET status = 'queued',
            completed_at = NULL,
            failure_reason = NULL,
            failure_code = NULL,
            deadline_at = $2 + COALESCE(
              (SELECT GREATEST(max_runtime_minutes, 1)::BIGINT * 60000
               FROM "LiteLLM_ManagedAgentsTable" WHERE id = agent_id),
              1800000
            ),
            current_attempt_number = current_attempt_number + 1
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(task_id)
    .bind(now_ms())
    .fetch_one(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;
    for (criterion_index, criterion) in criteria {
        sqlx::query(
            r#"
            INSERT INTO "LiteLLM_ManagedAgentTaskAcceptanceChecksTable"
              (id, task_id, attempt_number, criterion_index, criterion)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(id("acceptance"))
        .bind(task_id)
        .bind(task.current_attempt_number)
        .bind(criterion_index)
        .bind(criterion)
        .execute(tx.as_mut())
        .await
        .map_err(GatewayError::Database)?;
    }
    tx.commit().await.map_err(GatewayError::Database)?;
    Ok(task)
}

pub async fn cancel(pool: &PgPool, task_id: &str) -> Result<TaskCancellation, GatewayError> {
    let mut tx = pool.begin().await.map_err(GatewayError::Database)?;
    let task = sqlx::query_as::<_, AgentTaskRow>(
        r#"
        SELECT *
        FROM "LiteLLM_ManagedAgentTasksTable"
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(task_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?
    .ok_or_else(|| GatewayError::NotFound("task not found".to_owned()))?;
    if matches!(task.status.as_str(), "succeeded" | "failed" | "cancelled") {
        return Err(GatewayError::BadRequest(format!(
            "task cannot be cancelled from {} status",
            task.status
        )));
    }
    let task = sqlx::query_as::<_, AgentTaskRow>(
        r#"
        UPDATE "LiteLLM_ManagedAgentTasksTable"
        SET status = 'cancelled', completed_at = $2, failure_reason = NULL, failure_code = NULL
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(task_id)
    .bind(now_ms())
    .fetch_one(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;
    let session_id = sqlx::query_scalar::<_, String>(
        r#"
        UPDATE "LiteLLM_ManagedAgentSessionsTable"
        SET status = 'cancelled', updated_at = $3
        WHERE task_id = $1 AND attempt_number = $2
        RETURNING id
        "#,
    )
    .bind(task_id)
    .bind(task.current_attempt_number)
    .bind(now_ms())
    .fetch_optional(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;
    let run_id = sqlx::query_scalar::<_, String>(
        r#"
        UPDATE "LiteLLM_ManagedAgentRunsTable"
        SET status = 'cancelled', finished_at = $3
        WHERE task_id = $1 AND attempt_number = $2
        RETURNING id
        "#,
    )
    .bind(task_id)
    .bind(task.current_attempt_number)
    .bind(now_ms())
    .fetch_optional(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;
    tx.commit().await.map_err(GatewayError::Database)?;
    Ok(TaskCancellation {
        task,
        session_id,
        run_id,
    })
}

pub async fn list_due_for_timeout(
    pool: &PgPool,
    now: i64,
    limit: i64,
) -> Result<Vec<String>, GatewayError> {
    sqlx::query_scalar(
        r#"
        SELECT id
        FROM "LiteLLM_ManagedAgentTasksTable"
        WHERE deadline_at IS NOT NULL
          AND deadline_at <= $1
          AND status IN ('draft', 'queued', 'running', 'waiting_input')
        ORDER BY deadline_at ASC
        LIMIT $2
        "#,
    )
    .bind(now)
    .bind(limit.clamp(1, 100))
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn timeout(
    pool: &PgPool,
    task_id: &str,
    now: i64,
) -> Result<Option<TaskCancellation>, GatewayError> {
    let mut tx = pool.begin().await.map_err(GatewayError::Database)?;
    let task = sqlx::query_as::<_, AgentTaskRow>(
        r#"
        SELECT *
        FROM "LiteLLM_ManagedAgentTasksTable"
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(task_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;
    let Some(task) = task else {
        return Ok(None);
    };
    if !matches!(
        task.status.as_str(),
        "draft" | "queued" | "running" | "waiting_input"
    ) || task.deadline_at.is_none_or(|deadline| deadline > now)
    {
        return Ok(None);
    }
    let task = sqlx::query_as::<_, AgentTaskRow>(
        r#"
        UPDATE "LiteLLM_ManagedAgentTasksTable"
        SET status = 'failed',
            completed_at = $2,
            failure_code = 'timeout',
            failure_reason = 'task exceeded its configured runtime limit'
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(task_id)
    .bind(now)
    .fetch_one(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;
    let session_id = sqlx::query_scalar::<_, String>(
        r#"
        UPDATE "LiteLLM_ManagedAgentSessionsTable"
        SET status = 'timed_out', updated_at = $3
        WHERE task_id = $1 AND attempt_number = $2
        RETURNING id
        "#,
    )
    .bind(task_id)
    .bind(task.current_attempt_number)
    .bind(now)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;
    let run_id = sqlx::query_scalar::<_, String>(
        r#"
        UPDATE "LiteLLM_ManagedAgentRunsTable"
        SET status = 'timed_out', finished_at = $3, error = 'task timed out'
        WHERE task_id = $1 AND attempt_number = $2
        RETURNING id
        "#,
    )
    .bind(task_id)
    .bind(task.current_attempt_number)
    .bind(now)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;
    tx.commit().await.map_err(GatewayError::Database)?;
    Ok(Some(TaskCancellation {
        task,
        session_id,
        run_id,
    }))
}

pub async fn mark_running(pool: &PgPool, task_id: &str) -> Result<(), GatewayError> {
    set_status(pool, task_id, "running", None, false).await
}

pub async fn mark_waiting_input(pool: &PgPool, task_id: &str) -> Result<(), GatewayError> {
    set_status(pool, task_id, "waiting_input", None, false).await
}

pub async fn mark_verifying_for_session(
    pool: &PgPool,
    session_id: &str,
) -> Result<(), GatewayError> {
    set_status_for_session(pool, session_id, "verifying", None, false).await
}

pub async fn mark_running_for_session(pool: &PgPool, session_id: &str) -> Result<(), GatewayError> {
    set_status_for_session(pool, session_id, "running", None, false).await
}

pub async fn fail(pool: &PgPool, task_id: &str, reason: &str) -> Result<(), GatewayError> {
    set_status(pool, task_id, "failed", Some(reason), true).await
}

pub async fn succeed(pool: &PgPool, task_id: &str) -> Result<(), GatewayError> {
    set_status(pool, task_id, "succeeded", None, true).await
}

pub async fn fail_for_session(
    pool: &PgPool,
    session_id: &str,
    reason: &str,
) -> Result<(), GatewayError> {
    set_status_for_session(pool, session_id, "failed", Some(reason), true).await
}

pub async fn cancel_for_session(pool: &PgPool, session_id: &str) -> Result<(), GatewayError> {
    set_status_for_session(pool, session_id, "cancelled", None, true).await
}

pub async fn mark_running_for_run(pool: &PgPool, run_id: &str) -> Result<(), GatewayError> {
    set_status_for_run(pool, run_id, "running", None, false).await
}

pub async fn mark_verifying_for_run(pool: &PgPool, run_id: &str) -> Result<(), GatewayError> {
    set_status_for_run(pool, run_id, "verifying", None, false).await
}

pub async fn fail_for_run(pool: &PgPool, run_id: &str, reason: &str) -> Result<(), GatewayError> {
    set_status_for_run(pool, run_id, "failed", Some(reason), true).await
}

async fn set_status(
    pool: &PgPool,
    task_id: &str,
    status: &str,
    failure_reason: Option<&str>,
    terminal: bool,
) -> Result<(), GatewayError> {
    sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentTasksTable"
        SET status = $2,
            started_at = CASE WHEN $2 = 'running' THEN COALESCE(started_at, $3) ELSE started_at END,
            completed_at = CASE WHEN $5 THEN $3 ELSE completed_at END,
            failure_reason = COALESCE($4, failure_reason)
        WHERE id = $1 AND status NOT IN ('succeeded', 'failed', 'cancelled')
        "#,
    )
    .bind(task_id)
    .bind(status)
    .bind(now_ms())
    .bind(failure_reason)
    .bind(terminal)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(())
}

async fn set_status_for_session(
    pool: &PgPool,
    session_id: &str,
    status: &str,
    failure_reason: Option<&str>,
    terminal: bool,
) -> Result<(), GatewayError> {
    sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentTasksTable"
        SET status = $2,
            started_at = CASE WHEN $2 = 'running' THEN COALESCE(started_at, $3) ELSE started_at END,
            completed_at = CASE WHEN $5 THEN $3 ELSE completed_at END,
            failure_reason = COALESCE($4, failure_reason),
            deadline_at = CASE WHEN $2 = 'verifying' THEN NULL ELSE deadline_at END
        WHERE status NOT IN ('succeeded', 'failed', 'cancelled') AND id = (
          SELECT task_id FROM "LiteLLM_ManagedAgentSessionsTable" WHERE id = $1
        )
        AND current_attempt_number = (
          SELECT attempt_number FROM "LiteLLM_ManagedAgentSessionsTable" WHERE id = $1
        )
        "#,
    )
    .bind(session_id)
    .bind(status)
    .bind(now_ms())
    .bind(failure_reason)
    .bind(terminal)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(())
}

async fn set_status_for_run(
    pool: &PgPool,
    run_id: &str,
    status: &str,
    failure_reason: Option<&str>,
    terminal: bool,
) -> Result<(), GatewayError> {
    sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentTasksTable"
        SET status = $2,
            started_at = CASE WHEN $2 = 'running' THEN COALESCE(started_at, $3) ELSE started_at END,
            completed_at = CASE WHEN $5 THEN $3 ELSE completed_at END,
            failure_reason = COALESCE($4, failure_reason),
            deadline_at = CASE WHEN $2 = 'verifying' THEN NULL ELSE deadline_at END
        WHERE status NOT IN ('succeeded', 'failed', 'cancelled') AND id = (
          SELECT task_id FROM "LiteLLM_ManagedAgentRunsTable" WHERE id = $1
        )
        AND current_attempt_number = (
          SELECT attempt_number FROM "LiteLLM_ManagedAgentRunsTable" WHERE id = $1
        )
        "#,
    )
    .bind(run_id)
    .bind(status)
    .bind(now_ms())
    .bind(failure_reason)
    .bind(terminal)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(())
}
