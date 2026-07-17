CREATE TABLE IF NOT EXISTS "LiteLLM_McpInvocationGrantsTable" (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    session_id TEXT NOT NULL REFERENCES "LiteLLM_ManagedAgentSessionsTable" (id) ON DELETE CASCADE,
    turn_id TEXT NOT NULL REFERENCES "LiteLLM_SessionTurnsTable" (id) ON DELETE CASCADE,
    invocation_id TEXT NOT NULL REFERENCES "LiteLLM_SessionInvocationsTable" (id) ON DELETE CASCADE,
    server_id TEXT NOT NULL,
    allowed_tools JSONB NOT NULL DEFAULT '[]'::JSONB,
    allow_all BOOLEAN NOT NULL DEFAULT FALSE,
    issued_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    last_used_at BIGINT,
    use_count INTEGER NOT NULL DEFAULT 0,
    revoked_at BIGINT,
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    UNIQUE (invocation_id, server_id),
    CHECK (expires_at > issued_at),
    CHECK (use_count >= 0),
    CHECK (jsonb_typeof(allowed_tools) = 'array')
);

CREATE INDEX IF NOT EXISTS "LiteLLM_McpInvocationGrants_active_idx"
  ON "LiteLLM_McpInvocationGrantsTable" (session_id, server_id, expires_at)
  WHERE revoked_at IS NULL;
