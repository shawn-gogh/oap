ALTER TABLE "LiteLLM_SessionTurnsTable"
  ADD COLUMN IF NOT EXISTS input_json JSONB NOT NULL DEFAULT '{}'::JSONB,
  ADD COLUMN IF NOT EXISTS input_schema_json JSONB NOT NULL DEFAULT '{"type":"object"}'::JSONB,
  ADD COLUMN IF NOT EXISTS output_schema_json JSONB NOT NULL DEFAULT '{}'::JSONB,
  ADD COLUMN IF NOT EXISTS interaction_profile_json JSONB NOT NULL DEFAULT '{}'::JSONB,
  ADD COLUMN IF NOT EXISTS result_json JSONB,
  ADD COLUMN IF NOT EXISTS trigger_type TEXT NOT NULL DEFAULT 'conversation',
  ADD COLUMN IF NOT EXISTS retry_of_turn_id TEXT REFERENCES "LiteLLM_SessionTurnsTable" (id) ON DELETE SET NULL,
  ADD COLUMN IF NOT EXISTS attempt_number INTEGER NOT NULL DEFAULT 1;

ALTER TABLE "LiteLLM_SessionTurnsTable"
  ADD CONSTRAINT "LiteLLM_SessionTurns_trigger_type_check"
  CHECK (trigger_type IN ('conversation', 'manual', 'api', 'routine', 'event', 'delegate', 'retry'));

ALTER TABLE "LiteLLM_SessionTurnsTable"
  ADD CONSTRAINT "LiteLLM_SessionTurns_attempt_number_check"
  CHECK (attempt_number > 0);

CREATE INDEX IF NOT EXISTS "LiteLLM_SessionTurns_retry_of_idx"
  ON "LiteLLM_SessionTurnsTable" (retry_of_turn_id)
  WHERE retry_of_turn_id IS NOT NULL;
