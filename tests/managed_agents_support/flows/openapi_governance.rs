use serde_json::{json, Value};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

use super::super::{request_json, start_reachable_mock_server, AppFixture};

/// End-to-end governance ("纳管") flow through the real OpenAPI/REST adapter,
/// the sibling of the Dify flow among the synchronous-execution federated
/// bridges. OpenAPI has a real execution bridge
/// (`sessions::external_bridge::invoke_openapi`), so `openapi_rest` is a
/// "conformant" runtime contract and an OpenAPI-imported agent can pass the
/// governance test and publish.
///
/// The mocked spec carries an `x-lap-runtime` mapping, which is what makes the
/// source executable (invoke_openapi requires it) and clears the
/// `openapi_runtime_mapping_required` approval advisory, so the flow is the
/// clean happy path — the analogue of a chat-mode (not workflow-mode) Dify app.
pub async fn exercise_openapi_governance(fixture: &AppFixture) {
    let server = start_reachable_mock_server().await;
    mount_spec(&server, "Weather Service", "Answers weather questions").await;

    let discovered = discover(fixture, &server).await;
    assert_eq!(discovered["agents"].as_array().unwrap().len(), 1);
    let external_agent = discovered["agents"][0].clone();
    assert_eq!(external_agent["name"], "Weather Service");

    let previewed = preview(fixture, &server, &external_agent).await;
    assert_eq!(previewed["items"][0]["can_import"], true);
    // The x-lap-runtime mapping is present, so the runtime-mapping approval
    // advisory must not fire.
    let issues = previewed["items"][0]["issues"].as_array().unwrap();
    assert!(
        issues
            .iter()
            .all(|issue| issue["code"] != "openapi_runtime_mapping_required"),
        "got: {issues:?}"
    );

    let imported = import(fixture, &server, &external_agent).await;
    assert_eq!(imported["results"][0]["status"], "imported");
    let agent_id = imported["results"][0]["agent_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let agent = request_json(
        fixture.app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}"),
        None,
    )
    .await;
    assert_eq!(agent["status"], "draft");
    assert_eq!(agent["config"]["source"]["provider"], "openapi");

    assert_governance_passes(fixture, &agent_id).await;
    publish_and_activate(fixture, &agent_id).await;
}

/// The governance test must pass: openapi_rest is conformant and every other
/// check is non-failing (byo credential is exists_only, source is in sync).
async fn assert_governance_passes(fixture: &AppFixture, agent_id: &str) {
    let tested = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/governance/test"),
        Some(json!({})),
    )
    .await;
    assert_eq!(tested["governance"]["lifecycle_status"], "tested");
    assert_eq!(tested["governance"]["runtime_health"], "healthy");
    let checks = tested["preflight"]["checks"].as_array().unwrap();
    let runtime = checks.iter().find(|c| c["id"] == "runtime").unwrap();
    assert_eq!(runtime["verdict"], "verified");
    let contract = checks
        .iter()
        .find(|c| c["id"] == "runtime_contract")
        .unwrap();
    assert_eq!(contract["verdict"], "verified", "got: {contract:?}");
    assert!(
        contract["detail"].as_str().unwrap().contains("conformant"),
        "got: {contract:?}"
    );
    // Execution-smoke is A2A-only; OpenAPI must not fabricate one.
    assert!(checks.iter().all(|c| c["id"] != "execution_smoke"));
}

/// Request publish, approve, and activate a tested agent. Separation of duties
/// defaults on; this single-actor pipeline disables it (dedicated coverage
/// lives in agent_governance_roles.rs).
async fn publish_and_activate(fixture: &AppFixture, agent_id: &str) {
    request_json(
        fixture.app.clone(),
        "PUT",
        "/api/governance/settings",
        Some(json!({ "separation_of_duties": false })),
    )
    .await;
    let requested = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/governance/request-publish"),
        Some(json!({})),
    )
    .await;
    let approval_id = requested["approval"]["id"].as_str().unwrap().to_owned();
    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/approvals/{approval_id}/accept"),
        Some(json!({ "arguments": null })),
    )
    .await;
    let activated = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/activate"),
        None,
    )
    .await;
    assert_eq!(activated["status"], "active");
}

async fn discover(fixture: &AppFixture, server: &MockServer) -> Value {
    request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents/import/openapi/discover",
        Some(json!({ "endpoint": server.uri(), "api_key": "openapi-test-key" })),
    )
    .await
}

fn agent_payload(external_agent: &Value) -> Value {
    json!({
        "external_id": external_agent["id"],
        "name": external_agent["name"],
        "description": external_agent["description"],
        "model": external_agent["model"],
        "raw": external_agent["raw"],
    })
}

async fn preview(fixture: &AppFixture, server: &MockServer, external_agent: &Value) -> Value {
    request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents/import/openapi/preview",
        Some(json!({
            "endpoint": server.uri(),
            "credential_mode": "byo",
            "agents": [agent_payload(external_agent)]
        })),
    )
    .await
}

async fn import(fixture: &AppFixture, server: &MockServer, external_agent: &Value) -> Value {
    request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents/import/openapi",
        Some(json!({
            "endpoint": server.uri(),
            "credential_mode": "byo",
            "agents": [agent_payload(external_agent)]
        })),
    )
    .await
}

/// An OpenAPI 3.x spec with a confirmed x-lap-runtime execution mapping, so the
/// imported agent is executable and the runtime-mapping advisory is cleared.
async fn mount_spec(server: &MockServer, title: &str, description: &str) {
    Mock::given(method("GET"))
        .and(path("/openapi.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "openapi": "3.1.0",
            "info": { "title": title, "description": description },
            "paths": {
                "/v1/run": {
                    "post": { "operationId": "run", "responses": { "200": { "description": "ok" } } }
                }
            },
            "x-lap-runtime": {
                "path": "/v1/run",
                "input_field": "input",
                "output_field": "output"
            }
        })))
        .mount(server)
        .await;
}
