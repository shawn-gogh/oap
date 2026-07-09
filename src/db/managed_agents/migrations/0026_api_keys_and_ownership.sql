CREATE TABLE IF NOT EXISTS "LiteLLM_GatewayApiKeysTable" (
    id TEXT PRIMARY KEY,
    key_hash TEXT NOT NULL UNIQUE,
    label TEXT,
    user_id TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'user',
    created_at BIGINT NOT NULL,
    last_used_at BIGINT
);

ALTER TABLE "LiteLLM_ManagedAgentSessionsTable"
  ADD COLUMN IF NOT EXISTS owner_id TEXT;

CREATE INDEX IF NOT EXISTS idx_managed_agent_sessions_owner
  ON "LiteLLM_ManagedAgentSessionsTable" (owner_id);
