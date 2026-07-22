UPDATE "LiteLLM_ManagedAgentsTable"
SET config = jsonb_set(
  COALESCE(config, '{}'::JSONB),
  '{interaction_profile}',
  COALESCE(config->'interaction_profile', '{}'::JSONB) || jsonb_build_object(
    'primary_surface', 'run',
    'execution_mode', 'async_stream',
    'progress_mode', 'steps',
    'continuation_modes', '["input","approval","choice","file_upload"]'::JSONB,
    'artifact_media_types', '["application/json","text/plain","image/*","audio/*","video/*","application/pdf"]'::JSONB,
    'supports_checkpoint_resume', true,
    'supports_child_invocations', true,
    'supports_retry', true
  ),
  true
)
WHERE config->>'runtime' = 'langgraph_assistant'
   OR config#>>'{source,api_spec}' = 'langgraph_assistant';
