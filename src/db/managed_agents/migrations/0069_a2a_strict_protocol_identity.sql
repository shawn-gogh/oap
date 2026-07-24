ALTER TABLE "LiteLLM_AgentSourceSnapshotsTable"
  ADD COLUMN IF NOT EXISTS protocol_profile JSONB NOT NULL DEFAULT '{}'::JSONB;

UPDATE "LiteLLM_AgentSourceConnectorsTable"
SET protocol = 'a2a',
    protocol_version = CASE
      WHEN protocol_version IN ('0.3', '1.0') THEN protocol_version
      ELSE 'unverified'
    END
WHERE (adapter_id = 'a2a' OR provider = 'a2a')
  AND (
    protocol IS DISTINCT FROM 'a2a'
    OR protocol_version IS NULL
    OR protocol_version NOT IN ('unverified', '0.3', '1.0')
  );
