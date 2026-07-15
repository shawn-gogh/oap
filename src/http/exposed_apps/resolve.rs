use reqwest::Url;
use sqlx::PgPool;

use crate::{
    db::managed_agents::{runtime_refs, sessions::schema::SessionRow},
    errors::GatewayError,
    http::runtime_resolution,
    proxy::state::AppState,
};

/// Resolves the runtime container's base URL for a session: the runtime ref's
/// provider_url when set, else the registered harness credential's api_base.
/// The exposed app's port is swapped in by the caller.
pub(crate) async fn container_base(
    pool: &PgPool,
    state: &AppState,
    session: &SessionRow,
) -> Result<Url, GatewayError> {
    if let Some(ref_id) = session.runtime_agent_ref_id.as_deref() {
        if let Some(row) = runtime_refs::repository::get_by_id(pool, ref_id).await? {
            if let Some(url) = row.provider_url.as_deref() {
                if let Ok(parsed) = parse_base(url) {
                    return Ok(parsed);
                }
            }
        }
    }

    let alias = session.runtime.as_deref().unwrap_or(&session.harness);
    let credential = runtime_resolution::harness_credential(pool, state, alias).await?;
    parse_base(&credential.api_base)
}

pub(crate) fn parse_base(raw: &str) -> Result<Url, GatewayError> {
    let trimmed = raw.trim();
    let url = Url::parse(trimmed)
        .or_else(|_| Url::parse(&format!("http://{trimmed}")))
        .map_err(|_| {
            GatewayError::InvalidConfig(format!("invalid runtime container URL: {raw}"))
        })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(GatewayError::InvalidConfig(format!(
            "runtime container URL must be http(s): {raw}"
        )));
    }
    if url.host_str().is_none() {
        return Err(GatewayError::InvalidConfig(format!(
            "runtime container URL has no host: {raw}"
        )));
    }
    Ok(url)
}
