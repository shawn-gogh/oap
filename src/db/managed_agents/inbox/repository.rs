use sqlx::PgPool;

use crate::{
    db::managed_agents::{id, now_ms},
    errors::GatewayError,
};

use super::schema::InboxItemRow;

/// `owner`: None lists everything (admin); Some(user) restricts to items
/// whose linked session or agent belongs to that user.
pub async fn list(
    pool: &PgPool,
    filter: &str,
    owner: Option<&str>,
) -> Result<Vec<InboxItemRow>, GatewayError> {
    sqlx::query_as::<_, InboxItemRow>(
        r#"
        SELECT i.*
        FROM "LiteLLM_ManagedAgentInboxItemsTable" i
        WHERE CASE $1
                WHEN 'attention' THEN i.status IN ('pending', 'open')
                WHEN 'completed' THEN i.status IN ('accepted', 'rejected', 'resolved')
                ELSE TRUE
              END
          AND ($2::TEXT IS NULL
               OR EXISTS (
                    SELECT 1 FROM "LiteLLM_ManagedAgentSessionsTable" s
                    WHERE s.id = i.session_id AND s.owner_id = $2
               )
               OR EXISTS (
                    SELECT 1 FROM "LiteLLM_ManagedAgentsTable" a
                    WHERE (a.id = i.agent OR a.name = i.agent) AND a.owner_id = $2
               ))
        ORDER BY i.created_at DESC
        "#,
    )
    .bind(filter)
    .bind(owner)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

/// Pending approvals, optionally scoped to one session and/or one owner.
/// `owner`: None returns everything (admin); Some(user) restricts to
/// approvals whose linked session or agent belongs to that user.
/// Approvals pointing at a deleted session are excluded — they can never be
/// meaningfully resumed.
pub async fn pending_approvals(
    pool: &PgPool,
    session_id: Option<&str>,
    owner: Option<&str>,
) -> Result<Vec<InboxItemRow>, GatewayError> {
    sqlx::query_as::<_, InboxItemRow>(
        r#"
        SELECT i.*
        FROM "LiteLLM_ManagedAgentInboxItemsTable" i
        WHERE i.kind IN ('approval', 'tool_permission', 'unlisted_data_egress') AND i.status = 'pending'
          AND ($1::TEXT IS NULL OR i.session_id = $1)
          AND (i.session_id IS NULL OR EXISTS (
                SELECT 1 FROM "LiteLLM_ManagedAgentSessionsTable" s
                WHERE s.id = i.session_id
          ))
          AND ($2::TEXT IS NULL
               OR EXISTS (
                    SELECT 1 FROM "LiteLLM_ManagedAgentSessionsTable" s
                    WHERE s.id = i.session_id AND s.owner_id = $2
               )
               OR EXISTS (
                    SELECT 1 FROM "LiteLLM_ManagedAgentsTable" a
                    WHERE (a.id = i.agent OR a.name = i.agent) AND a.owner_id = $2
               ))
        ORDER BY i.created_at ASC
        "#,
    )
    .bind(session_id)
    .bind(owner)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

/// Marks every pending approval linked to a session as expired. Called when
/// the session is deleted so the inbox doesn't accumulate approvals that can
/// never be decided into a live session.
pub async fn expire_pending_for_session(
    pool: &PgPool,
    session_id: &str,
) -> Result<u64, GatewayError> {
    let result = sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentInboxItemsTable"
        SET status = 'expired', resolved_at = $2
        WHERE kind IN ('approval', 'tool_permission', 'unlisted_data_egress') AND status = 'pending' AND session_id = $1
        "#,
    )
    .bind(session_id)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected())
}

/// True when the approval's linked session or agent belongs to `user`.
/// Approvals with neither linkage are admin-only and return false here.
pub async fn approval_scope_owned_by(
    pool: &PgPool,
    item: &InboxItemRow,
    user: &str,
) -> Result<bool, GatewayError> {
    let owned: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM "LiteLLM_ManagedAgentSessionsTable" s
            WHERE s.id = $1 AND s.owner_id = $3
        ) OR EXISTS (
            SELECT 1 FROM "LiteLLM_ManagedAgentsTable" a
            WHERE (a.id = $2 OR a.name = $2) AND a.owner_id = $3
        )
        "#,
    )
    .bind(item.session_id.as_deref())
    .bind(item.agent.as_deref())
    .bind(user)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(owned)
}

pub async fn get(pool: &PgPool, item_id: &str) -> Result<Option<InboxItemRow>, GatewayError> {
    sqlx::query_as::<_, InboxItemRow>(
        r#"
        SELECT *
        FROM "LiteLLM_ManagedAgentInboxItemsTable"
        WHERE id = $1
        "#,
    )
    .bind(item_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn create_approval(
    pool: &PgPool,
    kind: &str,
    title: String,
    session_id: Option<String>,
    agent: Option<String>,
    body: Option<String>,
    arguments: Option<serde_json::Value>,
) -> Result<InboxItemRow, GatewayError> {
    let args_json = arguments.map(|value| value.to_string());
    sqlx::query_as::<_, InboxItemRow>(
        r#"
        INSERT INTO "LiteLLM_ManagedAgentInboxItemsTable"
          (id, kind, title, session_id, agent, body, args_json, status, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending', $8)
        RETURNING *
        "#,
    )
    .bind(id("appr"))
    .bind(kind)
    .bind(title)
    .bind(session_id)
    .bind(agent)
    .bind(body)
    .bind(args_json)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn resolve_issue(
    pool: &PgPool,
    item_id: &str,
    note: Option<String>,
) -> Result<bool, GatewayError> {
    let result = sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentInboxItemsTable"
        SET status = 'resolved', feedback = COALESCE($2, feedback), resolved_at = $3
        WHERE id = $1 AND kind = 'issue' AND status = 'open'
        "#,
    )
    .bind(item_id)
    .bind(note)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;

    Ok(result.rows_affected() > 0)
}

pub async fn decide_approval(
    pool: &PgPool,
    item_id: &str,
    decision: &str,
    feedback: Option<String>,
    arguments: Option<serde_json::Value>,
) -> Result<bool, GatewayError> {
    let status = match decision {
        "accept" => "accepted",
        "reject" => "rejected",
        _ => {
            return Err(GatewayError::InvalidJsonMessage(
                "invalid decision".to_owned(),
            ))
        }
    };
    let args_json = arguments.map(|value| value.to_string());
    let result = sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentInboxItemsTable"
        SET status = $2,
            feedback = COALESCE($3, feedback),
            args_json = COALESCE($4, args_json),
            resolved_at = $5
        WHERE id = $1 AND kind IN ('approval', 'tool_permission', 'unlisted_data_egress') AND status = 'pending'
        "#,
    )
    .bind(item_id)
    .bind(status)
    .bind(feedback)
    .bind(args_json)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;

    Ok(result.rows_affected() > 0)
}
