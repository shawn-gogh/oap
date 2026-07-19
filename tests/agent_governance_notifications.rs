#[path = "managed_agents_support/mod.rs"]
#[allow(dead_code)]
mod support;

use serde_json::{json, Value};
use support::{request_json, request_json_raw_with_key, AppFixture};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn publish_request_reaches_configured_mattermost_channel() {
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping notification integration test: TEST_DATABASE_URL is not set");
        return;
    };
    let (owner_id, owner_key) = create_owner(&fixture).await;
    let agent_id = create_governed_agent(&fixture, &owner_id).await;
    let mattermost = mattermost_server().await;
    connect_mattermost(&fixture, &owner_key, &agent_id, &mattermost).await;
    mark_agent_tested(&fixture, &agent_id).await;
    request_with_key(
        &fixture,
        &owner_key,
        "POST",
        &format!("/api/agents/{agent_id}/governance/request-publish"),
        Some(json!({})),
    )
    .await;
    assert_notification(&mattermost).await;
}

async fn create_owner(fixture: &AppFixture) -> (String, String) {
    let owner = litellm_rust::db::managed_agents::users::repository::create(
        &fixture.pool,
        "notification-owner",
        "通知负责人",
        None,
    )
    .await
    .unwrap();
    let key = litellm_rust::db::managed_agents::api_keys::repository::create(
        &fixture.pool,
        Some("notification-owner-key"),
        Some(&owner.id),
        Some("user"),
    )
    .await
    .unwrap()
    .key;
    (owner.id, key)
}

async fn create_governed_agent(fixture: &AppFixture, owner_id: &str) -> String {
    let agent = request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents",
        Some(json!({
            "name": "governance-notification-agent",
            "owner_id": owner_id,
            "model": "test-model",
            "system": "test",
            "tools": [],
            "config": {},
        })),
    )
    .await;
    let agent_id = agent["id"].as_str().unwrap().to_owned();
    litellm_rust::db::managed_agents::governance::record_import(
        &fixture.pool,
        litellm_rust::db::managed_agents::governance::ImportedSource {
            agent_id: &agent_id,
            owner_id,
            provider: "external-test",
            endpoint: "https://runtime.example.test",
            external_agent_id: "external-1",
            source_hash: "source-v1",
            credential_scope: "personal",
            credential_name: None,
        },
    )
    .await
    .unwrap();
    agent_id
}

async fn mattermost_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v4/users/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "bot-1" })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v4/posts"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "id": "post-1" })))
        .mount(&server)
        .await;
    server
}

async fn connect_mattermost(
    fixture: &AppFixture,
    owner_key: &str,
    agent_id: &str,
    server: &MockServer,
) {
    request_with_key(
        fixture,
        owner_key,
        "POST",
        &format!("/api/agents/{agent_id}/mattermost/connect"),
        Some(json!({
            "server_url": server.uri(),
            "bot_token": "bot-token",
            "webhook_token": "webhook-token",
            "notification_channel_id": "governance-channel",
        })),
    )
    .await;
}

async fn mark_agent_tested(fixture: &AppFixture, agent_id: &str) {
    let revision = litellm_rust::db::managed_agents::registry::revisions::latest_version(
        &fixture.pool,
        agent_id,
    )
    .await
    .unwrap()
    .unwrap();
    litellm_rust::db::managed_agents::governance::mark_tested(
        &fixture.pool,
        agent_id,
        revision,
        true,
        "healthy",
    )
    .await
    .unwrap();
}

async fn assert_notification(server: &MockServer) {
    let requests = server.received_requests().await.unwrap();
    let post = requests
        .iter()
        .find(|request| request.url.path() == "/api/v4/posts")
        .unwrap();
    let body: Value = serde_json::from_slice(&post.body).unwrap();
    assert_eq!(body["channel_id"], "governance-channel");
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains("待审批：智能体发布"));
}

async fn request_with_key(
    fixture: &AppFixture,
    key: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> Value {
    let (status, response) =
        request_json_raw_with_key(fixture.app.clone(), method, path, body, key).await;
    assert!(status.is_success(), "{method} {path}: {status} {response}");
    serde_json::from_str(&response).unwrap_or_else(|_| json!({}))
}
