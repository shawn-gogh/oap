//! Per-session workspace file API. Upload/download go directly between the
//! browser and MinIO via presigned URLs — this only issues/revokes those URLs
//! and lists/deletes objects, it never proxies file bytes through the gateway.

use std::{sync::Arc, time::Duration};

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{errors::GatewayError, object_storage::ObjectStorageClient, proxy::state::AppState};

use super::storage::{db, session};

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

async fn workspace_bucket(
    state: &AppState,
    headers: &HeaderMap,
    session_id: &str,
) -> Result<(String, ObjectStorageClient), GatewayError> {
    let pool = db(state, headers).await?;
    let row = session(pool, session_id).await?;
    let bucket = row.workspace_bucket.ok_or_else(|| {
        GatewayError::NotFound(format!("session {session_id} has no workspace"))
    })?;
    let storage = state
        .object_storage
        .clone()
        .ok_or_else(|| GatewayError::InvalidConfig("object storage is not configured".to_owned()))?;
    Ok((bucket, storage))
}

pub async fn list_files(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<WorkspaceFileResponse>>, GatewayError> {
    let (bucket, storage) = workspace_bucket(&state, &headers, &session_id).await?;
    let objects = storage.list_objects(&bucket).await?;
    Ok(Json(
        objects
            .into_iter()
            .filter(|obj| !is_internal_path(&obj.key))
            .map(|obj| WorkspaceFileResponse {
                path: obj.key,
                size_bytes: obj.size,
                updated_at: obj.last_modified_ms,
            })
            .collect(),
    ))
}

/// Each session's bucket is mounted as the opencode project directory, so it
/// also carries opencode's own bookkeeping (a `git init`'d repo, its agent
/// config) — implementation detail the user never uploaded, not their content.
fn is_internal_path(path: &str) -> bool {
    path.starts_with(".git/") || path == ".git" || path.starts_with(".opencode/") || path == "opencode.json"
}

pub async fn create_upload_url(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(input): Json<UploadUrlRequest>,
) -> Result<Json<PresignedUrlResponse>, GatewayError> {
    let path = normalize_path(&input.path)?;
    let (bucket, storage) = workspace_bucket(&state, &headers, &session_id).await?;
    let url = storage.presign_put(&bucket, &path, PRESIGN_TTL).await?;
    Ok(Json(PresignedUrlResponse { url, path }))
}

pub async fn download_url(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Query(query): Query<PathQuery>,
) -> Result<Json<PresignedUrlResponse>, GatewayError> {
    let path = normalize_path(&query.path)?;
    let (bucket, storage) = workspace_bucket(&state, &headers, &session_id).await?;
    let url = storage.presign_get(&bucket, &path, PRESIGN_TTL).await?;
    Ok(Json(PresignedUrlResponse { url, path }))
}

pub async fn delete_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Query(query): Query<PathQuery>,
) -> Result<Json<bool>, GatewayError> {
    let path = normalize_path(&query.path)?;
    let (bucket, storage) = workspace_bucket(&state, &headers, &session_id).await?;
    storage.delete_object(&bucket, &path).await?;
    Ok(Json(true))
}

/// Rejects paths that could escape the object's intended key namespace
/// (leading slash, `..` segments) — these aren't filesystem paths so `..`
/// can't traverse anything, but a client-controlled key with `../` segments
/// makes bucket listings confusing and is never a legitimate upload path.
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
