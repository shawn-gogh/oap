ALTER TABLE "LiteLLM_GatewayApiKeysTable"
  DROP CONSTRAINT IF EXISTS gateway_api_keys_role_check;

ALTER TABLE "LiteLLM_GatewayApiKeysTable"
  ADD CONSTRAINT gateway_api_keys_role_check
  CHECK (role IN ('admin', 'user', 'importer', 'approver', 'operator'));

ALTER TABLE "LiteLLM_WebSessionsTable"
  ADD COLUMN IF NOT EXISTS role TEXT NOT NULL DEFAULT 'user';

UPDATE "LiteLLM_WebSessionsTable"
SET role = 'admin'
WHERE is_admin = TRUE;

ALTER TABLE "LiteLLM_WebSessionsTable"
  DROP CONSTRAINT IF EXISTS web_sessions_role_check;

ALTER TABLE "LiteLLM_WebSessionsTable"
  ADD CONSTRAINT web_sessions_role_check
  CHECK (role IN ('admin', 'user', 'importer', 'approver', 'operator'));
