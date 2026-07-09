ALTER TABLE "LiteLLM_ManagedAgentSessionsTable"
  ADD COLUMN IF NOT EXISTS workspace_bucket TEXT;
