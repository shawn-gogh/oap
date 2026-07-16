INSERT INTO "LiteLLM_ManagedAgentGovernanceTable" (
  agent_id,
  owner_id,
  source_provider,
  source_endpoint,
  external_agent_id,
  source_version,
  source_hash,
  lifecycle_status,
  runtime_health,
  health_detail,
  credential_scope,
  credential_name,
  tested_revision,
  published_revision,
  created_at,
  updated_at
)
SELECT
  agent.id,
  COALESCE(NULLIF(agent.owner_id, ''), 'system'),
  COALESCE(NULLIF(agent.config #>> '{source,provider}', ''), 'opencode'),
  CASE agent.config #>> '{source,kind}'
    WHEN 'agent_bundle' THEN
      'agent-bundle://legacy/' || agent.id
    ELSE 'opencode-file://legacy/' || agent.id
  END,
  COALESCE(
    NULLIF(agent.config #>> '{source,external_agent_id}', ''),
    NULLIF(agent.config #>> '{source,filename}', ''),
    agent.id
  ),
  1,
  md5((agent.config -> 'source')::text),
  CASE WHEN agent.status = 'active' THEN 'published' ELSE 'imported' END,
  'unknown',
  CASE
    WHEN agent.status = 'active'
      THEN '历史文件导入智能体已保留激活状态，需要重新执行纳管检查。'
    ELSE NULL
  END,
  'byo',
  NULL,
  CASE
    WHEN agent.status = 'active' THEN (
      SELECT MAX(revision.version)
      FROM "LiteLLM_ManagedAgentRevisionsTable" revision
      WHERE revision.agent_id = agent.id
    )
    ELSE NULL
  END,
  CASE
    WHEN agent.status = 'active' THEN (
      SELECT MAX(revision.version)
      FROM "LiteLLM_ManagedAgentRevisionsTable" revision
      WHERE revision.agent_id = agent.id
    )
    ELSE NULL
  END,
  agent.created_at,
  agent.created_at
FROM "LiteLLM_ManagedAgentsTable" agent
WHERE agent.config #>> '{source,kind}' IN ('opencode_agent_file', 'agent_bundle')
ON CONFLICT (agent_id) DO NOTHING;

INSERT INTO "LiteLLM_AuditLogsTable" (
  id,
  actor_user_id,
  action,
  target_type,
  target_id,
  metadata,
  created_at
)
SELECT
  'audit_' || md5('governance-backfill:' || agent.id),
  'system',
  'agent.governance.backfilled',
  'agent',
  agent.id,
  jsonb_build_object(
    'source_kind', agent.config #>> '{source,kind}',
    'preserved_status', agent.status
  ),
  agent.created_at
FROM "LiteLLM_ManagedAgentsTable" agent
WHERE agent.config #>> '{source,kind}' IN ('opencode_agent_file', 'agent_bundle')
  AND NOT EXISTS (
    SELECT 1
    FROM "LiteLLM_AuditLogsTable" audit
    WHERE audit.action = 'agent.governance.backfilled'
      AND audit.target_type = 'agent'
      AND audit.target_id = agent.id
  );
