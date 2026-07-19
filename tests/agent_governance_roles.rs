#[path = "managed_agents_support/mod.rs"]
#[allow(dead_code)]
mod support;

use axum::http::StatusCode;
use serde_json::{json, Value};
use support::{request_json, request_json_raw_with_key, AppFixture};

static DB_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn governance_roles_separate_import_approval_and_operations() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping governance role test: TEST_DATABASE_URL is not set");
        return;
    };
    let importer = create_user_key(&fixture, "import-owner", "importer").await;
    let self_approver = create_key(&fixture, "import-owner", "approver").await;
    let approver = create_user_key(&fixture, "release-approver", "approver").await;
    let operator = create_user_key(&fixture, "runtime-operator", "operator").await;
    let user = create_user_key(&fixture, "plain-user", "user").await;

    let (denied, _) = request_json_raw_with_key(
        fixture.app.clone(),
        "POST",
        "/api/agents/import/elastic/preview",
        Some(json!({
            "endpoint": "",
            "credential_mode": "byo",
            "agents": []
        })),
        &user.key,
    )
    .await;
    assert_eq!(denied, StatusCode::FORBIDDEN);
    let (importer_status, _) = request_json_raw_with_key(
        fixture.app.clone(),
        "POST",
        "/api/agents/import/elastic",
        Some(json!({
            "endpoint": "",
            "credential_mode": "byo",
            "agents": []
        })),
        &importer.key,
    )
    .await;
    assert_eq!(importer_status, StatusCode::BAD_REQUEST);

    let agent_id = governed_agent(&fixture, &importer.user_id, "role-gated").await;
    let approval_id = request_publish(&fixture, &importer.key, &agent_id).await;
    let (self_status, self_body) =
        decide(&fixture, &self_approver.key, &approval_id, "accept").await;
    assert_eq!(self_status, StatusCode::BAD_REQUEST, "{self_body}");
    assert!(self_body.contains("不能审批自己导入"));

    let (approved, body) = decide(&fixture, &approver.key, &approval_id, "accept").await;
    assert_eq!(approved, StatusCode::OK, "{body}");
    let governance = governance(&fixture, &importer.key, &agent_id).await;
    assert_eq!(governance["governance"]["lifecycle_status"], "published");

    let (operator_list_status, operator_list_body) = request_json_raw_with_key(
        fixture.app.clone(),
        "GET",
        "/api/agents",
        None,
        &operator.key,
    )
    .await;
    assert_eq!(operator_list_status, StatusCode::OK, "{operator_list_body}");
    let operator_list: Value = serde_json::from_str(&operator_list_body).unwrap();
    assert!(operator_list["agents"]
        .as_array()
        .unwrap()
        .iter()
        .any(|agent| agent["id"] == agent_id));
    let (operator_get_status, operator_get_body) = request_json_raw_with_key(
        fixture.app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}"),
        None,
        &operator.key,
    )
    .await;
    assert_eq!(operator_get_status, StatusCode::OK, "{operator_get_body}");

    let (user_stop, _) = request_json_raw_with_key(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/emergency-stop"),
        Some(json!({})),
        &user.key,
    )
    .await;
    assert_eq!(user_stop, StatusCode::NOT_FOUND);
    let (operator_stop, operator_body) = request_json_raw_with_key(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/emergency-stop"),
        Some(json!({})),
        &operator.key,
    )
    .await;
    assert_eq!(operator_stop, StatusCode::OK, "{operator_body}");
}

#[tokio::test]
async fn administrators_can_explicitly_disable_self_approval_blocking() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping governance setting test: TEST_DATABASE_URL is not set");
        return;
    };
    let importer = create_user_key(&fixture, "dual-role-owner", "importer").await;
    let approver = create_key(&fixture, "dual-role-owner", "approver").await;
    let agent_id = governed_agent(&fixture, &importer.user_id, "self-approved").await;
    let approval_id = request_publish(&fixture, &importer.key, &agent_id).await;

    let settings = request_json(fixture.app.clone(), "GET", "/api/governance/settings", None).await;
    assert_eq!(settings["separation_of_duties"], true);
    let updated = request_json(
        fixture.app.clone(),
        "PUT",
        "/api/governance/settings",
        Some(json!({ "separation_of_duties": false })),
    )
    .await;
    assert_eq!(updated["separation_of_duties"], false);

    let (status, body) = decide(&fixture, &approver.key, &approval_id, "accept").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let published = governance(&fixture, &importer.key, &agent_id).await;
    assert_eq!(published["governance"]["lifecycle_status"], "published");
}

struct TestIdentity {
    user_id: String,
    key: String,
}

async fn create_user_key(fixture: &AppFixture, user_id: &str, role: &str) -> TestIdentity {
    litellm_rust::db::managed_agents::users::repository::create(
        &fixture.pool,
        user_id,
        user_id,
        None,
    )
    .await
    .unwrap();
    create_key(fixture, user_id, role).await
}

async fn create_key(fixture: &AppFixture, user_id: &str, role: &str) -> TestIdentity {
    let created = litellm_rust::db::managed_agents::api_keys::repository::create(
        &fixture.pool,
        Some(role),
        Some(user_id),
        Some(role),
    )
    .await
    .unwrap();
    TestIdentity {
        user_id: user_id.to_owned(),
        key: created.key,
    }
}

async fn governed_agent(fixture: &AppFixture, owner_id: &str, name: &str) -> String {
    let agent = request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents",
        Some(json!({
            "name": name,
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
            external_agent_id: &format!("external-{agent_id}"),
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

async fn request_publish(fixture: &AppFixture, key: &str, agent_id: &str) -> String {
    let (status, body) = request_json_raw_with_key(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/governance/request-publish"),
        Some(json!({})),
        key,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    serde_json::from_str::<Value>(&body).unwrap()["approval"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn decide(
    fixture: &AppFixture,
    key: &str,
    approval_id: &str,
    decision: &str,
) -> (StatusCode, String) {
    request_json_raw_with_key(
        fixture.app.clone(),
        "POST",
        &format!("/api/approvals/{approval_id}/{decision}"),
        Some(json!({ "arguments": null })),
        key,
    )
    .await
}

async fn governance(fixture: &AppFixture, key: &str, agent_id: &str) -> Value {
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
