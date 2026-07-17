CREATE TABLE IF NOT EXISTS "LiteLLM_CloudEventReceiptsTable" (
    id TEXT PRIMARY KEY,
    direction TEXT NOT NULL,
    session_id TEXT NOT NULL REFERENCES "LiteLLM_ManagedAgentSessionsTable" (id) ON DELETE CASCADE,
    cloud_event_id TEXT NOT NULL,
    cloud_event_source TEXT NOT NULL,
    cloud_event_type TEXT NOT NULL,
    subject TEXT,
    data_digest TEXT NOT NULL,
    canonical_event_key TEXT NOT NULL,
    actor_user_id TEXT NOT NULL,
    first_seen_at BIGINT NOT NULL,
    last_seen_at BIGINT NOT NULL,
    delivery_count INTEGER NOT NULL DEFAULT 1,
    UNIQUE (direction, session_id, cloud_event_source, cloud_event_id),
    CHECK (direction IN ('ingress', 'egress')),
    CHECK (delivery_count > 0)
);

CREATE INDEX IF NOT EXISTS "LiteLLM_CloudEventReceipts_session_idx"
  ON "LiteLLM_CloudEventReceiptsTable" (session_id, direction, first_seen_at DESC);
