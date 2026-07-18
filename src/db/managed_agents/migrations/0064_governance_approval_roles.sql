UPDATE "LiteLLM_ManagedAgentInboxItemsTable"
SET required_role = 'approver'
WHERE status = 'pending'
  AND kind IN ('agent_publish', 'unlisted_data_egress', 'data_egress');

UPDATE "LiteLLM_ManagedAgentInboxItemsTable"
SET required_role = 'operator'
WHERE status = 'pending'
  AND kind = 'platform_action';
