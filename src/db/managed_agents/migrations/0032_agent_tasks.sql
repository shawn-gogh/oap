CREATE TABLE IF NOT EXISTS "LiteLLM_ManagedAgentTasksTable" (
  id TEXT PRIMARY KEY,
  agent_id TEXT NOT NULL REFERENCES "LiteLLM_ManagedAgentsTable" (id) ON DELETE CASCADE,
  application_version INTEGER NOT NULL DEFAULT 1,
  source TEXT NOT NULL,
  source_id TEXT,
  title TEXT NOT NULL,
  input_json JSONB NOT NULL DEFAULT '{}'::jsonb,
  status TEXT NOT NULL DEFAULT 'queued',
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  started_at BIGINT,
  completed_at BIGINT,
  failure_reason TEXT,
  CHECK (source IN ('manual', 'routine', 'api', 'event', 'test')),
  CHECK (status IN ('draft', 'queued', 'running', 'waiting_input', 'verifying', 'succeeded', 'failed', 'cancelled'))
);

CREATE INDEX IF NOT EXISTS "LiteLLM_ManagedAgentTasks_agent_created_idx"
  ON "LiteLLM_ManagedAgentTasksTable" (agent_id, created_at DESC);

CREATE INDEX IF NOT EXISTS "LiteLLM_ManagedAgentTasks_status_idx"
  ON "LiteLLM_ManagedAgentTasksTable" (status);

ALTER TABLE "LiteLLM_ManagedAgentSessionsTable"
  ADD COLUMN IF NOT EXISTS task_id TEXT REFERENCES "LiteLLM_ManagedAgentTasksTable" (id) ON DELETE SET NULL,
  ADD COLUMN IF NOT EXISTS attempt_number INTEGER NOT NULL DEFAULT 1;

CREATE INDEX IF NOT EXISTS "LiteLLM_ManagedAgentSessions_task_id_idx"
  ON "LiteLLM_ManagedAgentSessionsTable" (task_id);

CREATE UNIQUE INDEX IF NOT EXISTS "LiteLLM_ManagedAgentSessions_task_attempt_idx"
  ON "LiteLLM_ManagedAgentSessionsTable" (task_id, attempt_number)
  WHERE task_id IS NOT NULL;

ALTER TABLE "LiteLLM_ManagedAgentRunsTable"
  ADD COLUMN IF NOT EXISTS task_id TEXT REFERENCES "LiteLLM_ManagedAgentTasksTable" (id) ON DELETE SET NULL,
  ADD COLUMN IF NOT EXISTS attempt_number INTEGER NOT NULL DEFAULT 1;

CREATE INDEX IF NOT EXISTS "LiteLLM_ManagedAgentRuns_task_id_idx"
  ON "LiteLLM_ManagedAgentRunsTable" (task_id);

CREATE UNIQUE INDEX IF NOT EXISTS "LiteLLM_ManagedAgentRuns_task_attempt_idx"
  ON "LiteLLM_ManagedAgentRunsTable" (task_id, attempt_number)
  WHERE task_id IS NOT NULL;
