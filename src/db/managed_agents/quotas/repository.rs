use sqlx::PgPool;

use crate::{db::managed_agents::now_ms, errors::GatewayError};

pub async fn current_month_cost(pool: &PgPool, agent_id: &str) -> Result<f64, GatewayError> {
    sqlx::query_scalar::<_, f64>(
        r#"
        SELECT COALESCE(SUM(spend), 0)::DOUBLE PRECISION
        FROM "LiteLLM_SpendLogs"
        WHERE agent_id = $1
          AND purpose = 'production'
          AND "startTime" >= date_trunc('month', timezone('UTC', now()))
        "#,
    )
    .bind(agent_id)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn active_sessions(pool: &PgPool, agent_id: &str) -> Result<i64, GatewayError> {
    sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT
        FROM "LiteLLM_ManagedAgentSessionsTable"
        WHERE agent_id = $1
          AND COALESCE(status, 'idle')
              NOT IN ('idle', 'cancelled', 'timed_out', 'completed', 'failed', 'error')
        "#,
    )
    .bind(agent_id)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn rate_count(pool: &PgPool, agent_id: &str) -> Result<i64, GatewayError> {
    let bucket_start = minute_start(now_ms());
    sqlx::query_scalar::<_, Option<i64>>(
        r#"
        SELECT request_count
        FROM "LiteLLM_AgentRateLimitBucketsTable"
        WHERE agent_id = $1 AND bucket_start = $2
        "#,
    )
    .bind(agent_id)
    .bind(bucket_start)
    .fetch_optional(pool)
    .await
    .map(|value| value.flatten().unwrap_or_default())
    .map_err(GatewayError::Database)
}

pub async fn consume_rate(
    pool: &PgPool,
    agent_id: &str,
    limit: i64,
) -> Result<Option<i64>, GatewayError> {
    let bucket_start = minute_start(now_ms());
    sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO "LiteLLM_AgentRateLimitBucketsTable"
          (agent_id, bucket_start, request_count, updated_at)
        VALUES ($1, $2, 1, $3)
        ON CONFLICT (agent_id, bucket_start) DO UPDATE SET
          request_count = "LiteLLM_AgentRateLimitBucketsTable".request_count + 1,
          updated_at = EXCLUDED.updated_at
        WHERE "LiteLLM_AgentRateLimitBucketsTable".request_count < $4
        RETURNING request_count
        "#,
    )
    .bind(agent_id)
    .bind(bucket_start)
    .bind(now_ms())
    .bind(limit)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub fn minute_reset_at(now: i64) -> i64 {
    minute_start(now).saturating_add(60_000)
}

fn minute_start(now: i64) -> i64 {
    now - now.rem_euclid(60_000)
}
