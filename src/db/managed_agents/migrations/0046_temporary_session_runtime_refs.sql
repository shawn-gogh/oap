ALTER TABLE "LiteLLM_ManagedAgentRuntimeRefsTable"
  ALTER COLUMN agent_id DROP NOT NULL,
  ADD COLUMN IF NOT EXISTS session_id TEXT REFERENCES "LiteLLM_ManagedAgentSessionsTable" (id) ON DELETE CASCADE;

CREATE UNIQUE INDEX IF NOT EXISTS "LiteLLM_ManagedAgentRuntimeRefs_session_id_idx"
  ON "LiteLLM_ManagedAgentRuntimeRefsTable" (session_id)
  WHERE session_id IS NOT NULL;
