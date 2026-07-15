use serde_json::{json, Value};
use sqlx::PgPool;

use crate::{
    db::managed_agents::{
        exposed_apps::{repository, schema::NewExposedApp},
        now_ms, sessions,
    },
    errors::GatewayError,
    http::exposed_apps::resolve,
    proxy::state::AppState,
};

use super::public_base_url;

/// Base URL for the returned app link. The MCP proxy base (`public_base_url`)
/// is the compose-internal address (http://lap:4000) that runtimes use, which
/// a host browser cannot open — prefer the browser-facing configuration,
/// mirroring the MINIO_PUBLIC_ENDPOINT split.
fn browser_base_url(state: &AppState) -> Result<String, GatewayError> {
    if let Some(configured) = state.config.general_settings.public_base_url.as_deref() {
        let trimmed = configured.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_owned());
        }
    }
    if let Ok(env) = std::env::var("LAP_PUBLIC_BASE_URL") {
        let trimmed = env.trim().to_owned();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }
    public_base_url(state)
}

const DEFAULT_TTL_MS: i64 = 24 * 60 * 60 * 1000;

pub async fn expose_port(
    state: &AppState,
    pool: &PgPool,
    agent_id: &str,
    session_id: Option<&str>,
    arguments: Value,
) -> Result<Value, GatewayError> {
    let session_id = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            GatewayError::BadRequest("expose_port requires a session context".to_owned())
        })?;
    let session = sessions::repository::get(pool, session_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("session not found: {session_id}")))?;

    let base = resolve::container_base(pool, state, &session).await?;
    let container_key = base
        .host_str()
        .ok_or_else(|| GatewayError::InvalidConfig("runtime URL has no host".to_owned()))?
        .to_owned();

    let requested_port = match arguments.get("port") {
        None | Some(Value::Null) => None,
        Some(value) => {
            let port = value.as_i64().filter(|port| (1024..65536).contains(port));
            Some(port.ok_or_else(|| {
                GatewayError::BadRequest("port must be an integer in 1024-65535".to_owned())
            })? as i32)
        }
    };
    let ttl_ms = arguments
        .get("ttl_seconds")
        .and_then(Value::as_i64)
        .map(|seconds| seconds.max(1) * 1000)
        .unwrap_or(DEFAULT_TTL_MS);
    let name = arguments
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let preserve_prefix = arguments
        .get("preserve_prefix")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let app = repository::allocate(
        pool,
        NewExposedApp {
            session_id,
            agent_id,
            owner_user_id: session.owner_id.as_deref(),
            container_key: &container_key,
            name,
            expires_at: Some(now_ms() + ttl_ms),
            preserve_prefix,
        },
        requested_port,
    )
    .await?;

    let public_base = browser_base_url(state)?;
    Ok(json!({
        "app_id": app.id,
        "port": app.port,
        "url": format!("{}/apps/{}/", public_base.trim_end_matches('/'), app.id),
        "expires_at": app.expires_at,
        "instructions": format!(
            "Start your HTTP/WebSocket server inside this container listening on 0.0.0.0:{}. \
             It will then be reachable at the returned url.",
            app.port
        ),
    }))
}
