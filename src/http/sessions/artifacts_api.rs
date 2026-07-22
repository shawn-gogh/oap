use std::{sync::Arc, time::Duration};

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    db::managed_agents::{
        artifacts::{repository, schema::ManagedArtifactRow},
        session_control,
    },
    errors::GatewayError,
    managed_agents::adapters::{
        artifacts::DatabaseArtifactAdapter, types::ArtifactReference, AdapterError, ArtifactAdapter,
    },
    proxy::state::AppState,
};

use super::{auth_db, owned_session};

const DOWNLOAD_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Deserialize)]
pub struct ArtifactListQuery {
    pub turn_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ArtifactResponse {
    pub id: String,
    pub session_id: String,
    pub turn_id: String,
    pub invocation_id: Option<String>,
    pub task_id: Option<String>,
    pub source_artifact_id: String,
    pub media_type: String,
    pub digest: Option<String>,
    pub size_bytes: Option<i64>,
    pub status: String,
    pub metadata: Value,
    pub created_at: i64,
    pub verified_at: Option<i64>,
    /// Short-lived platform-issued URL. Only available for verified objects.
    pub download_url: Option<String>,
    /// Caller-supplied reference. It is never fetched or vouched for by LAP.
    pub external_reference_url: Option<String>,
}

pub async fn create_artifact(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((session_id, turn_id)): Path<(String, String)>,
    Json(input): Json<ArtifactReference>,
) -> Result<(StatusCode, Json<ArtifactResponse>), GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    owned_session(pool, &auth, &session_id).await?;
    let adapter = DatabaseArtifactAdapter::new(pool.clone(), state.object_storage.clone());
    let persisted = adapter
        .persist(&session_id, &turn_id, &input)
        .await
        .map_err(adapter_error)?;
    let artifact_id = persisted
        .id
        .ok_or_else(|| GatewayError::SandboxError("artifact adapter returned no id".to_owned()))?;
    let row = repository::get(pool, &session_id, &artifact_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("artifact not found".to_owned()))?;
    session_control::repository::append_event(
        pool,
        session_control::repository::NewControlEvent {
            session_id: &session_id,
            turn_id: Some(&turn_id),
            invocation_id: row.invocation_id.as_deref(),
            request_id: None,
            event_key: &format!("turn:{turn_id}:artifact:{}", row.id),
            event_type: "artifact.available",
            event: serde_json::json!({
                "schema_version": 1,
                "artifact": row.clone(),
            }),
        },
    )
    .await?;
    let response = response(&state, row).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn list_artifacts(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Query(query): Query<ArtifactListQuery>,
) -> Result<Json<Vec<ArtifactResponse>>, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    owned_session(pool, &auth, &session_id).await?;
    let rows = repository::list(pool, &session_id, query.turn_id.as_deref()).await?;
    let mut artifacts = Vec::with_capacity(rows.len());
    for row in rows {
        artifacts.push(response(&state, row).await?);
    }
    Ok(Json(artifacts))
}

pub async fn get_artifact(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((session_id, artifact_id)): Path<(String, String)>,
) -> Result<Json<ArtifactResponse>, GatewayError> {
    let (pool, auth) = auth_db(&state, &headers).await?;
    owned_session(pool, &auth, &session_id).await?;
    let row = repository::get(pool, &session_id, &artifact_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound("artifact not found".to_owned()))?;
    Ok(Json(response(&state, row).await?))
}

async fn response(
    state: &AppState,
    row: ManagedArtifactRow,
) -> Result<ArtifactResponse, GatewayError> {
    let download_url = match (
        row.status.as_str(),
        state.object_storage.as_ref(),
        row.object_bucket.as_deref(),
        row.object_key.as_deref(),
    ) {
        ("verified", Some(storage), Some(bucket), Some(key)) => {
            Some(storage.presign_get(bucket, key, DOWNLOAD_TTL).await?)
        }
        _ => None,
    };
    let external_reference_url = (row.status == "unverified_external")
        .then_some(row.external_uri)
        .flatten();
    Ok(ArtifactResponse {
        id: row.id,
        session_id: row.session_id,
        turn_id: row.turn_id,
        invocation_id: row.invocation_id,
        task_id: row.task_id,
        source_artifact_id: row.source_artifact_id,
        media_type: row.media_type,
        digest: row.digest,
        size_bytes: row.size_bytes,
        status: row.status,
        metadata: row.metadata,
        created_at: row.created_at,
        verified_at: row.verified_at,
        download_url,
        external_reference_url,
    })
}

fn adapter_error(error: AdapterError) -> GatewayError {
    match error {
        AdapterError::InvalidConfiguration(message)
        | AdapterError::Decode(message)
        | AdapterError::UnmappedIdentity(message)
        | AdapterError::BlockedIdentity(message) => GatewayError::BadRequest(message),
        AdapterError::Unsupported(feature) => {
            GatewayError::BadRequest(format!("unsupported artifact feature: {feature}"))
        }
        AdapterError::Authentication => GatewayError::Unauthorized,
        AdapterError::Transport(message)
        | AdapterError::StateUnknown(message)
        | AdapterError::Storage(message) => GatewayError::SandboxError(message),
    }
}
