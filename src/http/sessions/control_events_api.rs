use std::{convert::Infallible, sync::Arc, time::Duration};

use axum::{
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Response,
};
use serde::Deserialize;

use crate::{db::managed_agents::session_control, errors::GatewayError, proxy::state::AppState};

use super::{
    run_types::ControlEventV1,
    storage::{auth_db, owned_session},
};

#[derive(Debug, Default, Deserialize)]
pub struct ControlEventStreamQuery {
    after_sequence: Option<i32>,
}

pub async fn control_event_stream(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Query(query): Query<ControlEventStreamQuery>,
) -> Result<Response, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    owned_session(pool, &auth, &session_id).await?;
    let header_sequence = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i32>().ok());
    let mut sequence = query
        .after_sequence
        .or(header_sequence)
        .unwrap_or_default()
        .max(0);
    let pool = pool.clone();
    let stream = async_stream::stream! {
        let mut idle_ticks = 0_u8;
        loop {
            match session_control::repository::list_events(&pool, &session_id, sequence).await {
                Ok(events) if !events.is_empty() => {
                    idle_ticks = 0;
                    for event in events {
                        sequence = event.seq;
                        let event_type = event.event_type.clone();
                        let data = serde_json::to_string(&ControlEventV1::from(event))
                            .unwrap_or_else(|_| "{}".to_owned());
                        let frame = format!(
                            "id: {}\nevent: {}\ndata: {}\n\n",
                            sequence, event_type, data
                        );
                        yield Ok::<Bytes, Infallible>(Bytes::from(frame));
                    }
                }
                Ok(_) => {
                    idle_ticks = idle_ticks.saturating_add(1);
                    if idle_ticks >= 30 {
                        idle_ticks = 0;
                        yield Ok::<Bytes, Infallible>(Bytes::from_static(b": keepalive\n\n"));
                    }
                }
                Err(error) => {
                    tracing::warn!(session_id, "control event stream stopped: {error}");
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    };
    Response::builder()
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("x-accel-buffering", "no")
        .body(Body::from_stream(stream))
        .map_err(|error| GatewayError::SandboxError(error.to_string()))
}
