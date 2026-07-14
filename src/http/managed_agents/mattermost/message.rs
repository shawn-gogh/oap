use serde::Deserialize;

use super::types::MattermostIncomingMessage;

/// Mattermost Outgoing Webhook payload — posted as
/// `application/x-www-form-urlencoded`. Reference: Mattermost docs,
/// "Outgoing Webhooks". `root_id` is empty on a thread's first post and set
/// to that post's id on every reply within the thread.
#[derive(Debug, Deserialize)]
pub(crate) struct OutgoingWebhookPayload {
    pub token: String,
    #[serde(default)]
    pub team_id: String,
    pub channel_id: String,
    #[serde(default)]
    pub user_id: String,
    pub post_id: String,
    #[serde(default)]
    pub root_id: String,
    #[serde(default)]
    pub text: String,
}

pub(super) fn incoming_message(payload: &OutgoingWebhookPayload) -> MattermostIncomingMessage {
    let requires_existing_thread = !payload.root_id.trim().is_empty();
    let root_id = if requires_existing_thread {
        payload.root_id.clone()
    } else {
        payload.post_id.clone()
    };
    MattermostIncomingMessage {
        channel_id: payload.channel_id.clone(),
        root_id,
        post_id: payload.post_id.clone(),
        team_id: (!payload.team_id.trim().is_empty()).then(|| payload.team_id.clone()),
        user_id: (!payload.user_id.trim().is_empty()).then(|| payload.user_id.clone()),
        prompt: clean_prompt(&payload.text),
        requires_existing_thread,
    }
}

fn clean_prompt(text: &str) -> String {
    match text.trim() {
        "" => "Proceed with your task.".to_owned(),
        trimmed => trimmed.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{incoming_message, OutgoingWebhookPayload};

    fn payload(post_id: &str, root_id: &str, text: &str) -> OutgoingWebhookPayload {
        OutgoingWebhookPayload {
            token: "tok".to_owned(),
            team_id: "t1".to_owned(),
            channel_id: "c1".to_owned(),
            user_id: "u1".to_owned(),
            post_id: post_id.to_owned(),
            root_id: root_id.to_owned(),
            text: text.to_owned(),
        }
    }

    #[test]
    fn new_top_level_post_becomes_its_own_thread_root() {
        let message = incoming_message(&payload("p1", "", "hello"));
        assert_eq!(message.root_id, "p1");
        assert!(!message.requires_existing_thread);
    }

    #[test]
    fn thread_reply_binds_to_existing_root_and_requires_it() {
        let message = incoming_message(&payload("p2", "p1", "follow up"));
        assert_eq!(message.root_id, "p1");
        assert!(message.requires_existing_thread);
    }

    #[test]
    fn blank_text_falls_back_to_a_default_prompt() {
        let message = incoming_message(&payload("p1", "", "   "));
        assert_eq!(message.prompt, "Proceed with your task.");
    }
}
