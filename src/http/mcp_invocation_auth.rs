use axum::http::{header::AUTHORIZATION, HeaderMap};

use crate::{
    db::managed_agents::{sessions, sources},
    errors::GatewayError,
    proxy::{
        auth::master_key::{authenticate, AuthContext},
        state::AppState,
    },
};

pub async fn authenticate_request(
    state: &AppState,
    headers: &HeaderMap,
    session_id: Option<&str>,
) -> Result<AuthContext, GatewayError> {
    let capability_token = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|token| token.starts_with("cap_") && !token.is_empty());
    let Some(token) = capability_token else {
        return authenticate(headers, state).await;
    };
    let session_id = session_id
        .map(str::trim)
        .filter(|session_id| !session_id.is_empty())
        .ok_or(GatewayError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    if !sources::repository::validate_capability_token(pool, session_id, token).await? {
        return Err(GatewayError::Unauthorized);
    }
    let session = sessions::repository::get(pool, session_id)
        .await?
        .ok_or(GatewayError::Unauthorized)?;
    Ok(AuthContext {
        user_id: session.owner_id.unwrap_or_else(|| "system".to_owned()),
        is_admin: false,
    })
}
