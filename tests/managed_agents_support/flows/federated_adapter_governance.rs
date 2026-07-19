use serde_json::{json, Value};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

use super::super::{request_json, request_json_raw, start_reachable_mock_server, AppFixture};

/// Real end-to-end governance ("纳管") flow through the federated import
/// adapters added by the multi-source work: LangGraph/LangSmith, CrewAI AMP,
/// and OpenAI Assistants. Each drives discover → preview → import → governance
/// test against a live (mocked) upstream that speaks that adapter's real
/// discovery contract.
///
/// Like the Dify flow, none of these reach published/active, and that is the
/// point of the test. `inspect_runtime_contract` only grants full
/// runtime-contract capability to `a2a_v1` and the managed-protocol runtimes;
/// every federated adapter whose `expose_runtime_harness()` is false — Dify,
/// OpenAPI, ACP, and now LangGraph/CrewAI/OpenAI Assistants — resolves to
/// "partial" conformance (`sdk/agents/conformance.rs`). `check_source_contract`
/// then fails the `runtime_contract` preflight check (it requires exactly
/// "conformant"), so a governance test on one of these agents can never pass
/// and request-publish is permanently refused — regardless of how reachable
/// the upstream is. Asserting it here means any future change to the contract
/// gate or to an adapter's harness story has to update this test rather than
/// silently flipping behavior nothing was watching.
pub async fn exercise_federated_adapter_governance(fixture: &AppFixture) {
    for provider in ["langgraph", "crewai", "openai_assistants"] {
        run_governance_gate_flow(fixture, provider).await;
    }
    // Drift detection must work on a never-activated federated agent for any
    // adapter, not just Dify — exercise it once through LangGraph's array-based
    // discovery contract to prove the sync/accept path generalizes.
    run_drift_flow(fixture).await;
}

/// discover → import → governance-test-fails-permanently → publish-refused,
/// shared across every partial-conformance federated adapter.
async fn run_governance_gate_flow(fixture: &AppFixture, provider: &str) {
    let server = start_reachable_mock_server().await;
    mount_discover(&server, provider, "Research Assistant", "Summarizes OSINT feeds").await;

    let discovered = discover(fixture, provider, &server).await;
    let agents = discovered["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 1, "{provider} discover: {discovered}");
    let external_agent = agents[0].clone();
    assert_eq!(external_agent["name"], "Research Assistant", "{provider}");

    let previewed = preview(fixture, provider, &server, &external_agent).await;
    assert_eq!(
        previewed["items"][0]["can_import"], true,
        "{provider} preview must allow import: {previewed}"
    );

    let imported = import(fixture, provider, &server, &external_agent).await;
    assert_eq!(imported["results"][0]["status"], "imported", "{provider}");
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
    assert_eq!(agent["status"], "draft", "{provider}");
    assert_eq!(agent["config"]["source"]["provider"], provider);

    let tested = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/governance/test"),
        Some(json!({})),
    )
    .await;
    assert_eq!(tested["governance"]["runtime_health"], "unhealthy", "{provider}");
    let checks = tested["preflight"]["checks"].as_array().unwrap();
    let runtime = checks.iter().find(|c| c["id"] == "runtime").unwrap();
    assert_eq!(
        runtime["verdict"], "verified",
        "{provider} reachability probe must pass: {runtime}"
    );
    let contract = checks
        .iter()
        .find(|c| c["id"] == "runtime_contract")
        .unwrap();
    assert_eq!(contract["verdict"], "failed", "{provider}: {contract}");
    assert!(
        contract["detail"].as_str().unwrap().contains("partial"),
        "{provider}: {contract}"
    );
    // Execution-smoke is A2A-only; a partial adapter must not fabricate one.
    assert!(checks.iter().all(|c| c["id"] != "execution_smoke"), "{provider}");

    let (publish_status, publish_body) = request_json_raw(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/governance/request-publish"),
        Some(json!({})),
    )
    .await;
    assert_eq!(
        publish_status,
        axum::http::StatusCode::BAD_REQUEST,
        "{provider}: {publish_body}"
    );
    assert!(
        publish_body.contains("尚未通过运行测试"),
        "{provider}: {publish_body}"
    );
}

/// Import a LangGraph assistant, then change the upstream definition and prove
/// sync flags drift and accept re-baselines the source snapshot.
async fn run_drift_flow(fixture: &AppFixture) {
    let server = start_reachable_mock_server().await;
    mount_discover(&server, "langgraph", "Sync Assistant", "Original definition").await;
    let discovered = discover(fixture, "langgraph", &server).await;
    let external_agent = discovered["agents"][0].clone();
    let imported = import(fixture, "langgraph", &server, &external_agent).await;
    let agent_id = imported["results"][0]["agent_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // A lower priority number wins in wiremock, so the changed response must
    // out-rank the default (priority 5) mount above to take effect.
    Mock::given(method("POST"))
        .and(path("/assistants/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "assistant_id": "assistant-1",
            "name": "Sync Assistant",
            "description": "Definition changed upstream",
            "config": {"configurable": {"model": "openai/gpt-4.1"}}
        }])))
        .with_priority(1)
        .mount(&server)
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

async fn discover(fixture: &AppFixture, provider: &str, server: &MockServer) -> Value {
    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/import/{provider}/discover"),
        Some(json!({ "endpoint": server.uri(), "api_key": "federated-test-key" })),
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

async fn preview(
    fixture: &AppFixture,
    provider: &str,
    server: &MockServer,
    external_agent: &Value,
) -> Value {
    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/import/{provider}/preview"),
        Some(json!({
            "endpoint": server.uri(),
            "credential_mode": "byo",
            "agents": [agent_payload(external_agent)]
        })),
    )
    .await
}

async fn import(
    fixture: &AppFixture,
    provider: &str,
    server: &MockServer,
    external_agent: &Value,
) -> Value {
    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/import/{provider}"),
        Some(json!({
            "endpoint": server.uri(),
            "credential_mode": "byo",
            "agents": [agent_payload(external_agent)]
        })),
    )
    .await
}

/// Mounts each adapter's real discovery contract: the request shape the
/// provider's `discover()` actually issues, and a response its parser accepts.
async fn mount_discover(server: &MockServer, provider: &str, name: &str, description: &str) {
    match provider {
        // POST /assistants/search returns a bare array of assistants.
        "langgraph" => {
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
        // GET /inputs describes a single deployment.
        "crewai" => {
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
        // GET /v1/assistants returns a paginated data array (assistants=v2).
        "openai_assistants" => {
            Mock::given(method("GET"))
                .and(path("/v1/assistants"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "data": [{
                        "id": "asst_1",
                        "name": name,
                        "description": description,
                        "model": "gpt-4.1",
                        "instructions": "Find primary sources."
                    }],
                    "has_more": false
                })))
                .mount(server)
                .await;
        }
        other => panic!("unhandled adapter in mount_discover: {other}"),
    }
}
