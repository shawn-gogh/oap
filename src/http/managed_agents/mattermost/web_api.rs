use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

use crate::errors::GatewayError;

/// Mattermost's post body hard limit is 16383 characters.
pub(super) const MAX_TEXT_CHARS: usize = 16_000;

#[derive(Debug, Deserialize)]
struct PostResponse {
    id: Option<String>,
    message: Option<String>,
}

pub struct UpsertPostParams<'a> {
    pub client: &'a Client,
    pub server_url: &'a str,
    pub bot_token: &'a str,
    pub channel_id: &'a str,
    pub root_id: &'a str,
    pub post_id: Option<&'a str>,
    pub text: &'a str,
}

/// Creates a post if `post_id` is `None`, otherwise edits the existing post
/// in place — mirrors Slack's `chat.postMessage`/`chat.update` pair, giving
/// the same "stream into one message" UX via Mattermost's REST API.
pub(super) async fn upsert_post(params: UpsertPostParams<'_>) -> Result<String, GatewayError> {
    match params.post_id {
        Some(post_id) => {
            update_post(
                params.client,
                params.server_url,
                params.bot_token,
                post_id,
                params.text,
            )
            .await?;
            Ok(post_id.to_owned())
        }
        None => {
            create_post(
                params.client,
                params.server_url,
                params.bot_token,
                params.channel_id,
                params.root_id,
                params.text,
            )
            .await
        }
    }
}

async fn create_post(
    client: &Client,
    server_url: &str,
    bot_token: &str,
    channel_id: &str,
    root_id: &str,
    text: &str,
) -> Result<String, GatewayError> {
    let response = client
        .post(api_url(server_url, "/posts"))
        .bearer_auth(bot_token)
        .json(&json!({
            "channel_id": channel_id,
            "root_id": root_id,
            "message": truncate(text),
        }))
        .send()
        .await
        .map_err(GatewayError::Upstream)?;
    parse_post_response(response, "create post").await
}

pub(super) async fn create_channel_post(
    client: &Client,
    server_url: &str,
    bot_token: &str,
    channel_id: &str,
    text: &str,
) -> Result<String, GatewayError> {
    create_post(client, server_url, bot_token, channel_id, "", text).await
}

async fn update_post(
    client: &Client,
    server_url: &str,
    bot_token: &str,
    post_id: &str,
    text: &str,
) -> Result<(), GatewayError> {
    let response = client
        .put(api_url(server_url, &format!("/posts/{post_id}/patch")))
        .bearer_auth(bot_token)
        .json(&json!({ "message": truncate(text) }))
        .send()
        .await
        .map_err(GatewayError::Upstream)?;
    parse_post_response(response, "update post")
        .await
        .map(|_| ())
}

async fn parse_post_response(
    response: reqwest::Response,
    action: &str,
) -> Result<String, GatewayError> {
    let status = response.status();
    let body: PostResponse = response.json().await.map_err(GatewayError::Upstream)?;
    if status.is_success() {
        body.id.ok_or_else(|| {
            mattermost_api_error(action, Some("response omitted post id".to_owned()))
        })
    } else {
        Err(mattermost_api_error(action, body.message))
    }
}

fn api_url(server_url: &str, path: &str) -> String {
    format!("{}/api/v4{path}", server_url.trim_end_matches('/'))
}

fn mattermost_api_error(action: &str, message: Option<String>) -> GatewayError {
    GatewayError::SandboxError(format!(
        "mattermost {action} failed: {}",
        message.unwrap_or_else(|| "unknown_error".to_owned())
    ))
}

fn truncate(text: &str) -> String {
    text.chars().take(MAX_TEXT_CHARS).collect()
}

#[derive(Debug, Deserialize)]
struct MeResponse {
    id: String,
}

/// Resolves the bot account's own user id via `GET /users/me`, both to
/// validate the token at connect-time and to give `events.rs` something to
/// compare inbound posts against so the bot never replies to itself.
pub(crate) async fn verify_bot_token(
    client: &Client,
    server_url: &str,
    bot_token: &str,
) -> Result<String, GatewayError> {
    let response = client
        .get(api_url(server_url, "/users/me"))
        .bearer_auth(bot_token)
        .send()
        .await
        .map_err(GatewayError::Upstream)?;
    if !response.status().is_success() {
        return Err(GatewayError::InvalidConfig(
            "mattermost bot token rejected by /users/me — check the server URL and token"
                .to_owned(),
        ));
    }
    let me: MeResponse = response.json().await.map_err(GatewayError::Upstream)?;
    Ok(me.id)
}

#[cfg(test)]
mod tests {
    use reqwest::Client;
    use serde_json::Value;
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use super::create_channel_post;

    #[tokio::test]
    async fn channel_post_uses_bot_token_and_target_channel() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v4/posts"))
            .and(header("authorization", "Bearer bot-token"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": "post-1"
            })))
            .mount(&server)
            .await;

        let post_id = create_channel_post(
            &Client::new(),
            &server.uri(),
            "bot-token",
            "channel-1",
            "governance alert",
        )
        .await
        .unwrap();

        assert_eq!(post_id, "post-1");
        let requests = server.received_requests().await.unwrap();
        let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(body["channel_id"], "channel-1");
        assert_eq!(body["root_id"], "");
        assert_eq!(body["message"], "governance alert");
    }
}
