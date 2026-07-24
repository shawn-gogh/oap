use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    db::managed_agents::{runtime_events, session_control},
    errors::GatewayError,
    proxy::state::AppState,
};

pub async fn receive(
    State(state): State<Arc<AppState>>,
    Path(invocation_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Result<(StatusCode, Json<Value>), GatewayError> {
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let binding = session_control::repository::get_invocation(pool, &invocation_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("A2A push invocation not found".to_owned()))?;
    let push = binding
        .metadata
        .get("a2a_push")
        .filter(|push| push.get("enabled").and_then(Value::as_bool) == Some(true))
        .ok_or(GatewayError::Unauthorized)?;
    let expected = push
        .get("token_sha256")
        .and_then(Value::as_str)
        .ok_or(GatewayError::Unauthorized)?;
    let token = bearer_token(&headers).ok_or(GatewayError::Unauthorized)?;
    let actual = format!("{:x}", Sha256::digest(token.as_bytes()));
    if !constant_time_eq(expected.as_bytes(), actual.as_bytes()) {
        return Err(GatewayError::Unauthorized);
    }
    let version = headers
        .get("A2A-Version")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| GatewayError::BadRequest("A2A-Version header is required".to_owned()))?;
    if version != binding.protocol_version {
        return Err(GatewayError::BadRequest(format!(
            "A2A push version `{version}` does not match frozen invocation version `{}`",
            binding.protocol_version
        )));
    }
    let event = payload
        .get("task")
        .or_else(|| payload.get("statusUpdate"))
        .or_else(|| payload.get("artifactUpdate"))
        .or_else(|| payload.get("data"))
        .cloned()
        .unwrap_or(payload);
    if let Some(expected_task_id) = binding.remote_task_id.as_deref() {
        let actual_task_id = event
            .get("id")
            .or_else(|| event.get("taskId"))
            .and_then(Value::as_str);
        if actual_task_id.is_some_and(|task_id| task_id != expected_task_id) {
            return Err(GatewayError::BadRequest(
                "A2A push task id does not match the invocation".to_owned(),
            ));
        }
    }
    let event_bytes = serde_json::to_vec(&event)
        .map_err(|error| GatewayError::BadRequest(format!("invalid A2A push event: {error}")))?;
    let event_digest = format!("{:x}", Sha256::digest(event_bytes));
    let accepted = session_control::repository::accept_a2a_push_event(
        pool,
        &invocation_id,
        &event_digest,
        &event,
    )
    .await?;
    if !accepted {
        return Ok((
            StatusCode::OK,
            Json(json!({"accepted": true, "duplicate": true})),
        ));
    }
    let runtime_event = json!({
        "type": "agent.progress",
        "protocol": "a2a",
        "protocol_version": binding.protocol_version,
        "source": "push_notification",
        "data": event,
    });
    runtime_events::repository::append(pool, &binding.session_id, runtime_event.clone()).await?;
    state
        .local_session_events
        .publish(&binding.session_id, runtime_event);
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"accepted": true, "duplicate": false})),
    ))
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

#[cfg(test)]
mod tests {
    use axum::http::{header, HeaderValue};

    use super::*;

    #[test]
    fn extracts_bearer_token_and_compares_digest_in_constant_time() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret"),
        );
        assert_eq!(bearer_token(&headers), Some("secret"));
        assert!(constant_time_eq(b"same", b"same"));
        assert!(!constant_time_eq(b"same", b"diff"));
        assert!(!constant_time_eq(b"short", b"longer"));
    }
}
