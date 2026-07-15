ALTER TABLE "LiteLLM_AgentGrantsTable"
  ADD COLUMN IF NOT EXISTS expires_at BIGINT;

ALTER TABLE "LiteLLM_AgentGroupGrantsTable"
  ADD COLUMN IF NOT EXISTS expires_at BIGINT;

CREATE INDEX IF NOT EXISTS "LiteLLM_AgentGrantsTable_expires_at_idx"
  ON "LiteLLM_AgentGrantsTable" (expires_at);

CREATE INDEX IF NOT EXISTS "LiteLLM_AgentGroupGrantsTable_expires_at_idx"
  ON "LiteLLM_AgentGroupGrantsTable" (expires_at);
