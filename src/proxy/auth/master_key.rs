use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::http::{
    header::{AUTHORIZATION, COOKIE},
    HeaderMap,
};

use crate::{errors::GatewayError, proxy::state::AppState};

/// TTL cache of key-validation results: value None caches a rejection.
type KeyValidationCache = Mutex<Option<HashMap<String, (Instant, Option<AuthContext>)>>>;

// Cache litellm key validation results for 60 s to avoid a round-trip on every request.
static LITELLM_KEY_CACHE: KeyValidationCache = Mutex::new(None);
// Same TTL cache for DB-backed gateway keys, keyed by key hash.
static GATEWAY_KEY_CACHE: KeyValidationCache = Mutex::new(None);
const CACHE_TTL: Duration = Duration::from_secs(60);
pub const WEB_SESSION_COOKIE: &str = "lap_session";

/// Identity derived from the presented key. `user_id` is the ownership
/// boundary for sessions/agents/workspaces; admins bypass it.
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user_id: String,
    pub is_admin: bool,
}

impl AuthContext {
    pub fn admin() -> Self {
        Self {
            user_id: "admin".to_owned(),
            is_admin: true,
        }
    }
}

pub fn require_master_key(
    headers: &HeaderMap,
    configured: Option<&str>,
) -> Result<(), GatewayError> {
    let Some(master_key) = configured else {
        return Ok(());
    };

    if presented_key(headers) == Some(master_key) {
        Ok(())
    } else {
        Err(GatewayError::Unauthorized)
    }
}

pub async fn require_any_gateway_key(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<(), GatewayError> {
    authenticate(headers, state).await.map(|_| ())
}

/// Validates the presented key and resolves it to an identity.
pub async fn authenticate(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<AuthContext, GatewayError> {
    if let Some(auth) = authenticate_web_session(headers, state).await? {
        return Ok(auth);
    }
    authenticate_key(presented_key(headers), state).await
}

/// Like `authenticate`, but falls back to an explicit key (e.g. an SSE
/// `?key=` query parameter, since EventSource can't set headers) when the
/// request carries no auth header.
pub async fn authenticate_with_fallback_key(
    headers: &HeaderMap,
    fallback_key: Option<&str>,
    state: &AppState,
) -> Result<AuthContext, GatewayError> {
    if let Some(auth) = authenticate_web_session(headers, state).await? {
        return Ok(auth);
    }
    authenticate_key(presented_key(headers).or(fallback_key), state).await
}

pub async fn authenticate_explicit_key(
    key: &str,
    state: &AppState,
) -> Result<AuthContext, GatewayError> {
    authenticate_key(Some(key), state).await
}

async fn authenticate_web_session(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<Option<AuthContext>, GatewayError> {
    let Some(token) = session_token(headers) else {
        return Ok(None);
    };
    let Some(pool) = &state.db else {
        return Ok(None);
    };
    crate::db::managed_agents::web_sessions::authenticate(pool, token).await
}

async fn authenticate_key(
    presented: Option<&str>,
    state: &AppState,
) -> Result<AuthContext, GatewayError> {
    let master_key = state.config.general_settings.master_key.as_deref();

    // No auth configured — open mode, everyone is the admin.
    let Some(configured_master_key) = master_key else {
        return Ok(AuthContext::admin());
    };

    let Some(key) = presented else {
        return Err(GatewayError::Unauthorized);
    };

    // Fast path: local master key.
    if key == configured_master_key {
        return Ok(AuthContext::admin());
    }

    // DB-persisted gateway API key (cached by hash).
    if let Some(pool) = &state.db {
        let hash = crate::db::managed_agents::api_keys::repository::hash_key(key);
        if let Some(cached) = cache_get(&GATEWAY_KEY_CACHE, &hash) {
            if let Some(ctx) = cached {
                if crate::db::managed_agents::users::repository::find(pool, &ctx.user_id)
                    .await
                    .ok()
                    .flatten()
                    .is_some_and(|user| user.is_active())
                {
                    return Ok(ctx);
                }
                return Err(GatewayError::Unauthorized);
            }
        } else {
            let found =
                match crate::db::managed_agents::api_keys::repository::find_by_key(pool, key)
                    .await
                    .ok()
                    .flatten()
                {
                    Some(row) => {
                        crate::db::managed_agents::users::repository::ensure(pool, &row.user_id)
                            .await
                            .ok()
                            .filter(|user| user.is_active())
                            .map(|_| AuthContext {
                                is_admin: row.is_admin(),
                                user_id: row.user_id,
                            })
                    }
                    None => None,
                };
            cache_put(&GATEWAY_KEY_CACHE, hash, found.clone());
            if let Some(ctx) = found {
                return Ok(ctx);
            }
        }
    }

    // Legacy in-memory API keys (kept for DB-less deployments).
    if state.api_keys.accepts(key) {
        return Ok(AuthContext {
            user_id: format!("key:{}", short_hash(key)),
            is_admin: false,
        });
    }

    // Slow path: validate against litellm if configured. Never admin.
    if let Some(base_url) = state.config.general_settings.litellm_base_url.as_deref() {
        if let Some(ctx) = validate_with_litellm(key, base_url, &state.http).await {
            if let Some(pool) = &state.db {
                let user = crate::db::managed_agents::users::repository::ensure(pool, &ctx.user_id)
                    .await?;
                if !user.is_active() {
                    return Err(GatewayError::Unauthorized);
                }
            }
            return Ok(ctx);
        }
    }

    Err(GatewayError::Unauthorized)
}

fn short_hash(key: &str) -> String {
    let hash = crate::db::managed_agents::api_keys::repository::hash_key(key);
    hash[..16].to_owned()
}

fn cache_get(cache: &KeyValidationCache, key: &str) -> Option<Option<AuthContext>> {
    let mut guard = cache.lock().unwrap_or_else(|p| p.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    match map.get(key) {
        Some((ts, ctx)) if ts.elapsed() < CACHE_TTL => Some(ctx.clone()),
        Some(_) => {
            map.remove(key);
            None
        }
        None => None,
    }
}

fn cache_put(cache: &KeyValidationCache, key: String, ctx: Option<AuthContext>) {
    let mut guard = cache.lock().unwrap_or_else(|p| p.into_inner());
    guard
        .get_or_insert_with(HashMap::new)
        .insert(key, (Instant::now(), ctx));
}

/// Evicts a DB-backed gateway key from the identity cache immediately, so a
/// revoked/deleted key stops authenticating right away instead of staying
/// valid for up to CACHE_TTL more seconds.
pub fn evict_gateway_key_cache(key_hash: &str) {
    let mut guard = GATEWAY_KEY_CACHE.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(map) = guard.as_mut() {
        map.remove(key_hash);
    }
}

/// Call litellm's /key/info to validate a foreign key and derive an identity
/// (litellm's user_id when present, else a hash of the key).
/// Results are cached for CACHE_TTL to reduce latency.
async fn validate_with_litellm(
    key: &str,
    base_url: &str,
    client: &reqwest::Client,
) -> Option<AuthContext> {
    if let Some(cached) = cache_get(&LITELLM_KEY_CACHE, key) {
        return cached;
    }

    let url = format!("{}/key/info", base_url.trim_end_matches('/'));
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {key}"))
        .send()
        .await;

    let result = match response {
        Ok(r) if r.status().is_success() => {
            let user_id = r
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|v| {
                    v.pointer("/info/user_id")
                        .or_else(|| v.pointer("/user_id"))
                        .and_then(|u| u.as_str())
                        .map(str::to_owned)
                })
                .filter(|u| !u.is_empty())
                .unwrap_or_else(|| format!("litellm:{}", short_hash(key)));
            Some(AuthContext {
                user_id,
                is_admin: false,
            })
        }
        _ => None,
    };

    cache_put(&LITELLM_KEY_CACHE, key.to_owned(), result.clone());
    result
}

pub fn presented_key(headers: &HeaderMap) -> Option<&str> {
    if let Some(bearer) = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        return Some(bearer);
    }
    headers.get("x-api-key").and_then(|v| v.to_str().ok())
}

pub fn session_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let (name, value) = cookie.trim().split_once('=')?;
                (name == WEB_SESSION_COOKIE).then_some(value)
            })
        })
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap;

    use super::require_master_key;

    fn headers(name: &'static str, value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(name, value.parse().unwrap());
        h
    }

    #[test]
    fn accepts_authorization_bearer() {
        let h = headers("authorization", "Bearer sk-local");
        assert!(require_master_key(&h, Some("sk-local")).is_ok());
    }

    #[test]
    fn accepts_x_api_key() {
        let h = headers("x-api-key", "sk-local");
        assert!(require_master_key(&h, Some("sk-local")).is_ok());
    }

    #[test]
    fn rejects_wrong_key() {
        let h = headers("x-api-key", "nope");
        assert!(require_master_key(&h, Some("sk-local")).is_err());
    }

    #[test]
    fn rejects_missing_header() {
        assert!(require_master_key(&HeaderMap::new(), Some("sk-local")).is_err());
    }

    #[test]
    fn no_master_key_configured_allows_all() {
        assert!(require_master_key(&HeaderMap::new(), None).is_ok());
    }
}
