ALTER TABLE "LiteLLM_SpendLogs"
  ADD COLUMN IF NOT EXISTS invocation_id TEXT,
  ADD COLUMN IF NOT EXISTS purpose TEXT NOT NULL DEFAULT 'api';

CREATE INDEX IF NOT EXISTS "LiteLLM_SpendLogs_agent_id_startTime_idx"
  ON "LiteLLM_SpendLogs" (agent_id, "startTime");

CREATE INDEX IF NOT EXISTS "LiteLLM_SpendLogs_invocation_id_idx"
  ON "LiteLLM_SpendLogs" (invocation_id);
