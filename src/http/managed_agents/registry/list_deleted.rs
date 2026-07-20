use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    Json,
};

use crate::{
    db::managed_agents::registry::repository,
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

use super::types::{AgentsResponse, ListAgentsQuery};

pub async fn list_deleted(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ListAgentsQuery>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;

    let rows = repository::list_deleted(pool, query.owner_id.as_deref()).await?;
    // Same visibility rule as the live agent list: operators see everything,
    // everyone else only their own or explicitly-granted agents.
    let rows = if auth.can_operate() {
        rows
    } else {
        let granted = crate::db::managed_agents::agent_grants::repository::agent_ids_for_user(
            pool,
            &auth.user_id,
        )
        .await?;
        let group_granted = crate::db::managed_agents::groups::agent_grants::agent_ids_for_user(
            pool,
            &auth.user_id,
        )
        .await?;
        rows.into_iter()
            .filter(|row| match row.owner_id.as_deref() {
                None => true,
                Some(owner) => {
                    owner == auth.user_id
                        || granted.iter().any(|id| id == &row.id)
                        || group_granted.iter().any(|id| id == &row.id)
                }
            })
            .collect()
    };

    let agents = rows
        .into_iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(serde_json::json!(AgentsResponse { agents })))
}
