use std::sync::Arc;

use axum::{
    extract::DefaultBodyLimit,
    routing::{delete, get, post},
    Router,
};

use crate::{channels::webhook, proxy::state::AppState};

mod agent_observability;

// Bundle/opencode-files imports carry a base64-encoded zip in the JSON body,
// which inflates the payload ~33% over the raw file size (plus the
// MAX_BUNDLE_BYTES uncompressed-size guard in import_files.rs already caps
// the decoded archive at 20MB). Axum's default 2MB body limit is far too
// small for these routes, so it's raised just for them.
const IMPORT_BODY_LIMIT_BYTES: usize = 64 * 1024 * 1024;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .merge(agent_routes())
        .merge(rule_routes())
        .merge(routine_routes())
        .merge(skill_routes())
        .merge(inbox_routes())
        .merge(webhook_routes())
        .merge(mattermost_routes())
}

fn agent_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/identity-mappings",
            get(super::identity_mappings::list),
        )
        .route(
            "/api/identity-mappings/{mapping_id}/bind",
            post(super::identity_mappings::bind),
        )
        .route(
            "/api/identity-mappings/{mapping_id}/block",
            post(super::identity_mappings::block),
        )
        .route(
            "/api/agent-source-connectors",
            get(super::source_management::list_connectors)
                .post(super::source_management::create_connector),
        )
        .route(
            "/api/agent-source-connectors/{connector_id}",
            post(super::source_management::test_connector)
                .patch(super::source_management::update_connector)
                .delete(super::source_management::delete_connector),
        )
        .route(
            "/api/agent-source-connectors/{connector_id}/webhook",
            post(super::source_management::connector_webhook),
        )
        .route(
            "/api/agents/import/providers",
            get(super::import::list_providers),
        )
        .route(
            "/api/agents/import/opencode-files",
            post(super::import_files::import_opencode_files)
                .layer(DefaultBodyLimit::max(IMPORT_BODY_LIMIT_BYTES)),
        )
        .route(
            "/api/agents/import/bundle",
            post(super::import_files::import_agent_bundle)
                .layer(DefaultBodyLimit::max(IMPORT_BODY_LIMIT_BYTES)),
        )
        .route(
            "/api/agents/import/{provider_id}/discover",
            post(super::import::discover),
        )
        .route(
            "/api/agents/import/{provider_id}/preview",
            post(super::import::preview),
        )
        .route(
            "/api/agents/import/{provider_id}",
            post(super::import::import),
        )
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
            "/api/agents/{agent_id}/restore",
            post(super::registry::restore::restore),
        )
        .route(
            "/api/agents/byo-credentials",
            get(super::byo_credentials::list_configured),
        )
        .route(
            "/api/agents/{agent_id}/byo-credential",
            get(super::byo_credentials::status)
                .put(super::byo_credentials::store)
                .delete(super::byo_credentials::delete),
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
            "/api/agents/{agent_id}/tasks",
            get(super::tasks::list).post(super::tasks::create),
        )
        .route(
            "/api/agents/{agent_id}/tasks/{task_id}",
            get(super::tasks::get),
        )
        .route(
            "/api/agents/{agent_id}/tasks/{task_id}/artifacts",
            get(super::tasks::list_artifacts).post(super::tasks::create_artifact),
        )
        .route(
            "/api/agents/{agent_id}/tasks/{task_id}/acceptance",
            get(super::tasks::list_acceptance).post(super::tasks::update_acceptance),
        )
        .route(
            "/api/agents/{agent_id}/tasks/{task_id}/resume",
            post(super::tasks::resume),
        )
        .route(
            "/api/agents/{agent_id}/tasks/{task_id}/attempts",
            get(super::tasks::list_attempts),
        )
        .route(
            "/api/agents/{agent_id}/tasks/{task_id}/retry",
            post(super::tasks::retry),
        )
        .route(
            "/api/agents/{agent_id}/tasks/{task_id}/cancel",
            post(super::tasks::cancel),
        )
        .merge(agent_observability::routes())
        .route(
            "/api/agents/{agent_id}/governance",
            get(super::governance::get),
        )
        .route(
            "/api/agents/{agent_id}/governance/test",
            post(super::governance::test),
        )
        .route(
            "/api/agents/{agent_id}/governance/request-publish",
            post(super::governance::request_publish),
        )
        .route(
            "/api/agents/{agent_id}/governance/rollback",
            post(super::governance::rollback),
        )
        .route(
            "/api/agents/{agent_id}/source",
            get(super::source_management::get_source),
        )
        .route(
            "/api/agents/{agent_id}/source/normalize",
            post(super::source_management::normalize_source),
        )
        .route(
            "/api/agents/{agent_id}/source/sync",
            post(super::source_management::sync_source),
        )
        .route(
            "/api/agents/{agent_id}/source/drift/accept",
            post(super::source_management::accept_drift),
        )
        .route(
            "/api/agents/{agent_id}/source/drift/reject",
            post(super::source_management::reject_drift),
        )
        .route(
            "/api/agents/{agent_id}/governance/conformance",
            post(super::source_management::check_conformance),
        )
        .route(
            "/api/agents/{agent_id}/governance/health",
            post(super::source_management::check_health),
        )
        .route(
            "/api/agents/{agent_id}/emergency-stop",
            post(super::source_management::emergency_stop),
        )
        .route(
            "/api/agents/{agent_id}/retire",
            post(super::source_management::retire),
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
            "/api/agents/{agent_id}/grants/batch",
            post(super::grants::create_batch),
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
            "/api/agents/{agent_id}/group-grants/batch",
            post(super::grants::create_batch_group_grant),
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
        .route(
            "/api/approvals/{item_id}/retry",
            post(super::inbox::approvals::retry),
        )
        .route("/api/tool-approvals", post(super::tool_approvals::asked))
}

fn webhook_routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/agents/{agent_id}/webhook", post(webhook::events))
}

fn mattermost_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/agents/{agent_id}/mattermost/events",
            post(super::mattermost::events),
        )
        .route(
            "/api/agents/{agent_id}/mattermost/connect",
            post(super::mattermost::connect),
        )
}
