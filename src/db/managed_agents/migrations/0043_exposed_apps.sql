CREATE TABLE IF NOT EXISTS "LiteLLM_ExposedAppsTable" (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES "LiteLLM_ManagedAgentSessionsTable" (id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL,
    owner_user_id TEXT,
    container_key TEXT NOT NULL,
    port INTEGER NOT NULL,
    name TEXT,
    share_version INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',
    created_at BIGINT NOT NULL,
    expires_at BIGINT,
    deleted_at BIGINT,
    CHECK (status IN ('active', 'deleted')),
    CHECK (port > 0 AND port < 65536)
);

-- Partial unique index: a soft-deleted row frees its (container, port) slot
-- for reallocation while staying queryable for audit.
CREATE UNIQUE INDEX IF NOT EXISTS "LiteLLM_ExposedAppsTable_container_port_uidx"
  ON "LiteLLM_ExposedAppsTable" (container_key, port) WHERE status = 'active';

CREATE INDEX IF NOT EXISTS "LiteLLM_ExposedAppsTable_session_idx"
  ON "LiteLLM_ExposedAppsTable" (session_id);

CREATE INDEX IF NOT EXISTS "LiteLLM_ExposedAppsTable_expires_at_idx"
  ON "LiteLLM_ExposedAppsTable" (expires_at) WHERE status = 'active';
