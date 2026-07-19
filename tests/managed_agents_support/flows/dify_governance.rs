use serde_json::{json, Value};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

use super::super::{request_json, request_json_raw, start_reachable_mock_server, AppFixture};

/// Governance pipeline through the real Dify adapter: discover a live
/// (mocked) Dify app, preview it (including the workflow-mode mapping
/// warning), import, run the governance test, publish + activate, then drive
/// drift detection + emergency stop.
///
/// Chat-mode Dify has a real synchronous execution bridge
/// (`sessions::external_bridge::invoke_dify`), so `dify_app` is a "conformant"
/// runtime contract (`conformance::runtime_contract_capabilities`) and a
/// chat-mode Dify agent passes the governance test and publishes — the same as
/// A2A, just without an execution-smoke round trip (that check is A2A-only).
/// Contrast the federated adapters that have no execution bridge
/// (LangGraph/CrewAI/OpenAI Assistants/ACP): they stay "partial" and can never
/// publish — locked in by federated_adapter_governance.rs. Any change to the
/// contract gate has to touch both tests, not silently flip behavior.
pub async fn exercise_dify_governance(fixture: &AppFixture) {
    let dify = start_reachable_mock_server().await;
    mount_dify_info(
        &dify,
        "Research Assistant",
        "Summarizes OSINT feeds",
        "chat",
    )
    .await;

    // Discovery returns exactly the app's raw /info payload; a real client
    // (the import dialog) threads that same object through preview and
    // import unchanged, so the test does too — this is also what lets the
    // Dify-specific "workflow mode needs mapping confirmation" preview rule
    // fire below, since that rule reads raw.mode.
    let discovered = discover(fixture, &dify).await;
    assert_eq!(discovered["agents"].as_array().unwrap().len(), 1);
    let external_agent = discovered["agents"][0].clone();
    assert_eq!(external_agent["name"], "Research Assistant");
    assert_eq!(external_agent["raw"]["mode"], "chat");

    let previewed = preview(fixture, &dify, &external_agent).await;
    assert_eq!(previewed["items"][0]["can_import"], true);
    assert_eq!(previewed["items"][0]["issues"], json!([]));

    let imported = import(fixture, &dify, &external_agent).await;
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
    assert_eq!(agent["config"]["source"]["kind"], "external_agent");
    assert_eq!(agent["config"]["source"]["provider"], "dify");
    assert_eq!(
        agent["config"]["source"]["raw"]["description"],
        "Summarizes OSINT feeds"
    );

    // A workflow-mode Dify app must be flagged as needing input/output
    // mapping confirmation before it can be trusted to execute. Each Dify
    // endpoint+key maps to exactly one app (GET /info has no app selector),
    // so a distinct workflow-mode app means a distinct mock deployment, not
    // a second mount on the same server.
    let dify_workflow = start_reachable_mock_server().await;
    mount_dify_info(
        &dify_workflow,
        "Workflow App",
        "Runs a multi-step pipeline",
        "workflow",
    )
    .await;
    let workflow_discovered = discover(fixture, &dify_workflow).await;
    let workflow_agent = workflow_discovered["agents"][0].clone();
    assert_eq!(workflow_agent["name"], "Workflow App");
    let workflow_preview = preview(fixture, &dify_workflow, &workflow_agent).await;
    let workflow_issues = workflow_preview["items"][0]["issues"].as_array().unwrap();
    assert!(
        workflow_issues
            .iter()
            .any(|issue| issue["code"] == "dify_workflow_mapping_required"),
        "got: {workflow_issues:?}"
    );
    assert_eq!(
        workflow_preview["items"][0]["can_import"], true,
        "approval_required must not block import"
    );

    // --- Governance test: a chat-mode Dify agent has a real execution bridge,
    // so the runtime_contract gate passes and the agent becomes testable.
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
    let runtime_check = checks
        .iter()
        .find(|check| check["id"] == "runtime")
        .unwrap();
    assert_eq!(runtime_check["verdict"], "verified");
    let contract_check = checks
        .iter()
        .find(|check| check["id"] == "runtime_contract")
        .unwrap();
    assert_eq!(
        contract_check["verdict"], "verified",
        "got: {contract_check:?}"
    );
    assert!(
        contract_check["detail"]
            .as_str()
            .unwrap()
            .contains("conformant"),
        "got: {contract_check:?}"
    );
    // Dify has no execution-smoke: unlike A2A, no "execution_smoke" check
    // appears — it is gated on api_spec == "a2a_v1".
    assert!(checks.iter().all(|check| check["id"] != "execution_smoke"));

    publish_and_activate(fixture, &agent_id).await;

    // --- Drift detection still works on a never-activated governed agent:
    // sync/accept doesn't depend on lifecycle_status or agent.status.
    // wiremock matches the lowest-priority mock first (default priority 5,
    // ties broken by mount order), so the updated response needs a lower
    // priority number to actually take effect over the mock mounted above.
    Mock::given(method("GET"))
        .and(path("/info"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "Research Assistant",
            "description": "Now covers dark-web sources too",
            "mode": "chat"
        })))
        .with_priority(1)
        .mount(&dify)
        .await;
    let synced = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/source/sync"),
        None,
    )
    .await;
    assert_eq!(synced["source"]["sync_state"], "drifted");
    assert!(synced["source"]["candidate_snapshot_id"].as_str().is_some());
    let open_findings: Vec<&Value> = synced["drift_findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|finding| finding["resolution"] == "open")
        .collect();
    assert!(
        !open_findings.is_empty(),
        "expected at least one open drift finding"
    );

    let accepted = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/source/drift/accept"),
        Some(json!({ "reason": "remote description update only, no risk" })),
    )
    .await;
    assert_eq!(accepted["source"]["sync_state"], "in_sync");
    let post_accept_agent = request_json(
        fixture.app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}"),
        None,
    )
    .await;
    assert_eq!(
        post_accept_agent["config"]["source"]["raw"]["description"],
        "Now covers dark-web sources too"
    );

    // --- Emergency stop must still work — and still actually block new
    // interaction — on a governed agent that was never activated. Safety
    // controls must not assume "was live" as a precondition.
    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/emergency-stop"),
        Some(json!({})),
    )
    .await;
    let (resume_status, resume_body) = request_json_raw(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/resume"),
        Some(json!({})),
    )
    .await;
    assert_eq!(resume_status, axum::http::StatusCode::BAD_REQUEST);
    assert!(resume_body.contains("治理挂起"), "got: {resume_body}");

    let governance_after_stop = request_json(
        fixture.app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/governance"),
        None,
    )
    .await;
    assert_eq!(
        governance_after_stop["governance"]["lifecycle_status"],
        "suspended"
    );
}

async fn discover(fixture: &AppFixture, dify: &MockServer) -> Value {
    request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents/import/dify/discover",
        Some(json!({ "endpoint": dify.uri(), "api_key": "dify-test-key" })),
    )
    .await
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

/// Mirrors what the import dialog actually sends: the exact `ExternalAgent`
/// object returned by discover (id/name/description/model/raw), unmodified.
fn agent_payload(external_agent: &Value) -> Value {
    json!({
        "external_id": external_agent["id"],
        "name": external_agent["name"],
        "description": external_agent["description"],
        "model": external_agent["model"],
        "raw": external_agent["raw"],
    })
}

async fn preview(fixture: &AppFixture, dify: &MockServer, external_agent: &Value) -> Value {
    request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents/import/dify/preview",
        Some(json!({
            "endpoint": dify.uri(),
            "credential_mode": "byo",
            "agents": [agent_payload(external_agent)]
        })),
    )
    .await
}

async fn import(fixture: &AppFixture, dify: &MockServer, external_agent: &Value) -> Value {
    request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents/import/dify",
        Some(json!({
            "endpoint": dify.uri(),
            "credential_mode": "byo",
            "agents": [agent_payload(external_agent)]
        })),
    )
    .await
}

async fn mount_dify_info(dify: &MockServer, name: &str, description: &str, mode: &str) {
    Mock::given(method("GET"))
        .and(path("/info"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": name,
            "description": description,
            "mode": mode
        })))
        .mount(dify)
        .await;
}
