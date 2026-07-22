use litellm_rust::db::managed_agents::{messages, session_control, sessions as db_sessions};
use litellm_rust::managed_agents::adapters::types::InteractionProfileV1;
use serde_json::{json, Value};

use crate::support::{request_json, AppFixture};

pub async fn assert_human_approval(fixture: &AppFixture) {
    let agent_id = create_approval_agent(fixture).await;
    assert_multiple_async_approvals(fixture, &agent_id).await;
    assert_approval_with_options(fixture, &agent_id).await;
}

/// Session-scoped platform MCP calls require an active invocation grant, and
/// the grant's allowed tools derive from the agent's `platform_mcp_ids` — so
/// the approval flow needs its own agent with the approval tools selected.
async fn create_approval_agent(fixture: &AppFixture) -> String {
    let agent = request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents",
        Some(json!({
            "name": "approval-flow-agent",
            "owner_id": "user-1",
            "model": "test-model",
            "system": "approval flow test",
            "config": {
                "platform_mcp_ids": [
                    "read_platform_session",
                    "send_platform_session_message",
                    "request_human_approval",
                    "check_human_approval"
                ]
            }
        })),
    )
    .await;
    agent["id"].as_str().unwrap().to_owned()
}

/// Creates the platform turn that carries the invocation MCP grant for
/// session-scoped calls and moves it to `running` (the state from which the
/// approval tool may park it at `waiting_approval`). Returns the turn id so
/// the caller can resume/complete it — an active turn would otherwise block
/// the approval-resume prompt.
async fn seed_granted_turn(fixture: &AppFixture, session_id: &str, agent_id: &str) -> String {
    let turn_input = json!({});
    let input_schema = json!({"type": "object"});
    let output_schema = json!({});
    let interaction_profile = serde_json::to_value(InteractionProfileV1::default()).unwrap();
    let created = session_control::repository::create_or_get(
        &fixture.pool,
        session_control::repository::NewTurn {
            session_id,
            request_id: &format!("approval-grant-{session_id}"),
            model: Some("test-model"),
            input: &turn_input,
            input_schema: &input_schema,
            output_schema: &output_schema,
            interaction_profile: &interaction_profile,
            trigger_type: "conversation",
            retry_of_turn_id: None,
            attempt_number: 1,
            deadline_at: i64::MAX,
            agent_id: Some(agent_id),
            runtime: None,
            protocol: "platform",
            protocol_version: "1",
            adapter_id: "platform",
            traceparent: None,
            tracestate: None,
        },
    )
    .await
    .unwrap();
    let turn_id = created.snapshot.turn.id;
    session_control::repository::transition(&fixture.pool, &turn_id, "running", None)
        .await
        .unwrap();
    turn_id
}

/// Brings a turn parked at `waiting_approval` back to `running`, mirroring
/// what the provider resume does after an approval is filed.
async fn resume_turn(fixture: &AppFixture, turn_id: &str) {
    session_control::repository::transition(&fixture.pool, turn_id, "running", None)
        .await
        .unwrap();
}

async fn complete_turn(fixture: &AppFixture, turn_id: &str) {
    resume_turn(fixture, turn_id).await;
    session_control::repository::transition(&fixture.pool, turn_id, "completed", None)
        .await
        .unwrap();
}

async fn assert_multiple_async_approvals(fixture: &AppFixture, agent_id: &str) {
    let session_id = seed_empty_session(fixture, agent_id, "approval session").await;
    let turn_id = seed_granted_turn(fixture, &session_id, agent_id).await;
    let first =
        request_approval_call(fixture, agent_id, &session_id, 7, "approve deploy", "prod").await;
    resume_turn(fixture, &turn_id).await;
    let second =
        request_approval_call(fixture, agent_id, &session_id, 8, "reject deploy", "prod").await;
    // The approval decision resumes the linked session with a new prompt turn;
    // complete the grant turn first so it does not block that resume.
    complete_turn(fixture, &turn_id).await;

    let first_payload = content_json(&first);
    let second_payload = content_json(&second);
    assert_eq!(first_payload["status"], json!("pending"));
    assert_eq!(second_payload["status"], json!("pending"));
    let first_id = first_payload["approval_id"].as_str().unwrap();
    let second_id = second_payload["approval_id"].as_str().unwrap();
    assert_ne!(first_id, second_id);

    let approval_id = wait_for_approval_item(fixture, "approve deploy", &session_id).await;
    assert_eq!(approval_id, first_id);
    let rejection_id = wait_for_approval_item(fixture, "reject deploy", &session_id).await;
    assert_eq!(rejection_id, second_id);

    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/approvals/{approval_id}/accept"),
        Some(json!({"arguments": {"environment": "staging"}})),
    )
    .await;
    assert_resume_message(fixture, &session_id, "Human approved approval").await;
    let checked = check_approval(fixture, agent_id, &approval_id).await;
    assert_eq!(checked["status"], json!("accepted"));
    assert_eq!(checked["arguments"]["environment"], json!("staging"));

    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/approvals/{rejection_id}/reject"),
        Some(json!({"feedback": "Need a canary plan."})),
    )
    .await;
    assert_resume_message(fixture, &session_id, "Need a canary plan.").await;
    let checked = check_approval(fixture, agent_id, &rejection_id).await;
    assert_eq!(checked["status"], json!("rejected"));
    assert_eq!(checked["feedback"], json!("Need a canary plan."));
}

async fn request_approval_call(
    fixture: &AppFixture,
    agent_id: &str,
    session_id: &str,
    id: i32,
    title: &str,
    environment: &str,
) -> Value {
    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/mcp/platform/{agent_id}?session_id={session_id}"),
        approval_call(id, title, environment),
    )
    .await
}

fn approval_call(id: i32, title: &str, environment: &str) -> Option<Value> {
    Some(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {
            "name": "request_human_approval",
            "arguments": {
                "title": title,
                "body": "Deploy production after smoke tests pass.",
                "session_id": "$SESSION_ID",
                "arguments": { "environment": environment }
            }
        }
    }))
}

async fn check_approval(fixture: &AppFixture, agent_id: &str, approval_id: &str) -> Value {
    let checked = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/mcp/platform/{agent_id}"),
        Some(json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "tools/call",
            "params": {
                "name": "check_human_approval",
                "arguments": { "approval_id": approval_id }
            }
        })),
    )
    .await;
    content_json(&checked)
}

async fn wait_for_approval_item(fixture: &AppFixture, title: &str, session_id: &str) -> String {
    for _ in 0..20 {
        let inbox = request_json(
            fixture.app.clone(),
            "GET",
            "/api/inbox?filter=attention",
            None,
        )
        .await;
        let found = inbox["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| is_target_approval(item, title));
        if let Some(item) = found {
            let args = serde_json::from_str::<Value>(item["args_json"].as_str().unwrap()).unwrap();
            assert_eq!(args["environment"], json!("prod"));
            assert_eq!(item["session_id"], json!(session_id));
            return item["id"].as_str().unwrap().to_owned();
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("approval item did not land in inbox");
}

async fn assert_resume_message(fixture: &AppFixture, session_id: &str, expected: &str) {
    for _ in 0..20 {
        let rows = messages::repository::list(&fixture.pool, session_id)
            .await
            .unwrap();
        if rows
            .iter()
            .any(|message| message.parts_json.contains(expected))
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("approval decision did not resume linked session");
}

async fn seed_empty_session(fixture: &AppFixture, agent_id: &str, title: &str) -> String {
    db_sessions::repository::create(
        &fixture.pool,
        "claude-code",
        Some(agent_id),
        title,
        None,
        None,
        None,
    )
    .await
    .unwrap()
    .id
}

fn is_target_approval(item: &Value, title: &str) -> bool {
    item["kind"] == "business_decision" && item["status"] == "pending" && item["title"] == title
}

fn content_text(value: &Value) -> &str {
    value["result"]["content"][0]["text"].as_str().unwrap()
}

fn content_json(value: &Value) -> Value {
    serde_json::from_str(content_text(value)).unwrap()
}

async fn assert_approval_with_options(fixture: &AppFixture, agent_id: &str) {
    let session_id = seed_empty_session(fixture, agent_id, "options session").await;
    let turn_id = seed_granted_turn(fixture, &session_id, agent_id).await;
    let call = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/mcp/platform/{agent_id}?session_id={session_id}"),
        Some(json!({
            "jsonrpc": "2.0",
            "id": 15,
            "method": "tools/call",
            "params": {
                "name": "request_human_approval",
                "arguments": {
                    "title": "Select deploy target",
                    "body": "Which target?",
                    "session_id": "$SESSION_ID",
                    "options": ["staging", "production"],
                    "arguments": { "environment": "default" }
                }
            }
        })),
    )
    .await;

    let payload = content_json(&call);
    assert_eq!(payload["status"], json!("pending"));
    assert_eq!(
        payload["arguments"]["options"],
        json!(["staging", "production"])
    );
    complete_turn(fixture, &turn_id).await;
}
