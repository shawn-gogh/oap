ALTER TABLE "LiteLLM_ManagedAgentTasksTable"
  ADD COLUMN IF NOT EXISTS current_attempt_number INTEGER NOT NULL DEFAULT 1;

ALTER TABLE "LiteLLM_ManagedAgentTaskArtifactsTable"
  ADD COLUMN IF NOT EXISTS attempt_number INTEGER NOT NULL DEFAULT 1;

ALTER TABLE "LiteLLM_ManagedAgentTaskAcceptanceChecksTable"
  ADD COLUMN IF NOT EXISTS attempt_number INTEGER NOT NULL DEFAULT 1;

UPDATE "LiteLLM_ManagedAgentTaskArtifactsTable" artifact
SET attempt_number = COALESCE(
  (SELECT attempt_number FROM "LiteLLM_ManagedAgentSessionsTable" WHERE id = artifact.session_id),
  (SELECT attempt_number FROM "LiteLLM_ManagedAgentRunsTable" WHERE id = artifact.run_id),
  1
);

UPDATE "LiteLLM_ManagedAgentTasksTable" task
SET current_attempt_number = GREATEST(
  COALESCE((SELECT MAX(attempt_number) FROM "LiteLLM_ManagedAgentSessionsTable" WHERE task_id = task.id), 1),
  COALESCE((SELECT MAX(attempt_number) FROM "LiteLLM_ManagedAgentRunsTable" WHERE task_id = task.id), 1),
  COALESCE((SELECT MAX(attempt_number) FROM "LiteLLM_ManagedAgentTaskAcceptanceChecksTable" WHERE task_id = task.id), 1)
);

DO $$
DECLARE
  constraint_name TEXT;
BEGIN
  FOR constraint_name IN
    SELECT conname
    FROM pg_constraint
    WHERE conrelid = '"LiteLLM_ManagedAgentTaskAcceptanceChecksTable"'::regclass
      AND contype = 'u'
  LOOP
    EXECUTE format(
      'ALTER TABLE "LiteLLM_ManagedAgentTaskAcceptanceChecksTable" DROP CONSTRAINT %I',
      constraint_name
    );
  END LOOP;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS "LiteLLM_ManagedAgentTaskAcceptance_attempt_criterion_idx"
  ON "LiteLLM_ManagedAgentTaskAcceptanceChecksTable" (task_id, attempt_number, criterion_index);

CREATE INDEX IF NOT EXISTS "LiteLLM_ManagedAgentTaskArtifacts_attempt_idx"
  ON "LiteLLM_ManagedAgentTaskArtifactsTable" (task_id, attempt_number, created_at DESC);

CREATE INDEX IF NOT EXISTS "LiteLLM_ManagedAgentTaskAcceptance_attempt_idx"
  ON "LiteLLM_ManagedAgentTaskAcceptanceChecksTable" (task_id, attempt_number, criterion_index);
