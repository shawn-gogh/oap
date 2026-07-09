//! Agent-level workspace: a MinIO bucket of knowledge/template files that is
//! copied into every new session workspace. Mirrors the per-session workspace
//! API (src/http/sessions/workspace_api.rs) — presigned URLs only, the
//! gateway never proxies file bytes.

use std::{sync::Arc, time::Duration};

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    db::managed_agents::registry::repository,
    errors::GatewayError,
    object_storage::ObjectStorageClient,
    proxy::{
        auth::master_key::authenticate,
        state::AppState,
    },
};

const PRESIGN_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Serialize)]
pub struct WorkspaceFileResponse {
    pub path: String,
    pub size_bytes: i64,
    pub updated_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct PathQuery {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct UploadUrlRequest {
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct PresignedUrlResponse {
    pub url: String,
    pub path: String,
}

use super::assert_agent_access;

async fn agent_workspace_bucket(
    state: &AppState,
    headers: &HeaderMap,
    agent_id: &str,
) -> Result<(String, ObjectStorageClient), GatewayError> {
    let auth = authenticate(headers, state).await?;
    let pool = state.db.as_ref().ok_or(GatewayError::MissingDatabase)?;
    let agent = repository::get(pool, agent_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("agent {agent_id}")))?;
    assert_agent_access(&auth, &agent)?;
    let storage = state
        .object_storage
        .clone()
        .ok_or_else(|| GatewayError::InvalidConfig("object storage is not configured".to_owned()))?;
    Ok((ObjectStorageClient::agent_bucket_name(agent_id), storage))
}

pub async fn list_files(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> Result<Json<Vec<WorkspaceFileResponse>>, GatewayError> {
    let (bucket, storage) = agent_workspace_bucket(&state, &headers, &agent_id).await?;
    // Bucket is created lazily on first upload; absent bucket = empty list.
    if !storage.bucket_exists(&bucket).await {
        return Ok(Json(Vec::new()));
    }
    let objects = storage.list_objects(&bucket).await?;
    Ok(Json(
        objects
            .into_iter()
            .map(|obj| WorkspaceFileResponse {
                path: obj.key,
                size_bytes: obj.size,
                updated_at: obj.last_modified_ms,
            })
            .collect(),
    ))
}

pub async fn create_upload_url(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Json(input): Json<UploadUrlRequest>,
) -> Result<Json<PresignedUrlResponse>, GatewayError> {
    let path = normalize_path(&input.path)?;
    let (bucket, storage) = agent_workspace_bucket(&state, &headers, &agent_id).await?;
    storage.ensure_bucket(&bucket).await?;
    let url = storage.presign_put(&bucket, &path, PRESIGN_TTL).await?;
    Ok(Json(PresignedUrlResponse { url, path }))
}

pub async fn download_url(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Query(query): Query<PathQuery>,
) -> Result<Json<PresignedUrlResponse>, GatewayError> {
    let path = normalize_path(&query.path)?;
    let (bucket, storage) = agent_workspace_bucket(&state, &headers, &agent_id).await?;
    let url = storage.presign_get(&bucket, &path, PRESIGN_TTL).await?;
    Ok(Json(PresignedUrlResponse { url, path }))
}

pub async fn delete_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Query(query): Query<PathQuery>,
) -> Result<Json<bool>, GatewayError> {
    let path = normalize_path(&query.path)?;
    let (bucket, storage) = agent_workspace_bucket(&state, &headers, &agent_id).await?;
    storage.delete_object(&bucket, &path).await?;
    Ok(Json(true))
}

fn normalize_path(path: &str) -> Result<String, GatewayError> {
    let trimmed = path.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        return Err(GatewayError::InvalidConfig("path must not be empty".to_owned()));
    }
    if trimmed.split('/').any(|segment| segment == "..") {
        return Err(GatewayError::InvalidConfig("path must not contain '..'".to_owned()));
    }
    Ok(trimmed.to_owned())
}
