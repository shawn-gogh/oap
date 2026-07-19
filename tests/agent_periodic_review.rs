#[path = "managed_agents_support/mod.rs"]
#[allow(dead_code)]
mod support;

use axum::http::StatusCode;
use serde_json::{json, Value};
use support::{request_json, request_json_raw_with_key, AppFixture};

#[tokio::test]
async fn published_agents_expire_once_and_complete_review_before_resuming() {
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping periodic review test: TEST_DATABASE_URL is not set");
        return;
    };
    let (agent_id, published_at) = publish_with_review_period(&fixture).await;
    let now = expire_review(&fixture, &agent_id).await;
    assert_paused(&fixture, &agent_id).await;
    renew_review(&fixture, &agent_id, published_at, now).await;
}

async fn publish_with_review_period(fixture: &AppFixture) -> (String, i64) {
    let settings = request_json(
        fixture.app.clone(),
        "PUT",
        "/api/governance/settings",
        Some(json!({
            "separation_of_duties": true,
            "review_period_days": 30,
        })),
    )
    .await;
    assert_eq!(settings["review_period_days"], 30);

    let agent_id = governed_agent(fixture).await;
    let approval_id = request_publish(fixture, &agent_id).await;
    approve(fixture, &approval_id).await;
    let published = governance(fixture, &agent_id).await;
    let published_at = published["governance"]["published_at"].as_i64().unwrap();
    assert_eq!(
        published["governance"]["review_due_at"].as_i64(),
        Some(published_at + 30 * 86_400_000)
    );
    (agent_id, published_at)
}

async fn expire_review(fixture: &AppFixture, agent_id: &str) -> i64 {
    let now = litellm_rust::db::managed_agents::now_ms();
    sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentGovernanceTable"
        SET review_due_at = $2
        WHERE agent_id = $1
        "#,
    )
    .bind(agent_id)
    .bind(now - 1)
    .execute(&fixture.pool)
    .await
    .unwrap();
    let expired = litellm_rust::http::managed_agents::source_scheduler::expire_due_reviews_once(
        fixture.state.clone(),
        now,
    )
    .await
    .unwrap();
    assert_eq!(expired, 1);
    assert_eq!(
        litellm_rust::http::managed_agents::source_scheduler::expire_due_reviews_once(
            fixture.state.clone(),
            now,
        )
        .await
        .unwrap(),
        0
    );
    now
}

async fn assert_paused(fixture: &AppFixture, agent_id: &str) {
    let due = governance(fixture, agent_id).await;
    assert_eq!(due["governance"]["lifecycle_status"], "review_due");
    let agent = request_json(
        fixture.app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}"),
        None,
    )
    .await;
    assert_eq!(agent["status"], "paused");
    let (run_status, run_body) = request_json_raw_with_key(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/run"),
        Some(json!({ "prompt": "should not run" })),
        "sk-local",
    )
    .await;
    assert_eq!(run_status, StatusCode::BAD_REQUEST, "{run_body}");
}

async fn renew_review(fixture: &AppFixture, agent_id: &str, published_at: i64, now: i64) {
    let due = governance(fixture, agent_id).await;
    let revision = due["current_revision"].as_i64().unwrap() as i32;
    litellm_rust::db::managed_agents::governance::mark_tested(
        &fixture.pool,
        agent_id,
        revision,
        true,
        "periodic review passed",
    )
    .await
    .unwrap();
    let renewal_approval = request_publish(fixture, agent_id).await;
    approve(fixture, &renewal_approval).await;
    let renewed = governance(fixture, agent_id).await;
    assert_eq!(renewed["governance"]["lifecycle_status"], "published");
    assert!(renewed["governance"]["published_at"].as_i64().unwrap() >= published_at);
    assert!(renewed["governance"]["review_due_at"].as_i64().unwrap() > now);
    let resumed = request_json(
        fixture.app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}"),
        None,
    )
    .await;
    assert_eq!(resumed["status"], "active");
}

async fn governed_agent(fixture: &AppFixture) -> String {
    let agent = request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents",
        Some(json!({
            "name": "periodic-review-agent",
            "owner_id": "review-owner",
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
            owner_id: "review-owner",
            provider: "external-test",
            endpoint: "https://runtime.example.test",
            external_agent_id: "periodic-review-external",
            source_hash: "source-v1",
            credential_scope: "personal",
            credential_name: None,
        },
    )
    .await
    .unwrap();
    let revision = litellm_rust::db::managed_agents::registry::revisions::latest_version(
        &fixture.pool,
        &agent_id,
    )
    .await
    .unwrap()
    .unwrap();
    litellm_rust::db::managed_agents::governance::mark_tested(
        &fixture.pool,
        &agent_id,
        revision,
        true,
        "healthy",
    )
    .await
    .unwrap();
    agent_id
}

async fn request_publish(fixture: &AppFixture, agent_id: &str) -> String {
    let value = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/governance/request-publish"),
        Some(json!({})),
    )
    .await;
    value["approval"]["id"].as_str().unwrap().to_owned()
}

async fn approve(fixture: &AppFixture, approval_id: &str) {
    let value = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/approvals/{approval_id}/accept"),
        Some(json!({ "arguments": null })),
    )
    .await;
    assert_eq!(value["ok"], true);
}

async fn governance(fixture: &AppFixture, agent_id: &str) -> Value {
    request_json(
        fixture.app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/governance"),
        None,
    )
    .await
}
