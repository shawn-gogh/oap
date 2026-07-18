use sqlx::PgPool;

use crate::{
    db::managed_agents::{id, now_ms},
    errors::GatewayError,
};

use super::schema::InboxItemRow;

struct ApprovalPolicy {
    enforcement_owner: &'static str,
    effect_handler: &'static str,
    required_role: &'static str,
    ttl_ms: i64,
    escalation_role: Option<&'static str>,
}

fn approval_policy(kind: &str) -> ApprovalPolicy {
    match kind {
        "tool_permission" | "runtime_permission" => ApprovalPolicy {
            enforcement_owner: "runtime",
            effect_handler: "runtime_permission",
            required_role: "owner",
            ttl_ms: 5 * 60 * 1000,
            escalation_role: None,
        },
        "unlisted_data_egress" | "data_egress" => ApprovalPolicy {
            enforcement_owner: "runtime",
            effect_handler: "runtime_permission",
            required_role: "approver",
            ttl_ms: 15 * 60 * 1000,
            escalation_role: None,
        },
        // A2A task paused in `input-required`/`auth-required`: the runtime
        // contract's approval-terminal-result guarantee for federated bridges
        // (see sessions::external_bridge::resolve_continuation) — accept
        // resumes the task with the user's reply, reject cancels it via
        // tasks/cancel. Short TTL: an interactive continuation stale for
        // longer than this is unlikely to still make sense to resume.
        "a2a_continuation" => ApprovalPolicy {
            enforcement_owner: "runtime",
            effect_handler: "a2a_continuation",
            required_role: "owner",
            ttl_ms: 15 * 60 * 1000,
            escalation_role: None,
        },
        "agent_publish" => ApprovalPolicy {
            enforcement_owner: "platform",
            effect_handler: "agent_publish",
            required_role: "approver",
            ttl_ms: 7 * 24 * 60 * 60 * 1000,
            escalation_role: None,
        },
        "agent_change" => ApprovalPolicy {
            enforcement_owner: "platform",
            effect_handler: "agent_change",
            required_role: "owner",
            ttl_ms: 7 * 24 * 60 * 60 * 1000,
            escalation_role: Some("admin"),
        },
        "platform_action" => ApprovalPolicy {
            enforcement_owner: "platform",
            effect_handler: "platform_action",
            required_role: "operator",
            ttl_ms: 24 * 60 * 60 * 1000,
            escalation_role: Some("admin"),
        },
        _ => ApprovalPolicy {
            enforcement_owner: "workflow",
            effect_handler: "resume_session",
            required_role: "owner",
            ttl_ms: 24 * 60 * 60 * 1000,
            escalation_role: Some("group_admin"),
        },
    }
}

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
                WHEN 'attention' THEN i.status IN ('pending', 'open') OR i.delivery_status = 'delivery_failed'
                WHEN 'completed' THEN i.status IN ('accepted', 'rejected', 'resolved', 'expired') AND i.delivery_status <> 'delivery_failed'
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
        WHERE i.kind IN ('approval', 'business_decision', 'tool_permission', 'runtime_permission', 'unlisted_data_egress', 'data_egress', 'agent_publish', 'agent_change', 'platform_action', 'a2a_continuation') AND i.status = 'pending'
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
        SET status = 'expired', resolved_at = $2, delivery_status = 'applied', applied_at = $2
        WHERE kind IN ('approval', 'business_decision', 'tool_permission', 'runtime_permission', 'unlisted_data_egress', 'data_egress', 'agent_publish', 'agent_change', 'platform_action', 'a2a_continuation') AND status = 'pending' AND session_id = $1
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
    let policy = approval_policy(kind);
    let created_at = now_ms();
    let args_json = arguments.map(|value| value.to_string());
    let item = sqlx::query_as::<_, InboxItemRow>(
        r#"
        INSERT INTO "LiteLLM_ManagedAgentInboxItemsTable"
          (id, kind, title, session_id, agent, body, args_json, status, created_at,
           enforcement_owner, effect_handler, required_role, delivery_status, expires_at,
           escalation_role, escalate_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending', $8, $9, $10, $11, 'pending', $12, $13, $14)
        RETURNING *
        "#,
    )
    .bind(id("appr"))
    .bind(kind)
    .bind(title)
    .bind(session_id.as_deref())
    .bind(agent)
    .bind(body)
    .bind(args_json)
    .bind(created_at)
    .bind(policy.enforcement_owner)
    .bind(policy.effect_handler)
    .bind(policy.required_role)
    .bind(created_at + policy.ttl_ms)
    .bind(policy.escalation_role)
    .bind(
        policy
            .escalation_role
            .map(|_| created_at + policy.ttl_ms / 2),
    )
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)?;

    let Some(session_id) = session_id.as_deref() else {
        return Ok(item);
    };
    let bound = sqlx::query_as::<_, InboxItemRow>(
        r#"
        WITH active AS (
          SELECT turn.id AS turn_id, turn.request_id,
                 invocation.id AS invocation_id
          FROM "LiteLLM_SessionTurnsTable" turn
          LEFT JOIN LATERAL (
            SELECT id FROM "LiteLLM_SessionInvocationsTable"
            WHERE turn_id = turn.id AND role = 'primary'
            ORDER BY created_at LIMIT 1
          ) invocation ON TRUE
          WHERE turn.session_id = $2
            AND turn.status IN ('queued', 'running', 'waiting_input', 'waiting_approval', 'cancelling')
          ORDER BY turn.created_at DESC LIMIT 1
        )
        UPDATE "LiteLLM_ManagedAgentInboxItemsTable" item
        SET turn_id = active.turn_id,
            invocation_id = active.invocation_id,
            request_id = active.request_id
        FROM active
        WHERE item.id = $1
        RETURNING item.*
        "#,
    )
    .bind(&item.id)
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)?;
    let Some(bound) = bound else {
        return Ok(item);
    };
    if let Some(turn_id) = bound.turn_id.as_deref() {
        crate::db::managed_agents::session_control::repository::transition(
            pool,
            turn_id,
            "waiting_approval",
            None,
        )
        .await?;
    }
    Ok(bound)
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
    actor: &str,
    decision_scope: &str,
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
            args_json = CASE
              WHEN effect_handler = 'runtime_permission' THEN args_json
              ELSE COALESCE($4, args_json)
            END,
            resolved_at = $5,
            decided_by = $6,
            decision_scope = $7,
            delivery_status = 'delivering',
            last_delivery_error = NULL
        WHERE id = $1 AND kind IN ('approval', 'business_decision', 'tool_permission', 'runtime_permission', 'unlisted_data_egress', 'data_egress', 'agent_publish', 'agent_change', 'platform_action', 'a2a_continuation') AND status = 'pending'
        "#,
    )
    .bind(item_id)
    .bind(status)
    .bind(feedback)
    .bind(args_json)
    .bind(now_ms())
    .bind(actor)
    .bind(decision_scope)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;

    Ok(result.rows_affected() > 0)
}

pub async fn mark_delivery_applied(pool: &PgPool, item_id: &str) -> Result<(), GatewayError> {
    sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentInboxItemsTable"
        SET delivery_status = 'applied', applied_at = $2,
            delivery_attempts = delivery_attempts + 1, last_delivery_error = NULL
        WHERE id = $1
        "#,
    )
    .bind(item_id)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(())
}

pub async fn mark_delivery_failed(
    pool: &PgPool,
    item_id: &str,
    error: &str,
) -> Result<(), GatewayError> {
    sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentInboxItemsTable"
        SET delivery_status = 'delivery_failed', delivery_attempts = delivery_attempts + 1,
            last_delivery_error = $2
        WHERE id = $1
        "#,
    )
    .bind(item_id)
    .bind(error)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(())
}

pub async fn list_due_for_expiry(
    pool: &PgPool,
    now: i64,
    limit: i64,
) -> Result<Vec<String>, GatewayError> {
    sqlx::query_scalar(
        r#"
        SELECT id FROM "LiteLLM_ManagedAgentInboxItemsTable"
        WHERE status = 'pending' AND expires_at IS NOT NULL AND expires_at <= $1
        ORDER BY expires_at
        LIMIT $2
        "#,
    )
    .bind(now)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn list_due_for_escalation(
    pool: &PgPool,
    now: i64,
    limit: i64,
) -> Result<Vec<String>, GatewayError> {
    sqlx::query_scalar(
        r#"
        SELECT id FROM "LiteLLM_ManagedAgentInboxItemsTable"
        WHERE status = 'pending' AND escalate_at IS NOT NULL AND escalate_at <= $1
          AND escalated_at IS NULL
        ORDER BY escalate_at
        LIMIT $2
        "#,
    )
    .bind(now)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn mark_escalated(pool: &PgPool, item_id: &str) -> Result<bool, GatewayError> {
    let result = sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentInboxItemsTable"
        SET escalated_at = $2
        WHERE id = $1 AND status = 'pending' AND escalated_at IS NULL
        "#,
    )
    .bind(item_id)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected() > 0)
}

pub async fn expire(pool: &PgPool, item_id: &str) -> Result<Option<InboxItemRow>, GatewayError> {
    sqlx::query_as::<_, InboxItemRow>(
        r#"
        UPDATE "LiteLLM_ManagedAgentInboxItemsTable"
        SET status = 'expired', feedback = COALESCE(feedback, 'approval expired'),
            resolved_at = $2, delivery_status = 'delivering'
        WHERE id = $1 AND status = 'pending'
        RETURNING *
        "#,
    )
    .bind(item_id)
    .bind(now_ms())
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

#[cfg(test)]
mod tests {
    use super::approval_policy;

    #[test]
    fn runtime_permissions_are_short_lived_and_runtime_enforced() {
        let policy = approval_policy("runtime_permission");
        assert_eq!(policy.enforcement_owner, "runtime");
        assert_eq!(policy.effect_handler, "runtime_permission");
        assert_eq!(policy.required_role, "owner");
        assert_eq!(policy.ttl_ms, 5 * 60 * 1000);
    }

    #[test]
    fn data_egress_requires_an_approver() {
        let policy = approval_policy("data_egress");
        assert_eq!(policy.enforcement_owner, "runtime");
        assert_eq!(policy.effect_handler, "runtime_permission");
        assert_eq!(policy.required_role, "approver");
    }

    #[test]
    fn typed_governance_approvals_use_platform_handlers() {
        let publish = approval_policy("agent_publish");
        assert_eq!(publish.effect_handler, "agent_publish");
        assert_eq!(publish.required_role, "approver");

        let change = approval_policy("agent_change");
        assert_eq!(change.effect_handler, "agent_change");
        assert_eq!(change.required_role, "owner");
        assert_eq!(change.escalation_role, Some("admin"));

        let business = approval_policy("business_decision");
        assert_eq!(business.escalation_role, Some("group_admin"));
    }
}
