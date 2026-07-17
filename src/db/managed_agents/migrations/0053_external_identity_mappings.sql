CREATE TABLE IF NOT EXISTS "LiteLLM_ExternalIdentityMappingsTable" (
    id TEXT PRIMARY KEY,
    issuer TEXT NOT NULL,
    subject TEXT NOT NULL,
    audience TEXT NOT NULL DEFAULT '',
    platform_user_id TEXT REFERENCES "LiteLLM_UsersTable" (id) ON DELETE SET NULL,
    platform_agent_id TEXT REFERENCES "LiteLLM_ManagedAgentsTable" (id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    claims_digest TEXT NOT NULL,
    evidence JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    last_seen_at BIGINT NOT NULL,
    bound_by TEXT,
    bound_at BIGINT,
    UNIQUE (issuer, subject, audience),
    CHECK (status IN ('pending', 'active', 'blocked')),
    CHECK (status <> 'active' OR platform_user_id IS NOT NULL)
);

CREATE INDEX IF NOT EXISTS "LiteLLM_ExternalIdentityMappings_status_idx"
  ON "LiteLLM_ExternalIdentityMappingsTable" (status, last_seen_at DESC);

CREATE INDEX IF NOT EXISTS "LiteLLM_ExternalIdentityMappings_user_idx"
  ON "LiteLLM_ExternalIdentityMappingsTable" (platform_user_id)
  WHERE platform_user_id IS NOT NULL;
