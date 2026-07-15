use serde_json::json;
use sqlx::PgPool;

use crate::{
    db::managed_agents::{id, messages, now_ms},
    errors::GatewayError,
};

use super::schema::{NewArtifact, TaskArtifactRow};

pub async fn create(
    pool: &PgPool,
    artifact: NewArtifact<'_>,
) -> Result<TaskArtifactRow, GatewayError> {
    sqlx::query_as::<_, TaskArtifactRow>(
        r#"
        INSERT INTO "LiteLLM_ManagedAgentTaskArtifactsTable" (
          id, task_id, session_id, run_id, artifact_type, name, content_json,
          location, dedupe_key, created_by, created_at, attempt_number
        )
        VALUES (
          $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
          COALESCE(
            (SELECT attempt_number FROM "LiteLLM_ManagedAgentSessionsTable" WHERE id = $3),
            (SELECT attempt_number FROM "LiteLLM_ManagedAgentRunsTable" WHERE id = $4),
            (SELECT current_attempt_number FROM "LiteLLM_ManagedAgentTasksTable" WHERE id = $2)
          )
        )
        ON CONFLICT (task_id, dedupe_key) DO UPDATE
        SET content_json = EXCLUDED.content_json,
            location = EXCLUDED.location,
            created_at = EXCLUDED.created_at
        RETURNING *
        "#,
    )
    .bind(id("artifact"))
    .bind(artifact.task_id)
    .bind(artifact.session_id)
    .bind(artifact.run_id)
    .bind(artifact.artifact_type)
    .bind(artifact.name)
    .bind(artifact.content)
    .bind(artifact.location)
    .bind(artifact.dedupe_key)
    .bind(artifact.created_by)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn list(pool: &PgPool, task_id: &str) -> Result<Vec<TaskArtifactRow>, GatewayError> {
    sqlx::query_as::<_, TaskArtifactRow>(
        r#"
        SELECT *
        FROM "LiteLLM_ManagedAgentTaskArtifactsTable"
        WHERE task_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(task_id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn count(pool: &PgPool, task_id: &str) -> Result<i64, GatewayError> {
    sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "LiteLLM_ManagedAgentTaskArtifactsTable" WHERE task_id = $1"#,
    )
    .bind(task_id)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn list_for_attempt(
    pool: &PgPool,
    task_id: &str,
    attempt_number: i32,
) -> Result<Vec<TaskArtifactRow>, GatewayError> {
    sqlx::query_as::<_, TaskArtifactRow>(
        r#"
        SELECT *
        FROM "LiteLLM_ManagedAgentTaskArtifactsTable"
        WHERE task_id = $1 AND attempt_number = $2
        ORDER BY created_at DESC
        "#,
    )
    .bind(task_id)
    .bind(attempt_number)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn count_for_attempt(
    pool: &PgPool,
    task_id: &str,
    attempt_number: i32,
) -> Result<i64, GatewayError> {
    sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM "LiteLLM_ManagedAgentTaskArtifactsTable"
        WHERE task_id = $1 AND attempt_number = $2
        "#,
    )
    .bind(task_id)
    .bind(attempt_number)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn capture_session_output(pool: &PgPool, session_id: &str) -> Result<(), GatewayError> {
    let Some(task_id) = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT task_id FROM "LiteLLM_ManagedAgentSessionsTable" WHERE id = $1"#,
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)?
    .flatten() else {
        return Ok(());
    };
    let content = latest_assistant_text(pool, session_id)
        .await?
        .map(|text| json!({ "text": text }))
        .unwrap_or_else(|| json!({ "session_id": session_id }));
    let location = format!("/chat/?id={session_id}");
    let dedupe_key = format!("session:{session_id}:output");
    create(
        pool,
        NewArtifact {
            task_id: &task_id,
            session_id: Some(session_id),
            run_id: None,
            artifact_type: "session_output",
            name: "Session output",
            content: Some(content),
            location: Some(&location),
            dedupe_key: Some(&dedupe_key),
            created_by: "system",
        },
    )
    .await?;
    super::acceptance::reconcile(pool, &task_id).await?;
    Ok(())
}

pub async fn capture_run_output(pool: &PgPool, run_id: &str) -> Result<(), GatewayError> {
    let Some((task_id, agent_id, logs)) = sqlx::query_as::<_, (Option<String>, String, String)>(
        r#"
        SELECT task_id, agent_id, logs
        FROM "LiteLLM_ManagedAgentRunsTable"
        WHERE id = $1
        "#,
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)?
    else {
        return Ok(());
    };
    let Some(task_id) = task_id else {
        return Ok(());
    };
    let location = format!("/api/agents/{agent_id}/runs/{run_id}/logs");
    let dedupe_key = format!("run:{run_id}:output");
    let output_text = logs
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| {
            event.get("type").and_then(serde_json::Value::as_str) == Some("message.part.delta")
        })
        .filter_map(|event| {
            event
                .pointer("/properties/delta")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .collect::<Vec<_>>()
        .join("");
    let content = if output_text.trim().is_empty() {
        json!({ "run_id": run_id })
    } else {
        json!({ "text": output_text })
    };
    create(
        pool,
        NewArtifact {
            task_id: &task_id,
            session_id: None,
            run_id: Some(run_id),
            artifact_type: "run_output",
            name: "Run output",
            content: Some(content),
            location: Some(&location),
            dedupe_key: Some(&dedupe_key),
            created_by: "system",
        },
    )
    .await?;
    super::acceptance::reconcile(pool, &task_id).await?;
    Ok(())
}

async fn latest_assistant_text(
    pool: &PgPool,
    session_id: &str,
) -> Result<Option<String>, GatewayError> {
    let rows = messages::repository::list(pool, session_id).await?;
    Ok(rows.iter().rev().find_map(|row| {
        let info: serde_json::Value = serde_json::from_str(&row.info_json).ok()?;
        if info.get("role").and_then(serde_json::Value::as_str) != Some("assistant") {
            return None;
        }
        let parts: serde_json::Value = serde_json::from_str(&row.parts_json).ok()?;
        let text = parts
            .as_array()?
            .iter()
            .filter_map(|part| part.get("text").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
        (!text.trim().is_empty()).then_some(text)
    }))
}
