#[path = "managed_agents_support/mod.rs"]
#[allow(dead_code)]
mod support;

use axum::http::StatusCode;
use serde_json::{json, Value};
use support::{request_json, request_json_raw, AppFixture};

#[tokio::test]
async fn agent_budget_rate_and_concurrency_are_enforced_against_postgres() {
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping quota integration test: TEST_DATABASE_URL is not set");
        return;
    };
    let agent_id = create_agent(&fixture).await;
    seed_active_session(&fixture, &agent_id).await;
    assert_concurrency_rejected(&fixture, &agent_id).await;
    let session_id = seed_idle_session(&fixture, &agent_id).await;
    assert_rate_rejected(&fixture, &agent_id, &session_id).await;
    seed_monthly_spend(&fixture, &agent_id).await;
    assert_budget_rejected(&fixture, &agent_id, &session_id).await;
    assert_budget_rejected_via_proxy(&fixture, &session_id).await;
    assert_metrics_and_audit(&fixture, &agent_id).await;
}

async fn create_agent(fixture: &AppFixture) -> String {
    request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents",
        Some(json!({
            "name": "quota-agent",
            "model": "test-model",
            "system": "test",
            "tools": [],
            "config": { "max_concurrent_sessions": 1 },
        })),
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn seed_active_session(fixture: &AppFixture, agent_id: &str) {
    litellm_rust::db::managed_agents::sessions::repository::create_runtime(
        &fixture.pool,
        litellm_rust::db::managed_agents::sessions::repository::CreateRuntimeSession {
            runtime: "generic_chat",
            agent_id: Some(agent_id),
            title: "active",
            timezone: None,
            runtime_agent_ref_id: None,
            environment: json!({}),
            provider_session_id: None,
            provider_run_id: None,
            owner_id: Some("admin"),
            task_id: None,
        },
    )
    .await
    .unwrap();
}

async fn assert_concurrency_rejected(fixture: &AppFixture, agent_id: &str) {
    let (status, body) = request_json_raw(
        fixture.app.clone(),
        "POST",
        "/session",
        Some(json!({ "agent": agent_id, "title": "blocked" })),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(body.contains("max_concurrent_sessions"));
}

async fn seed_idle_session(fixture: &AppFixture, agent_id: &str) -> String {
    litellm_rust::db::managed_agents::sessions::repository::create(
        &fixture.pool,
        "claude-code",
        Some(agent_id),
        "quota prompts",
        None,
        Some("admin"),
        None,
    )
    .await
    .unwrap()
    .id
}

async fn assert_rate_rejected(fixture: &AppFixture, agent_id: &str, session_id: &str) {
    update_config(fixture, agent_id, json!({ "rate_per_minute": 1 })).await;
    let first = prompt(fixture, session_id, "rate-1").await;
    assert_eq!(first.0, StatusCode::NO_CONTENT);
    let second = prompt(fixture, session_id, "rate-2").await;
    assert_eq!(second.0, StatusCode::TOO_MANY_REQUESTS);
    assert!(second.1.contains("rate_per_minute"));
}

async fn seed_monthly_spend(fixture: &AppFixture, agent_id: &str) {
    sqlx::query(
        r#"
        INSERT INTO "LiteLLM_SpendLogs"
          (request_id, call_type, spend, "startTime", "endTime", agent_id, purpose)
        VALUES ($1, 'responses', 1.0, now(), now(), $2, 'production')
        "#,
    )
    .bind(format!("quota-spend-{agent_id}"))
    .bind(agent_id)
    .execute(&fixture.pool)
    .await
    .unwrap();
}

async fn assert_budget_rejected(fixture: &AppFixture, agent_id: &str, session_id: &str) {
    update_config(fixture, agent_id, json!({ "budget_usd_monthly": 0.5 })).await;
    let response = prompt(fixture, session_id, "budget").await;
    assert_eq!(response.0, StatusCode::TOO_MANY_REQUESTS);
    assert!(response.1.contains("monthly_budget"));
    assert!(response.1.contains("重置时间"));
}

/// Regression: the monthly budget must also gate the raw model-proxy path
/// (`/v1/messages`), which attributes spend to the agent via `x-lap-session-id`.
/// Before this was enforced, a caller could bypass `budget_usd_monthly` entirely
/// by calling the proxy directly instead of going through the session/prompt
/// flow. The budget check runs before model routing, so no upstream model is
/// needed for this assertion.
async fn assert_budget_rejected_via_proxy(fixture: &AppFixture, session_id: &str) {
    let body = support::request_with_headers(
        fixture.app.clone(),
        "POST",
        "/v1/messages",
        json!({
            "model": "test-model",
            "messages": [{ "role": "user", "content": "hi" }],
        })
        .to_string(),
        "application/json",
        &[("x-lap-session-id", session_id.to_owned())],
        StatusCode::TOO_MANY_REQUESTS,
    )
    .await;
    assert!(
        body.contains("monthly_budget"),
        "proxy path must enforce the agent monthly budget, got: {body}"
    );
}

async fn assert_metrics_and_audit(fixture: &AppFixture, agent_id: &str) {
    let metrics = request_json(
        fixture.app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/metrics?days=7"),
        None,
    )
    .await;
    assert_eq!(metrics["quota"]["month_cost_usd"], 1.0);
    assert_eq!(metrics["quota"]["month_remaining_usd"], 0.0);
    let audit = request_json(
        fixture.app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/audit"),
        None,
    )
    .await;
    assert!(
        audit["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["action"] == "agent.quota.rejected")
            .count()
            >= 3
    );
}

async fn update_config(fixture: &AppFixture, agent_id: &str, config: Value) {
    request_json(
        fixture.app.clone(),
        "PATCH",
        &format!("/api/agents/{agent_id}"),
        Some(json!({ "config": config })),
    )
    .await;
}

async fn prompt(fixture: &AppFixture, session_id: &str, request_id: &str) -> (StatusCode, String) {
    request_json_raw(
        fixture.app.clone(),
        "POST",
        &format!("/session/{session_id}/prompt_async"),
        Some(json!({
            "request_id": request_id,
            "model": { "providerID": "litellm", "modelID": "test-model" },
            "parts": [{ "type": "text", "text": "hello" }],
        })),
    )
    .await
}
