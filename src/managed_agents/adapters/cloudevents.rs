use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::db::managed_agents::{now_ms, runtime_events::schema::RuntimeEventRow};

use super::{
    types::{CanonicalRuntimeEvent, CloudEventEnvelope, EventNormalizationContext},
    AdapterError, AdapterFuture, EventAdapter,
};

#[derive(Default)]
pub struct CloudEventsAdapter;

impl EventAdapter for CloudEventsAdapter {
    fn normalize<'a>(
        &'a self,
        context: &'a EventNormalizationContext,
        raw: &'a Value,
    ) -> AdapterFuture<'a, Vec<CanonicalRuntimeEvent>> {
        Box::pin(async move {
            let envelope = serde_json::from_value::<CloudEventEnvelope>(raw.clone())
                .map_err(|error| AdapterError::Decode(format!("CloudEvent: {error}")))?;
            validate(&envelope)?;
            let event_key = canonical_event_key(&envelope);
            let occurred_at = envelope
                .time
                .as_deref()
                .and_then(|time| DateTime::parse_from_rfc3339(time).ok())
                .map(|time| time.timestamp_millis())
                .unwrap_or_else(now_ms);
            Ok(vec![CanonicalRuntimeEvent {
                event_key,
                event_type: "external.event.received".to_owned(),
                provider_sequence: context.provider_sequence.clone(),
                resume_cursor: context.resume_cursor.clone(),
                payload: json!({
                    "cloud_event_type": envelope.event_type,
                    "cloud_event_source": envelope.source,
                    "subject": envelope.subject,
                    "data": envelope.data,
                    "session_id": context.session_id,
                    "turn_id": context.turn_id,
                    "invocation_id": context.invocation_id,
                    "request_id": context.request_id,
                }),
                raw: raw.clone(),
                occurred_at,
            }])
        })
    }
}

pub fn validate(envelope: &CloudEventEnvelope) -> Result<(), AdapterError> {
    if envelope.specversion != "1.0" {
        return Err(AdapterError::InvalidConfiguration(
            "CloudEvents specversion must be 1.0".to_owned(),
        ));
    }
    for (name, value, max_len) in [
        ("id", envelope.id.trim(), 512),
        ("source", envelope.source.trim(), 1024),
        ("type", envelope.event_type.trim(), 512),
    ] {
        if value.is_empty() || value.len() > max_len || value.chars().any(char::is_whitespace) {
            return Err(AdapterError::InvalidConfiguration(format!(
                "CloudEvent {name} is invalid"
            )));
        }
    }
    if !envelope
        .datacontenttype
        .to_ascii_lowercase()
        .contains("json")
    {
        return Err(AdapterError::Unsupported(
            "non-JSON CloudEvent data content type",
        ));
    }
    if let Some(time) = envelope.time.as_deref() {
        DateTime::parse_from_rfc3339(time).map_err(|_| {
            AdapterError::InvalidConfiguration("CloudEvent time must be RFC 3339".to_owned())
        })?;
    }
    Ok(())
}

pub fn canonical_event_key(envelope: &CloudEventEnvelope) -> String {
    let source_digest = Sha256::digest(envelope.source.as_bytes());
    format!("cloudevent:{source_digest:x}:{}", envelope.id)
}

pub fn envelope_digest(envelope: &CloudEventEnvelope) -> Result<String, AdapterError> {
    let encoded = serde_json::to_vec(envelope)
        .map_err(|error| AdapterError::Decode(format!("CloudEvent: {error}")))?;
    Ok(format!("sha256:{:x}", Sha256::digest(encoded)))
}

pub fn project(session_id: &str, row: &RuntimeEventRow) -> CloudEventEnvelope {
    let time = DateTime::<Utc>::from_timestamp_millis(row.created_at)
        .map(|time| time.to_rfc3339_opts(SecondsFormat::Millis, true));
    CloudEventEnvelope {
        specversion: "1.0".to_owned(),
        id: row.id.clone(),
        source: format!("/lap/sessions/{session_id}"),
        event_type: format!("io.lap.runtime.{}", event_type_segment(&row.event_type)),
        subject: Some(format!("session/{session_id}")),
        time,
        datacontenttype: "application/json".to_owned(),
        data: row.event_json.clone(),
        extensions: Default::default(),
    }
}

fn event_type_segment(event_type: &str) -> String {
    event_type
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '.'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{canonical_event_key, envelope_digest, validate};
    use crate::managed_agents::adapters::types::CloudEventEnvelope;

    fn event() -> CloudEventEnvelope {
        CloudEventEnvelope {
            specversion: "1.0".to_owned(),
            id: "delivery-1".to_owned(),
            source: "urn:example:runtime".to_owned(),
            event_type: "com.example.agent.completed".to_owned(),
            subject: Some("task/1".to_owned()),
            time: Some("2026-07-16T12:00:00Z".to_owned()),
            datacontenttype: "application/json".to_owned(),
            data: json!({"result": "ok"}),
            extensions: Default::default(),
        }
    }

    #[test]
    fn validates_json_cloud_event_and_builds_stable_keys() {
        let event = event();
        validate(&event).expect("valid event");
        assert_eq!(canonical_event_key(&event), canonical_event_key(&event));
        assert_eq!(
            envelope_digest(&event).expect("digest"),
            envelope_digest(&event).expect("digest")
        );
    }

    #[test]
    fn rejects_unsupported_versions_and_non_json_profiles() {
        let mut event = event();
        event.specversion = "0.3".to_owned();
        assert!(validate(&event).is_err());
        event.specversion = "1.0".to_owned();
        event.datacontenttype = "application/octet-stream".to_owned();
        assert!(validate(&event).is_err());
    }
}
