use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header::CONTENT_TYPE, HeaderMap, HeaderValue, StatusCode},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    db::managed_agents::{audit, cloud_events, runtime_events, session_control},
    errors::GatewayError,
    managed_agents::adapters::{
        cloudevents::{
            canonical_event_key, envelope_digest, project, validate, CloudEventsAdapter,
        },
        types::{CloudEventEnvelope, EventNormalizationContext},
        EventAdapter,
    },
    proxy::state::AppState,
};

use super::storage::{auth_db, owned_session};

#[derive(Debug, Default, Deserialize)]
pub struct CloudEventListQuery {
    after_sequence: Option<i32>,
    limit: Option<i64>,
}

pub async fn ingress(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>), GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    owned_session(pool, &auth, &session_id).await?;
    let envelope = parse_http_event(&headers, &body)?;
    validate(&envelope).map_err(adapter_error)?;
    let digest = envelope_digest(&envelope).map_err(adapter_error)?;
    let event_key = canonical_event_key(&envelope);
    let receipt = cloud_events::repository::record(
        pool,
        cloud_events::repository::RecordCloudEvent {
            direction: "ingress",
            session_id: &session_id,
            cloud_event_id: &envelope.id,
            cloud_event_source: &envelope.source,
            cloud_event_type: &envelope.event_type,
            subject: envelope.subject.as_deref(),
            data_digest: &digest,
            canonical_event_key: &event_key,
            actor_user_id: &auth.user_id,
        },
    )
    .await?;

    let snapshot = session_control::repository::active_turn(pool, &session_id).await?;
    let context = snapshot
        .as_ref()
        .map(|snapshot| EventNormalizationContext {
            session_id: session_id.clone(),
            turn_id: snapshot.turn.id.clone(),
            invocation_id: snapshot
                .invocations
                .first()
                .map(|invocation| invocation.id.clone())
                .unwrap_or_default(),
            request_id: snapshot.turn.request_id.clone(),
            provider_sequence: None,
            resume_cursor: None,
        })
        .unwrap_or_else(|| EventNormalizationContext {
            session_id: session_id.clone(),
            turn_id: String::new(),
            invocation_id: String::new(),
            request_id: String::new(),
            provider_sequence: None,
            resume_cursor: None,
        });
    let raw = serde_json::to_value(&envelope)?;
    let normalized = CloudEventsAdapter
        .normalize(&context, &raw)
        .await
        .map_err(adapter_error)?
        .into_iter()
        .next()
        .ok_or_else(|| GatewayError::BadRequest("CloudEvent 未生成规范事件。".to_owned()))?;
    let runtime_event = json!({
        "id": normalized.event_key,
        "type": normalized.event_type,
        "sessionID": session_id,
        "payload": normalized.payload,
        "raw": normalized.raw,
        "time": { "created": normalized.occurred_at },
    });
    let row = runtime_events::repository::append(pool, &session_id, runtime_event.clone()).await?;
    state
        .local_session_events
        .publish(&session_id, runtime_event);
    audit::record(
        pool,
        &auth.user_id,
        "cloudevent.ingress",
        "session",
        &session_id,
        json!({
            "cloud_event_id": envelope.id,
            "cloud_event_source": envelope.source,
            "cloud_event_type": envelope.event_type,
            "data_digest": digest,
            "duplicate": receipt.duplicate,
            "receipt_id": receipt.row.id,
        }),
    )
    .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "receipt_id": receipt.row.id,
            "event_id": row.id,
            "duplicate": receipt.duplicate,
        })),
    ))
}

pub async fn egress(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Query(query): Query<CloudEventListQuery>,
) -> Result<(HeaderMap, Json<Vec<CloudEventEnvelope>>), GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    owned_session(pool, &auth, &session_id).await?;
    let rows = runtime_events::repository::list_rows_after(
        pool,
        &session_id,
        query.after_sequence.unwrap_or_default(),
        query.limit.unwrap_or(100),
    )
    .await?;
    let mut events = Vec::with_capacity(rows.len());
    for row in &rows {
        let envelope = project(&session_id, row);
        let digest = envelope_digest(&envelope).map_err(adapter_error)?;
        cloud_events::repository::record(
            pool,
            cloud_events::repository::RecordCloudEvent {
                direction: "egress",
                session_id: &session_id,
                cloud_event_id: &envelope.id,
                cloud_event_source: &envelope.source,
                cloud_event_type: &envelope.event_type,
                subject: envelope.subject.as_deref(),
                data_digest: &digest,
                canonical_event_key: &row.event_key,
                actor_user_id: &auth.user_id,
            },
        )
        .await?;
        events.push(envelope);
    }
    audit::record(
        pool,
        &auth.user_id,
        "cloudevent.egress",
        "session",
        &session_id,
        json!({
            "after_sequence": query.after_sequence.unwrap_or_default(),
            "count": events.len(),
        }),
    )
    .await?;
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/cloudevents-batch+json"),
    );
    Ok((response_headers, Json(events)))
}

fn parse_http_event(headers: &HeaderMap, body: &[u8]) -> Result<CloudEventEnvelope, GatewayError> {
    let content_type = header(headers, "content-type").unwrap_or("application/json");
    if content_type
        .to_ascii_lowercase()
        .starts_with("application/cloudevents+json")
    {
        return serde_json::from_slice(body).map_err(GatewayError::InvalidJson);
    }
    let specversion = required_header(headers, "ce-specversion")?;
    let id = required_header(headers, "ce-id")?;
    let source = required_header(headers, "ce-source")?;
    let event_type = required_header(headers, "ce-type")?;
    if !content_type.to_ascii_lowercase().contains("json") {
        return Err(GatewayError::BadRequest(
            "当前 CloudEvents HTTP 二进制模式仅支持 JSON data。".to_owned(),
        ));
    }
    let data = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(body)?
    };
    let extensions = headers
        .iter()
        .filter_map(|(name, value)| {
            let name = name.as_str().strip_prefix("ce-")?;
            if matches!(
                name,
                "specversion" | "id" | "source" | "type" | "subject" | "time"
            ) {
                return None;
            }
            value
                .to_str()
                .ok()
                .map(|value| (name.to_owned(), Value::String(value.to_owned())))
        })
        .collect();
    Ok(CloudEventEnvelope {
        specversion,
        id,
        source,
        event_type,
        subject: header(headers, "ce-subject").map(str::to_owned),
        time: header(headers, "ce-time").map(str::to_owned),
        datacontenttype: content_type.to_owned(),
        data,
        extensions,
    })
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn required_header(headers: &HeaderMap, name: &str) -> Result<String, GatewayError> {
    header(headers, name)
        .map(str::to_owned)
        .ok_or_else(|| GatewayError::BadRequest(format!("缺少 CloudEvent HTTP 头 {name}。")))
}

fn adapter_error(error: crate::managed_agents::adapters::AdapterError) -> GatewayError {
    GatewayError::BadRequest(error.to_string())
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};
    use serde_json::json;

    use super::parse_http_event;

    #[test]
    fn parses_binary_json_cloud_event() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        headers.insert("ce-specversion", HeaderValue::from_static("1.0"));
        headers.insert("ce-id", HeaderValue::from_static("event-1"));
        headers.insert("ce-source", HeaderValue::from_static("urn:test"));
        headers.insert("ce-type", HeaderValue::from_static("com.test.finished"));
        let event = parse_http_event(&headers, br#"{"ok":true}"#).expect("event");
        assert_eq!(event.data, json!({"ok": true}));
    }
}
