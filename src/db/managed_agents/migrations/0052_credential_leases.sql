CREATE TABLE IF NOT EXISTS "LiteLLM_AgentCredentialLeasesTable" (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    session_id TEXT NOT NULL REFERENCES "LiteLLM_ManagedAgentSessionsTable" (id) ON DELETE CASCADE,
    turn_id TEXT NOT NULL REFERENCES "LiteLLM_SessionTurnsTable" (id) ON DELETE CASCADE,
    invocation_id TEXT NOT NULL REFERENCES "LiteLLM_SessionInvocationsTable" (id) ON DELETE CASCADE,
    credential_name TEXT NOT NULL,
    adapter_id TEXT NOT NULL,
    purpose TEXT NOT NULL DEFAULT 'agent_runtime',
    issued_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    last_resolved_at BIGINT,
    revoked_at BIGINT,
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    UNIQUE (invocation_id, credential_name),
    CHECK (expires_at > issued_at)
);

CREATE INDEX IF NOT EXISTS "LiteLLM_AgentCredentialLeases_active_idx"
  ON "LiteLLM_AgentCredentialLeasesTable" (session_id, expires_at)
  WHERE revoked_at IS NULL;
