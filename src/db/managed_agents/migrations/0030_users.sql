CREATE TABLE IF NOT EXISTS "LiteLLM_UsersTable" (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    email TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    CHECK (status IN ('active', 'disabled'))
);

CREATE UNIQUE INDEX IF NOT EXISTS "LiteLLM_UsersTable_email_unique"
  ON "LiteLLM_UsersTable" (LOWER(email))
  WHERE email IS NOT NULL;

INSERT INTO "LiteLLM_UsersTable" (id, display_name, status, created_at, updated_at)
SELECT DISTINCT user_id, user_id, 'active', created_at, created_at
FROM "LiteLLM_GatewayApiKeysTable"
WHERE user_id IS NOT NULL AND btrim(user_id) <> ''
ON CONFLICT (id) DO NOTHING;
