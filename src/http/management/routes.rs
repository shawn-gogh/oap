use std::sync::Arc;

use axum::{
    routing::{delete, get, patch},
    Router,
};

use crate::proxy::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/keys",
            get(super::api_keys::list).post(super::api_keys::create),
        )
        .route("/api/keys/{id}", delete(super::api_keys::delete))
        .route("/api/auth/me", get(super::users::me))
        .route(
            "/api/users",
            get(super::users::list).post(super::users::create),
        )
        .route("/api/users/{id}", patch(super::users::update))
        .route(
            "/api/groups",
            get(super::groups::list).post(super::groups::create),
        )
        .route("/api/groups/{group_id}", patch(super::groups::update))
        .route(
            "/api/groups/{group_id}/members",
            get(super::groups::list_members).post(super::groups::add_member),
        )
        .route(
            "/api/groups/{group_id}/members/{user_id}",
            delete(super::groups::delete_member),
        )
}
