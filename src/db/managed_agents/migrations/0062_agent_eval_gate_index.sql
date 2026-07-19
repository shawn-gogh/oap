CREATE INDEX IF NOT EXISTS idx_agent_eval_runs_revision_created
  ON "LiteLLM_AgentEvalRunsTable" (agent_id, agent_version, created_at DESC);
