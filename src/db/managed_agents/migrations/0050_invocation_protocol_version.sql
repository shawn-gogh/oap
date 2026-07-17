ALTER TABLE "LiteLLM_SessionInvocationsTable"
  ADD COLUMN IF NOT EXISTS protocol_version TEXT NOT NULL DEFAULT 'unverified';
