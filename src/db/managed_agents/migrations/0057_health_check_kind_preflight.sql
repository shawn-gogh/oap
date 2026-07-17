-- Health checks gained a per-run summary record (check_kind='preflight') so
-- the auto-pause logic can count consecutive failed runs; the original CHECK
-- constraint predates it.
ALTER TABLE "LiteLLM_AgentHealthChecksTable"
    DROP CONSTRAINT IF EXISTS "LiteLLM_AgentHealthChecksTable_check_kind_check";
ALTER TABLE "LiteLLM_AgentHealthChecksTable"
    ADD CONSTRAINT "LiteLLM_AgentHealthChecksTable_check_kind_check"
    CHECK (check_kind IN ('source', 'runtime', 'model', 'tool', 'mcp', 'credential', 'conformance', 'preflight'));
