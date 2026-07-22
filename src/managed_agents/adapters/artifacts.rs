use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::{
    db::managed_agents::{
        artifacts::{repository, schema::NewManagedArtifact},
        audit, id, now_ms, session_control, sessions, tasks,
    },
    object_storage::ObjectStorageClient,
};

use super::{types::ArtifactReference, AdapterError, AdapterFuture, ArtifactAdapter};

pub const MAX_ARTIFACT_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Clone)]
pub struct DatabaseArtifactAdapter {
    pool: PgPool,
    storage: Option<ObjectStorageClient>,
}

impl DatabaseArtifactAdapter {
    pub fn new(pool: PgPool, storage: Option<ObjectStorageClient>) -> Self {
        Self { pool, storage }
    }
}

impl ArtifactAdapter for DatabaseArtifactAdapter {
    fn persist<'a>(
        &'a self,
        session_id: &'a str,
        turn_id: &'a str,
        artifact: &'a ArtifactReference,
    ) -> AdapterFuture<'a, ArtifactReference> {
        Box::pin(async move {
            validate_media_type(&artifact.media_type)?;
            validate_reference(artifact)?;
            let session = sessions::repository::get(&self.pool, session_id)
                .await
                .map_err(storage_error)?
                .ok_or_else(|| AdapterError::InvalidConfiguration("unknown session".to_owned()))?;
            let turn = session_control::repository::get_turn(&self.pool, session_id, turn_id)
                .await
                .map_err(storage_error)?
                .ok_or_else(|| AdapterError::InvalidConfiguration("unknown turn".to_owned()))?;
            let invocation_id = match artifact.invocation_id.as_deref() {
                Some(invocation_id) => Some(
                    turn.invocations
                        .iter()
                        .find(|invocation| invocation.id == invocation_id)
                        .map(|invocation| invocation.id.clone())
                        .ok_or_else(|| {
                            AdapterError::InvalidConfiguration(
                                "artifact invocation does not belong to turn".to_owned(),
                            )
                        })?,
                ),
                None => turn
                    .invocations
                    .first()
                    .map(|invocation| invocation.id.clone()),
            };
            let owner_id = session
                .owner_id
                .clone()
                .unwrap_or_else(|| "system".to_owned());
            let canonical_id = id("artifact");
            let source_artifact_id = artifact
                .id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| canonical_id.clone());
            let prepared = if let Some(encoded) = artifact.data_base64.as_deref() {
                let estimated = encoded.len().saturating_mul(3) / 4;
                if estimated as u64 > MAX_ARTIFACT_BYTES {
                    return Err(AdapterError::InvalidConfiguration(
                        "artifact exceeds 20 MiB".to_owned(),
                    ));
                }
                let bytes = STANDARD
                    .decode(encoded)
                    .map_err(|error| AdapterError::Decode(format!("artifact base64: {error}")))?;
                prepare_bytes(
                    self.storage.as_ref(),
                    session_id,
                    turn_id,
                    &canonical_id,
                    artifact,
                    bytes,
                )
                .await?
            } else if let Some(uri) = artifact.uri.as_deref() {
                prepare_uri(
                    self.storage.as_ref(),
                    session_id,
                    turn_id,
                    &canonical_id,
                    artifact,
                    uri,
                )
                .await?
            } else {
                return Err(AdapterError::InvalidConfiguration(
                    "artifact requires data_base64 or uri".to_owned(),
                ));
            };
            let mut metadata = artifact.metadata.clone();
            if !metadata.is_object() {
                metadata = json!({});
            }
            if let Some(name) = artifact.name.as_deref() {
                metadata["name"] = Value::String(name.to_owned());
            }
            metadata["verification"] = Value::String(prepared.status.clone());
            let row = repository::create(
                &self.pool,
                NewManagedArtifact {
                    id: canonical_id,
                    owner_id: owner_id.clone(),
                    session_id: session_id.to_owned(),
                    turn_id: turn_id.to_owned(),
                    invocation_id,
                    task_id: session.task_id.clone(),
                    source_artifact_id,
                    media_type: artifact.media_type.clone(),
                    digest: prepared.digest.clone(),
                    size_bytes: prepared.size_bytes.map(|size| size as i64),
                    storage_backend: prepared.storage_backend,
                    object_bucket: prepared.object_bucket,
                    object_key: prepared.object_key,
                    external_uri: prepared.external_uri,
                    status: prepared.status.clone(),
                    metadata: metadata.clone(),
                    created_by: "artifact_adapter".to_owned(),
                    verified_at: (prepared.status == "verified").then_some(now_ms()),
                },
            )
            .await
            .map_err(repository_error)?;
            if let Some(task_id) = session.task_id.as_deref() {
                let location = format!(
                    "/api/sessions/{session_id}/artifacts/{artifact_id}",
                    artifact_id = row.id
                );
                let dedupe_key = format!("canonical:{}", row.id);
                tasks::artifacts::create(
                    &self.pool,
                    tasks::schema::NewArtifact {
                        task_id,
                        session_id: Some(session_id),
                        run_id: None,
                        artifact_type: "canonical_artifact",
                        name: artifact.name.as_deref().unwrap_or("Artifact"),
                        content: Some(json!({
                            "artifact_id": &row.id,
                            "media_type": &row.media_type,
                            "digest": &row.digest,
                            "size_bytes": row.size_bytes,
                            "status": &row.status,
                        })),
                        location: Some(&location),
                        dedupe_key: Some(&dedupe_key),
                        created_by: "artifact_adapter",
                    },
                )
                .await
                .map_err(storage_error)?;
                tasks::acceptance::reconcile(&self.pool, task_id)
                    .await
                    .map_err(storage_error)?;
            }
            audit::record(
                &self.pool,
                &owner_id,
                "artifact.persist",
                "managed_artifact",
                &row.id,
                json!({
                    "session_id": &row.session_id,
                    "turn_id": &row.turn_id,
                    "invocation_id": &row.invocation_id,
                    "media_type": &row.media_type,
                    "digest": &row.digest,
                    "size_bytes": row.size_bytes,
                    "status": &row.status,
                }),
            )
            .await
            .map_err(storage_error)?;
            Ok(ArtifactReference {
                id: Some(row.id.clone()),
                invocation_id: row.invocation_id.clone(),
                name: artifact.name.clone(),
                media_type: row.media_type,
                digest: row.digest,
                size_bytes: row.size_bytes.map(|size| size as u64),
                uri: Some(format!("lap-artifact://{}", row.id)),
                data_base64: None,
                metadata,
            })
        })
    }
}

struct PreparedArtifact {
    digest: Option<String>,
    size_bytes: Option<u64>,
    storage_backend: String,
    object_bucket: Option<String>,
    object_key: Option<String>,
    external_uri: Option<String>,
    status: String,
}

async fn prepare_bytes(
    storage: Option<&ObjectStorageClient>,
    session_id: &str,
    turn_id: &str,
    artifact_id: &str,
    artifact: &ArtifactReference,
    bytes: Vec<u8>,
) -> Result<PreparedArtifact, AdapterError> {
    verify_bytes(artifact, &bytes)?;
    let storage = storage.ok_or(AdapterError::Unsupported("artifact object storage"))?;
    let bucket = ObjectStorageClient::bucket_name(session_id);
    let key = format!(".lap-artifacts/{turn_id}/{artifact_id}");
    storage
        .ensure_bucket(&bucket)
        .await
        .map_err(storage_error)?;
    storage
        .put_bytes(&bucket, &key, bytes.clone())
        .await
        .map_err(storage_error)?;
    Ok(PreparedArtifact {
        digest: Some(digest(&bytes)),
        size_bytes: Some(bytes.len() as u64),
        storage_backend: "object_storage".to_owned(),
        object_bucket: Some(bucket),
        object_key: Some(key),
        external_uri: None,
        status: "verified".to_owned(),
    })
}

async fn prepare_uri(
    storage: Option<&ObjectStorageClient>,
    session_id: &str,
    turn_id: &str,
    artifact_id: &str,
    artifact: &ArtifactReference,
    uri: &str,
) -> Result<PreparedArtifact, AdapterError> {
    if uri.len() > 4096 {
        return Err(AdapterError::InvalidConfiguration(
            "artifact URI exceeds 4096 characters".to_owned(),
        ));
    }
    let parsed = reqwest::Url::parse(uri)
        .map_err(|_| AdapterError::InvalidConfiguration("artifact URI is invalid".to_owned()))?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AdapterError::InvalidConfiguration(
            "artifact URI must not contain credentials".to_owned(),
        ));
    }
    if parsed.scheme() == "s3" {
        let storage = storage.ok_or(AdapterError::Unsupported("artifact object storage"))?;
        let bucket = parsed.host_str().unwrap_or_default();
        let expected_bucket = ObjectStorageClient::bucket_name(session_id);
        let key = parsed.path().trim_start_matches('/');
        if bucket != expected_bucket || !valid_object_key(key) {
            return Err(AdapterError::InvalidConfiguration(
                "artifact object must belong to the session bucket".to_owned(),
            ));
        }
        let meta = storage
            .object_meta(bucket, key)
            .await
            .map_err(storage_error)?;
        if meta.size < 0 || meta.size as u64 > MAX_ARTIFACT_BYTES {
            return Err(AdapterError::InvalidConfiguration(
                "artifact exceeds 20 MiB".to_owned(),
            ));
        }
        let bytes = storage
            .get_bytes(bucket, key)
            .await
            .map_err(storage_error)?;
        verify_bytes(artifact, &bytes)?;
        return prepare_bytes(
            Some(storage),
            session_id,
            turn_id,
            artifact_id,
            artifact,
            bytes,
        )
        .await;
    }
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AdapterError::Unsupported("artifact URI scheme"));
    }
    validate_claims(artifact)?;
    Ok(PreparedArtifact {
        digest: artifact.digest.as_deref().map(str::to_ascii_lowercase),
        size_bytes: artifact.size_bytes,
        storage_backend: "external_reference".to_owned(),
        object_bucket: None,
        object_key: None,
        external_uri: Some(parsed.to_string()),
        status: "unverified_external".to_owned(),
    })
}

fn verify_bytes(artifact: &ArtifactReference, bytes: &[u8]) -> Result<(), AdapterError> {
    if bytes.len() as u64 > MAX_ARTIFACT_BYTES {
        return Err(AdapterError::InvalidConfiguration(
            "artifact exceeds 20 MiB".to_owned(),
        ));
    }
    validate_claims(artifact)?;
    let actual_digest = digest(bytes);
    if artifact
        .digest
        .as_deref()
        .is_some_and(|claimed| !claimed.eq_ignore_ascii_case(&actual_digest))
    {
        return Err(AdapterError::InvalidConfiguration(
            "artifact digest does not match content".to_owned(),
        ));
    }
    if artifact
        .size_bytes
        .is_some_and(|claimed| claimed != bytes.len() as u64)
    {
        return Err(AdapterError::InvalidConfiguration(
            "artifact size does not match content".to_owned(),
        ));
    }
    Ok(())
}

fn validate_claims(artifact: &ArtifactReference) -> Result<(), AdapterError> {
    if artifact
        .size_bytes
        .is_some_and(|size| size > MAX_ARTIFACT_BYTES)
    {
        return Err(AdapterError::InvalidConfiguration(
            "artifact exceeds 20 MiB".to_owned(),
        ));
    }
    if artifact.digest.as_deref().is_some_and(|digest| {
        digest.len() != 71
            || !digest.starts_with("sha256:")
            || !digest[7..]
                .chars()
                .all(|character| character.is_ascii_hexdigit())
    }) {
        return Err(AdapterError::InvalidConfiguration(
            "artifact digest must be sha256:<64 hex characters>".to_owned(),
        ));
    }
    Ok(())
}

fn validate_media_type(media_type: &str) -> Result<(), AdapterError> {
    let media_type = media_type.trim();
    if media_type.is_empty()
        || media_type.len() > 255
        || !media_type.contains('/')
        || media_type.chars().any(char::is_whitespace)
    {
        return Err(AdapterError::InvalidConfiguration(
            "artifact media_type is invalid".to_owned(),
        ));
    }
    Ok(())
}

fn validate_reference(artifact: &ArtifactReference) -> Result<(), AdapterError> {
    if artifact
        .id
        .as_deref()
        .is_some_and(|id| id.trim().is_empty() || id.len() > 255)
    {
        return Err(AdapterError::InvalidConfiguration(
            "artifact id is empty or exceeds 255 characters".to_owned(),
        ));
    }
    if artifact
        .name
        .as_deref()
        .is_some_and(|name| name.trim().is_empty() || name.len() > 512)
    {
        return Err(AdapterError::InvalidConfiguration(
            "artifact name is empty or exceeds 512 characters".to_owned(),
        ));
    }
    let metadata_size = serde_json::to_vec(&artifact.metadata)
        .map_err(|error| AdapterError::Decode(format!("artifact metadata: {error}")))?
        .len();
    if metadata_size > 64 * 1024 {
        return Err(AdapterError::InvalidConfiguration(
            "artifact metadata exceeds 64 KiB".to_owned(),
        ));
    }
    Ok(())
}

fn valid_object_key(key: &str) -> bool {
    !key.is_empty()
        && !key.starts_with('/')
        && !key
            .split('/')
            .any(|segment| matches!(segment, "" | "." | ".."))
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn storage_error(error: crate::errors::GatewayError) -> AdapterError {
    AdapterError::Storage(error.to_string())
}

fn repository_error(error: crate::errors::GatewayError) -> AdapterError {
    match error {
        crate::errors::GatewayError::BadRequest(message) => {
            AdapterError::InvalidConfiguration(message)
        }
        error => storage_error(error),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{digest, verify_bytes};
    use crate::managed_agents::adapters::types::ArtifactReference;

    #[test]
    fn verifies_digest_and_size_claims() {
        let bytes = b"threat assessment";
        let artifact = ArtifactReference {
            id: Some("source-1".to_owned()),
            invocation_id: None,
            name: Some("assessment.txt".to_owned()),
            media_type: "text/plain".to_owned(),
            digest: Some(digest(bytes)),
            size_bytes: Some(bytes.len() as u64),
            uri: None,
            data_base64: None,
            metadata: json!({}),
        };
        verify_bytes(&artifact, bytes).expect("valid artifact");
        assert!(verify_bytes(&artifact, b"tampered").is_err());
    }
}
