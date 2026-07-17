CREATE TABLE IF NOT EXISTS "LiteLLM_ManagedArtifactsTable" (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    session_id TEXT NOT NULL REFERENCES "LiteLLM_ManagedAgentSessionsTable" (id) ON DELETE CASCADE,
    turn_id TEXT NOT NULL REFERENCES "LiteLLM_SessionTurnsTable" (id) ON DELETE CASCADE,
    invocation_id TEXT REFERENCES "LiteLLM_SessionInvocationsTable" (id) ON DELETE SET NULL,
    task_id TEXT REFERENCES "LiteLLM_ManagedAgentTasksTable" (id) ON DELETE SET NULL,
    source_artifact_id TEXT NOT NULL,
    media_type TEXT NOT NULL,
    digest TEXT,
    size_bytes BIGINT,
    storage_backend TEXT NOT NULL,
    object_bucket TEXT,
    object_key TEXT,
    external_uri TEXT,
    status TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_by TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    verified_at BIGINT,
    UNIQUE (session_id, turn_id, source_artifact_id),
    CHECK (size_bytes IS NULL OR size_bytes >= 0),
    CHECK (storage_backend IN ('object_storage', 'external_reference')),
    CHECK (status IN ('verified', 'unverified_external')),
    CHECK (
      (storage_backend = 'object_storage' AND object_bucket IS NOT NULL AND object_key IS NOT NULL)
      OR
      (storage_backend = 'external_reference' AND external_uri IS NOT NULL)
    ),
    CHECK (status <> 'verified' OR (digest IS NOT NULL AND size_bytes IS NOT NULL AND verified_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS "LiteLLM_ManagedArtifacts_session_idx"
  ON "LiteLLM_ManagedArtifactsTable" (session_id, turn_id, created_at DESC);

CREATE INDEX IF NOT EXISTS "LiteLLM_ManagedArtifacts_digest_idx"
  ON "LiteLLM_ManagedArtifactsTable" (digest)
  WHERE digest IS NOT NULL;
