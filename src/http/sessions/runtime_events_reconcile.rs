use serde_json::Value;
use sqlx::PgPool;

use crate::{db::managed_agents::runtime_events, errors::GatewayError, proxy::state::AppState};

use super::runtime_lifecycle::mark_session_status;

pub(super) async fn reconcile_terminal_status_from_events(
    state: &AppState,
    pool: &PgPool,
    session_id: &str,
    current_status: &str,
    events: &Value,
) -> Result<(), GatewayError> {
    let (terminal_status, terminal_error) = terminal_status_from_event_values(events);
    if let Some(status) = terminal_status {
        if current_status != status {
            mark_session_status(state, pool, session_id, status, terminal_error).await?;
        }
    }
    Ok(())
}

pub(super) async fn persist_runtime_event_values(
    pool: &PgPool,
    session_id: &str,
    events: &Value,
) -> Result<(), GatewayError> {
    let Some(items) = event_items(events) else {
        return Ok(());
    };
    runtime_events::repository::append_batch(pool, session_id, items.clone()).await?;
    Ok(())
}

pub(super) fn event_items(events: &Value) -> Option<&Vec<Value>> {
    events
        .as_array()
        .or_else(|| events.get("data").and_then(Value::as_array))
}

fn terminal_status_from_event_values(events: &Value) -> (Option<&'static str>, Option<String>) {
    let mut terminal_status = None;
    let mut terminal_error = None;
    let Some(items) = event_items(events) else {
        return (None, None);
    };
    for event in items {
        match event.get("type").and_then(Value::as_str) {
            Some("session.status_running") => {
                terminal_status = None;
                terminal_error = None;
            }
            Some("session.status_idle") => {
                terminal_status = Some("idle");
                terminal_error = None;
            }
            Some("session.error") => {
                terminal_status = Some("error");
                terminal_error = Some(event_value_error_message(event));
            }
            // Generic status event carrying the state in its payload.
            Some("session.status") => match event_value_status(event) {
                Some("busy") | Some("running") => {
                    terminal_status = None;
                    terminal_error = None;
                }
                Some("idle") => {
                    terminal_status = Some("idle");
                    terminal_error = None;
                }
                _ => {}
            },
            // Conversation activity after the last status marker means a new
            // turn is underway: the replayed history ends with the PREVIOUS
            // turn's idle, and treating that as terminal flipped busy sessions
            // back to idle on every poll.
            Some(event_type)
                if event_type.starts_with("user.") || event_type.starts_with("agent.") =>
            {
                terminal_status = None;
                terminal_error = None;
            }
            _ => {}
        }
    }
    (terminal_status, terminal_error)
}

fn event_value_status(event: &Value) -> Option<&str> {
    let status = event.get("status")?;
    status
        .as_str()
        .or_else(|| status.get("type").and_then(Value::as_str))
}

fn event_value_error_message(event: &Value) -> String {
    event
        .get("error")
        .and_then(|error| {
            error
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| error.as_str())
        })
        .unwrap_or("managed agent interaction failed")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::terminal_status_from_event_values;

    #[test]
    fn terminal_status_from_event_list_values() {
        let (status, error) = terminal_status_from_event_values(&json!({
            "data": [{ "type": "session.error", "error": { "message": "boom" } }]
        }));
        assert_eq!(status, Some("error"));
        assert_eq!(error.as_deref(), Some("boom"));

        let (status, error) = terminal_status_from_event_values(&json!([
            { "type": "session.status_running" },
            { "type": "session.status_idle" }
        ]));
        assert_eq!(status, Some("idle"));
        assert_eq!(error, None);
    }

    #[test]
    fn running_event_clears_stale_terminal_status() {
        let (status, error) = terminal_status_from_event_values(&json!([
            { "type": "session.status_idle" },
            { "type": "session.status_running" }
        ]));
        assert_eq!(status, None);
        assert_eq!(error, None);
    }

    #[test]
    fn conversation_activity_after_idle_means_running() {
        let (status, _) = terminal_status_from_event_values(&json!([
            { "type": "session.status_idle" },
            { "type": "user.message" },
            { "type": "agent.tool_use" }
        ]));
        assert_eq!(status, None);
    }

    #[test]
    fn idle_after_activity_is_still_terminal() {
        let (status, _) = terminal_status_from_event_values(&json!([
            { "type": "user.message" },
            { "type": "agent.message" },
            { "type": "session.status_idle" }
        ]));
        assert_eq!(status, Some("idle"));
    }

    #[test]
    fn generic_status_event_payload_is_honored() {
        let (status, _) = terminal_status_from_event_values(&json!([
            { "type": "session.status_idle" },
            { "type": "session.status", "status": { "type": "busy" } }
        ]));
        assert_eq!(status, None);

        let (status, _) = terminal_status_from_event_values(&json!([
            { "type": "session.status", "status": "idle" }
        ]));
        assert_eq!(status, Some("idle"));
    }
}
