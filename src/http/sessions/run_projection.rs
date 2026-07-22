use sqlx::PgPool;

use crate::{
    db::managed_agents::{artifacts, inbox, session_control},
    errors::GatewayError,
};

use super::run_types::{canonical_progress, canonical_steps, PendingInputRequestV1, RunSnapshotV1};

pub async fn load(
    pool: &PgPool,
    session_id: &str,
    turn_id: &str,
) -> Result<RunSnapshotV1, GatewayError> {
    let snapshot = session_control::repository::get_turn(pool, session_id, turn_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("turn {turn_id} not found")))?;
    let operations = session_control::repository::operations_for_turn(pool, turn_id).await?;
    let events = session_control::repository::events_for_turn(pool, turn_id).await?;
    let pending_input_request = pending_input_request(&events, turn_id);
    let progress = canonical_progress(&events);
    let steps = canonical_steps(&events);
    let pending_requests = inbox::repository::pending_approvals(pool, Some(session_id), None)
        .await?
        .into_iter()
        .filter(|item| item.turn_id.as_deref().is_none_or(|id| id == turn_id))
        .collect();
    let artifacts = artifacts::repository::list(pool, session_id, Some(turn_id)).await?;
    let latest_sequence =
        session_control::repository::latest_event_sequence(pool, session_id).await?;
    RunSnapshotV1::from_parts(
        snapshot,
        operations,
        progress,
        steps,
        pending_input_request,
        pending_requests,
        artifacts,
        latest_sequence,
    )
}

fn pending_input_request(
    events: &[session_control::schema::SessionControlEventRow],
    turn_id: &str,
) -> Option<PendingInputRequestV1> {
    let mut pending = None;
    for event in events
        .iter()
        .filter(|event| event.turn_id.as_deref() == Some(turn_id))
    {
        match event.event_type.as_str() {
            "input.requested" => {
                let request_id = event
                    .event_json
                    .get("request_id")
                    .and_then(serde_json::Value::as_str)
                    .or(event.request_id.as_deref());
                let Some(request_id) = request_id else {
                    continue;
                };
                pending = Some(PendingInputRequestV1 {
                    request_id: request_id.to_owned(),
                    invocation_id: event.invocation_id.clone(),
                    prompt: event
                        .event_json
                        .get("prompt")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("需要补充输入")
                        .to_owned(),
                    schema: event
                        .event_json
                        .get("schema")
                        .cloned()
                        .filter(|value| !value.is_null()),
                    fields: event
                        .event_json
                        .get("fields")
                        .cloned()
                        .filter(|value| !value.is_null()),
                    requested_at: event.created_at,
                });
            }
            "input.resolved" => {
                let resolved_id = event
                    .event_json
                    .get("request_id")
                    .and_then(serde_json::Value::as_str)
                    .or(event.request_id.as_deref());
                if pending
                    .as_ref()
                    .is_some_and(|request| resolved_id == Some(request.request_id.as_str()))
                {
                    pending = None;
                }
            }
            _ => {}
        }
    }
    pending
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::db::managed_agents::session_control::schema::SessionControlEventRow;

    use super::pending_input_request;

    fn event(seq: i32, event_type: &str, request_id: &str) -> SessionControlEventRow {
        SessionControlEventRow {
            id: format!("event_{seq}"),
            session_id: "session_1".to_owned(),
            turn_id: Some("turn_1".to_owned()),
            invocation_id: Some("invocation_1".to_owned()),
            request_id: Some(request_id.to_owned()),
            seq,
            event_key: format!("input:{seq}"),
            event_type: event_type.to_owned(),
            event_json: json!({
                "request_id": request_id,
                "prompt": "Select a region",
                "fields": [{"id": "region", "label": "Region", "kind": "choice", "required": true}]
            }),
            created_at: i64::from(seq),
        }
    }

    #[test]
    fn restores_the_latest_unresolved_input_request() {
        let request = pending_input_request(&[event(1, "input.requested", "input_1")], "turn_1")
            .expect("pending input");
        assert_eq!(request.request_id, "input_1");
        assert_eq!(request.prompt, "Select a region");
    }

    #[test]
    fn resolved_input_is_not_projected() {
        let events = [
            event(1, "input.requested", "input_1"),
            event(2, "input.resolved", "input_1"),
        ];
        assert!(pending_input_request(&events, "turn_1").is_none());
    }
}
