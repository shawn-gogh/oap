use sqlx::PgPool;

use crate::{db::managed_agents::now_ms, errors::GatewayError};

use super::schema::MattermostThreadSessionRow;

pub async fn get(
    pool: &PgPool,
    agent_id: &str,
    channel_id: &str,
    root_id: &str,
) -> Result<Option<MattermostThreadSessionRow>, GatewayError> {
    sqlx::query_as::<_, MattermostThreadSessionRow>(
        r#"
        SELECT *
        FROM "LiteLLM_ManagedAgentMattermostThreadSessionsTable"
        WHERE agent_id = $1 AND channel_id = $2 AND root_id = $3
        "#,
    )
    .bind(agent_id)
    .bind(channel_id)
    .bind(root_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

/// Binds a thread to a session that a caller already created via the normal
/// runtime-aware session path (`create_runtime_session_for_agent`) — this
/// does not create sessions itself, it only records the binding. `ON
/// CONFLICT DO UPDATE` makes it safe if two webhook deliveries for the same
/// brand-new thread race: both create a session, both try to bind, and
/// whichever upsert commits last "wins" the mapping — the loser's session
/// row is simply never referenced again (an orphaned pre-created session,
/// same tradeoff Slack's integration made).
pub async fn upsert(
    pool: &PgPool,
    agent_id: &str,
    channel_id: &str,
    root_id: &str,
    session_id: &str,
) -> Result<MattermostThreadSessionRow, GatewayError> {
    let now = now_ms();
    sqlx::query_as::<_, MattermostThreadSessionRow>(
        r#"
        INSERT INTO "LiteLLM_ManagedAgentMattermostThreadSessionsTable"
          (agent_id, channel_id, root_id, session_id, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $5)
        ON CONFLICT (agent_id, channel_id, root_id) DO UPDATE SET
          session_id = EXCLUDED.session_id,
          updated_at = EXCLUDED.updated_at
        RETURNING *
        "#,
    )
    .bind(agent_id)
    .bind(channel_id)
    .bind(root_id)
    .bind(session_id)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn record_event(
    pool: &PgPool,
    agent_id: &str,
    event_id: &str,
) -> Result<bool, GatewayError> {
    let result = sqlx::query(
        r#"
        INSERT INTO "LiteLLM_ManagedAgentMattermostEventsTable" (agent_id, event_id, created_at)
        VALUES ($1, $2, $3)
        ON CONFLICT (agent_id, event_id) DO NOTHING
        "#,
    )
    .bind(agent_id)
    .bind(event_id)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected() == 1)
}
