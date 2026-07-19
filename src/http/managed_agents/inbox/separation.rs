use crate::{
    db::managed_agents::inbox::{repository, schema::InboxItemRow},
    errors::GatewayError,
    proxy::auth::master_key::AuthContext,
};

pub async fn blocked(
    pool: &sqlx::PgPool,
    auth: &AuthContext,
    item: &InboxItemRow,
) -> Result<bool, GatewayError> {
    if item.effect_handler != "agent_publish"
        || !crate::db::managed_agents::settings::repository::enforce_separation_of_duties(pool)
            .await?
    {
        return Ok(false);
    }
    repository::approval_scope_owned_by(pool, item, &auth.user_id).await
}

pub async fn assert_not_blocked(
    pool: &sqlx::PgPool,
    auth: &AuthContext,
    item: &InboxItemRow,
) -> Result<(), GatewayError> {
    if blocked(pool, auth, item).await? {
        return Err(GatewayError::BadRequest(
            "职责分离已启用：不能审批自己导入的智能体。请由其他审批者处理。".to_owned(),
        ));
    }
    Ok(())
}

pub async fn role_allows(
    pool: &sqlx::PgPool,
    auth: &AuthContext,
    item: &InboxItemRow,
    role: &str,
) -> Result<bool, GatewayError> {
    match role {
        "approver" | "admin" | "security" => Ok(auth.can_approve()),
        "operator" => Ok(auth.can_operate()),
        "group_admin" => {
            crate::db::managed_agents::groups::members::is_any_group_admin(pool, &auth.user_id)
                .await
        }
        _ => repository::approval_scope_owned_by(pool, item, &auth.user_id).await,
    }
}
