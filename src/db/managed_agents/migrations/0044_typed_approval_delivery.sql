ALTER TABLE "LiteLLM_ManagedAgentInboxItemsTable"
  ADD COLUMN IF NOT EXISTS enforcement_owner TEXT NOT NULL DEFAULT 'workflow',
  ADD COLUMN IF NOT EXISTS effect_handler TEXT NOT NULL DEFAULT 'resume_session',
  ADD COLUMN IF NOT EXISTS required_role TEXT NOT NULL DEFAULT 'owner',
  ADD COLUMN IF NOT EXISTS delivery_status TEXT NOT NULL DEFAULT 'pending',
  ADD COLUMN IF NOT EXISTS delivery_attempts INTEGER NOT NULL DEFAULT 0,
  ADD COLUMN IF NOT EXISTS last_delivery_error TEXT,
  ADD COLUMN IF NOT EXISTS expires_at BIGINT,
  ADD COLUMN IF NOT EXISTS escalation_role TEXT,
  ADD COLUMN IF NOT EXISTS escalate_at BIGINT,
  ADD COLUMN IF NOT EXISTS escalated_at BIGINT,
  ADD COLUMN IF NOT EXISTS decided_by TEXT,
  ADD COLUMN IF NOT EXISTS decision_scope TEXT NOT NULL DEFAULT 'once',
  ADD COLUMN IF NOT EXISTS applied_at BIGINT;

UPDATE "LiteLLM_ManagedAgentInboxItemsTable"
SET enforcement_owner = 'runtime',
    effect_handler = 'runtime_permission',
    required_role = CASE WHEN kind = 'unlisted_data_egress' THEN 'admin' ELSE 'owner' END
WHERE kind IN ('tool_permission', 'unlisted_data_egress');

UPDATE "LiteLLM_ManagedAgentInboxItemsTable"
SET enforcement_owner = 'platform', effect_handler = 'agent_publish', required_role = 'admin'
WHERE kind = 'approval' AND args_json LIKE '%"action":"publish_agent"%';

UPDATE "LiteLLM_ManagedAgentInboxItemsTable"
SET enforcement_owner = 'platform', effect_handler = 'agent_change', required_role = 'owner'
WHERE kind = 'approval' AND args_json LIKE '%"type":"agent_improvement"%';

UPDATE "LiteLLM_ManagedAgentInboxItemsTable"
SET delivery_status = 'applied', applied_at = resolved_at
WHERE status IN ('accepted', 'rejected', 'resolved', 'expired');

CREATE INDEX IF NOT EXISTS "LiteLLM_ManagedAgentInboxItems_expiry_idx"
  ON "LiteLLM_ManagedAgentInboxItemsTable" (expires_at)
  WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS "LiteLLM_ManagedAgentInboxItems_delivery_idx"
  ON "LiteLLM_ManagedAgentInboxItemsTable" (delivery_status, resolved_at)
  WHERE delivery_status = 'delivery_failed';
