use sqlx::PgPool;

use crate::{
    db::managed_agents::{id, now_ms},
    errors::GatewayError,
};

use super::schema::AgentGroupGrantRow;

pub async fn upsert(
    pool: &PgPool,
    agent_id: &str,
    group_id: &str,
    permission: &str,
    granted_by: &str,
) -> Result<AgentGroupGrantRow, GatewayError> {
    let permission = if permission == "edit" { "edit" } else { "use" };
    sqlx::query_as::<_, AgentGroupGrantRow>(
        r#"
        INSERT INTO "LiteLLM_AgentGroupGrantsTable"
          (id, agent_id, group_id, permission, granted_by, created_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (agent_id, group_id)
          DO UPDATE SET permission = EXCLUDED.permission, granted_by = EXCLUDED.granted_by
        RETURNING *
        "#,
    )
    .bind(id("group_grant"))
    .bind(agent_id)
    .bind(group_id)
    .bind(permission)
    .bind(granted_by)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn list_for_agent(
    pool: &PgPool,
    agent_id: &str,
) -> Result<Vec<AgentGroupGrantRow>, GatewayError> {
    sqlx::query_as::<_, AgentGroupGrantRow>(
        r#"SELECT * FROM "LiteLLM_AgentGroupGrantsTable" WHERE agent_id = $1 ORDER BY created_at"#,
    )
    .bind(agent_id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn delete(pool: &PgPool, agent_id: &str, group_id: &str) -> Result<bool, GatewayError> {
    let result = sqlx::query(
        r#"DELETE FROM "LiteLLM_AgentGroupGrantsTable" WHERE agent_id = $1 AND group_id = $2"#,
    )
    .bind(agent_id)
    .bind(group_id)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_all_for_agent(pool: &PgPool, agent_id: &str) -> Result<(), GatewayError> {
    sqlx::query(r#"DELETE FROM "LiteLLM_AgentGroupGrantsTable" WHERE agent_id = $1"#)
        .bind(agent_id)
        .execute(pool)
        .await
        .map_err(GatewayError::Database)?;
    Ok(())
}

pub async fn has_permission(
    pool: &PgPool,
    agent_id: &str,
    user_id: &str,
    permission: Option<&str>,
) -> Result<bool, GatewayError> {
    let permission = permission.unwrap_or("use");
    sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
          SELECT 1
          FROM "LiteLLM_AgentGroupGrantsTable" grant
          JOIN "LiteLLM_GroupMembersTable" member ON member.group_id = grant.group_id
          JOIN "LiteLLM_GroupsTable" groups ON groups.id = grant.group_id
          WHERE grant.agent_id = $1 AND member.user_id = $2 AND groups.status = 'active'
            AND ($3 = 'use' OR grant.permission = 'edit')
        )
        "#,
    )
    .bind(agent_id)
    .bind(user_id)
    .bind(permission)
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn agent_ids_for_user(pool: &PgPool, user_id: &str) -> Result<Vec<String>, GatewayError> {
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT DISTINCT grant.agent_id
        FROM "LiteLLM_AgentGroupGrantsTable" grant
        JOIN "LiteLLM_GroupMembersTable" member ON member.group_id = grant.group_id
        JOIN "LiteLLM_GroupsTable" groups ON groups.id = grant.group_id
        WHERE member.user_id = $1 AND groups.status = 'active'
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}
