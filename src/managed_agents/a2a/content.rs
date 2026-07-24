use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};

use crate::managed_agents::adapters::types::ArtifactReference;

#[derive(Debug, Clone)]
pub struct A2aNormalizedResult {
    pub text: String,
    pub artifacts: Vec<ArtifactReference>,
    pub raw: Value,
}

pub fn input_parts(input: &Value, fallback: &str) -> Vec<Value> {
    let mut parts = Vec::new();
    let content = input
        .get("content")
        .or_else(|| input.get("parts"))
        .and_then(Value::as_array);
    if let Some(content) = content {
        for item in content {
            if let Some(part) = input_part(item) {
                parts.push(part);
            }
        }
    }
    if parts.is_empty() {
        let text = input
            .get("message")
            .or_else(|| input.get("text"))
            .and_then(Value::as_str)
            .unwrap_or(fallback);
        parts.push(Value::String(text.to_owned()));
    }
    parts
}

fn input_part(value: &Value) -> Option<Value> {
    if let Some(text) = value.as_str() {
        return Some(Value::String(text.to_owned()));
    }
    let kind = value.get("type").and_then(Value::as_str);
    if matches!(kind, Some("text") | Some("input_text")) || value.get("text").is_some() {
        return value
            .get("text")
            .and_then(Value::as_str)
            .map(|text| Value::String(text.to_owned()));
    }
    if matches!(kind, Some("data") | Some("json")) || value.get("data").is_some() {
        return Some(json!({"data": value.get("data").cloned().unwrap_or(Value::Null)}));
    }
    if matches!(
        kind,
        Some("file") | Some("image") | Some("audio") | Some("video") | Some("document")
    ) || value.get("file").is_some()
    {
        let file = value.get("file").unwrap_or(value);
        let mut normalized = serde_json::Map::new();
        for (target, candidates) in [
            ("name", &["name", "filename"][..]),
            ("mediaType", &["mediaType", "mimeType", "media_type"][..]),
            ("bytes", &["bytes", "data_base64"][..]),
            ("uri", &["uri", "url"][..]),
        ] {
            if let Some(found) = candidates.iter().find_map(|key| file.get(*key)).cloned() {
                normalized.insert(target.to_owned(), found);
            }
        }
        if normalized.contains_key("bytes") || normalized.contains_key("uri") {
            return Some(json!({"file": normalized}));
        }
    }
    None
}

pub fn normalize_result(value: Value) -> A2aNormalizedResult {
    let mut texts = Vec::new();
    collect_text(&value, &mut texts);
    let mut artifacts = Vec::new();
    collect_artifacts(&value, &mut artifacts);
    A2aNormalizedResult {
        text: texts.join("\n"),
        artifacts,
        raw: value,
    }
}

fn collect_text(value: &Value, texts: &mut Vec<String>) {
    if let Some(text) = value.get("text").and_then(Value::as_str) {
        if !text.is_empty() {
            texts.push(text.to_owned());
        }
    }
    if let Some(parts) = value.get("parts").and_then(Value::as_array) {
        for part in parts {
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                if !text.is_empty() {
                    texts.push(text.to_owned());
                }
            }
        }
    }
    for field in ["message", "status", "history"] {
        if let Some(child) = value.get(field) {
            if let Some(items) = child.as_array() {
                for item in items {
                    collect_text(item, texts);
                }
            } else {
                collect_text(child, texts);
            }
        }
    }
    if texts.is_empty() {
        if let Some(artifacts) = value.get("artifacts").and_then(Value::as_array) {
            for artifact in artifacts {
                collect_text(artifact, texts);
            }
        }
    }
}

fn collect_artifacts(value: &Value, artifacts: &mut Vec<ArtifactReference>) {
    if value.get("parts").is_some()
        && (value.get("messageId").is_some() || value.get("role").is_some())
    {
        artifact_parts(
            &json!({
                "artifactId": value
                    .get("messageId")
                    .cloned()
                    .unwrap_or_else(|| Value::String("a2a-message".to_owned())),
                "name": "A2A message attachment",
                "parts": value.get("parts").cloned().unwrap_or_else(|| json!([])),
            }),
            0,
            artifacts,
        );
    }
    if let Some(items) = value.get("artifacts").and_then(Value::as_array) {
        for (artifact_index, artifact) in items.iter().enumerate() {
            artifact_parts(artifact, artifact_index, artifacts);
        }
    }
    for field in ["message", "status", "history"] {
        if let Some(child) = value.get(field) {
            if let Some(items) = child.as_array() {
                for item in items {
                    collect_artifacts(item, artifacts);
                }
            } else {
                collect_artifacts(child, artifacts);
            }
        }
    }
}

fn artifact_parts(artifact: &Value, artifact_index: usize, artifacts: &mut Vec<ArtifactReference>) {
    let source_id = artifact
        .get("artifactId")
        .or_else(|| artifact.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("a2a-artifact-{artifact_index}"));
    let name = artifact
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let Some(parts) = artifact.get("parts").and_then(Value::as_array) else {
        return;
    };
    for (part_index, part) in parts.iter().enumerate() {
        let id = (parts.len() == 1)
            .then_some(source_id.clone())
            .unwrap_or_else(|| format!("{source_id}-part-{part_index}"));
        if let Some(reference) = artifact_part(id, name.clone(), part, artifact) {
            artifacts.push(reference);
        }
    }
}

fn artifact_part(
    id: String,
    name: Option<String>,
    part: &Value,
    artifact: &Value,
) -> Option<ArtifactReference> {
    let metadata = json!({"provider": "a2a", "artifact": artifact, "part": part});
    if let Some(file) = part.get("file") {
        return Some(ArtifactReference {
            id: Some(id),
            invocation_id: None,
            name: file
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or(name),
            media_type: file
                .get("mediaType")
                .or_else(|| file.get("mimeType"))
                .and_then(Value::as_str)
                .unwrap_or("application/octet-stream")
                .to_owned(),
            digest: file
                .get("digest")
                .and_then(Value::as_str)
                .map(str::to_owned),
            size_bytes: file.get("sizeBytes").and_then(Value::as_u64),
            uri: file.get("uri").and_then(Value::as_str).map(str::to_owned),
            data_base64: file.get("bytes").and_then(Value::as_str).map(str::to_owned),
            metadata,
        });
    }
    if let Some(data) = part.get("data") {
        let bytes = serde_json::to_vec(data).ok()?;
        return Some(ArtifactReference {
            id: Some(id),
            invocation_id: None,
            name,
            media_type: "application/json".to_owned(),
            digest: None,
            size_bytes: Some(bytes.len() as u64),
            uri: None,
            data_base64: Some(STANDARD.encode(bytes)),
            metadata,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_rich_input_parts() {
        let parts = input_parts(
            &json!({"content": [
                {"type": "text", "text": "inspect"},
                {"type": "data", "data": {"priority": 1}},
                {"type": "file", "name": "report.pdf", "url": "https://files.example/report.pdf"}
            ]}),
            "fallback",
        );
        assert_eq!(parts[0], "inspect");
        assert_eq!(parts[1]["data"]["priority"], 1);
        assert_eq!(parts[2]["file"]["mediaType"], Value::Null);
        assert_eq!(parts[2]["file"]["uri"], "https://files.example/report.pdf");
    }

    #[test]
    fn normalizes_text_and_artifact_parts_without_flattening_raw_evidence() {
        let raw = json!({
            "id": "task-1",
            "status": {"state": "completed"},
            "artifacts": [{
                "artifactId": "report",
                "name": "report.json",
                "parts": [
                    {"text": "done"},
                    {"data": {"answer": 42}},
                    {"file": {"name": "source.txt", "mediaType": "text/plain", "bytes": "aGk="}}
                ]
            }]
        });
        let result = normalize_result(raw.clone());
        assert_eq!(result.text, "done");
        assert_eq!(result.artifacts.len(), 2);
        assert_eq!(result.artifacts[0].media_type, "application/json");
        assert_eq!(result.artifacts[1].data_base64.as_deref(), Some("aGk="));
        assert_eq!(result.raw, raw);
    }
}
