//! Per-session workspace file API. Upload/download go directly between the
//! browser and MinIO via presigned URLs — this only issues/revokes those URLs
//! and lists/deletes objects, it never proxies file bytes through the gateway.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{errors::GatewayError, object_storage::ObjectStorageClient, proxy::state::AppState};

use super::storage::{auth_db, owned_session};

const PRESIGN_TTL: Duration = Duration::from_secs(15 * 60);
const TRASH_PREFIX: &str = ".trash";
const TRASH_RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1000;

#[derive(Debug, Serialize)]
pub struct WorkspaceFileResponse {
    pub path: String,
    pub size_bytes: i64,
    pub updated_at: Option<i64>,
    pub content_type: String,
    pub etag: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceBrowseQuery {
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub query: String,
    pub cursor: Option<usize>,
    pub limit: Option<usize>,
    pub sort_by: Option<String>,
    pub direction: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceBrowseResponse {
    pub files: Vec<WorkspaceFileResponse>,
    pub folders: Vec<String>,
    pub total: usize,
    pub next_cursor: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct PathQuery {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct UploadUrlRequest {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceTransferRequest {
    pub source_path: String,
    pub destination_path: String,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceBatchDeleteRequest {
    pub paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceFolderRequest {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceBatchTransferRequest {
    pub source_paths: Vec<String>,
    pub destination_directory: String,
    pub operation: String,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceTrashSelectionRequest {
    pub ids: Vec<String>,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceTrashItemResponse {
    pub id: String,
    pub paths: Vec<String>,
    pub deleted_at: i64,
    pub expires_at: i64,
    pub size_bytes: i64,
    pub object_count: usize,
}

#[derive(Debug, Deserialize, Serialize)]
struct WorkspaceTrashManifest {
    id: String,
    paths: Vec<String>,
    deleted_at: i64,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceOperationResponse {
    pub affected: usize,
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
    let (pool, auth) = auth_db(state, headers).await?;
    let row = owned_session(pool, &auth, session_id).await?;
    let bucket = row
        .workspace_bucket
        .ok_or_else(|| GatewayError::NotFound(format!("session {session_id} has no workspace")))?;
    let storage = state.object_storage.clone().ok_or_else(|| {
        GatewayError::InvalidConfig("object storage is not configured".to_owned())
    })?;
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
            .filter(|obj| !obj.key.ends_with('/'))
            .map(|obj| WorkspaceFileResponse {
                content_type: content_type_for_path(&obj.key),
                path: obj.key,
                size_bytes: obj.size,
                updated_at: obj.last_modified_ms,
                etag: obj.etag,
            })
            .collect(),
    ))
}

pub async fn browse_files(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Query(query): Query<WorkspaceBrowseQuery>,
) -> Result<Json<WorkspaceBrowseResponse>, GatewayError> {
    let prefix = normalize_optional_path(&query.prefix)?;
    let search = query.query.trim().to_lowercase();
    let offset = query.cursor.unwrap_or(0);
    let limit = query.limit.unwrap_or(100).clamp(1, 200);
    let (bucket, storage) = workspace_bucket(&state, &headers, &session_id).await?;
    let objects = storage.list_objects(&bucket).await?;
    let mut folders = HashSet::new();
    let mut files = Vec::new();
    let directory_prefix = prefix
        .as_ref()
        .map(|value| format!("{value}/"))
        .unwrap_or_default();
    for object in objects {
        if is_internal_path(&object.key) || object.key.ends_with('/') {
            continue;
        }
        let Some(remainder) = object.key.strip_prefix(&directory_prefix) else {
            continue;
        };
        if !search.is_empty() {
            if !remainder.to_lowercase().contains(&search) {
                continue;
            }
        } else {
            if let Some((folder, _)) = remainder.split_once('/') {
                folders.insert(if directory_prefix.is_empty() {
                    folder.to_owned()
                } else {
                    format!("{}{folder}", directory_prefix)
                });
                continue;
            }
        }
        files.push(WorkspaceFileResponse {
            content_type: content_type_for_path(&object.key),
            path: object.key,
            size_bytes: object.size,
            updated_at: object.last_modified_ms,
            etag: object.etag,
        });
    }
    sort_workspace_files(
        &mut files,
        query.sort_by.as_deref().unwrap_or("name"),
        query.direction.as_deref() == Some("desc"),
    );
    let total = files.len();
    let files = files
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let next_cursor = (offset + files.len() < total).then_some(offset + files.len());
    let mut folders = folders.into_iter().collect::<Vec<_>>();
    folders.sort();
    Ok(Json(WorkspaceBrowseResponse {
        files,
        folders,
        total,
        next_cursor,
    }))
}

pub async fn list_folders(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<String>>, GatewayError> {
    let (bucket, storage) = workspace_bucket(&state, &headers, &session_id).await?;
    let objects = storage.list_objects(&bucket).await?;
    let mut folders = HashSet::new();
    for object in objects {
        if is_internal_path(&object.key) {
            continue;
        }
        let parts = object
            .key
            .trim_end_matches('/')
            .split('/')
            .collect::<Vec<_>>();
        for index in 1..parts.len() {
            folders.insert(parts[..index].join("/"));
        }
        if object.key.ends_with('/') && !parts.is_empty() {
            folders.insert(parts.join("/"));
        }
    }
    let mut folders = folders.into_iter().collect::<Vec<_>>();
    folders.sort();
    Ok(Json(folders))
}

pub async fn create_folder(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(input): Json<WorkspaceFolderRequest>,
) -> Result<Json<bool>, GatewayError> {
    let path = normalize_user_path(&input.path)?;
    let (bucket, storage) = workspace_bucket(&state, &headers, &session_id).await?;
    let marker = format!("{path}/");
    if storage
        .list_objects(&bucket)
        .await?
        .iter()
        .any(|object| object.key == path || object.key == marker)
    {
        return Err(GatewayError::BadRequest(
            "当前目录已存在同名文件或文件夹。".to_owned(),
        ));
    }
    storage.put_bytes(&bucket, &marker, Vec::new()).await?;
    Ok(Json(true))
}

/// Each session's bucket is mounted as the opencode project directory, so it
/// also carries opencode's own bookkeeping (a `git init`'d repo, its agent
/// config) — implementation detail the user never uploaded, not their content.
fn is_internal_path(path: &str) -> bool {
    path.starts_with(".git/")
        || path == ".git"
        || path.starts_with(".opencode/")
        || path == "opencode.json"
        || path.starts_with(".trash/")
        || path == ".trash"
}

pub async fn create_upload_url(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(input): Json<UploadUrlRequest>,
) -> Result<Json<PresignedUrlResponse>, GatewayError> {
    let path = normalize_user_path(&input.path)?;
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
    let path = normalize_user_path(&query.path)?;
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
    let path = normalize_user_path(&query.path)?;
    let (bucket, storage) = workspace_bucket(&state, &headers, &session_id).await?;
    storage.delete_object(&bucket, &path).await?;
    Ok(Json(true))
}

pub async fn move_files(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(input): Json<WorkspaceTransferRequest>,
) -> Result<Json<WorkspaceOperationResponse>, GatewayError> {
    transfer_files(&state, &headers, &session_id, input, true).await
}

pub async fn copy_files(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(input): Json<WorkspaceTransferRequest>,
) -> Result<Json<WorkspaceOperationResponse>, GatewayError> {
    transfer_files(&state, &headers, &session_id, input, false).await
}

async fn transfer_files(
    state: &AppState,
    headers: &HeaderMap,
    session_id: &str,
    input: WorkspaceTransferRequest,
    remove_source: bool,
) -> Result<Json<WorkspaceOperationResponse>, GatewayError> {
    let source = normalize_user_path(&input.source_path)?;
    let destination = normalize_user_path(&input.destination_path)?;
    let (bucket, storage) = workspace_bucket(state, headers, session_id).await?;
    let keys = storage
        .list_objects(&bucket)
        .await?
        .into_iter()
        .map(|object| object.key)
        .collect::<Vec<_>>();
    let plan = plan_transfer(&keys, &source, &destination, input.overwrite)?;
    for (source_key, destination_key) in &plan {
        storage
            .copy_object_as(&bucket, source_key, &bucket, destination_key)
            .await?;
    }
    if remove_source {
        for (source_key, _) in &plan {
            storage.delete_object(&bucket, source_key).await?;
        }
    }
    Ok(Json(WorkspaceOperationResponse {
        affected: plan.len(),
    }))
}

pub async fn batch_delete_files(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(input): Json<WorkspaceBatchDeleteRequest>,
) -> Result<Json<WorkspaceOperationResponse>, GatewayError> {
    if input.paths.is_empty() || input.paths.len() > 100 {
        return Err(GatewayError::BadRequest(
            "每次请选择 1 到 100 个文件或目录。".to_owned(),
        ));
    }
    let paths = input
        .paths
        .iter()
        .map(|path| normalize_user_path(path))
        .collect::<Result<Vec<_>, _>>()?;
    let (bucket, storage) = workspace_bucket(&state, &headers, &session_id).await?;
    let keys = storage
        .list_objects(&bucket)
        .await?
        .into_iter()
        .map(|object| object.key)
        .collect::<Vec<_>>();
    let targets = expand_paths(&keys, &paths);
    for key in &targets {
        storage.delete_object(&bucket, key).await?;
    }
    Ok(Json(WorkspaceOperationResponse {
        affected: targets.len(),
    }))
}

pub async fn trash_workspace_paths(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(input): Json<WorkspaceBatchDeleteRequest>,
) -> Result<Json<WorkspaceOperationResponse>, GatewayError> {
    let paths = normalize_batch_paths(&input.paths)?;
    let (bucket, storage) = workspace_bucket(&state, &headers, &session_id).await?;
    let keys = storage
        .list_objects(&bucket)
        .await?
        .into_iter()
        .filter(|object| !is_internal_path(&object.key))
        .map(|object| object.key)
        .collect::<Vec<_>>();
    let targets = expand_paths(&keys, &paths);
    if targets.is_empty() {
        return Err(GatewayError::NotFound("文件或目录不存在。".to_owned()));
    }

    let id = uuid::Uuid::new_v4().simple().to_string();
    let object_prefix = trash_object_prefix(&id);
    let mut copied: Vec<String> = Vec::new();
    for key in &targets {
        let trash_key = format!("{object_prefix}{key}");
        if let Err(error) = storage
            .copy_object_as(&bucket, key, &bucket, &trash_key)
            .await
        {
            for copied_key in copied {
                let _ = storage.delete_object(&bucket, &copied_key).await;
            }
            return Err(error);
        }
        copied.push(trash_key);
    }
    let manifest = WorkspaceTrashManifest {
        id: id.clone(),
        paths,
        deleted_at: now_ms(),
    };
    let manifest_key = trash_manifest_key(&id);
    let manifest_bytes = serde_json::to_vec(&manifest)?;
    if let Err(error) = storage
        .put_bytes(&bucket, &manifest_key, manifest_bytes)
        .await
    {
        for copied_key in copied {
            let _ = storage.delete_object(&bucket, &copied_key).await;
        }
        return Err(error);
    }
    for key in &targets {
        storage.delete_object(&bucket, key).await?;
    }
    Ok(Json(WorkspaceOperationResponse {
        affected: targets.len(),
    }))
}

pub async fn list_workspace_trash(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<WorkspaceTrashItemResponse>>, GatewayError> {
    let (bucket, storage) = workspace_bucket(&state, &headers, &session_id).await?;
    let objects = storage.list_objects(&bucket).await?;
    let current_time = now_ms();
    let mut grouped_objects: HashMap<String, Vec<_>> = HashMap::new();
    for object in objects {
        if let Some(id) = trash_id_from_key(&object.key) {
            grouped_objects
                .entry(id.to_owned())
                .or_default()
                .push(object);
        }
    }

    let mut items = Vec::new();
    for (id, objects) in grouped_objects {
        let manifest_key = trash_manifest_key(&id);
        if !objects.iter().any(|object| object.key == manifest_key) {
            continue;
        }
        let Ok(bytes) = storage.get_bytes(&bucket, &manifest_key).await else {
            continue;
        };
        let Ok(manifest) = serde_json::from_slice::<WorkspaceTrashManifest>(&bytes) else {
            continue;
        };
        let expires_at = manifest.deleted_at.saturating_add(TRASH_RETENTION_MS);
        if expires_at <= current_time {
            for object in &objects {
                storage.delete_object(&bucket, &object.key).await?;
            }
            continue;
        }
        let object_prefix = trash_object_prefix(&id);
        let trashed_objects = objects
            .iter()
            .filter(|object| object.key.starts_with(&object_prefix))
            .collect::<Vec<_>>();
        items.push(WorkspaceTrashItemResponse {
            id,
            paths: manifest.paths,
            deleted_at: manifest.deleted_at,
            expires_at,
            size_bytes: trashed_objects.iter().map(|object| object.size).sum(),
            object_count: trashed_objects.len(),
        });
    }
    items.sort_by_key(|item| std::cmp::Reverse(item.deleted_at));
    Ok(Json(items))
}

pub async fn restore_workspace_trash(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(input): Json<WorkspaceTrashSelectionRequest>,
) -> Result<Json<WorkspaceOperationResponse>, GatewayError> {
    let ids = normalize_trash_ids(&input.ids)?;
    let (bucket, storage) = workspace_bucket(&state, &headers, &session_id).await?;
    let objects = storage.list_objects(&bucket).await?;
    let active_keys = objects
        .iter()
        .filter(|object| !is_internal_path(&object.key))
        .map(|object| object.key.as_str())
        .collect::<HashSet<_>>();
    let mut destinations = HashSet::new();
    let mut plan = Vec::new();
    for id in &ids {
        let manifest_key = trash_manifest_key(id);
        if !objects.iter().any(|object| object.key == manifest_key) {
            return Err(GatewayError::NotFound(
                "回收站项目不存在或已过期。".to_owned(),
            ));
        }
        let manifest = serde_json::from_slice::<WorkspaceTrashManifest>(
            &storage.get_bytes(&bucket, &manifest_key).await?,
        )?;
        if manifest.deleted_at.saturating_add(TRASH_RETENTION_MS) <= now_ms() {
            delete_trash_group(&storage, &bucket, id, &objects).await?;
            return Err(GatewayError::NotFound(
                "回收站项目不存在或已过期。".to_owned(),
            ));
        }
        let prefix = trash_object_prefix(id);
        let group = objects
            .iter()
            .filter_map(|object| {
                object
                    .key
                    .strip_prefix(&prefix)
                    .map(|destination| (object.key.clone(), destination.to_owned()))
            })
            .collect::<Vec<_>>();
        if group.is_empty() {
            return Err(GatewayError::NotFound(
                "回收站项目不存在或已过期。".to_owned(),
            ));
        }
        for (_, destination) in &group {
            if !destinations.insert(destination.clone()) {
                return Err(GatewayError::BadRequest(
                    "所选回收站项目包含重复路径，请分批还原。".to_owned(),
                ));
            }
            if !input.overwrite && active_keys.contains(destination.as_str()) {
                return Err(GatewayError::BadRequest(format!(
                    "原位置已有同名文件：{destination}"
                )));
            }
        }
        plan.extend(group);
    }
    for (source, destination) in &plan {
        storage
            .copy_object_as(&bucket, source, &bucket, destination)
            .await?;
    }
    for id in &ids {
        delete_trash_group(&storage, &bucket, id, &objects).await?;
    }
    Ok(Json(WorkspaceOperationResponse {
        affected: plan.len(),
    }))
}

pub async fn delete_workspace_trash(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(input): Json<WorkspaceTrashSelectionRequest>,
) -> Result<Json<WorkspaceOperationResponse>, GatewayError> {
    let ids = normalize_trash_ids(&input.ids)?;
    let (bucket, storage) = workspace_bucket(&state, &headers, &session_id).await?;
    let objects = storage.list_objects(&bucket).await?;
    let mut affected = 0;
    for id in &ids {
        affected += delete_trash_group(&storage, &bucket, id, &objects).await?;
    }
    Ok(Json(WorkspaceOperationResponse { affected }))
}

pub async fn empty_workspace_trash(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<WorkspaceOperationResponse>, GatewayError> {
    let (bucket, storage) = workspace_bucket(&state, &headers, &session_id).await?;
    let objects = storage.list_objects(&bucket).await?;
    let targets = objects
        .iter()
        .filter(|object| object.key.starts_with(&format!("{TRASH_PREFIX}/")))
        .collect::<Vec<_>>();
    for object in &targets {
        storage.delete_object(&bucket, &object.key).await?;
    }
    Ok(Json(WorkspaceOperationResponse {
        affected: targets.len(),
    }))
}

pub async fn batch_transfer_files(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(input): Json<WorkspaceBatchTransferRequest>,
) -> Result<Json<WorkspaceOperationResponse>, GatewayError> {
    if input.source_paths.is_empty() || input.source_paths.len() > 100 {
        return Err(GatewayError::BadRequest(
            "每次请选择 1 到 100 个文件或目录。".to_owned(),
        ));
    }
    if input.operation != "move" && input.operation != "copy" {
        return Err(GatewayError::BadRequest(
            "operation 必须是 move 或 copy。".to_owned(),
        ));
    }
    let sources = input
        .source_paths
        .iter()
        .map(|path| normalize_user_path(path))
        .collect::<Result<Vec<_>, _>>()?;
    let destination_directory = normalize_optional_path(&input.destination_directory)?;
    let (bucket, storage) = workspace_bucket(&state, &headers, &session_id).await?;
    let keys = storage
        .list_objects(&bucket)
        .await?
        .into_iter()
        .map(|object| object.key)
        .collect::<Vec<_>>();
    let mut plan = Vec::new();
    let mut destinations = HashSet::new();
    for source in &sources {
        let name = source.rsplit('/').next().unwrap_or(source);
        let destination = destination_directory
            .as_ref()
            .map(|directory| format!("{directory}/{name}"))
            .unwrap_or_else(|| name.to_owned());
        for pair in plan_transfer(&keys, source, &destination, input.overwrite)? {
            if !destinations.insert(pair.1.clone()) {
                return Err(GatewayError::BadRequest(
                    "所选项目在目标目录中会产生重名。".to_owned(),
                ));
            }
            plan.push(pair);
        }
    }
    for (source, destination) in &plan {
        storage
            .copy_object_as(&bucket, source, &bucket, destination)
            .await?;
    }
    if input.operation == "move" {
        for (source, _) in &plan {
            storage.delete_object(&bucket, source).await?;
        }
    }
    Ok(Json(WorkspaceOperationResponse {
        affected: plan.len(),
    }))
}

fn plan_transfer(
    keys: &[String],
    source: &str,
    destination: &str,
    overwrite: bool,
) -> Result<Vec<(String, String)>, GatewayError> {
    if source == destination {
        return Err(GatewayError::BadRequest(
            "源路径和目标路径不能相同。".to_owned(),
        ));
    }
    let exact_file = keys.iter().any(|key| key == source);
    let destination_prefix = format!("{destination}/");
    if exact_file && keys.iter().any(|key| key.starts_with(&destination_prefix)) {
        return Err(GatewayError::BadRequest(
            "不能用文件覆盖同名目录。".to_owned(),
        ));
    }
    if !exact_file && keys.iter().any(|key| key == destination) {
        return Err(GatewayError::BadRequest(
            "不能用目录覆盖同名文件。".to_owned(),
        ));
    }
    if !exact_file && destination.starts_with(&format!("{source}/")) {
        return Err(GatewayError::BadRequest(
            "不能把目录移动或复制到自身的子目录。".to_owned(),
        ));
    }
    let source_prefix = format!("{source}/");
    let plan = keys
        .iter()
        .filter_map(|key| {
            if exact_file {
                (key == source).then(|| (key.clone(), destination.to_owned()))
            } else {
                key.strip_prefix(&source_prefix)
                    .map(|suffix| (key.clone(), format!("{destination}/{suffix}")))
            }
        })
        .collect::<Vec<_>>();
    if plan.is_empty() {
        return Err(GatewayError::NotFound("文件或目录不存在。".to_owned()));
    }
    if !overwrite {
        let source_keys = plan
            .iter()
            .map(|(source_key, _)| source_key.as_str())
            .collect::<HashSet<_>>();
        let conflicts = plan
            .iter()
            .filter(|(_, destination_key)| {
                keys.iter().any(|key| key == destination_key)
                    && !source_keys.contains(destination_key.as_str())
            })
            .map(|(_, destination_key)| destination_key.clone())
            .collect::<Vec<_>>();
        if !conflicts.is_empty() {
            return Err(GatewayError::BadRequest(format!(
                "目标位置已有同名文件：{}",
                conflicts.join("、")
            )));
        }
    }
    Ok(plan)
}

fn expand_paths(keys: &[String], paths: &[String]) -> Vec<String> {
    let mut targets = HashSet::new();
    for path in paths {
        let prefix = format!("{path}/");
        for key in keys {
            if key == path || key.starts_with(&prefix) {
                targets.insert(key.clone());
            }
        }
    }
    let mut targets = targets.into_iter().collect::<Vec<_>>();
    targets.sort();
    targets
}

fn normalize_batch_paths(paths: &[String]) -> Result<Vec<String>, GatewayError> {
    if paths.is_empty() || paths.len() > 100 {
        return Err(GatewayError::BadRequest(
            "每次请选择 1 到 100 个文件或目录。".to_owned(),
        ));
    }
    let mut unique = HashSet::new();
    let mut normalized = Vec::new();
    for path in paths {
        let path = normalize_user_path(path)?;
        if unique.insert(path.clone()) {
            normalized.push(path);
        }
    }
    Ok(normalized)
}

fn normalize_trash_ids(ids: &[String]) -> Result<Vec<String>, GatewayError> {
    if ids.is_empty() || ids.len() > 100 {
        return Err(GatewayError::BadRequest(
            "每次请选择 1 到 100 个回收站项目。".to_owned(),
        ));
    }
    let mut unique = HashSet::new();
    let mut normalized = Vec::new();
    for id in ids {
        if id.is_empty()
            || id.len() > 64
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(GatewayError::BadRequest("回收站项目 ID 无效。".to_owned()));
        }
        if unique.insert(id.clone()) {
            normalized.push(id.clone());
        }
    }
    Ok(normalized)
}

fn trash_object_prefix(id: &str) -> String {
    format!("{TRASH_PREFIX}/{id}/objects/")
}

fn trash_manifest_key(id: &str) -> String {
    format!("{TRASH_PREFIX}/{id}/manifest.json")
}

fn trash_id_from_key(key: &str) -> Option<&str> {
    let remainder = key.strip_prefix(&format!("{TRASH_PREFIX}/"))?;
    let (id, _) = remainder.split_once('/')?;
    (!id.is_empty()).then_some(id)
}

async fn delete_trash_group(
    storage: &ObjectStorageClient,
    bucket: &str,
    id: &str,
    objects: &[crate::object_storage::ObjectMeta],
) -> Result<usize, GatewayError> {
    let prefix = format!("{TRASH_PREFIX}/{id}/");
    let targets = objects
        .iter()
        .filter(|object| object.key.starts_with(&prefix))
        .collect::<Vec<_>>();
    for object in &targets {
        storage.delete_object(bucket, &object.key).await?;
    }
    Ok(targets.len())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn normalize_user_path(path: &str) -> Result<String, GatewayError> {
    let normalized = normalize_path(path)?;
    if is_internal_path(&normalized) {
        return Err(GatewayError::NotFound("文件或目录不存在。".to_owned()));
    }
    Ok(normalized)
}

fn normalize_optional_path(path: &str) -> Result<Option<String>, GatewayError> {
    if path.trim().trim_matches('/').is_empty() {
        return Ok(None);
    }
    normalize_user_path(path).map(Some)
}

fn content_type_for_path(path: &str) -> String {
    let extension = path.rsplit('.').next().unwrap_or_default().to_lowercase();
    match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "md" | "mdx" => "text/markdown",
        "txt" | "log" => "text/plain",
        "html" | "htm" => "text/html",
        "yaml" | "yml" => "application/yaml",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
    .to_owned()
}

fn sort_workspace_files(files: &mut [WorkspaceFileResponse], sort_by: &str, descending: bool) {
    files.sort_by(|left, right| {
        let ordering = match sort_by {
            "size" => left.size_bytes.cmp(&right.size_bytes),
            "updated" => left.updated_at.cmp(&right.updated_at),
            _ => left.path.to_lowercase().cmp(&right.path.to_lowercase()),
        };
        if descending {
            ordering.reverse()
        } else {
            ordering
        }
    });
}

/// Rejects paths that could escape the object's intended key namespace
/// (leading slash, `..` segments) — these aren't filesystem paths so `..`
/// can't traverse anything, but a client-controlled key with `../` segments
/// makes bucket listings confusing and is never a legitimate upload path.
fn normalize_path(path: &str) -> Result<String, GatewayError> {
    let trimmed = path.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Err(GatewayError::InvalidConfig(
            "path must not be empty".to_owned(),
        ));
    }
    if trimmed.split('/').any(|segment| segment == "..") {
        return Err(GatewayError::InvalidConfig(
            "path must not contain '..'".to_owned(),
        ));
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{
        content_type_for_path, expand_paths, normalize_trash_ids, normalize_user_path,
        plan_transfer, sort_workspace_files, trash_id_from_key, WorkspaceFileResponse,
    };

    fn keys() -> Vec<String> {
        vec![
            "README.md".to_owned(),
            "reports/2026/july.csv".to_owned(),
            "reports/summary.pdf".to_owned(),
            "archive/summary.pdf".to_owned(),
        ]
    }

    #[test]
    fn plans_file_and_directory_transfers() {
        assert_eq!(
            plan_transfer(&keys(), "README.md", "docs/README.md", false).unwrap(),
            vec![("README.md".to_owned(), "docs/README.md".to_owned())]
        );
        assert_eq!(
            plan_transfer(&keys(), "reports", "analysis", false).unwrap(),
            vec![
                (
                    "reports/2026/july.csv".to_owned(),
                    "analysis/2026/july.csv".to_owned()
                ),
                (
                    "reports/summary.pdf".to_owned(),
                    "analysis/summary.pdf".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn rejects_conflicts_and_nested_directory_targets() {
        assert!(
            plan_transfer(&keys(), "reports/summary.pdf", "archive/summary.pdf", false).is_err()
        );
        assert!(plan_transfer(&keys(), "reports", "reports/archive", true).is_err());
        assert!(plan_transfer(&keys(), "README.md", "reports", true).is_err());
        assert!(plan_transfer(&keys(), "reports", "README.md", true).is_err());
    }

    #[test]
    fn expands_and_deduplicates_batch_paths() {
        assert_eq!(
            expand_paths(
                &keys(),
                &["reports".to_owned(), "reports/summary.pdf".to_owned()]
            ),
            vec![
                "reports/2026/july.csv".to_owned(),
                "reports/summary.pdf".to_owned()
            ]
        );
    }

    #[test]
    fn hides_internal_workspace_paths() {
        assert!(normalize_user_path(".git/config").is_err());
        assert!(normalize_user_path(".opencode/state").is_err());
        assert!(normalize_user_path(".trash/item/manifest.json").is_err());
        assert_eq!(
            normalize_user_path("reports/file.md").unwrap(),
            "reports/file.md"
        );
    }

    #[test]
    fn validates_and_parses_trash_ids() {
        assert_eq!(
            normalize_trash_ids(&["abc_123".to_owned(), "abc_123".to_owned()]).unwrap(),
            vec!["abc_123".to_owned()]
        );
        assert!(normalize_trash_ids(&["../item".to_owned()]).is_err());
        assert_eq!(
            trash_id_from_key(".trash/abc_123/objects/reports/data.csv"),
            Some("abc_123")
        );
        assert_eq!(trash_id_from_key("reports/data.csv"), None);
    }

    #[test]
    fn detects_common_content_types() {
        assert_eq!(content_type_for_path("报告.pdf"), "application/pdf");
        assert_eq!(content_type_for_path("数据.CSV"), "text/csv");
        assert_eq!(
            content_type_for_path("archive.unknown"),
            "application/octet-stream"
        );
    }

    #[test]
    fn sorts_workspace_files_by_requested_field_and_direction() {
        let mut files = vec![
            WorkspaceFileResponse {
                path: "b.txt".to_owned(),
                size_bytes: 1,
                updated_at: Some(20),
                content_type: "text/plain".to_owned(),
                etag: Some("b".to_owned()),
            },
            WorkspaceFileResponse {
                path: "a.txt".to_owned(),
                size_bytes: 3,
                updated_at: Some(10),
                content_type: "text/plain".to_owned(),
                etag: Some("a".to_owned()),
            },
        ];
        sort_workspace_files(&mut files, "size", true);
        assert_eq!(files[0].path, "a.txt");
        sort_workspace_files(&mut files, "updated", false);
        assert_eq!(files[0].path, "a.txt");
    }
}
