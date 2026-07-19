CREATE INDEX IF NOT EXISTS "LiteLLM_AuditLogsTable_target_created_idx"
  ON "LiteLLM_AuditLogsTable" (target_type, target_id, created_at DESC);
