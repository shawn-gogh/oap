use serde::{Deserialize, Serialize};

/// Stored under `agent.config["mattermost"]`. Unlike Slack's OAuth App
/// Manifest flow, Mattermost self-hosted bots are set up manually by an
/// admin in the Mattermost server (create a bot account, mint a Personal
/// Access Token, configure an Outgoing Webhook pointing back at OAP) — so
/// this only needs to record where those pieces live, not drive an OAuth
/// exchange.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub(crate) struct MattermostAgentConfig {
    /// Base URL of the Mattermost server, e.g. `https://chat.example.com`.
    pub server_url: Option<String>,
    /// Vault key name holding the bot account's Personal Access Token.
    pub bot_token_key: Option<String>,
    /// Vault key name holding the Outgoing Webhook's verification token.
    pub webhook_token_key: Option<String>,
    /// The bot account's own Mattermost user id, resolved via `/users/me`
    /// when the connection is saved. Used solely to recognize and ignore
    /// the bot's own posts — otherwise a reply would re-trigger the
    /// outgoing webhook and the agent would talk to itself forever.
    pub bot_user_id: Option<String>,
    pub notification_channel_id: Option<String>,
    pub status: Option<String>,
    pub connected_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub(super) struct MattermostIncomingMessage {
    pub channel_id: String,
    /// Normalized thread-root post id: the outgoing webhook's `root_id` when
    /// this is a reply, else the triggering post's own `post_id` (Mattermost
    /// leaves `root_id` empty on a thread's first post).
    pub root_id: String,
    pub post_id: String,
    pub team_id: Option<String>,
    pub user_id: Option<String>,
    pub prompt: String,
    pub requires_existing_thread: bool,
}
