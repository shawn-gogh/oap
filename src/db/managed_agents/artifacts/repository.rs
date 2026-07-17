use sqlx::PgPool;

use crate::{db::managed_agents::now_ms, errors::GatewayError};

use super::schema::{ManagedArtifactRow, NewManagedArtifact};

pub async fn create(
    pool: &PgPool,
    artifact: NewManagedArtifact,
) -> Result<ManagedArtifactRow, GatewayError> {
    let row = sqlx::query_as::<_, ManagedArtifactRow>(
        r#"
        INSERT INTO "LiteLLM_ManagedArtifactsTable" (
          id, owner_id, session_id, turn_id, invocation_id, task_id,
          source_artifact_id, media_type, digest, size_bytes, storage_backend,
          object_bucket, object_key, external_uri, status, metadata,
          created_by, created_at, verified_at
        )
        VALUES (
          $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
          $12, $13, $14, $15, $16, $17, $18, $19
        )
        ON CONFLICT (session_id, turn_id, source_artifact_id) DO UPDATE SET
          metadata = "LiteLLM_ManagedArtifactsTable".metadata || EXCLUDED.metadata
        WHERE "LiteLLM_ManagedArtifactsTable".digest IS NOT DISTINCT FROM EXCLUDED.digest
          AND "LiteLLM_ManagedArtifactsTable".size_bytes IS NOT DISTINCT FROM EXCLUDED.size_bytes
          AND "LiteLLM_ManagedArtifactsTable".media_type = EXCLUDED.media_type
        RETURNING *
        "#,
    )
    .bind(artifact.id)
    .bind(artifact.owner_id)
    .bind(artifact.session_id)
    .bind(artifact.turn_id)
    .bind(artifact.invocation_id)
    .bind(artifact.task_id)
    .bind(artifact.source_artifact_id)
    .bind(artifact.media_type)
    .bind(artifact.digest)
    .bind(artifact.size_bytes)
    .bind(artifact.storage_backend)
    .bind(artifact.object_bucket)
    .bind(artifact.object_key)
    .bind(artifact.external_uri)
    .bind(artifact.status)
    .bind(artifact.metadata)
    .bind(artifact.created_by)
    .bind(now_ms())
    .bind(artifact.verified_at)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)?;
    row.ok_or_else(|| {
        GatewayError::BadRequest(
            "制品 source_artifact_id 已存在，但摘要、大小或媒体类型不同。".to_owned(),
        )
    })
}

pub async fn get(
    pool: &PgPool,
    session_id: &str,
    artifact_id: &str,
) -> Result<Option<ManagedArtifactRow>, GatewayError> {
    sqlx::query_as::<_, ManagedArtifactRow>(
        r#"
        SELECT * FROM "LiteLLM_ManagedArtifactsTable"
        WHERE id = $1 AND session_id = $2
        "#,
    )
    .bind(artifact_id)
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(GatewayError::Database)
}

pub async fn list(
    pool: &PgPool,
    session_id: &str,
    turn_id: Option<&str>,
) -> Result<Vec<ManagedArtifactRow>, GatewayError> {
    sqlx::query_as::<_, ManagedArtifactRow>(
        r#"
        SELECT * FROM "LiteLLM_ManagedArtifactsTable"
        WHERE session_id = $1 AND ($2::TEXT IS NULL OR turn_id = $2)
        ORDER BY created_at DESC, id
        LIMIT 200
        "#,
    )
    .bind(session_id)
    .bind(turn_id)
    .fetch_all(pool)
    .await
    .map_err(GatewayError::Database)
}
