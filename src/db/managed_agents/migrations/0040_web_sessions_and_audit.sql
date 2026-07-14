CREATE TABLE IF NOT EXISTS "LiteLLM_WebSessionsTable" (
    id TEXT PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    is_admin BOOLEAN NOT NULL DEFAULT FALSE,
    created_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    revoked_at BIGINT
);

CREATE INDEX IF NOT EXISTS "LiteLLM_WebSessionsTable_expiry_idx"
  ON "LiteLLM_WebSessionsTable" (expires_at);

CREATE TABLE IF NOT EXISTS "LiteLLM_AuditLogsTable" (
    id TEXT PRIMARY KEY,
    actor_user_id TEXT NOT NULL,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS "LiteLLM_AuditLogsTable_created_at_idx"
  ON "LiteLLM_AuditLogsTable" (created_at DESC);

CREATE INDEX IF NOT EXISTS "LiteLLM_AuditLogsTable_target_idx"
  ON "LiteLLM_AuditLogsTable" (target_type, target_id);
