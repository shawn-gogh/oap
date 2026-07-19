use serde_json::{json, Value};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

use super::super::{request_json, request_json_raw, start_reachable_mock_server, AppFixture};

/// Real end-to-end governance ("纳管") flow through a federated import adapter
/// that still has **no execution bridge**: OpenAI Assistants. It drives
/// discover → preview → import → governance test against a live (mocked)
/// upstream that speaks the adapter's real discovery contract.
///
/// It does not reach published/active, and that is the point of the test.
/// `sessions::external_bridge` has no `invoke_*` case for `openai_assistant`,
/// so `inspect_runtime_contract` leaves it at "partial" conformance
/// (`sdk/agents/conformance.rs`). `check_source_contract` then fails the
/// `runtime_contract` preflight check (it requires exactly "conformant"), so a
/// governance test on the agent can never pass and request-publish is
/// permanently refused — regardless of how reachable the upstream is. Adding an
/// execution bridge (making it publishable) has to move it out of this test,
/// the way LangGraph and CrewAI moved to their own flows once their bridges
/// landed.
pub async fn exercise_federated_adapter_governance(fixture: &AppFixture) {
    for provider in ["openai_assistants"] {
        run_governance_gate_flow(fixture, provider).await;
    }
}

/// discover → import → governance-test-fails-permanently → publish-refused,
/// shared across every partial-conformance federated adapter.
async fn run_governance_gate_flow(fixture: &AppFixture, provider: &str) {
    let server = start_reachable_mock_server().await;
    mount_discover(
        &server,
        provider,
        "Research Assistant",
        "Summarizes OSINT feeds",
    )
    .await;

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
    // The preview advisory must be honest that this adapter is catalog-only.
    let issues = previewed["items"][0]["issues"].as_array().unwrap();
    assert!(
        issues.iter().any(|issue| issue["message"]
            .as_str()
            .unwrap_or_default()
            .contains("仅可编目发现")),
        "{provider} preview must flag catalog-only: {issues:?}"
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

    assert_governance_gate_blocks(fixture, provider, &agent_id).await;
}

/// The governance test must fail on the runtime_contract gate (partial
/// conformance) and request-publish must therefore be refused.
async fn assert_governance_gate_blocks(fixture: &AppFixture, provider: &str, agent_id: &str) {
    let tested = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/governance/test"),
        Some(json!({})),
    )
    .await;
    assert_eq!(
        tested["governance"]["runtime_health"], "unhealthy",
        "{provider}"
    );
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
    // The failure must be honest about *why*: no execution bridge, catalog-only
    // — not a cryptic contract status.
    assert!(
        contract["detail"]
            .as_str()
            .unwrap()
            .contains("暂不支持平台托管执行"),
        "{provider} must explain catalog-only: {contract}"
    );
    // Execution-smoke is A2A-only; a partial adapter must not fabricate one.
    assert!(
        checks.iter().all(|c| c["id"] != "execution_smoke"),
        "{provider}"
    );

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
