use serde_json::{json, Value};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

use super::super::{request_json, start_reachable_mock_server, AppFixture};

/// End-to-end governance pipeline through the real A2A adapter, the one
/// import provider with a real execution smoke test: discover an agent card,
/// preview, import (shared credential), run governance test (which actually
/// POSTs `message/send` to the mock and expects a real reply — not just a
/// discover probe), publish, approve, activate, send one real message through
/// the activated agent and see the reply land, then break the mock and drive
/// three consecutive automatic health checks to prove the platform's own
/// auto-pause (not a manually seeded governance row) actually suspends a
/// live, real federated agent.
pub async fn exercise_a2a_governance(fixture: &AppFixture) {
    let a2a = start_reachable_mock_server().await;
    let rpc_url = format!("{}/a2a-rpc", a2a.uri());
    mount_agent_card(&a2a, &rpc_url).await;
    mount_message_send(&a2a, "pong from the mock agent").await;

    let discovered = discover(fixture, &a2a).await;
    let external_agent = discovered["agents"][0].clone();
    assert_eq!(external_agent["name"], "Threat Analyst");
    assert_eq!(external_agent["raw"]["url"], rpc_url);

    let preview = preview(fixture, &a2a, &external_agent).await;
    assert_eq!(preview["items"][0]["can_import"], true);

    let imported = import(fixture, &a2a, &external_agent, "a2a-shared-secret").await;
    assert_eq!(imported["results"][0]["status"], "imported");
    let agent_id = imported["results"][0]["agent_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Governance test drives the real execution-smoke path — a genuine
    // message/send round trip against the mock, not just a discover probe.
    let tested = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/governance/test"),
        Some(json!({})),
    )
    .await;
    assert_eq!(tested["governance"]["lifecycle_status"], "tested");
    assert_eq!(tested["governance"]["runtime_health"], "healthy");
    let smoke = tested["preflight"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["id"] == "execution_smoke")
        .expect("A2A must run a real execution smoke check");
    assert_eq!(smoke["verdict"], "verified");
    assert!(
        mock_saw_message_send(&a2a).await,
        "smoke test never called message/send"
    );

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

    // --- Real session round trip through the activated agent. send_message
    // enqueues and returns before the background invocation completes, so
    // poll persisted messages the same way the approval-flow test waits for
    // an async resume.
    let session = request_json(
        fixture.app.clone(),
        "POST",
        "/session",
        Some(json!({
            "agent": agent_id,
            "agent_id": agent_id,
            "runtime": "a2a_v1",
            "title": "a2a governance smoke session"
        })),
    )
    .await;
    let session_id = session["id"].as_str().unwrap().to_owned();
    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/session/{session_id}/message"),
        Some(json!({
            "model": { "modelID": "a2a-remote" },
            "parts": [{ "type": "text", "text": "what is the current threat level?" }]
        })),
    )
    .await;
    let reply = wait_for_assistant_reply(fixture, &session_id, "pong from the mock agent").await;
    assert!(reply, "expected the mocked A2A reply to be persisted");

    // --- Break the mock's reachability check and drive three consecutive
    // automatic health checks: the platform's own HEALTH_PAUSE_THRESHOLD
    // logic (not a manually seeded governance row) must suspend it.
    Mock::given(method("GET"))
        .and(path("/.well-known/agent-card.json"))
        .respond_with(ResponseTemplate::new(503))
        .with_priority(1)
        .mount(&a2a)
        .await;
    for _ in 0..3 {
        request_json(
            fixture.app.clone(),
            "POST",
            &format!("/api/agents/{agent_id}/governance/health"),
            None,
        )
        .await;
    }
    let agent_after = request_json(
        fixture.app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}"),
        None,
    )
    .await;
    assert_eq!(agent_after["status"], "paused");
    let governance_after = request_json(
        fixture.app.clone(),
        "GET",
        &format!("/api/agents/{agent_id}/governance"),
        None,
    )
    .await;
    assert_eq!(
        governance_after["governance"]["lifecycle_status"],
        "suspended"
    );
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

async fn discover(fixture: &AppFixture, a2a: &MockServer) -> Value {
    request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents/import/a2a/discover",
        Some(json!({ "endpoint": a2a.uri(), "api_key": "" })),
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

async fn preview(fixture: &AppFixture, a2a: &MockServer, external_agent: &Value) -> Value {
    request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents/import/a2a/preview",
        Some(json!({
            "endpoint": a2a.uri(),
            "credential_mode": "shared",
            "agents": [agent_payload(external_agent)]
        })),
    )
    .await
}

async fn import(
    fixture: &AppFixture,
    a2a: &MockServer,
    external_agent: &Value,
    api_key: &str,
) -> Value {
    request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents/import/a2a",
        Some(json!({
            "endpoint": a2a.uri(),
            "credential_mode": "shared",
            "api_key": api_key,
            "agents": [agent_payload(external_agent)]
        })),
    )
    .await
}

async fn mount_agent_card(a2a: &MockServer, rpc_url: &str) {
    Mock::given(method("GET"))
        .and(path("/.well-known/agent-card.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "Threat Analyst",
            "description": "Assesses open-source intelligence",
            "url": rpc_url,
            "version": "1.0.0"
        })))
        .mount(a2a)
        .await;
}

async fn mount_message_send(a2a: &MockServer, reply_text: &str) {
    Mock::given(method("POST"))
        .and(path("/a2a-rpc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": "rpc-mock",
            "result": {
                "kind": "message",
                "role": "agent",
                "parts": [{ "kind": "text", "text": reply_text }]
            }
        })))
        .mount(a2a)
        .await;
}

async fn mock_saw_message_send(a2a: &MockServer) -> bool {
    a2a.received_requests()
        .await
        .unwrap()
        .iter()
        .any(|request| request.url.path() == "/a2a-rpc")
}
