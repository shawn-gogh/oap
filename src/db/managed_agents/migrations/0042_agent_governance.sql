CREATE TABLE IF NOT EXISTS "LiteLLM_ManagedAgentGovernanceTable" (
    agent_id TEXT PRIMARY KEY REFERENCES "LiteLLM_ManagedAgentsTable" (id) ON DELETE CASCADE,
    owner_id TEXT NOT NULL,
    source_provider TEXT NOT NULL,
    source_endpoint TEXT NOT NULL,
    external_agent_id TEXT NOT NULL,
    source_version INTEGER NOT NULL DEFAULT 1,
    source_hash TEXT NOT NULL,
    lifecycle_status TEXT NOT NULL DEFAULT 'imported',
    runtime_health TEXT NOT NULL DEFAULT 'unknown',
    health_detail TEXT,
    credential_scope TEXT NOT NULL,
    credential_name TEXT,
    tested_revision INTEGER,
    published_revision INTEGER,
    previous_published_revision INTEGER,
    publish_approval_id TEXT,
    last_health_at BIGINT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    UNIQUE (owner_id, source_provider, source_endpoint, external_agent_id),
    CHECK (lifecycle_status IN ('imported', 'tested', 'pending_approval', 'published', 'unhealthy', 'rolled_back')),
    CHECK (runtime_health IN ('unknown', 'healthy', 'unhealthy')),
    CHECK (credential_scope IN ('personal', 'byo'))
);

CREATE INDEX IF NOT EXISTS "LiteLLM_ManagedAgentGovernanceTable_status_idx"
  ON "LiteLLM_ManagedAgentGovernanceTable" (lifecycle_status, updated_at DESC);
