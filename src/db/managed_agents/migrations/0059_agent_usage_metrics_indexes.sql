CREATE INDEX IF NOT EXISTS "LiteLLM_SessionInvocations_session_created_idx"
  ON "LiteLLM_SessionInvocationsTable" (session_id, created_at)
  WHERE role = 'primary';
