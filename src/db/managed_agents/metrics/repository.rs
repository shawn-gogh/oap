use sqlx::PgPool;

use crate::errors::GatewayError;

use super::schema::AgentUsageDayRow;

pub async fn daily(
    pool: &PgPool,
    agent_id: &str,
    days: i32,
) -> Result<Vec<AgentUsageDayRow>, GatewayError> {
    sqlx::query_as::<_, AgentUsageDayRow>(
        r#"
        WITH days AS (
          SELECT timezone('UTC', now())::date - day_offset AS day
          FROM generate_series(0, $2::INT - 1) AS offsets(day_offset)
        ),
        spend AS (
          SELECT
            ("startTime" AT TIME ZONE 'UTC')::date AS day,
            COUNT(*)::BIGINT AS model_calls,
            COUNT(*) FILTER (WHERE status = 'error')::BIGINT AS error_calls,
            COALESCE(SUM(total_tokens), 0)::BIGINT AS total_tokens,
            COALESCE(SUM(spend), 0)::DOUBLE PRECISION AS estimated_cost_usd,
            COALESCE(SUM(request_duration_ms), 0)::BIGINT AS duration_ms_sum,
            COUNT(request_duration_ms)::BIGINT AS duration_samples
          FROM "LiteLLM_SpendLogs"
          WHERE agent_id = $1
            AND purpose = 'production'
            AND "startTime" >= (timezone('UTC', now())::date - ($2::INT - 1))
          GROUP BY 1
        ),
        invocation_usage AS (
          SELECT
            (to_timestamp(invocation.created_at / 1000.0) AT TIME ZONE 'UTC')::date AS day,
            COUNT(*)::BIGINT AS invocations,
            COUNT(*) FILTER (
              WHERE EXISTS (
                SELECT 1 FROM "LiteLLM_SpendLogs" log
                WHERE log.invocation_id = invocation.id
                  AND log.purpose = 'production'
              )
            )::BIGINT AS gateway_metered_invocations
          FROM "LiteLLM_SessionInvocationsTable" invocation
          JOIN "LiteLLM_ManagedAgentSessionsTable" session
            ON session.id = invocation.session_id
          WHERE session.agent_id = $1
            AND invocation.role = 'primary'
            AND invocation.created_at >= (
              EXTRACT(EPOCH FROM (
                (timezone('UTC', now())::date - ($2::INT - 1))::timestamp
                AT TIME ZONE 'UTC'
              )) * 1000
            )::BIGINT
          GROUP BY 1
        )
        SELECT
          to_char(days.day, 'YYYY-MM-DD') AS date,
          COALESCE(spend.model_calls, 0)::BIGINT AS model_calls,
          COALESCE(spend.error_calls, 0)::BIGINT AS error_calls,
          COALESCE(spend.total_tokens, 0)::BIGINT AS total_tokens,
          COALESCE(spend.estimated_cost_usd, 0)::DOUBLE PRECISION AS estimated_cost_usd,
          COALESCE(spend.duration_ms_sum, 0)::BIGINT AS duration_ms_sum,
          COALESCE(spend.duration_samples, 0)::BIGINT AS duration_samples,
          COALESCE(invocation_usage.invocations, 0)::BIGINT AS invocations,
          COALESCE(invocation_usage.gateway_metered_invocations, 0)::BIGINT
            AS gateway_metered_invocations,
          (
            COALESCE(invocation_usage.invocations, 0)
            - COALESCE(invocation_usage.gateway_metered_invocations, 0)
          )::BIGINT AS unmetered_invocations
        FROM days
        LEFT JOIN spend ON spend.day = days.day
        LEFT JOIN invocation_usage ON invocation_usage.day = days.day
        ORDER BY days.day
        "#,
    )
    .bind(agent_id)
    .bind(days)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}
