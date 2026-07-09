CREATE TABLE IF NOT EXISTS "LiteLLM_ManagedAgentRevisionsTable" (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    snapshot JSONB NOT NULL,
    created_by TEXT,
    created_at BIGINT NOT NULL,
    UNIQUE (agent_id, version)
);

CREATE INDEX IF NOT EXISTS idx_managed_agent_revisions_agent
  ON "LiteLLM_ManagedAgentRevisionsTable" (agent_id, version DESC);
