-- OAP dropped Slack, Microsoft Teams, and Google Chat as sharing channels in
-- favor of a Mattermost-only integration. Their tables are no longer read or
-- written by any code path; drop them rather than leaving dead schema
-- around. This migration is additive (a new forward migration), not a rewrite
-- of the historical migrations that created these tables.
DROP TABLE IF EXISTS "LiteLLM_ManagedAgentSlackThreadSessionsTable";
DROP TABLE IF EXISTS "LiteLLM_ManagedAgentSlackEventsTable";
DROP TABLE IF EXISTS "LiteLLM_ManagedAgentSlackOAuthStatesTable";
DROP TABLE IF EXISTS "LiteLLM_SlackAgentBindingsTable";
DROP TABLE IF EXISTS "LiteLLM_SlackPendingInstallsTable";
DROP TABLE IF EXISTS "LiteLLM_ManagedAgentTeamsConversationSessionsTable";
DROP TABLE IF EXISTS "LiteLLM_ManagedAgentTeamsEventsTable";
DROP TABLE IF EXISTS "LiteLLM_ManagedAgentGoogleChatSpaceSessionsTable";
DROP TABLE IF EXISTS "LiteLLM_ManagedAgentGoogleChatEventsTable";
