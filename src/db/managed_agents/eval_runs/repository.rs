//! Eval runs double as the experience pool: each row ties an agent revision
//! to measured outcomes, which is what makes single-variable iteration
//! attributable.

use serde_json::Value;
use sqlx::PgPool;

use crate::{
    db::managed_agents::{id, now_ms},
    errors::GatewayError,
};

use super::schema::EvalRunRow;

pub async fn insert_running(
    pool: &PgPool,
    agent_id: &str,
    agent_version: Option<i32>,
    model: &str,
    total: i32,
    created_by: Option<&str>,
) -> Result<EvalRunRow, GatewayError> {
    sqlx::query_as::<_, EvalRunRow>(
        r#"
        INSERT INTO "LiteLLM_AgentEvalRunsTable"
          (id, agent_id, agent_version, model, status, total, created_by, created_at)
        VALUES ($1, $2, $3, $4, 'running', $5, $6, $7)
        RETURNING *
        "#,
    )
    .bind(id("eval"))
    .bind(agent_id)
    .bind(agent_version)
    .bind(model)
    .bind(total)
    .bind(created_by)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn complete(
    pool: &PgPool,
    run_id: &str,
    passed: i32,
    results: &Value,
) -> Result<(), GatewayError> {
    sqlx::query(
        r#"
        UPDATE "LiteLLM_AgentEvalRunsTable"
        SET status = 'completed', passed = $2, results = $3, completed_at = $4
        WHERE id = $1
        "#,
    )
    .bind(run_id)
    .bind(passed)
    .bind(results)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(())
}

pub async fn fail(pool: &PgPool, run_id: &str, error: &str) -> Result<(), GatewayError> {
    sqlx::query(
        r#"
        UPDATE "LiteLLM_AgentEvalRunsTable"
        SET status = 'failed', error = $2, completed_at = $3
        WHERE id = $1
        "#,
    )
    .bind(run_id)
    .bind(error)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(())
}

pub async fn list(
    pool: &PgPool,
    agent_id: &str,
    limit: i64,
) -> Result<Vec<EvalRunRow>, GatewayError> {
    sqlx::query_as::<_, EvalRunRow>(
        r#"
        SELECT * FROM "LiteLLM_AgentEvalRunsTable"
        WHERE agent_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(agent_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}
