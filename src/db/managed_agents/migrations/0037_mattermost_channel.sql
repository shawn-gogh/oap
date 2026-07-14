-- Mattermost sharing channel: thread <-> session binding, mirroring the
-- shape of the removed Slack thread-session table (see
-- 0036_drop_legacy_channels.sql), plus event dedup for the outgoing
-- webhook (Mattermost does not guarantee at-most-once delivery).
CREATE TABLE IF NOT EXISTS "LiteLLM_ManagedAgentMattermostThreadSessionsTable" (
  agent_id TEXT NOT NULL,
  channel_id TEXT NOT NULL,
  root_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  PRIMARY KEY (agent_id, channel_id, root_id)
);

CREATE INDEX IF NOT EXISTS "LiteLLM_ManagedAgentMattermostThreadSessions_session_idx"
  ON "LiteLLM_ManagedAgentMattermostThreadSessionsTable" (session_id);

CREATE TABLE IF NOT EXISTS "LiteLLM_ManagedAgentMattermostEventsTable" (
  agent_id TEXT NOT NULL,
  event_id TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  PRIMARY KEY (agent_id, event_id)
);
