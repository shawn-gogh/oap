use serde::Serialize;
use serde_json::Value;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ManagedArtifactRow {
    pub id: String,
    pub owner_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub invocation_id: Option<String>,
    pub task_id: Option<String>,
    pub source_artifact_id: String,
    pub media_type: String,
    pub digest: Option<String>,
    pub size_bytes: Option<i64>,
    pub storage_backend: String,
    pub object_bucket: Option<String>,
    pub object_key: Option<String>,
    pub external_uri: Option<String>,
    pub status: String,
    pub metadata: Value,
    pub created_by: String,
    pub created_at: i64,
    pub verified_at: Option<i64>,
}

pub struct NewManagedArtifact {
    pub id: String,
    pub owner_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub invocation_id: Option<String>,
    pub task_id: Option<String>,
    pub source_artifact_id: String,
    pub media_type: String,
    pub digest: Option<String>,
    pub size_bytes: Option<i64>,
    pub storage_backend: String,
    pub object_bucket: Option<String>,
    pub object_key: Option<String>,
    pub external_uri: Option<String>,
    pub status: String,
    pub metadata: Value,
    pub created_by: String,
    pub verified_at: Option<i64>,
}
