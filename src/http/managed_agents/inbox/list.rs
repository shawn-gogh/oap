use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    Json,
};

use crate::{
    db::managed_agents::inbox::repository,
    errors::GatewayError,
    proxy::{auth::master_key::authenticate, state::AppState},
};

use super::types::{InboxResponse, ListInboxQuery};

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ListInboxQuery>,
) -> Result<Json<InboxResponse>, GatewayError> {
    let auth = authenticate(&headers, &state).await?;
    let pool = super::super::db(&state, &headers).await?;
    let owner = (!auth.is_admin).then_some(auth.user_id.as_str());
    Ok(Json(InboxResponse {
        items: repository::list(pool, query.filter.as_deref().unwrap_or("all"), owner).await?,
    }))
}
