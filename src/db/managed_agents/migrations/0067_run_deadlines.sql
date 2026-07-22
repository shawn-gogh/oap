ALTER TABLE "LiteLLM_SessionTurnsTable"
  ADD COLUMN IF NOT EXISTS deadline_at BIGINT;

UPDATE "LiteLLM_SessionTurnsTable"
SET deadline_at = created_at + (30 * 60 * 1000)
WHERE deadline_at IS NULL
  AND status IN ('queued', 'running', 'waiting_input', 'waiting_approval', 'cancelling');

CREATE INDEX IF NOT EXISTS "LiteLLM_SessionTurns_active_deadline_idx"
  ON "LiteLLM_SessionTurnsTable" (deadline_at)
  WHERE status IN ('queued', 'running', 'waiting_input', 'waiting_approval', 'cancelling')
    AND deadline_at IS NOT NULL;
