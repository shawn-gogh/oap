use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    Json,
};

use crate::{
    db::managed_agents::registry::repository,
    errors::GatewayError,
    http::agents::configured_agent_values,
    proxy::{auth::master_key::authenticate, state::AppState},
};

use super::types::{AgentsResponse, ListAgentsQuery};

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ListAgentsQuery>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;

    let mut agents = configured_agent_values(&state);
    if let Some(pool) = state.db.as_ref() {
        let rows = repository::list(pool, query.owner_id.as_deref()).await?;
        // Isolation: non-admins see their own agents, agents shared with
        // them via grants, and legacy ownerless agents.
        let rows = if auth.is_admin {
            rows
        } else {
            let granted = crate::db::managed_agents::agent_grants::repository::agent_ids_for_user(
                pool,
                &auth.user_id,
            )
            .await?;
            let group_granted =
                crate::db::managed_agents::groups::agent_grants::agent_ids_for_user(
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
        agents.extend(
            rows.into_iter()
                .map(serde_json::to_value)
                .collect::<Result<Vec<_>, _>>()?,
        );
    } else if agents.is_empty() {
        return Err(GatewayError::MissingDatabase);
    }

    Ok(Json(serde_json::json!(AgentsResponse { agents })))
}
