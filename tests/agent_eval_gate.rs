#[path = "managed_agents_support/mod.rs"]
#[allow(dead_code)]
mod support;

use std::time::Duration;

use axum::http::StatusCode;
use serde_json::{json, Value};
use support::{request_json, request_json_raw_with_key, AppFixture};

static DB_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn golden_eval_gate_blocks_publish_until_current_revision_passes() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping eval gate integration test: TEST_DATABASE_URL is not set");
        return;
    };
    let (owner_id, owner_key) = create_owner(&fixture).await;
    let config = json!({
        "design": {
            "evaluation": {
                "success_criteria": "回答准确且安全",
                "normal_cases": ["正常问题"],
                "edge_cases": ["边界问题"],
                "recovery_cases": ["恢复问题"],
                "safety_cases": ["危险请求"]
            }
        }
    });
    let agent_id = create_governed_agent(&fixture, &owner_id, "golden-gated", config).await;
    let revision = current_revision(&fixture, &agent_id).await;
    mark_tested(&fixture, &agent_id, revision).await;

    let gate = governance_status(&fixture, &owner_key, &agent_id).await;
    assert_eq!(gate["eval_gate"]["state"], "not_run");
    assert_eq!(gate["eval_gate"]["required"], true);
    assert_publish_blocked(&fixture, &owner_key, &agent_id, "尚未运行黄金用例评估").await;

    let failed = litellm_rust::db::managed_agents::eval_runs::repository::insert_running(
        &fixture.pool,
        &agent_id,
        Some(revision),
        "test-model",
        4,
        Some(&owner_id),
    )
    .await
    .unwrap();
    litellm_rust::db::managed_agents::eval_runs::repository::complete(
        &fixture.pool,
        &failed.id,
        3,
        &json!([]),
    )
    .await
    .unwrap();
    assert_publish_blocked(&fixture, &owner_key, &agent_id, "仅通过 3/4 项").await;

    tokio::time::sleep(Duration::from_millis(2)).await;
    let passed = litellm_rust::db::managed_agents::eval_runs::repository::insert_running(
        &fixture.pool,
        &agent_id,
        Some(revision),
        "test-model",
        4,
        Some(&owner_id),
    )
    .await
    .unwrap();
    litellm_rust::db::managed_agents::eval_runs::repository::complete(
        &fixture.pool,
        &passed.id,
        4,
        &json!([]),
    )
    .await
    .unwrap();

    let (status, body) = request_json_raw_with_key(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/governance/request-publish"),
        Some(json!({})),
        &owner_key,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let published: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(published["eval_gate"]["state"], "passed");
    assert_eq!(published["warnings"], json!([]));

    let blocked_events: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM "LiteLLM_AuditLogsTable"
        WHERE action = 'agent.governance.publish_blocked' AND target_id = $1
        "#,
    )
    .bind(&agent_id)
    .fetch_one(&fixture.pool)
    .await
    .unwrap();
    assert_eq!(blocked_events, 2);
}

#[tokio::test]
async fn publish_without_golden_cases_returns_a_non_blocking_warning() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping eval warning integration test: TEST_DATABASE_URL is not set");
        return;
    };
    let (owner_id, owner_key) = create_owner(&fixture).await;
    let agent_id = create_governed_agent(&fixture, &owner_id, "ungated", json!({})).await;
    let revision = current_revision(&fixture, &agent_id).await;
    mark_tested(&fixture, &agent_id, revision).await;

    let (status, body) = request_json_raw_with_key(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/governance/request-publish"),
        Some(json!({})),
        &owner_key,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let response: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(response["eval_gate"]["state"], "not_required");
    assert!(response["warnings"][0]
        .as_str()
        .unwrap()
        .contains("未定义黄金用例"));
}

async fn create_owner(fixture: &AppFixture) -> (String, String) {
    let owner = litellm_rust::db::managed_agents::users::repository::create(
        &fixture.pool,
        "eval-owner",
        "评估负责人",
        Some("eval-owner@example.com"),
    )
    .await
    .unwrap();
    let key = litellm_rust::db::managed_agents::api_keys::repository::create(
        &fixture.pool,
        Some("eval-owner-key"),
        Some(&owner.id),
        Some("user"),
    )
    .await
    .unwrap()
    .key;
    (owner.id, key)
}

async fn create_governed_agent(
    fixture: &AppFixture,
    owner_id: &str,
    name: &str,
    config: Value,
) -> String {
    let created = request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents",
        Some(json!({
            "name": name,
            "owner_id": owner_id,
            "model": "test-model",
            "system": "test",
            "tools": [],
            "config": config,
        })),
    )
    .await;
    let agent_id = created["id"].as_str().unwrap().to_owned();
    litellm_rust::db::managed_agents::governance::record_import(
        &fixture.pool,
        litellm_rust::db::managed_agents::governance::ImportedSource {
            agent_id: &agent_id,
            owner_id,
            provider: "external-test",
            endpoint: "https://runtime.example.test",
            external_agent_id: &format!("external-{agent_id}"),
            source_hash: "source-v1",
            credential_scope: "personal",
            credential_name: None,
        },
    )
    .await
    .unwrap();
    agent_id
}

async fn current_revision(fixture: &AppFixture, agent_id: &str) -> i32 {
    litellm_rust::db::managed_agents::registry::revisions::latest_version(&fixture.pool, agent_id)
        .await
        .unwrap()
        .unwrap()
}

async fn mark_tested(fixture: &AppFixture, agent_id: &str, revision: i32) {
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

async fn governance_status(fixture: &AppFixture, key: &str, agent_id: &str) -> Value {
    let (status, body) = request_json_raw_with_key(
        fixture.app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/governance"),
        None,
        key,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    serde_json::from_str(&body).unwrap()
}

async fn assert_publish_blocked(fixture: &AppFixture, key: &str, agent_id: &str, message: &str) {
    let (status, body) = request_json_raw_with_key(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/governance/request-publish"),
        Some(json!({})),
        key,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains(message), "{body}");
}
