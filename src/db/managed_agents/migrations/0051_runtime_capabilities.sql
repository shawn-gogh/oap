UPDATE "LiteLLM_ManagedAgentsTable"
SET config = COALESCE(config, '{}'::JSONB) || jsonb_build_object(
  'runtime_capabilities',
  COALESCE(config->'runtime_capabilities', '{}'::JSONB)
    || '{"session_workspace": true}'::JSONB
)
WHERE config->>'runtime' = 'local-opencode'
  AND config#>'{runtime_capabilities,session_workspace}' IS NULL;
