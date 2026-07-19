use axum::http::HeaderMap;

use crate::{
    callbacks::request_attribution::RequestAttribution,
    db::managed_agents::{session_control, sessions},
    errors::GatewayError,
    proxy::{auth::master_key::AuthContext, state::AppState},
};

pub const SESSION_ID_HEADER: &str = "x-lap-session-id";

pub async fn resolve(
    state: &AppState,
    auth: &AuthContext,
    headers: &HeaderMap,
) -> Result<RequestAttribution, GatewayError> {
    let Some(session_id) = header_value(headers, SESSION_ID_HEADER)? else {
        return Ok(RequestAttribution::api());
    };
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let session = sessions::repository::get(pool, &session_id)
        .await?
        .filter(|session| can_access(auth, session.owner_id.as_deref()))
        .ok_or_else(|| GatewayError::NotFound("session not found".to_owned()))?;
    let invocation_id = session_control::repository::active_turn(pool, &session.id)
        .await?
        .and_then(|snapshot| {
            snapshot
                .invocations
                .iter()
                .find(|invocation| invocation.role == "primary")
                .or_else(|| snapshot.invocations.first())
                .map(|invocation| invocation.id.clone())
        });
    Ok(RequestAttribution {
        session_id: Some(session.id),
        agent_id: session.agent_id,
        invocation_id,
        purpose: "production".to_owned(),
    })
}

fn header_value(headers: &HeaderMap, name: &str) -> Result<Option<String>, GatewayError> {
    headers
        .get(name)
        .map(|value| {
            value
                .to_str()
                .map(str::trim)
                .map(str::to_owned)
                .map_err(|_| GatewayError::BadRequest(format!("invalid {name} header")))
        })
        .transpose()
        .map(|value| value.filter(|value| !value.is_empty()))
}

fn can_access(auth: &AuthContext, owner_id: Option<&str>) -> bool {
    auth.is_admin || owner_id == Some(auth.user_id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ownership_is_enforced_for_non_admin_callers() {
        let user = AuthContext {
            user_id: "user-a".to_owned(),
            is_admin: false,
            role: "user".to_owned(),
        };
        assert!(can_access(&user, Some("user-a")));
        assert!(!can_access(&user, Some("user-b")));
        assert!(!can_access(&user, None));
        assert!(can_access(&AuthContext::admin(), None));
    }

    #[test]
    fn empty_session_header_is_treated_as_unattributed() {
        let mut headers = HeaderMap::new();
        headers.insert(SESSION_ID_HEADER, "  ".parse().expect("header"));
        assert_eq!(
            header_value(&headers, SESSION_ID_HEADER).expect("value"),
            None
        );
    }
}
