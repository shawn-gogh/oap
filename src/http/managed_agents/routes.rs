use std::sync::Arc;

use axum::{
    routing::{delete, get, post},
    Router,
};

use crate::{
    channels::{google_chat, webhook},
    proxy::state::AppState,
};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .merge(agent_routes())
        .merge(import_routes())
        .merge(rule_routes())
        .merge(routine_routes())
        .merge(skill_routes())
        .merge(inbox_routes())
        .merge(slack_routes())
        .merge(teams_routes())
        .merge(google_chat_routes())
        .merge(webhook_routes())
}

fn agent_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/agents",
            post(super::registry::create::create).get(super::registry::list::list),
        )
        .route(
            "/api/agents/{agent_id}",
            get(super::registry::get::get)
                .patch(super::registry::update::update)
                .delete(super::registry::delete::delete),
        )
        .route(
            "/api/agents/{agent_id}/pause",
            post(super::registry::pause::pause),
        )
        .route(
            "/api/agents/{agent_id}/resume",
            post(super::registry::resume::resume),
        )
        .route(
            "/api/agents/{agent_id}/preflight",
            get(super::registry::preflight::preflight),
        )
        .route(
            "/api/agents/{agent_id}/activate",
            post(super::registry::preflight::activate),
        )
        .route(
            "/api/agents/{agent_id}/revisions",
            get(super::registry::revisions::list),
        )
        .route(
            "/api/agents/{agent_id}/eval-runs",
            get(super::eval_runs::list).post(super::eval_runs::create),
        )
        .route(
            "/api/agents/{agent_id}/improvement-proposals",
            post(super::improvements::create),
        )
        .route("/api/evolution/sweep", post(super::evolution::sweep))
        .route(
            "/api/agents/{agent_id}/grants",
            get(super::grants::list).post(super::grants::create),
        )
        .route(
            "/api/agents/{agent_id}/grantable-users",
            get(super::grants::grantable_users),
        )
        .route(
            "/api/agents/{agent_id}/grants/{grantee}",
            delete(super::grants::delete),
        )
        .route(
            "/api/agents/{agent_id}/group-grants",
            get(super::grants::list_group_grants).post(super::grants::create_group_grant),
        )
        .route(
            "/api/agents/{agent_id}/group-grants/{group_id}",
            delete(super::grants::delete_group_grant),
        )
        .route(
            "/api/agents/{agent_id}/grantable-groups",
            get(super::grants::grantable_groups),
        )
        .route(
            "/api/agents/{agent_id}/workspace/files",
            get(super::workspace::list_files).delete(super::workspace::delete_file),
        )
        .route(
            "/api/agents/{agent_id}/workspace/files/upload-url",
            post(super::workspace::create_upload_url),
        )
        .route(
            "/api/agents/{agent_id}/workspace/files/download-url",
            get(super::workspace::download_url),
        )
        .route(
            "/api/agents/{agent_id}/memory",
            get(super::memory::list::list).post(super::memory::store::store),
        )
        .route(
            "/api/agents/{agent_id}/memory/{key}",
            delete(super::memory::delete::delete),
        )
        .route(
            "/api/agents/{agent_id}/run",
            post(super::runs::create::create),
        )
        .route("/api/agents/{agent_id}/runs", get(super::runs::list::list))
        .route(
            "/api/agents/{agent_id}/runs/{run_id}/logs",
            get(super::runs::logs::logs),
        )
}

fn import_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/agents/import/opencode-files",
            post(super::import_files::import_opencode_files),
        )
        .route(
            "/api/agents/import/bundle",
            post(super::import_files::import_agent_bundle),
        )
        .route(
            "/api/agents/import/{provider_id}/discover",
            post(super::import::discover),
        )
        .route(
            "/api/agents/import/{provider_id}",
            post(super::import::import),
        )
}

fn rule_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/rules",
            post(super::rules::create::create).get(super::rules::list::list),
        )
        .route(
            "/api/rules/{rule_id}",
            get(super::rules::get::get)
                .patch(super::rules::update::update)
                .delete(super::rules::delete::delete),
        )
}

fn routine_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/routines",
            post(super::routines::create::create).get(super::routines::list::list),
        )
        .route(
            "/api/routines/{routine_id}",
            axum::routing::patch(super::routines::update::update)
                .delete(super::routines::delete::delete),
        )
        .route(
            "/api/routines/{routine_id}/trigger",
            post(super::routines::trigger::trigger),
        )
}

fn skill_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/skills",
            post(super::skills::create::create).get(super::skills::list::list),
        )
        .route(
            "/api/skills/{skill_id}",
            get(super::skills::get::get)
                .patch(super::skills::update::update)
                .delete(super::skills::delete::delete),
        )
}

fn inbox_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/inbox", get(super::inbox::list::list))
        .route(
            "/api/inbox/{item_id}/resolve",
            post(super::inbox::resolve::resolve),
        )
        .route("/api/approvals", get(super::inbox::approvals::list_pending))
        .route(
            "/api/approvals/{item_id}/accept",
            post(super::inbox::approvals::accept),
        )
        .route(
            "/api/approvals/{item_id}/reject",
            post(super::inbox::approvals::reject),
        )
}

fn slack_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/agents/{agent_id}/slack/events",
            post(super::slack::events),
        )
        .route(
            "/api/agents/{agent_id}/slack/interactivity",
            post(super::slack::interactivity),
        )
        .route(
            "/api/agents/{agent_id}/slack/oauth-state",
            post(super::slack::oauth_state),
        )
        .route(
            "/host-oauth-callback/{provider_id}",
            get(super::slack::oauth_callback),
        )
}

fn teams_routes() -> Router<Arc<AppState>> {
    Router::new().route(
        "/api/agents/{agent_id}/teams/messages",
        post(super::teams::messages),
    )
}

fn google_chat_routes() -> Router<Arc<AppState>> {
    Router::new().route(
        "/api/agents/{agent_id}/google-chat/events",
        post(google_chat::events),
    )
}

fn webhook_routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/agents/{agent_id}/webhook", post(webhook::events))
}
