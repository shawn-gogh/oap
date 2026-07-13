use sqlx::PgPool;

use crate::{db::managed_agents::now_ms, errors::GatewayError};

use super::schema::GroupMemberRow;

pub async fn list(pool: &PgPool, group_id: &str) -> Result<Vec<GroupMemberRow>, GatewayError> {
    sqlx::query_as::<_, GroupMemberRow>(
        r#"SELECT * FROM "LiteLLM_GroupMembersTable" WHERE group_id = $1 ORDER BY created_at, user_id"#,
    )
    .bind(group_id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn upsert(
    pool: &PgPool,
    group_id: &str,
    user_id: &str,
    member_role: &str,
    added_by: &str,
) -> Result<GroupMemberRow, GatewayError> {
    let member_role = match member_role {
        "group_admin" => "group_admin",
        _ => "member",
    };
    sqlx::query_as::<_, GroupMemberRow>(
        r#"
        INSERT INTO "LiteLLM_GroupMembersTable" (group_id, user_id, member_role, added_by, created_at)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (group_id, user_id)
          DO UPDATE SET member_role = EXCLUDED.member_role, added_by = EXCLUDED.added_by
        RETURNING *
        "#,
    )
    .bind(group_id)
    .bind(user_id)
    .bind(member_role)
    .bind(added_by)
    .bind(now_ms())
    .fetch_one(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn delete(pool: &PgPool, group_id: &str, user_id: &str) -> Result<bool, GatewayError> {
    let result = sqlx::query(
        r#"DELETE FROM "LiteLLM_GroupMembersTable" WHERE group_id = $1 AND user_id = $2"#,
    )
    .bind(group_id)
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(GatewayError::Database)?;
    Ok(result.rows_affected() > 0)
}
