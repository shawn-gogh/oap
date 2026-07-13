//! Agent usage grants: lets an owner share an agent with specific users
//! without giving up ownership. One row per (agent, grantee); re-granting
//! upgrades/downgrades the permission in place.

use sqlx::PgPool;

use crate::{
    db::managed_agents::{id, now_ms},
    errors::GatewayError,
};

use super::schema::AgentGrantRow;

pub async fn upsert(
    pool: &PgPool,
    agent_id: &str,
    grantee_user_id: &str,
    permission: &str,
    granted_by: &str,
) -> Result<AgentGrantRow, GatewayError> {
    let permission = match permission {
        "edit" => "edit",
        _ => "use",
    };
    sqlx::query_as::<_, AgentGrantRow>(
        r#"
        INSERT INTO "LiteLLM_AgentGrantsTable"
          (id, agent_id, grantee_user_id, permission, granted_by, created_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (agent_id, grantee_user_id)
          DO UPDATE SET permission = EXCLUDED.permission, granted_by = EXCLUDED.granted_by
        RETURNING *
        "#,
    )
    .bind(id("grant"))
    .bind(agent_id)
    .bind(grantee_user_id)
    .bind(permission)
    .bind(granted_by)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn find(
    pool: &PgPool,
    agent_id: &str,
    grantee_user_id: &str,
) -> Result<Option<AgentGrantRow>, GatewayError> {
    sqlx::query_as::<_, AgentGrantRow>(
        r#"
        SELECT * FROM "LiteLLM_AgentGrantsTable"
        WHERE agent_id = $1 AND grantee_user_id = $2
        "#,
    )
    .bind(agent_id)
    .bind(grantee_user_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn list_for_agent(
    pool: &PgPool,
    agent_id: &str,
) -> Result<Vec<AgentGrantRow>, GatewayError> {
    sqlx::query_as::<_, AgentGrantRow>(
        r#"
        SELECT * FROM "LiteLLM_AgentGrantsTable"
        WHERE agent_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(agent_id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

/// Agent ids shared with this user (any permission).
pub async fn agent_ids_for_user(
    pool: &PgPool,
    grantee_user_id: &str,
) -> Result<Vec<String>, GatewayError> {
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT agent_id FROM "LiteLLM_AgentGrantsTable"
        WHERE grantee_user_id = $1
        "#,
    )
    .bind(grantee_user_id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn delete(
    pool: &PgPool,
    agent_id: &str,
    grantee_user_id: &str,
) -> Result<bool, GatewayError> {
    let result = sqlx::query(
        r#"
        DELETE FROM "LiteLLM_AgentGrantsTable"
        WHERE agent_id = $1 AND grantee_user_id = $2
        "#,
    )
    .bind(agent_id)
    .bind(grantee_user_id)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_all_for_agent(pool: &PgPool, agent_id: &str) -> Result<(), GatewayError> {
    sqlx::query(r#"DELETE FROM "LiteLLM_AgentGrantsTable" WHERE agent_id = $1"#)
        .bind(agent_id)
        .execute(pool)
        .await
        .map_err(GatewayError::Database)?;
    Ok(())
}
