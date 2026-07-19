CREATE TABLE IF NOT EXISTS "LiteLLM_AgentRateLimitBucketsTable" (
  agent_id TEXT NOT NULL REFERENCES "LiteLLM_ManagedAgentsTable"(id) ON DELETE CASCADE,
  bucket_start BIGINT NOT NULL,
  request_count BIGINT NOT NULL DEFAULT 0,
  updated_at BIGINT NOT NULL,
  PRIMARY KEY (agent_id, bucket_start),
  CHECK (request_count >= 0)
);

CREATE INDEX IF NOT EXISTS "LiteLLM_AgentRateLimitBuckets_updated_idx"
  ON "LiteLLM_AgentRateLimitBucketsTable" (updated_at);
