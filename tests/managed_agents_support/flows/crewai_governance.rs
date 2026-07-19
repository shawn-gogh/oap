use serde_json::{json, Value};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

use super::super::{request_json, start_reachable_mock_server, AppFixture};

/// End-to-end governance ("纳管") flow through the CrewAI adapter, now that it
/// has a real execution bridge (`sessions::external_bridge::invoke_crewai` →
/// POST /kickoff then poll GET /status/{id}). `crewai_crew` is a "conformant"
/// runtime contract, so a CrewAI deployment with a confirmed kickoff mapping
/// passes the governance test, publishes, activates, and actually executes a
/// real prompt through the bridge.
///
/// Unlike the synchronous Dify/OpenAPI/LangGraph bridges, CrewAI is async: the
/// bridge kicks off a run and polls to a terminal state. The confirmed mapping
/// is injected into the import payload's raw (`x-lap-runtime`), mirroring the
/// operator confirming the kickoff mapping in the UI — which also clears the
/// `crewai_kickoff_mapping_required` advisory. Contrast the adapters that still
/// have no execution bridge (OpenAI Assistants/ACP), which stay "partial" — see
/// federated_adapter_governance.rs.
pub async fn exercise_crewai_governance(fixture: &AppFixture) {
    let server = start_reachable_mock_server().await;
    mount_inputs(&server, "Research Crew", "Runs a research crew").await;
    mount_kickoff_and_status(&server, "the crew delivered its findings").await;

    let agent_id = import_crewai_agent(fixture, &server).await;
    assert_governance_passes(fixture, &agent_id).await;
    publish_and_activate(fixture, &agent_id).await;
    assert_execution_round_trip(fixture, &agent_id).await;
    assert_drift_cycle(fixture, &server, &agent_id).await;
}

/// discover → confirm mapping → preview → import; returns the draft agent id.
async fn import_crewai_agent(fixture: &AppFixture, server: &MockServer) -> String {
    let discovered = discover(fixture, server).await;
    assert_eq!(discovered["agents"].as_array().unwrap().len(), 1);
    let mut external_agent = discovered["agents"][0].clone();
    assert_eq!(external_agent["name"], "Research Crew");
    // Operator confirms the kickoff mapping; this makes the source executable
    // and clears the kickoff-mapping advisory.
    external_agent["raw"]["x-lap-runtime"] = json!({
        "base_url": server.uri(),
        "input_field": "topic",
        "output_path": "/result"
    });

    let previewed = preview(fixture, server, &external_agent).await;
    assert_eq!(previewed["items"][0]["can_import"], true);
    let issues = previewed["items"][0]["issues"].as_array().unwrap();
    assert!(
        issues
            .iter()
            .all(|issue| issue["code"] != "crewai_kickoff_mapping_required"),
        "confirmed mapping must clear the advisory, got: {issues:?}"
    );

    let imported = import(fixture, server, &external_agent).await;
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
    assert_eq!(agent["config"]["source"]["provider"], "crewai");
    agent_id
}

/// The governance test passes: crewai_crew is conformant and no check fails.
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
    let contract = checks
        .iter()
        .find(|c| c["id"] == "runtime_contract")
        .unwrap();
    assert_eq!(contract["verdict"], "verified", "got: {contract:?}");
    assert!(contract["detail"].as_str().unwrap().contains("conformant"));
    assert!(checks.iter().all(|c| c["id"] != "execution_smoke"));
}

/// Send a real prompt through the activated agent and confirm the mocked
/// kickoff+status result is persisted — proving invoke_crewai executes.
async fn assert_execution_round_trip(fixture: &AppFixture, agent_id: &str) {
    let session = request_json(
        fixture.app.clone(),
        "POST",
        "/session",
        Some(json!({
            "agent": agent_id,
            "agent_id": agent_id,
            "runtime": "crewai_crew",
            "title": "crewai governance smoke session"
        })),
    )
    .await;
    let session_id = session["id"].as_str().unwrap().to_owned();
    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/session/{session_id}/message"),
        Some(json!({
            "model": { "modelID": "crewai-remote" },
            "parts": [{ "type": "text", "text": "research the market" }]
        })),
    )
    .await;
    assert!(
        wait_for_assistant_reply(fixture, &session_id, "the crew delivered its findings").await,
        "expected the mocked kickoff/status result to be persisted"
    );
}

/// Change the upstream definition and prove sync flags drift and accept
/// re-baselines the snapshot on the live agent.
async fn assert_drift_cycle(fixture: &AppFixture, server: &MockServer, agent_id: &str) {
    Mock::given(method("GET"))
        .and(path("/inputs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "crew_id": "crew-1",
            "name": "Research Crew",
            "description": "Definition changed upstream",
            "model": "gpt-4.1",
            "inputs": [{"name": "topic"}]
        })))
        .with_priority(1)
        .mount(server)
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
    let accepted = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/source/drift/accept"),
        Some(json!({ "reason": "reviewed upstream definition change" })),
    )
    .await;
    assert_eq!(accepted["source"]["sync_state"], "in_sync");
}

/// Request publish, approve, and activate. Separation of duties defaults on;
/// this single-actor pipeline disables it (covered by agent_governance_roles).
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

async fn wait_for_assistant_reply(fixture: &AppFixture, session_id: &str, expected: &str) -> bool {
    for _ in 0..40 {
        let rows =
            litellm_rust::db::managed_agents::messages::repository::list(&fixture.pool, session_id)
                .await
                .unwrap();
        if rows.iter().any(|message| {
            message.info_json.contains("\"role\":\"assistant\"")
                && message.parts_json.contains(expected)
        }) {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    false
}

async fn discover(fixture: &AppFixture, server: &MockServer) -> Value {
    request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents/import/crewai/discover",
        Some(json!({ "endpoint": server.uri(), "api_key": "crewai-test-key" })),
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
        "/api/agents/import/crewai/preview",
        Some(json!({
            "endpoint": server.uri(),
            "credential_mode": "shared",
            "agents": [agent_payload(external_agent)]
        })),
    )
    .await
}

async fn import(fixture: &AppFixture, server: &MockServer, external_agent: &Value) -> Value {
    request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents/import/crewai",
        Some(json!({
            "endpoint": server.uri(),
            "credential_mode": "shared",
            "api_key": "crewai-exec-key",
            "agents": [agent_payload(external_agent)]
        })),
    )
    .await
}

/// GET /inputs describes the single deployment (CrewAI's discovery contract).
async fn mount_inputs(server: &MockServer, name: &str, description: &str) {
    Mock::given(method("GET"))
        .and(path("/inputs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "crew_id": "crew-1",
            "name": name,
            "description": description,
            "model": "gpt-4.1",
            "inputs": [{"name": "topic"}]
        })))
        .mount(server)
        .await;
}

/// POST /kickoff starts a run and returns a kickoff id; GET /status/{id} is
/// polled to a terminal state carrying the mapped result field.
async fn mount_kickoff_and_status(server: &MockServer, result: &str) {
    Mock::given(method("POST"))
        .and(path("/kickoff"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "kickoff_id": "kick-1" })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/status/kick-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "state": "SUCCESS",
            "result": result
        })))
        .mount(server)
        .await;
}
