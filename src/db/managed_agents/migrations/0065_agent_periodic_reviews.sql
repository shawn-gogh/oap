ALTER TABLE "LiteLLM_ManagedAgentGovernanceTable"
  ADD COLUMN IF NOT EXISTS published_at BIGINT,
  ADD COLUMN IF NOT EXISTS review_due_at BIGINT;

UPDATE "LiteLLM_ManagedAgentGovernanceTable"
SET
  published_at = COALESCE(published_at, updated_at),
  review_due_at = COALESCE(review_due_at, updated_at + 7776000000)
WHERE lifecycle_status IN ('published', 'rolled_back');

ALTER TABLE "LiteLLM_ManagedAgentGovernanceTable"
  DROP CONSTRAINT IF EXISTS "LiteLLM_ManagedAgentGovernanceTable_lifecycle_status_check";
ALTER TABLE "LiteLLM_ManagedAgentGovernanceTable"
  ADD CONSTRAINT "LiteLLM_ManagedAgentGovernanceTable_lifecycle_status_check"
  CHECK (lifecycle_status IN (
    'imported', 'tested', 'pending_approval', 'published', 'unhealthy',
    'rolled_back', 'mapping_failed', 'suspended', 'retired', 'review_due'
  ));

CREATE INDEX IF NOT EXISTS "LiteLLM_ManagedAgentGovernanceTable_review_due_idx"
  ON "LiteLLM_ManagedAgentGovernanceTable" (review_due_at)
  WHERE lifecycle_status IN ('published', 'rolled_back');
