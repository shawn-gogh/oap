ALTER TABLE "LiteLLM_ManagedAgentTasksTable"
  ADD COLUMN IF NOT EXISTS deadline_at BIGINT,
  ADD COLUMN IF NOT EXISTS failure_code TEXT;

UPDATE "LiteLLM_ManagedAgentTasksTable" task
SET deadline_at = task.created_at + GREATEST(agent.max_runtime_minutes, 1)::BIGINT * 60000
FROM "LiteLLM_ManagedAgentsTable" agent
WHERE task.agent_id = agent.id AND task.deadline_at IS NULL;

CREATE INDEX IF NOT EXISTS "LiteLLM_ManagedAgentTasks_deadline_idx"
  ON "LiteLLM_ManagedAgentTasksTable" (deadline_at)
  WHERE status IN ('draft', 'queued', 'running', 'waiting_input');
