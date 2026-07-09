CREATE TABLE IF NOT EXISTS "LiteLLM_AgentEvalRunsTable" (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    agent_version INTEGER,
    model TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'running',
    total INTEGER NOT NULL DEFAULT 0,
    passed INTEGER NOT NULL DEFAULT 0,
    results JSONB NOT NULL DEFAULT '[]'::jsonb,
    error TEXT,
    created_by TEXT,
    created_at BIGINT NOT NULL,
    completed_at BIGINT
);

CREATE INDEX IF NOT EXISTS idx_agent_eval_runs_agent
  ON "LiteLLM_AgentEvalRunsTable" (agent_id, created_at DESC);
