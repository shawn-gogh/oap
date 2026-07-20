use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Transaction};

use crate::{
    db::managed_agents::{id, now_ms},
    errors::GatewayError,
};

use super::schema::{
    SessionControlEventRow, SessionInvocationRow, SessionOperationRow, SessionTurnRow,
    TurnRecoveryCandidate, TurnSnapshot,
};

const ACTIVE_STATUSES: &[&str] = &[
    "queued",
    "running",
    "waiting_input",
    "waiting_approval",
    "cancelling",
];

const TERMINAL_STATUSES: &[&str] = &["completed", "failed", "rejected", "cancelled", "timed_out"];

#[derive(Debug)]
pub struct NewTurn<'a> {
    pub session_id: &'a str,
    pub request_id: &'a str,
    pub model: Option<&'a str>,
    pub input: &'a Value,
    pub input_schema: &'a Value,
    pub output_schema: &'a Value,
    pub interaction_profile: &'a Value,
    pub trigger_type: &'a str,
    pub retry_of_turn_id: Option<&'a str>,
    pub attempt_number: i32,
    pub agent_id: Option<&'a str>,
    pub runtime: Option<&'a str>,
    pub protocol: &'a str,
    pub protocol_version: &'a str,
    pub adapter_id: &'a str,
    pub traceparent: Option<&'a str>,
    pub tracestate: Option<&'a str>,
}

#[derive(Debug)]
pub struct CreatedTurn {
    pub snapshot: TurnSnapshot,
    pub created: bool,
}

pub struct NewControlEvent<'a> {
    pub session_id: &'a str,
    pub turn_id: Option<&'a str>,
    pub invocation_id: Option<&'a str>,
    pub request_id: Option<&'a str>,
    pub event_key: &'a str,
    pub event_type: &'a str,
    pub event: Value,
}

pub async fn create_or_get(pool: &PgPool, input: NewTurn<'_>) -> Result<CreatedTurn, GatewayError> {
    let mut tx = pool.begin().await.map_err(GatewayError::Database)?;
    let existing = turn_by_request(&mut tx, input.session_id, input.request_id).await?;
    if let Some(turn) = existing {
        let invocations = invocations_for_turn_tx(&mut tx, &turn.id).await?;
        tx.commit().await.map_err(GatewayError::Database)?;
        let created = CreatedTurn {
            snapshot: TurnSnapshot { turn, invocations },
            created: false,
        };
        crate::db::managed_agents::mcp_invocation_grants::repository::ensure_for_turn(
            pool,
            &created.snapshot,
        )
        .await?;
        return Ok(created);
    }

    let active = active_turn_tx(&mut tx, input.session_id).await?;
    if let Some(active) = active {
        return Err(GatewayError::BadRequest(format!(
            "session already has active turn {}",
            active.id
        )));
    }

    let now = now_ms();
    let turn = sqlx::query_as::<_, SessionTurnRow>(
        r#"
        INSERT INTO "LiteLLM_SessionTurnsTable"
          (id, session_id, request_id, status, model, input_json, input_schema_json,
           output_schema_json, interaction_profile_json, trigger_type,
           retry_of_turn_id, attempt_number, created_at, updated_at)
        VALUES ($1, $2, $3, 'queued', $4, $5, $6, $7, $8, $9, $10, $11, $12, $12)
        RETURNING *
        "#,
    )
    .bind(id("turn"))
    .bind(input.session_id)
    .bind(input.request_id)
    .bind(input.model)
    .bind(input.input)
    .bind(input.input_schema)
    .bind(input.output_schema)
    .bind(input.interaction_profile)
    .bind(input.trigger_type)
    .bind(input.retry_of_turn_id)
    .bind(input.attempt_number)
    .bind(now)
    .fetch_one(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;

    let mut invocation = sqlx::query_as::<_, SessionInvocationRow>(
        r#"
        INSERT INTO "LiteLLM_SessionInvocationsTable" (
          id, session_id, turn_id, agent_id, runtime, protocol, protocol_version, adapter_id,
          role, status, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'primary', 'queued', $9, $9)
        RETURNING *
        "#,
    )
    .bind(id("inv"))
    .bind(input.session_id)
    .bind(&turn.id)
    .bind(input.agent_id)
    .bind(input.runtime)
    .bind(input.protocol)
    .bind(input.protocol_version)
    .bind(input.adapter_id)
    .bind(now)
    .fetch_one(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;

    append_event_tx(
        &mut tx,
        NewControlEvent {
            session_id: input.session_id,
            turn_id: Some(&turn.id),
            invocation_id: Some(&invocation.id),
            request_id: Some(input.request_id),
            event_key: &format!("turn:{}:accepted", turn.id),
            event_type: "turn.accepted",
            event: json!({"status": "queued"}),
        },
    )
    .await?;
    tx.commit().await.map_err(GatewayError::Database)?;
    if let Err(error) = crate::managed_agents::adapters::telemetry::start_invocation(
        pool,
        &mut invocation,
        input.traceparent,
        input.tracestate,
    )
    .await
    {
        tracing::warn!(invocation_id = %invocation.id, %error, "failed to start invocation telemetry");
    }
    let created = CreatedTurn {
        snapshot: TurnSnapshot {
            turn,
            invocations: vec![invocation],
        },
        created: true,
    };
    crate::db::managed_agents::mcp_invocation_grants::repository::ensure_for_turn(
        pool,
        &created.snapshot,
    )
    .await?;
    Ok(created)
}

pub async fn transition(
    pool: &PgPool,
    turn_id: &str,
    status: &str,
    error: Option<Value>,
) -> Result<SessionTurnRow, GatewayError> {
    if !ACTIVE_STATUSES.contains(&status) && !TERMINAL_STATUSES.contains(&status) {
        return Err(GatewayError::BadRequest(format!(
            "invalid turn status {status}"
        )));
    }
    let mut tx = pool.begin().await.map_err(GatewayError::Database)?;
    let current = sqlx::query_as::<_, SessionTurnRow>(
        r#"SELECT * FROM "LiteLLM_SessionTurnsTable" WHERE id = $1 FOR UPDATE"#,
    )
    .bind(turn_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?
    .ok_or_else(|| GatewayError::NotFound(format!("turn {turn_id} not found")))?;
    if TERMINAL_STATUSES.contains(&current.status.as_str()) {
        if current.status == status {
            tx.commit().await.map_err(GatewayError::Database)?;
            return Ok(current);
        }
        return Err(GatewayError::BadRequest(format!(
            "turn {turn_id} is already terminal ({})",
            current.status
        )));
    }
    if current.status == status {
        tx.commit().await.map_err(GatewayError::Database)?;
        return Ok(current);
    }
    if !can_transition(&current.status, status) {
        return Err(GatewayError::BadRequest(format!(
            "turn {turn_id} cannot transition from {} to {status}",
            current.status
        )));
    }

    let now = now_ms();
    let terminal = TERMINAL_STATUSES.contains(&status);
    let row = sqlx::query_as::<_, SessionTurnRow>(
        r#"
        UPDATE "LiteLLM_SessionTurnsTable"
        SET status = $2,
            error_json = COALESCE($3, error_json),
            started_at = CASE WHEN $2 = 'running' THEN COALESCE(started_at, $4) ELSE started_at END,
            completed_at = CASE WHEN $5 THEN $4 ELSE completed_at END,
            updated_at = $4
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(turn_id)
    .bind(status)
    .bind(error)
    .bind(now)
    .bind(terminal)
    .fetch_one(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;

    sqlx::query(
        r#"
        UPDATE "LiteLLM_SessionInvocationsTable"
        SET status = $2,
            error_json = COALESCE($3, error_json),
            started_at = CASE WHEN $2 = 'running' THEN COALESCE(started_at, $4) ELSE started_at END,
            finished_at = CASE WHEN $5 THEN $4 ELSE finished_at END,
            updated_at = $4
        WHERE turn_id = $1 AND status NOT IN ('completed', 'failed', 'rejected', 'cancelled', 'timed_out')
        "#,
    )
    .bind(turn_id)
    .bind(status)
    .bind(row.error_json.clone())
    .bind(now)
    .bind(terminal)
    .execute(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;

    append_event_tx(
        &mut tx,
        NewControlEvent {
            session_id: &row.session_id,
            turn_id: Some(&row.id),
            invocation_id: None,
            request_id: Some(&row.request_id),
            event_key: &format!("turn:{}:{}:{status}", row.id, current.status),
            event_type: turn_event_type(status),
            event: json!({"status": status, "error": row.error_json}),
        },
    )
    .await?;
    tx.commit().await.map_err(GatewayError::Database)?;
    if terminal {
        if let Err(error) = crate::managed_agents::adapters::telemetry::finish_turn(
            pool,
            turn_id,
            status,
            row.error_json.as_ref(),
        )
        .await
        {
            tracing::warn!(turn_id, %error, "failed to finish invocation telemetry");
        }
        if let Err(error) =
            crate::db::managed_agents::credential_leases::repository::revoke_for_turn(pool, turn_id)
                .await
        {
            tracing::warn!(turn_id, %error, "failed to revoke turn credential leases");
        }
        if let Err(error) =
            crate::db::managed_agents::mcp_invocation_grants::repository::revoke_for_turn(
                pool, turn_id,
            )
            .await
        {
            tracing::warn!(turn_id, %error, "failed to revoke turn MCP invocation grants");
        }
    }
    Ok(row)
}

pub async fn active_turn(
    pool: &PgPool,
    session_id: &str,
) -> Result<Option<TurnSnapshot>, GatewayError> {
    let Some(turn) = sqlx::query_as::<_, SessionTurnRow>(
        r#"
        SELECT * FROM "LiteLLM_SessionTurnsTable"
        WHERE session_id = $1
          AND status IN ('queued', 'running', 'waiting_input', 'waiting_approval', 'cancelling')
        ORDER BY created_at DESC LIMIT 1
        "#,
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)?
    else {
        return Ok(None);
    };
    let invocations = invocations_for_turn(pool, &turn.id).await?;
    Ok(Some(TurnSnapshot { turn, invocations }))
}

pub async fn get_turn(
    pool: &PgPool,
    session_id: &str,
    turn_id: &str,
) -> Result<Option<TurnSnapshot>, GatewayError> {
    let Some(turn) = sqlx::query_as::<_, SessionTurnRow>(
        r#"SELECT * FROM "LiteLLM_SessionTurnsTable" WHERE id = $1 AND session_id = $2"#,
    )
    .bind(turn_id)
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)?
    else {
        return Ok(None);
    };
    let invocations = invocations_for_turn(pool, turn_id).await?;
    Ok(Some(TurnSnapshot { turn, invocations }))
}

pub async fn get_by_request(
    pool: &PgPool,
    session_id: &str,
    request_id: &str,
) -> Result<Option<TurnSnapshot>, GatewayError> {
    let Some(turn) = sqlx::query_as::<_, SessionTurnRow>(
        r#"SELECT * FROM "LiteLLM_SessionTurnsTable" WHERE session_id = $1 AND request_id = $2"#,
    )
    .bind(session_id)
    .bind(request_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)?
    else {
        return Ok(None);
    };
    let invocations = invocations_for_turn(pool, &turn.id).await?;
    Ok(Some(TurnSnapshot { turn, invocations }))
}

pub async fn list_turns(
    pool: &PgPool,
    session_id: &str,
) -> Result<Vec<SessionTurnRow>, GatewayError> {
    sqlx::query_as::<_, SessionTurnRow>(
        r#"SELECT * FROM "LiteLLM_SessionTurnsTable" WHERE session_id = $1 ORDER BY created_at"#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn recovery_candidates(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<TurnRecoveryCandidate>, GatewayError> {
    sqlx::query_as::<_, TurnRecoveryCandidate>(
        r#"
        SELECT turn.id AS turn_id,
               turn.session_id,
               turn.status AS turn_status,
               session.status AS session_status,
               session.runtime,
               turn.updated_at AS turn_updated_at,
               session.updated_at AS session_updated_at
        FROM "LiteLLM_SessionTurnsTable" turn
        JOIN "LiteLLM_ManagedAgentSessionsTable" session ON session.id = turn.session_id
        WHERE turn.status IN ('queued', 'running', 'waiting_input', 'waiting_approval', 'cancelling')
        ORDER BY turn.updated_at
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn list_events(
    pool: &PgPool,
    session_id: &str,
    after_sequence: i32,
) -> Result<Vec<SessionControlEventRow>, GatewayError> {
    sqlx::query_as::<_, SessionControlEventRow>(
        r#"
        SELECT * FROM "LiteLLM_SessionControlEventsTable"
        WHERE session_id = $1 AND seq > $2 ORDER BY seq
        "#,
    )
    .bind(session_id)
    .bind(after_sequence)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn operations_for_turn(
    pool: &PgPool,
    turn_id: &str,
) -> Result<Vec<SessionOperationRow>, GatewayError> {
    sqlx::query_as::<_, SessionOperationRow>(
        r#"
        SELECT * FROM "LiteLLM_SessionOperationsTable"
        WHERE turn_id = $1 ORDER BY created_at, id
        "#,
    )
    .bind(turn_id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn latest_event_sequence(pool: &PgPool, session_id: &str) -> Result<i32, GatewayError> {
    sqlx::query_scalar(
        r#"
        SELECT COALESCE(MAX(seq), 0)
        FROM "LiteLLM_SessionControlEventsTable" WHERE session_id = $1
        "#,
    )
    .bind(session_id)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn merge_turn_input(
    pool: &PgPool,
    session_id: &str,
    turn_id: &str,
    patch: &serde_json::Map<String, Value>,
) -> Result<SessionTurnRow, GatewayError> {
    sqlx::query_as::<_, SessionTurnRow>(
        r#"
        UPDATE "LiteLLM_SessionTurnsTable"
        SET input_json = input_json || $3::JSONB,
            updated_at = $4
        WHERE id = $1 AND session_id = $2
        RETURNING *
        "#,
    )
    .bind(turn_id)
    .bind(session_id)
    .bind(Value::Object(patch.clone()))
    .bind(now_ms())
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)?
    .ok_or_else(|| GatewayError::NotFound(format!("turn {turn_id} not found")))
}

pub async fn set_turn_result(
    pool: &PgPool,
    turn_id: &str,
    result: Value,
) -> Result<SessionTurnRow, GatewayError> {
    sqlx::query_as::<_, SessionTurnRow>(
        r#"
        UPDATE "LiteLLM_SessionTurnsTable"
        SET result_json = $2, updated_at = $3
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(turn_id)
    .bind(result)
    .bind(now_ms())
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)?
    .ok_or_else(|| GatewayError::NotFound(format!("turn {turn_id} not found")))
}

pub async fn append_event(
    pool: &PgPool,
    event: NewControlEvent<'_>,
) -> Result<SessionControlEventRow, GatewayError> {
    let mut tx = pool.begin().await.map_err(GatewayError::Database)?;
    let row = append_event_tx(&mut tx, event).await?;
    tx.commit().await.map_err(GatewayError::Database)?;
    Ok(row)
}

pub async fn bind_active_invocation(
    pool: &PgPool,
    session_id: &str,
    remote_session_id: Option<&str>,
    remote_context_id: Option<&str>,
    remote_task_id: Option<&str>,
    resume_cursor: Option<&str>,
) -> Result<(), GatewayError> {
    sqlx::query(
        r#"
        UPDATE "LiteLLM_SessionInvocationsTable" invocation
        SET remote_session_id = COALESCE($2, remote_session_id),
            remote_context_id = COALESCE($3, remote_context_id),
            remote_task_id = COALESCE($4, remote_task_id),
            resume_cursor = COALESCE($5, resume_cursor),
            updated_at = $6
        FROM "LiteLLM_SessionTurnsTable" turn
        WHERE invocation.turn_id = turn.id
          AND invocation.role = 'primary'
          AND turn.session_id = $1
          AND turn.status IN ('queued', 'running', 'waiting_input', 'waiting_approval', 'cancelling')
        "#,
    )
    .bind(session_id)
    .bind(remote_session_id)
    .bind(remote_context_id)
    .bind(remote_task_id)
    .bind(resume_cursor)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(())
}

async fn turn_by_request(
    tx: &mut Transaction<'_, Postgres>,
    session_id: &str,
    request_id: &str,
) -> Result<Option<SessionTurnRow>, GatewayError> {
    sqlx::query_as::<_, SessionTurnRow>(
        r#"SELECT * FROM "LiteLLM_SessionTurnsTable" WHERE session_id = $1 AND request_id = $2"#,
    )
    .bind(session_id)
    .bind(request_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(GatewayError::Database)
}

async fn active_turn_tx(
    tx: &mut Transaction<'_, Postgres>,
    session_id: &str,
) -> Result<Option<SessionTurnRow>, GatewayError> {
    sqlx::query_as::<_, SessionTurnRow>(
        r#"
        SELECT * FROM "LiteLLM_SessionTurnsTable"
        WHERE session_id = $1
          AND status IN ('queued', 'running', 'waiting_input', 'waiting_approval', 'cancelling')
        ORDER BY created_at DESC LIMIT 1 FOR UPDATE
        "#,
    )
    .bind(session_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(GatewayError::Database)
}

async fn invocations_for_turn(
    pool: &PgPool,
    turn_id: &str,
) -> Result<Vec<SessionInvocationRow>, GatewayError> {
    sqlx::query_as::<_, SessionInvocationRow>(
        r#"SELECT * FROM "LiteLLM_SessionInvocationsTable" WHERE turn_id = $1 ORDER BY created_at"#,
    )
    .bind(turn_id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

async fn invocations_for_turn_tx(
    tx: &mut Transaction<'_, Postgres>,
    turn_id: &str,
) -> Result<Vec<SessionInvocationRow>, GatewayError> {
    sqlx::query_as::<_, SessionInvocationRow>(
        r#"SELECT * FROM "LiteLLM_SessionInvocationsTable" WHERE turn_id = $1 ORDER BY created_at"#,
    )
    .bind(turn_id)
    .fetch_all(tx.as_mut())
    .await
    .map_err(GatewayError::Database)
}

async fn append_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    event: NewControlEvent<'_>,
) -> Result<SessionControlEventRow, GatewayError> {
    let _: Option<String> = sqlx::query_scalar(
        r#"
        SELECT id FROM "LiteLLM_ManagedAgentSessionsTable"
        WHERE id = $1 FOR UPDATE
        "#,
    )
    .bind(event.session_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;
    let next_seq: i32 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(MAX(seq), 0) + 1
        FROM "LiteLLM_SessionControlEventsTable" WHERE session_id = $1
        "#,
    )
    .bind(event.session_id)
    .fetch_one(tx.as_mut())
    .await
    .map_err(GatewayError::Database)?;
    sqlx::query_as::<_, SessionControlEventRow>(
        r#"
        INSERT INTO "LiteLLM_SessionControlEventsTable" (
          id, session_id, turn_id, invocation_id, request_id, seq,
          event_key, event_type, event_json, created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ON CONFLICT (session_id, event_key) DO UPDATE SET event_json = EXCLUDED.event_json
        RETURNING *
        "#,
    )
    .bind(id("scevt"))
    .bind(event.session_id)
    .bind(event.turn_id)
    .bind(event.invocation_id)
    .bind(event.request_id)
    .bind(next_seq)
    .bind(event.event_key)
    .bind(event.event_type)
    .bind(event.event)
    .bind(now_ms())
    .fetch_one(tx.as_mut())
    .await
    .map_err(GatewayError::Database)
}

fn turn_event_type(status: &str) -> &'static str {
    match status {
        "running" => "turn.started",
        "waiting_input" => "turn.waiting_input",
        "waiting_approval" => "turn.waiting_approval",
        "cancelling" => "turn.cancelling",
        "completed" => "turn.completed",
        "failed" => "turn.failed",
        "rejected" => "turn.rejected",
        "cancelled" => "turn.cancelled",
        "timed_out" => "turn.timed_out",
        _ => "turn.updated",
    }
}

fn can_transition(current: &str, next: &str) -> bool {
    if current == next {
        return true;
    }
    match current {
        "queued" => matches!(
            next,
            "running" | "cancelling" | "failed" | "cancelled" | "timed_out"
        ),
        "running" => matches!(
            next,
            "waiting_input"
                | "waiting_approval"
                | "cancelling"
                | "completed"
                | "failed"
                | "rejected"
                | "cancelled"
                | "timed_out"
        ),
        "waiting_input" | "waiting_approval" => matches!(
            next,
            "running" | "cancelling" | "failed" | "rejected" | "cancelled" | "timed_out"
        ),
        "cancelling" => matches!(next, "cancelled" | "failed" | "timed_out"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::can_transition;

    #[test]
    fn turn_state_machine_allows_expected_paths() {
        assert!(can_transition("queued", "running"));
        assert!(can_transition("running", "waiting_approval"));
        assert!(can_transition("waiting_approval", "running"));
        assert!(can_transition("running", "completed"));
        assert!(can_transition("running", "cancelling"));
        assert!(can_transition("cancelling", "cancelled"));
    }

    #[test]
    fn turn_state_machine_rejects_regressions() {
        assert!(!can_transition("running", "queued"));
        assert!(!can_transition("cancelling", "running"));
        assert!(!can_transition("completed", "running"));
    }
}
