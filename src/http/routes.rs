use std::sync::Arc;

use axum::{
    extract::DefaultBodyLimit,
    routing::{any, delete, get, post, put},
    Router,
};

use crate::{
    http::{
        agents::events,
        capabilities::capabilities,
        health::health,
        messages::messages,
        models::models,
        openapi::{openapi_json, swagger_ui},
        responses::responses,
        sessions, ui, vault,
    },
    mcp::route::{streamable_http, streamable_http_server},
    proxy::state::AppState,
};

pub fn router(state: Arc<AppState>) -> Router {
    public_routes()
        .merge(api_routes())
        .merge(exposed_app_routes())
        .merge(session_routes())
        .merge(crate::http::observability::routes::router())
        .merge(crate::http::management::routes::router())
        .merge(crate::http::managed_agents::routes::router())
        .merge(mcp_routes())
        .merge(mcp_registry_routes())
        .fallback_service(ui::static_files())
        // Outermost: reroute absolute-path asset requests escaping an exposed
        // app's /apps/{id}/ prefix (identified via Referer) back under it.
        .layer(axum::middleware::from_fn(
            crate::http::exposed_apps::proxy::referer_fallback,
        ))
        .with_state(state)
}

fn public_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(ui::redirect_to_sessions))
        .route("/inbox", get(ui::inbox_html))
        .route("/inbox/", get(ui::inbox_html))
        .route("/inbox.txt", get(ui::inbox_index_txt))
        .route("/inbox/index.txt", get(ui::inbox_index_txt))
        .route("/docs", get(swagger_ui))
        .route("/openapi.json", get(openapi_json))
        .route("/health", get(health))
        .route("/event", get(events))
        .route("/v1/messages", post(messages))
        .route("/v1/responses", post(responses))
        .route("/v1/models", get(models))
        .route(
            "/v1/sessions/{session_id}/events/stream",
            get(sessions::runtime_events),
        )
        .route(
            "/v1/sessions/{session_id}/events",
            get(sessions::runtime_event_list),
        )
}

fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/harness-proxy/{*path}",
            any(crate::http::harness_proxy::proxy),
        )
        .route("/api/capabilities", get(capabilities))
        .route(
            "/api/plugin-manifest",
            get(crate::http::plugin_manifest::plugin_manifest),
        )
        .route("/api/platform-mcps", get(crate::http::platform_mcps::list))
        .route(
            "/api/agent-runtimes",
            get(crate::http::agent_runtimes::list),
        )
        .route(
            "/api/agent-runtimes/{runtime}/credentials",
            put(crate::http::agent_runtimes::save).delete(crate::http::agent_runtimes::delete),
        )
        .route(
            "/api/runtime-harnesses",
            get(crate::http::runtime_harnesses::list).post(crate::http::runtime_harnesses::create),
        )
        .route(
            "/api/runtime-harnesses/test",
            post(crate::http::runtime_harnesses::test_connection),
        )
        .route(
            "/api/runtime-harnesses/{alias}",
            put(crate::http::runtime_harnesses::update)
                .delete(crate::http::runtime_harnesses::delete_harness),
        )
        .route(
            "/api/providers",
            get(crate::http::provider_credentials::list),
        )
        .route(
            "/api/providers/{provider_id}",
            post(crate::http::provider_credentials::save_provider)
                .delete(crate::http::provider_credentials::delete_provider),
        )
        .merge(vault_routes())
}

fn exposed_app_routes() -> Router<Arc<AppState>> {
    use crate::http::exposed_apps::{api, proxy};
    Router::new()
        // Browser-facing reverse proxy into agent runtime containers.
        .route("/apps/{app_id}", any(proxy::root))
        .route("/apps/{app_id}/", any(proxy::root_slash))
        .route("/apps/{app_id}/{*path}", any(proxy::proxy))
        // Owner management API.
        .route("/api/apps", get(api::list))
        .route("/api/apps/{app_id}", delete(api::delete))
        .route(
            "/api/apps/{app_id}/share",
            post(api::create_share).delete(api::revoke_share),
        )
}

fn vault_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/vault/global",
            get(vault::list_global).post(vault::save_global),
        )
        .route("/api/vault/global/{key}", delete(vault::delete_global))
        .route("/api/vault/{user_id}", get(vault::list).post(vault::save))
        .route("/api/vault/{user_id}/{key}", delete(vault::delete_personal))
}

fn mcp_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/mcp",
            get(streamable_http)
                .post(streamable_http)
                .delete(streamable_http),
        )
        .route(
            "/mcp/{server_id}",
            get(streamable_http_server)
                .post(streamable_http_server)
                .delete(streamable_http_server),
        )
        .route(
            "/mcp/platform/{agent_id}",
            post(crate::http::platform_mcps::serve),
        )
}

fn mcp_registry_routes() -> Router<Arc<AppState>> {
    use crate::http::mcp_registry::{
        admin, discover, oauth, proxy, public, settings, tools, user_credentials,
    };
    Router::new()
        // Public (no auth)
        .route("/public/mcp_hub", get(public::mcp_hub))
        .route(
            "/v1/mcp/server/{server_id}/oauth/start",
            post(oauth::start_oauth),
        )
        .route("/v1/mcp/oauth/callback", get(oauth::oauth_callback))
        .route(
            "/v1/mcp/server/{server_id}/tools",
            get(tools::list_tools).post(tools::test_tools),
        )
        // Batch tools discovery across all active servers (avoids N+1 from the UI)
        .route("/v1/mcp/servers/tools", get(tools::list_all_tools))
        // Discover tools from an arbitrary URL (no saved server required)
        .route("/v1/mcp/discover", post(discover::discover_tools))
        // Admin CRUD
        .route(
            "/v1/mcp/server",
            get(admin::list).post(admin::create).put(admin::update),
        )
        .route(
            "/v1/mcp/server/{server_id}",
            get(admin::get_one).delete(admin::delete_one),
        )
        .route(
            "/v1/mcp/settings/proxy-base-url",
            get(settings::get_proxy_base_url).put(settings::update_proxy_base_url),
        )
        // User credentials (BYOK)
        .route(
            "/v1/mcp/server/{server_id}/user-credential",
            post(user_credentials::store).delete(user_credentials::delete_credential),
        )
        .route("/v1/mcp/user-credentials", get(user_credentials::list))
        // Dynamic proxy — must be LAST (catch-all)
        .route(
            "/{mcp_server_name}/mcp",
            get(proxy::dynamic_mcp)
                .post(proxy::dynamic_mcp)
                .put(proxy::dynamic_mcp)
                .delete(proxy::dynamic_mcp)
                .patch(proxy::dynamic_mcp),
        )
}

fn session_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/session", get(sessions::list).post(sessions::create))
        .route(
            "/session/{session_id}",
            get(sessions::get)
                .patch(sessions::rename)
                .delete(sessions::delete),
        )
        .route(
            "/session/{session_id}/message",
            get(sessions::messages).post(sessions::send_message),
        )
        .route(
            "/session/{session_id}/prompt_async",
            post(sessions::prompt_async),
        )
        .route(
            "/session/{session_id}/approval-mode",
            put(sessions::set_approval_mode),
        )
        .route(
            "/session/{session_id}/runtime_events",
            get(sessions::runtime_events),
        )
        .route(
            "/session/{session_id}/runtime_events/list",
            get(sessions::runtime_event_list),
        )
        .route("/session/{session_id}/interrupt", post(sessions::interrupt))
        .route("/session/{session_id}/abort", post(sessions::abort))
        .route(
            "/api/sessions/{session_id}/turns",
            get(sessions::list_turns).post(sessions::create_turn),
        )
        .route(
            "/api/sessions/{session_id}/active-turn",
            get(sessions::active_turn),
        )
        .route(
            "/api/sessions/{session_id}/turns/{turn_id}",
            get(sessions::get_turn),
        )
        .route(
            "/api/sessions/{session_id}/turns/{turn_id}/artifacts",
            post(sessions::create_artifact).layer(DefaultBodyLimit::max(30 * 1024 * 1024)),
        )
        .route(
            "/api/sessions/{session_id}/artifacts",
            get(sessions::list_artifacts),
        )
        .route(
            "/api/sessions/{session_id}/artifacts/{artifact_id}",
            get(sessions::get_artifact),
        )
        .route(
            "/api/sessions/{session_id}/turns/{turn_id}/cancel",
            post(sessions::cancel_turn),
        )
        .route(
            "/api/sessions/{session_id}/turns/{turn_id}/resume",
            post(sessions::resume_turn),
        )
        .route(
            "/api/sessions/{session_id}/turns/{turn_id}/retry",
            post(sessions::retry_turn),
        )
        .route(
            "/api/sessions/{session_id}/control-events",
            get(sessions::control_events),
        )
        .route(
            "/api/sessions/{session_id}/control-events/stream",
            get(sessions::control_event_stream),
        )
        .route(
            "/api/sessions/{session_id}/cloudevents",
            get(sessions::cloud_events)
                .post(sessions::ingest_cloud_event)
                .layer(DefaultBodyLimit::max(1024 * 1024)),
        )
        .route(
            "/session/{session_id}/workspace/files",
            get(sessions::list_files).delete(sessions::delete_file),
        )
        .route(
            "/session/{session_id}/workspace/files/upload-url",
            post(sessions::create_upload_url),
        )
        .route(
            "/session/{session_id}/workspace/files/download-url",
            get(sessions::download_url),
        )
        .route(
            "/session/{session_id}/workspace/files/move",
            post(sessions::move_files),
        )
        .route(
            "/session/{session_id}/workspace/files/copy",
            post(sessions::copy_files),
        )
        .route(
            "/session/{session_id}/workspace/files/batch-delete",
            post(sessions::batch_delete_files),
        )
        .route(
            "/session/{session_id}/workspace/browse",
            get(sessions::browse_files),
        )
        .route(
            "/session/{session_id}/workspace/folders",
            get(sessions::list_folders).post(sessions::create_folder),
        )
        .route(
            "/session/{session_id}/workspace/files/batch-transfer",
            post(sessions::batch_transfer_files),
        )
        .route(
            "/session/{session_id}/workspace/trash",
            get(sessions::list_workspace_trash).post(sessions::trash_workspace_paths),
        )
        .route(
            "/session/{session_id}/workspace/trash/restore",
            post(sessions::restore_workspace_trash),
        )
        .route(
            "/session/{session_id}/workspace/trash/delete",
            post(sessions::delete_workspace_trash),
        )
        .route(
            "/session/{session_id}/workspace/trash/empty",
            post(sessions::empty_workspace_trash),
        )
}
