use serde_json::{json, Value};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

use super::super::{request_json, start_reachable_mock_server, AppFixture};

/// End-to-end governance ("纳管") flow through the LangGraph adapter, now that
/// it has a real execution bridge (`sessions::external_bridge::invoke_langgraph`
/// → POST /runs/wait). `langgraph_assistant` is a "conformant" runtime
/// contract, so a LangGraph assistant with a confirmed input/output mapping
/// passes the governance test, publishes, activates, and actually executes a
/// real prompt through the bridge.
///
/// The confirmed mapping is injected into the import payload's raw
/// (`x-lap-runtime`), mirroring the operator confirming the input/state mapping
/// in the UI — which is also what clears the `langgraph_input_mapping_required`
/// advisory. Contrast the adapters that still have no execution bridge
/// (CrewAI/OpenAI Assistants/ACP), which stay "partial" — see
/// federated_adapter_governance.rs.
pub async fn exercise_langgraph_governance(fixture: &AppFixture) {
    let server = start_reachable_mock_server().await;
    mount_assistant_search(&server, "Research Graph", "Runs a research graph").await;
    mount_runs_wait(&server, "answer from the langgraph run").await;

    let agent_id = import_langgraph_agent(fixture, &server).await;
    assert_governance_passes(fixture, &agent_id).await;
    publish_and_activate(fixture, &agent_id).await;
    assert_execution_round_trip(fixture, &agent_id).await;
    assert_drift_cycle(fixture, &server, &agent_id).await;
}

/// discover → confirm mapping → preview → import; returns the draft agent id.
async fn import_langgraph_agent(fixture: &AppFixture, server: &MockServer) -> String {
    let discovered = discover(fixture, server).await;
    assert_eq!(discovered["agents"].as_array().unwrap().len(), 1);
    let mut external_agent = discovered["agents"][0].clone();
    assert_eq!(external_agent["name"], "Research Graph");
    // Operator confirms the input/output mapping; this also makes the source
    // executable and clears the input-mapping advisory.
    external_agent["raw"]["x-lap-runtime"] = json!({
        "base_url": server.uri(),
        "input_field": "input",
        "output_path": "/output"
    });

    let previewed = preview(fixture, server, &external_agent).await;
    assert_eq!(previewed["items"][0]["can_import"], true);
    let issues = previewed["items"][0]["issues"].as_array().unwrap();
    assert!(
        issues
            .iter()
            .all(|issue| issue["code"] != "langgraph_input_mapping_required"),
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
    assert_eq!(agent["config"]["source"]["provider"], "langgraph");
    agent_id
}

/// The governance test passes: langgraph_assistant is conformant and no check
/// fails (shared credential is verified, source is in sync).
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
    // Execution-smoke is A2A-only; LangGraph must not fabricate one.
    assert!(checks.iter().all(|c| c["id"] != "execution_smoke"));
}

/// Send a real prompt through the activated agent and confirm the mocked
/// /runs/wait reply is persisted — proving invoke_langgraph executes.
async fn assert_execution_round_trip(fixture: &AppFixture, agent_id: &str) {
    let session = request_json(
        fixture.app.clone(),
        "POST",
        "/session",
        Some(json!({
            "agent": agent_id,
            "agent_id": agent_id,
            "runtime": "langgraph_assistant",
            "title": "langgraph governance smoke session"
        })),
    )
    .await;
    let session_id = session["id"].as_str().unwrap().to_owned();
    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/session/{session_id}/message"),
        Some(json!({
            "model": { "modelID": "langgraph-remote" },
            "parts": [{ "type": "text", "text": "what did the research find?" }]
        })),
    )
    .await;
    assert!(
        wait_for_assistant_reply(fixture, &session_id, "answer from the langgraph run").await,
        "expected the mocked /runs/wait reply to be persisted"
    );
}

/// Change the upstream definition and prove sync flags drift and accept
/// re-baselines the snapshot on the live agent.
async fn assert_drift_cycle(fixture: &AppFixture, server: &MockServer, agent_id: &str) {
    Mock::given(method("POST"))
        .and(path("/assistants/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "assistant_id": "assistant-1",
            "name": "Research Graph",
            "description": "Definition changed upstream",
            "config": {"configurable": {"model": "openai/gpt-4.1"}}
        }])))
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
        "/api/agents/import/langgraph/discover",
        Some(json!({ "endpoint": server.uri(), "api_key": "langgraph-test-key" })),
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
        "/api/agents/import/langgraph/preview",
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
        "/api/agents/import/langgraph",
        Some(json!({
            "endpoint": server.uri(),
            "credential_mode": "shared",
            "api_key": "langgraph-exec-key",
            "agents": [agent_payload(external_agent)]
        })),
    )
    .await
}

/// POST /assistants/search returns a bare array of assistants.
async fn mount_assistant_search(server: &MockServer, name: &str, description: &str) {
    Mock::given(method("POST"))
        .and(path("/assistants/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "assistant_id": "assistant-1",
            "name": name,
            "description": description,
            "config": {"configurable": {"model": "openai/gpt-4.1"}}
        }])))
        .mount(server)
        .await;
}

/// POST /runs/wait blocks and returns the graph's final state; the mapped
/// output pointer (/output) reads the answer.
async fn mount_runs_wait(server: &MockServer, answer: &str) {
    Mock::given(method("POST"))
        .and(path("/runs/wait"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "output": answer })))
        .mount(server)
        .await;
}
