use std::sync::Arc;

use axum::{
    extract::State,
    http::{header::SET_COOKIE, HeaderMap, HeaderValue, StatusCode},
    response::AppendHeaders,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    db::managed_agents::{audit, web_sessions},
    errors::GatewayError,
    proxy::{
        auth::master_key::{
            authenticate, authenticate_explicit_key, session_token, WEB_SESSION_COOKIE,
        },
        state::AppState,
    },
};

const COOKIE_MAX_AGE_SECONDS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub key: String,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(input): Json<LoginRequest>,
) -> Result<
    (
        StatusCode,
        AppendHeaders<[(axum::http::HeaderName, HeaderValue); 1]>,
        Json<serde_json::Value>,
    ),
    GatewayError,
> {
    let auth = authenticate_explicit_key(input.key.trim(), &state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let session = web_sessions::create(pool, &auth).await?;
    audit::record(
        pool,
        &auth.user_id,
        "auth.login",
        "web_session",
        &session.expires_at.to_string(),
        json!({
            "is_admin": auth.is_admin,
            "role": auth.role,
        }),
    )
    .await?;
    Ok((
        StatusCode::OK,
        cookie_header(&session.token, COOKIE_MAX_AGE_SECONDS)?,
        Json(json!({ "id": auth.user_id, "is_admin": auth.is_admin, "role": auth.role })),
    ))
}

pub async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<
    (
        StatusCode,
        AppendHeaders<[(axum::http::HeaderName, HeaderValue); 1]>,
    ),
    GatewayError,
> {
    let auth = authenticate(&headers, &state).await?;
    if let (Some(pool), Some(token)) = (state.db.as_ref(), session_token(&headers)) {
        if web_sessions::revoke(pool, token).await? {
            audit::record(
                pool,
                &auth.user_id,
                "auth.logout",
                "web_session",
                &auth.user_id,
                json!({}),
            )
            .await?;
        }
    }
    Ok((StatusCode::NO_CONTENT, cookie_header("", 0)?))
}

fn cookie_header(
    token: &str,
    max_age: i64,
) -> Result<AppendHeaders<[(axum::http::HeaderName, HeaderValue); 1]>, GatewayError> {
    let value =
        format!("{WEB_SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}");
    let value = HeaderValue::from_str(&value)
        .map_err(|_| GatewayError::InvalidConfig("无法创建登录会话 Cookie。".to_owned()))?;
    Ok(AppendHeaders([(SET_COOKIE, value)]))
}
