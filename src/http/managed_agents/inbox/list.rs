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
    let candidates = repository::list(pool, query.filter.as_deref().unwrap_or("all"), None).await?;
    let mut items = Vec::new();
    for item in candidates {
        let visible = if item.kind == "issue" {
            auth.is_admin || repository::approval_scope_owned_by(pool, &item, &auth.user_id).await?
        } else {
            super::approvals::can_decide(pool, &auth, &item).await?
        };
        if visible {
            items.push(item);
        }
    }
    Ok(Json(InboxResponse { items }))
}
