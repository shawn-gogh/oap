CREATE TABLE IF NOT EXISTS "LiteLLM_GroupsTable" (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_by TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    CHECK (status IN ('active', 'disabled'))
);

CREATE TABLE IF NOT EXISTS "LiteLLM_GroupMembersTable" (
    group_id TEXT NOT NULL REFERENCES "LiteLLM_GroupsTable" (id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES "LiteLLM_UsersTable" (id) ON DELETE CASCADE,
    member_role TEXT NOT NULL DEFAULT 'member',
    added_by TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (group_id, user_id),
    CHECK (member_role IN ('member', 'group_admin'))
);

CREATE INDEX IF NOT EXISTS "LiteLLM_GroupMembersTable_user_idx"
  ON "LiteLLM_GroupMembersTable" (user_id);

CREATE TABLE IF NOT EXISTS "LiteLLM_AgentGroupGrantsTable" (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    group_id TEXT NOT NULL REFERENCES "LiteLLM_GroupsTable" (id) ON DELETE CASCADE,
    permission TEXT NOT NULL DEFAULT 'use',
    granted_by TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    UNIQUE (agent_id, group_id),
    CHECK (permission IN ('use', 'edit'))
);

CREATE INDEX IF NOT EXISTS "LiteLLM_AgentGroupGrantsTable_agent_idx"
  ON "LiteLLM_AgentGroupGrantsTable" (agent_id);
