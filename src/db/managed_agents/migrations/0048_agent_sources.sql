CREATE TABLE IF NOT EXISTS "LiteLLM_AgentSourceConnectorsTable" (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    name TEXT NOT NULL,
    provider TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    credential_name TEXT,
    status TEXT NOT NULL DEFAULT 'unknown',
    capabilities JSONB NOT NULL DEFAULT '{}'::JSONB,
    last_test_detail TEXT,
    last_test_at BIGINT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    UNIQUE (owner_id, provider, endpoint),
    CHECK (status IN ('unknown', 'healthy', 'degraded', 'unreachable', 'disabled'))
);

CREATE TABLE IF NOT EXISTS "LiteLLM_ManagedAgentSourcesTable" (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL UNIQUE REFERENCES "LiteLLM_ManagedAgentsTable" (id) ON DELETE CASCADE,
    connector_id TEXT REFERENCES "LiteLLM_AgentSourceConnectorsTable" (id) ON DELETE SET NULL,
    management_mode TEXT NOT NULL,
    sync_state TEXT NOT NULL DEFAULT 'unknown',
    missing_count INTEGER NOT NULL DEFAULT 0,
    current_snapshot_id TEXT,
    candidate_snapshot_id TEXT,
    last_synced_at BIGINT,
    next_sync_at BIGINT,
    lease_owner TEXT,
    lease_until BIGINT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    CHECK (management_mode IN ('federated', 'mirrored', 'managed')),
    CHECK (sync_state IN ('unknown', 'in_sync', 'drifted', 'missing', 'sync_error', 'detached'))
);

CREATE TABLE IF NOT EXISTS "LiteLLM_AgentSourceSnapshotsTable" (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES "LiteLLM_ManagedAgentSourcesTable" (id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    digest TEXT NOT NULL,
    raw_spec JSONB NOT NULL,
    canonical_spec JSONB NOT NULL,
    normalization_issues JSONB NOT NULL DEFAULT '[]'::JSONB,
    agent_revision INTEGER,
    created_by TEXT,
    created_at BIGINT NOT NULL,
    UNIQUE (source_id, version),
    UNIQUE (source_id, digest)
);

ALTER TABLE "LiteLLM_ManagedAgentSourcesTable"
  ADD CONSTRAINT "LiteLLM_ManagedAgentSourcesTable_current_snapshot_fk"
  FOREIGN KEY (current_snapshot_id) REFERENCES "LiteLLM_AgentSourceSnapshotsTable" (id) ON DELETE SET NULL;

ALTER TABLE "LiteLLM_ManagedAgentSourcesTable"
  ADD CONSTRAINT "LiteLLM_ManagedAgentSourcesTable_candidate_snapshot_fk"
  FOREIGN KEY (candidate_snapshot_id) REFERENCES "LiteLLM_AgentSourceSnapshotsTable" (id) ON DELETE SET NULL;

CREATE TABLE IF NOT EXISTS "LiteLLM_AgentSourceSyncRunsTable" (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES "LiteLLM_ManagedAgentSourcesTable" (id) ON DELETE CASCADE,
    connector_id TEXT REFERENCES "LiteLLM_AgentSourceConnectorsTable" (id) ON DELETE SET NULL,
    status TEXT NOT NULL,
    trigger_kind TEXT NOT NULL,
    cursor_before TEXT,
    cursor_after TEXT,
    discovered_count INTEGER NOT NULL DEFAULT 0,
    changed_count INTEGER NOT NULL DEFAULT 0,
    missing_count INTEGER NOT NULL DEFAULT 0,
    error_detail TEXT,
    started_at BIGINT NOT NULL,
    finished_at BIGINT,
    CHECK (status IN ('running', 'succeeded', 'partial', 'failed')),
    CHECK (trigger_kind IN ('manual', 'scheduled', 'webhook', 'import'))
);

CREATE TABLE IF NOT EXISTS "LiteLLM_AgentDriftFindingsTable" (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES "LiteLLM_ManagedAgentSourcesTable" (id) ON DELETE CASCADE,
    snapshot_id TEXT NOT NULL REFERENCES "LiteLLM_AgentSourceSnapshotsTable" (id) ON DELETE CASCADE,
    field_path TEXT NOT NULL,
    risk TEXT NOT NULL,
    previous_value JSONB,
    candidate_value JSONB,
    resolution TEXT NOT NULL DEFAULT 'open',
    created_at BIGINT NOT NULL,
    resolved_at BIGINT,
    CHECK (risk IN ('low', 'medium', 'high', 'critical')),
    CHECK (resolution IN ('open', 'accepted', 'rejected', 'superseded'))
);

CREATE TABLE IF NOT EXISTS "LiteLLM_AgentHealthChecksTable" (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES "LiteLLM_ManagedAgentsTable" (id) ON DELETE CASCADE,
    source_id TEXT REFERENCES "LiteLLM_ManagedAgentSourcesTable" (id) ON DELETE CASCADE,
    check_kind TEXT NOT NULL,
    status TEXT NOT NULL,
    detail TEXT,
    latency_ms BIGINT,
    checked_at BIGINT NOT NULL,
    CHECK (check_kind IN ('source', 'runtime', 'model', 'tool', 'mcp', 'credential', 'conformance')),
    CHECK (status IN ('healthy', 'degraded', 'unhealthy', 'unreachable', 'unknown'))
);

CREATE TABLE IF NOT EXISTS "LiteLLM_AgentRuntimeConformanceTable" (
    agent_id TEXT PRIMARY KEY REFERENCES "LiteLLM_ManagedAgentsTable" (id) ON DELETE CASCADE,
    contract_version TEXT NOT NULL,
    status TEXT NOT NULL,
    checks JSONB NOT NULL DEFAULT '[]'::JSONB,
    checked_revision INTEGER,
    checked_at BIGINT NOT NULL,
    CHECK (status IN ('unknown', 'conformant', 'partial', 'non_conformant'))
);

CREATE TABLE IF NOT EXISTS "LiteLLM_AgentSessionCapabilityTokensTable" (
    session_id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES "LiteLLM_ManagedAgentsTable" (id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    capabilities JSONB NOT NULL DEFAULT '[]'::JSONB,
    expires_at BIGINT NOT NULL,
    revoked_at BIGINT,
    created_at BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS "LiteLLM_AgentConnectorWebhookDeliveriesTable" (
    connector_id TEXT NOT NULL REFERENCES "LiteLLM_AgentSourceConnectorsTable" (id) ON DELETE CASCADE,
    event_id TEXT NOT NULL,
    received_at BIGINT NOT NULL,
    PRIMARY KEY (connector_id, event_id)
);

ALTER TABLE "LiteLLM_ManagedAgentGovernanceTable"
  DROP CONSTRAINT IF EXISTS "LiteLLM_ManagedAgentGovernanceTable_lifecycle_status_check";
ALTER TABLE "LiteLLM_ManagedAgentGovernanceTable"
  ADD CONSTRAINT "LiteLLM_ManagedAgentGovernanceTable_lifecycle_status_check"
  CHECK (lifecycle_status IN (
    'imported', 'tested', 'pending_approval', 'published', 'unhealthy',
    'rolled_back', 'mapping_failed', 'suspended', 'retired'
  ));

ALTER TABLE "LiteLLM_ManagedAgentGovernanceTable"
  DROP CONSTRAINT IF EXISTS "LiteLLM_ManagedAgentGovernanceTable_runtime_health_check";
ALTER TABLE "LiteLLM_ManagedAgentGovernanceTable"
  ADD CONSTRAINT "LiteLLM_ManagedAgentGovernanceTable_runtime_health_check"
  CHECK (runtime_health IN ('unknown', 'healthy', 'degraded', 'unhealthy', 'unreachable'));

CREATE INDEX IF NOT EXISTS "LiteLLM_AgentSources_sync_idx"
  ON "LiteLLM_ManagedAgentSourcesTable" (sync_state, next_sync_at);
CREATE INDEX IF NOT EXISTS "LiteLLM_AgentSyncRuns_source_idx"
  ON "LiteLLM_AgentSourceSyncRunsTable" (source_id, started_at DESC);
CREATE INDEX IF NOT EXISTS "LiteLLM_AgentDrift_source_idx"
  ON "LiteLLM_AgentDriftFindingsTable" (source_id, resolution, created_at DESC);
CREATE INDEX IF NOT EXISTS "LiteLLM_AgentHealth_agent_idx"
  ON "LiteLLM_AgentHealthChecksTable" (agent_id, checked_at DESC);

INSERT INTO "LiteLLM_ManagedAgentSourcesTable" (
  id, agent_id, management_mode, sync_state, missing_count,
  last_synced_at, created_at, updated_at
)
SELECT
  'src_' || md5(governance.agent_id),
  governance.agent_id,
  CASE agent.config #>> '{source,kind}'
    WHEN 'external_agent' THEN 'federated'
    ELSE 'managed'
  END,
  'in_sync',
  0,
  governance.updated_at,
  governance.created_at,
  governance.updated_at
FROM "LiteLLM_ManagedAgentGovernanceTable" governance
JOIN "LiteLLM_ManagedAgentsTable" agent ON agent.id = governance.agent_id
ON CONFLICT (agent_id) DO NOTHING;

INSERT INTO "LiteLLM_AgentSourceSnapshotsTable" (
  id, source_id, version, digest, raw_spec, canonical_spec,
  normalization_issues, agent_revision, created_by, created_at
)
SELECT
  'snap_' || md5(source.agent_id || ':1'),
  source.id,
  governance.source_version,
  governance.source_hash,
  COALESCE(agent.config -> 'source' -> 'raw', agent.config -> 'source', '{}'::JSONB),
  jsonb_build_object(
    'spec_version', 'legacy-backfill',
    'identity', jsonb_build_object(
      'platform_agent_id', agent.id,
      'external_agent_id', governance.external_agent_id,
      'source_provider', governance.source_provider,
      'name', agent.name,
      'description', agent.description
    ),
    'execution', jsonb_build_object(
      'runtime', agent.config -> 'runtime',
      'model', agent.model,
      'harness', agent.harness,
      'max_runtime_minutes', agent.max_runtime_minutes,
      'on_failure', agent.on_failure
    )
  ),
  '[{"severity":"warning","code":"legacy_backfill","field":"canonical_spec","message":"历史来源快照需要在下次同步时重新归一化。"}]'::JSONB,
  governance.published_revision,
  'migration',
  governance.updated_at
FROM "LiteLLM_ManagedAgentSourcesTable" source
JOIN "LiteLLM_ManagedAgentGovernanceTable" governance ON governance.agent_id = source.agent_id
JOIN "LiteLLM_ManagedAgentsTable" agent ON agent.id = source.agent_id
ON CONFLICT (source_id, digest) DO NOTHING;

UPDATE "LiteLLM_ManagedAgentSourcesTable" source
SET current_snapshot_id = snapshot.id
FROM "LiteLLM_AgentSourceSnapshotsTable" snapshot
WHERE snapshot.source_id = source.id AND source.current_snapshot_id IS NULL;
