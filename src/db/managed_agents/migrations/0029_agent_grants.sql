CREATE TABLE IF NOT EXISTS "LiteLLM_AgentGrantsTable" (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    grantee_user_id TEXT NOT NULL,
    permission TEXT NOT NULL DEFAULT 'use',
    granted_by TEXT,
    created_at BIGINT NOT NULL,
    UNIQUE (agent_id, grantee_user_id)
);

CREATE INDEX IF NOT EXISTS idx_agent_grants_grantee
  ON "LiteLLM_AgentGrantsTable" (grantee_user_id);
