use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::{
    db::managed_agents::{id, now_ms, sessions},
    errors::GatewayError,
};

use super::schema::RuntimeEventRow;

pub async fn append(
    pool: &PgPool,
    session_id: &str,
    event: Value,
) -> Result<RuntimeEventRow, GatewayError> {
    let mut tx = pool.begin().await.map_err(GatewayError::Database)?;

    // Acquire a row lock on the session to serialize concurrent appends for this session ID
    let _: Option<String> = sqlx::query_scalar(
        r#"
        SELECT id
        FROM "LiteLLM_ManagedAgentSessionsTable"
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(session_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;

    let event_key = event_key(&event);
    let event_type = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let next_seq: i32 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(MAX(seq), 0) + 1
        FROM "LiteLLM_ManagedAgentRuntimeEventsTable"
        WHERE session_id = $1
        "#,
    )
    .bind(session_id)
    .fetch_one(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;

    let row = sqlx::query_as::<_, RuntimeEventRow>(
        r#"
        INSERT INTO "LiteLLM_ManagedAgentRuntimeEventsTable"
          (id, session_id, seq, event_key, event_type, event_json, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (session_id, event_key) DO UPDATE SET
          event_json = EXCLUDED.event_json
        RETURNING *
        "#,
    )
    .bind(id("rtevt"))
    .bind(session_id)
    .bind(next_seq)
    .bind(event_key)
    .bind(event_type)
    .bind(event)
    .bind(now_ms())
    .fetch_one(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;

    tx.commit().await.map_err(GatewayError::Database)?;
    sessions::repository::touch(pool, session_id).await?;
    Ok(row)
}

/// Persists many events in a single transaction, acquiring the session's row
/// lock once instead of once per event. `list`/replay endpoints call this
/// with the provider's full event history on every poll, so doing this as N
/// sequential single-row transactions (each with its own `FOR UPDATE`
/// round-trip) serialized against any concurrent writer and made every
/// events fetch scale with total history size — multi-second loads on
/// sessions with a few hundred events, repeating every poll interval.
pub async fn append_batch(
    pool: &PgPool,
    session_id: &str,
    events: Vec<Value>,
) -> Result<Vec<RuntimeEventRow>, GatewayError> {
    if events.is_empty() {
        return Ok(Vec::new());
    }

    let mut tx = pool.begin().await.map_err(GatewayError::Database)?;

    // Acquire a row lock on the session to serialize concurrent appends for this session ID
    let _: Option<String> = sqlx::query_scalar(
        r#"
        SELECT id
        FROM "LiteLLM_ManagedAgentSessionsTable"
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(session_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;

    let mut next_seq: i32 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(MAX(seq), 0) + 1
        FROM "LiteLLM_ManagedAgentRuntimeEventsTable"
        WHERE session_id = $1
        "#,
    )
    .bind(session_id)
    .fetch_one(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;

    // Candidate seq values are strictly greater than any existing row's seq
    // (computed once under the lock above), so assigning one to a row that
    // turns out to already exist (event_key conflict, which leaves the
    // existing seq untouched) never collides with a genuinely new row later
    // in the same batch.
    let mut rows = Vec::with_capacity(events.len());
    for event in events {
        let event_key = event_key(&event);
        let event_type = event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        let row = sqlx::query_as::<_, RuntimeEventRow>(
            r#"
            INSERT INTO "LiteLLM_ManagedAgentRuntimeEventsTable"
              (id, session_id, seq, event_key, event_type, event_json, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (session_id, event_key) DO UPDATE SET
              event_json = EXCLUDED.event_json
            RETURNING *
            "#,
        )
        .bind(id("rtevt"))
        .bind(session_id)
        .bind(next_seq)
        .bind(event_key)
        .bind(event_type)
        .bind(event)
        .bind(now_ms())
        .fetch_one(tx.as_mut())
        .await
        .map_err(GatewayError::Database)?;
        next_seq += 1;
        rows.push(row);
    }

    tx.commit().await.map_err(GatewayError::Database)?;
    sessions::repository::touch(pool, session_id).await?;
    Ok(rows)
}

pub async fn list(pool: &PgPool, session_id: &str) -> Result<Vec<Value>, GatewayError> {
    let rows = sqlx::query_as::<_, RuntimeEventRow>(
        r#"
        SELECT *
        FROM "LiteLLM_ManagedAgentRuntimeEventsTable"
        WHERE session_id = $1
        ORDER BY seq ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(rows.into_iter().map(|row| row.event_json).collect())
}

fn event_key(event: &Value) -> String {
    if let Some(id) = event.get("id").and_then(Value::as_str) {
        return format!("id:{id}");
    }
    let mut hash = Sha256::new();
    hash.update(event.to_string().as_bytes());
    format!("sha256:{:x}", hash.finalize())
}
