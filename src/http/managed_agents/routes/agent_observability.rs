use std::sync::Arc;

use axum::{routing::get, Router};

use crate::proxy::state::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/agents/{agent_id}/revisions",
            get(super::super::registry::revisions::list),
        )
        .route(
            "/api/agents/{agent_id}/metrics",
            get(super::super::metrics::get),
        )
        .route(
            "/api/agents/{agent_id}/revisions/{from_version}/diff/{to_version}",
            get(super::super::registry::revisions::diff),
        )
        .route(
            "/api/agents/{agent_id}/audit",
            get(super::super::audit_timeline::get),
        )
}
