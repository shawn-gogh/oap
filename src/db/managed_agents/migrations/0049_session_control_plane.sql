CREATE TABLE IF NOT EXISTS "LiteLLM_SessionTurnsTable" (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES "LiteLLM_ManagedAgentSessionsTable" (id) ON DELETE CASCADE,
    request_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    model TEXT,
    error_json JSONB,
    started_at BIGINT,
    completed_at BIGINT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    UNIQUE (session_id, request_id),
    CHECK (status IN (
        'queued', 'running', 'waiting_input', 'waiting_approval', 'cancelling',
        'completed', 'failed', 'rejected', 'cancelled', 'timed_out'
    ))
);

CREATE UNIQUE INDEX IF NOT EXISTS "LiteLLM_SessionTurns_one_active_idx"
  ON "LiteLLM_SessionTurnsTable" (session_id)
  WHERE status IN ('queued', 'running', 'waiting_input', 'waiting_approval', 'cancelling');

CREATE INDEX IF NOT EXISTS "LiteLLM_SessionTurns_session_created_idx"
  ON "LiteLLM_SessionTurnsTable" (session_id, created_at DESC);

CREATE TABLE IF NOT EXISTS "LiteLLM_SessionInvocationsTable" (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES "LiteLLM_ManagedAgentSessionsTable" (id) ON DELETE CASCADE,
    turn_id TEXT NOT NULL REFERENCES "LiteLLM_SessionTurnsTable" (id) ON DELETE CASCADE,
    parent_invocation_id TEXT REFERENCES "LiteLLM_SessionInvocationsTable" (id) ON DELETE SET NULL,
    agent_id TEXT,
    agent_revision INTEGER,
    runtime TEXT,
    protocol TEXT NOT NULL,
    adapter_id TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'primary',
    status TEXT NOT NULL DEFAULT 'queued',
    remote_agent_id TEXT,
    remote_session_id TEXT,
    remote_context_id TEXT,
    remote_task_id TEXT,
    resume_cursor TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    error_json JSONB,
    started_at BIGINT,
    finished_at BIGINT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    CHECK (role IN ('primary', 'delegate', 'tool', 'workflow')),
    CHECK (status IN (
        'queued', 'running', 'waiting_input', 'waiting_approval', 'cancelling',
        'completed', 'failed', 'rejected', 'cancelled', 'timed_out'
    ))
);

CREATE INDEX IF NOT EXISTS "LiteLLM_SessionInvocations_turn_idx"
  ON "LiteLLM_SessionInvocationsTable" (turn_id, created_at);
CREATE INDEX IF NOT EXISTS "LiteLLM_SessionInvocations_remote_task_idx"
  ON "LiteLLM_SessionInvocationsTable" (adapter_id, remote_task_id)
  WHERE remote_task_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS "LiteLLM_SessionOperationsTable" (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES "LiteLLM_ManagedAgentSessionsTable" (id) ON DELETE CASCADE,
    turn_id TEXT NOT NULL REFERENCES "LiteLLM_SessionTurnsTable" (id) ON DELETE CASCADE,
    invocation_id TEXT NOT NULL REFERENCES "LiteLLM_SessionInvocationsTable" (id) ON DELETE CASCADE,
    operation_key TEXT NOT NULL,
    operation_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'requested',
    request_json JSONB NOT NULL DEFAULT '{}'::JSONB,
    result_json JSONB,
    error_json JSONB,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    completed_at BIGINT,
    UNIQUE (invocation_id, operation_key),
    CHECK (status IN ('requested', 'waiting_approval', 'running', 'completed', 'rejected', 'failed', 'cancelled'))
);

CREATE TABLE IF NOT EXISTS "LiteLLM_SessionControlEventsTable" (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES "LiteLLM_ManagedAgentSessionsTable" (id) ON DELETE CASCADE,
    turn_id TEXT REFERENCES "LiteLLM_SessionTurnsTable" (id) ON DELETE CASCADE,
    invocation_id TEXT REFERENCES "LiteLLM_SessionInvocationsTable" (id) ON DELETE CASCADE,
    request_id TEXT,
    seq INTEGER NOT NULL,
    event_key TEXT NOT NULL,
    event_type TEXT NOT NULL,
    event_json JSONB NOT NULL,
    created_at BIGINT NOT NULL,
    UNIQUE (session_id, seq),
    UNIQUE (session_id, event_key)
);

CREATE INDEX IF NOT EXISTS "LiteLLM_SessionControlEvents_turn_idx"
  ON "LiteLLM_SessionControlEventsTable" (turn_id, seq)
  WHERE turn_id IS NOT NULL;

ALTER TABLE "LiteLLM_ManagedAgentInboxItemsTable"
  ADD COLUMN IF NOT EXISTS turn_id TEXT REFERENCES "LiteLLM_SessionTurnsTable" (id) ON DELETE SET NULL,
  ADD COLUMN IF NOT EXISTS invocation_id TEXT REFERENCES "LiteLLM_SessionInvocationsTable" (id) ON DELETE SET NULL,
  ADD COLUMN IF NOT EXISTS operation_id TEXT REFERENCES "LiteLLM_SessionOperationsTable" (id) ON DELETE SET NULL,
  ADD COLUMN IF NOT EXISTS request_id TEXT;

ALTER TABLE "LiteLLM_AgentSourceConnectorsTable"
  ADD COLUMN IF NOT EXISTS adapter_id TEXT,
  ADD COLUMN IF NOT EXISTS protocol TEXT,
  ADD COLUMN IF NOT EXISTS protocol_version TEXT,
  ADD COLUMN IF NOT EXISTS negotiated_profile JSONB NOT NULL DEFAULT '{}'::JSONB;

UPDATE "LiteLLM_AgentSourceConnectorsTable"
SET adapter_id = COALESCE(adapter_id, provider),
    protocol = COALESCE(protocol, provider)
WHERE adapter_id IS NULL OR protocol IS NULL;

