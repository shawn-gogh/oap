CREATE TABLE IF NOT EXISTS "LiteLLM_ManagedAgentTaskArtifactsTable" (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES "LiteLLM_ManagedAgentTasksTable" (id) ON DELETE CASCADE,
  session_id TEXT REFERENCES "LiteLLM_ManagedAgentSessionsTable" (id) ON DELETE SET NULL,
  run_id TEXT REFERENCES "LiteLLM_ManagedAgentRunsTable" (id) ON DELETE SET NULL,
  artifact_type TEXT NOT NULL,
  name TEXT NOT NULL,
  content_json JSONB,
  location TEXT,
  dedupe_key TEXT,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  CHECK (content_json IS NOT NULL OR location IS NOT NULL),
  UNIQUE (task_id, dedupe_key)
);

CREATE INDEX IF NOT EXISTS "LiteLLM_ManagedAgentTaskArtifacts_task_idx"
  ON "LiteLLM_ManagedAgentTaskArtifactsTable" (task_id, created_at DESC);

CREATE TABLE IF NOT EXISTS "LiteLLM_ManagedAgentTaskAcceptanceChecksTable" (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES "LiteLLM_ManagedAgentTasksTable" (id) ON DELETE CASCADE,
  criterion_index INTEGER NOT NULL,
  criterion TEXT NOT NULL,
  verdict TEXT NOT NULL DEFAULT 'pending',
  evidence TEXT,
  checked_by TEXT,
  checked_at BIGINT,
  CHECK (verdict IN ('pending', 'passed', 'failed')),
  UNIQUE (task_id, criterion_index)
);

CREATE INDEX IF NOT EXISTS "LiteLLM_ManagedAgentTaskAcceptance_task_idx"
  ON "LiteLLM_ManagedAgentTaskAcceptanceChecksTable" (task_id, criterion_index);
