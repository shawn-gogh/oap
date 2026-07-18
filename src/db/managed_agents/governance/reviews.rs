use sqlx::{PgPool, Postgres, Transaction};

use crate::{db::managed_agents::now_ms, errors::GatewayError};

use super::AgentGovernanceRow;

pub async fn publish(
    pool: &PgPool,
    agent_id: &str,
    revision: i32,
) -> Result<AgentGovernanceRow, GatewayError> {
    let published_at = now_ms();
    let review_days =
        crate::db::managed_agents::settings::repository::review_period_days(pool).await?;
    let review_due_at = published_at + i64::from(review_days) * 86_400_000;
    sqlx::query_as::<_, AgentGovernanceRow>(
        r#"
        UPDATE "LiteLLM_ManagedAgentGovernanceTable"
        SET lifecycle_status = 'published', runtime_health = 'healthy',
            previous_published_revision = published_revision,
            published_revision = $2, publish_approval_id = NULL,
            published_at = $3, review_due_at = $4, updated_at = $3
        WHERE agent_id = $1 AND lifecycle_status = 'pending_approval'
        RETURNING *
        "#,
    )
    .bind(agent_id)
    .bind(revision)
    .bind(published_at)
    .bind(review_due_at)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)?
    .ok_or_else(|| {
        GatewayError::BadRequest(
            "该智能体当前不处于待审批状态，发布审批可能已过期或被撤销。".to_owned(),
        )
    })
}

pub async fn mark_due_for_review(
    pool: &PgPool,
    now: i64,
    limit: i64,
) -> Result<Vec<AgentGovernanceRow>, GatewayError> {
    let mut transaction = pool.begin().await.map_err(GatewayError::Database)?;
    let due = load_due(&mut transaction, now, limit).await?;
    let mut marked = Vec::with_capacity(due.len());
    for governance in due {
        if let Some(updated) = mark_one(&mut transaction, &governance.agent_id, now).await? {
            marked.push(updated);
        }
    }
    transaction.commit().await.map_err(GatewayError::Database)?;
    Ok(marked)
}

async fn load_due(
    transaction: &mut Transaction<'_, Postgres>,
    now: i64,
    limit: i64,
) -> Result<Vec<AgentGovernanceRow>, GatewayError> {
    sqlx::query_as::<_, AgentGovernanceRow>(
        r#"
        SELECT *
        FROM "LiteLLM_ManagedAgentGovernanceTable"
        WHERE lifecycle_status IN ('published', 'rolled_back') AND review_due_at <= $1
        ORDER BY review_due_at
        LIMIT $2
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .bind(now)
    .bind(limit)
    .fetch_all(transaction.as_mut())
    .await
    .map_err(GatewayError::Database)
}

async fn mark_one(
    transaction: &mut Transaction<'_, Postgres>,
    agent_id: &str,
    now: i64,
) -> Result<Option<AgentGovernanceRow>, GatewayError> {
    let updated = sqlx::query_as::<_, AgentGovernanceRow>(
        r#"
        UPDATE "LiteLLM_ManagedAgentGovernanceTable"
        SET lifecycle_status = 'review_due',
            health_detail = '发布有效期已到，请重新运行治理检查并申请复审。',
            publish_approval_id = NULL, updated_at = $2
        WHERE agent_id = $1 AND lifecycle_status IN ('published', 'rolled_back')
        RETURNING *
        "#,
    )
    .bind(agent_id)
    .bind(now)
    .fetch_optional(transaction.as_mut())
    .await
    .map_err(GatewayError::Database)?;
    if updated.is_some() {
        sqlx::query(
            r#"
            UPDATE "LiteLLM_ManagedAgentsTable"
            SET status = 'paused'
            WHERE id = $1 AND status = 'active'
            "#,
        )
        .bind(agent_id)
        .execute(transaction.as_mut())
        .await
        .map_err(GatewayError::Database)?;
    }
    Ok(updated)
}
