use serde_json::{json, Value};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

use super::super::{request_json, request_json_raw, start_reachable_mock_server, AppFixture};

/// Governance pipeline through the real Dify adapter: discover a live
/// (mocked) Dify app, preview it, import, run the governance test, publish +
/// activate, execute streaming Chat and pausable Workflow runs, then drive
/// drift detection + emergency stop.
///
/// Dify has a native streaming execution bridge, so `dify_app` is a
/// "conformant" runtime contract (`conformance::runtime_contract_capabilities`)
/// and both chat- and workflow-mode agents pass governance and publish.
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
    mount_chat_stream(&dify).await;

    // Discovery returns exactly the app's raw /info payload; a real client
    // (the import dialog) threads that same object through preview and
    // import unchanged, so the test does too — this is also what lets the
    // Dify-specific interaction contract select the Workflow Run surface
    // below, since that normalization reads raw.mode.
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

    // Each Dify endpoint+key maps to exactly one app (GET /info has no app
    // selector), so a distinct workflow-mode app means a distinct mock
    // deployment, not a second mount on the same server. Workflow mode is a
    // first-class Run contract: its input fields are discovered from
    // /parameters and execution uses /workflows/run rather than being blocked
    // behind an adapter-specific manual mapping.
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
    assert!(workflow_issues.is_empty(), "got: {workflow_issues:?}");
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
    save_byo_credential(fixture, &agent_id).await;
    assert_chat_streaming_round_trip(fixture, &agent_id).await;

    let workflow_imported = import(fixture, &dify_workflow, &workflow_agent).await;
    assert_eq!(workflow_imported["results"][0]["status"], "imported");
    let workflow_agent_id = workflow_imported["results"][0]["agent_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let workflow_tested = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{workflow_agent_id}/governance/test"),
        Some(json!({})),
    )
    .await;
    assert_eq!(workflow_tested["governance"]["runtime_health"], "healthy");
    publish_and_activate(fixture, &workflow_agent_id).await;
    save_byo_credential(fixture, &workflow_agent_id).await;
    mount_workflow_human_input_stream(&dify_workflow).await;
    assert_workflow_human_input_round_trip(fixture, &workflow_agent_id).await;

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

async fn save_byo_credential(fixture: &AppFixture, agent_id: &str) {
    let saved = request_json(
        fixture.app.clone(),
        "PUT",
        &format!("/api/agents/{agent_id}/byo-credential"),
        Some(json!({"api_key": "dify-test-key"})),
    )
    .await;
    assert_eq!(saved["ok"], true);
}

async fn assert_chat_streaming_round_trip(fixture: &AppFixture, agent_id: &str) {
    let session = create_dify_session(fixture, agent_id, "Dify streaming chat").await;
    let session_id = session["id"].as_str().unwrap();
    let started = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/sessions/{session_id}/turns"),
        Some(json!({
            "request_id": "dify-chat-stream-1",
            "model": {"modelID": "dify-remote"},
            "input": {"message": "summarize the evidence"}
        })),
    )
    .await;
    let turn_id = started["turn"]["id"].as_str().unwrap();
    let completed = wait_for_turn_status(fixture, session_id, turn_id, "completed").await;
    assert_eq!(completed["result"], "streamed chat answer");
    assert_eq!(completed["invocations"][0]["remote_task_id"], "chat-task-1");
    assert_eq!(
        completed["invocations"][0]["remote_session_id"],
        "conversation-1"
    );
}

async fn assert_workflow_human_input_round_trip(fixture: &AppFixture, agent_id: &str) {
    let session = create_dify_session(fixture, agent_id, "Dify Human Input workflow").await;
    let session_id = session["id"].as_str().unwrap();
    let started = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/sessions/{session_id}/turns"),
        Some(json!({
            "request_id": "dify-workflow-1",
            "model": {"modelID": "dify-workflow"},
            "input": {"topic": "agent interoperability"}
        })),
    )
    .await;
    let turn_id = started["turn"]["id"].as_str().unwrap();
    let waiting = wait_for_turn_status(fixture, session_id, turn_id, "waiting_input").await;
    assert_eq!(
        waiting["pending_input_request"]["request_id"],
        "review-form"
    );
    assert!(waiting["steps"]
        .as_array()
        .is_some_and(|steps| !steps.is_empty()));
    assert!(waiting["invocations"]
        .as_array()
        .is_some_and(|items| items.len() > 1));

    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/sessions/{session_id}/turns/{turn_id}/resume"),
        Some(json!({
            "request_id": "dify-workflow-resume-1",
            "mode": "input",
            "input": {"comment": "approved by operator", "action": "approve"}
        })),
    )
    .await;
    let completed = wait_for_turn_status(fixture, session_id, turn_id, "completed").await;
    assert_eq!(
        completed["result"]["report"],
        "workflow completed after approval"
    );
    assert_eq!(
        completed["invocations"][0]["remote_context_id"],
        "workflow-run-1"
    );
}

async fn create_dify_session(fixture: &AppFixture, agent_id: &str, title: &str) -> Value {
    request_json(
        fixture.app.clone(),
        "POST",
        "/session",
        Some(json!({
            "agent": agent_id,
            "agent_id": agent_id,
            "runtime": "dify_app",
            "title": title
        })),
    )
    .await
}

async fn wait_for_turn_status(
    fixture: &AppFixture,
    session_id: &str,
    turn_id: &str,
    expected: &str,
) -> Value {
    let mut latest = Value::Null;
    for _ in 0..60 {
        latest = request_json(
            fixture.app.clone(),
            "GET",
            &format!("/api/sessions/{session_id}/turns/{turn_id}"),
            None,
        )
        .await;
        if latest["turn"]["status"] == expected {
            return latest;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("turn did not reach {expected}; latest snapshot: {latest}")
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
    let forms = if mode == "workflow" {
        json!([{
            "text-input": {
                "variable": "topic",
                "label": "Topic",
                "required": true
            }
        }])
    } else {
        json!([])
    };
    Mock::given(method("GET"))
        .and(path("/parameters"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "user_input_form": forms
        })))
        .mount(dify)
        .await;
}

async fn mount_chat_stream(dify: &MockServer) {
    let body = [
        r#"data: {"event":"message","task_id":"chat-task-1","conversation_id":"conversation-1","answer":"streamed "}"#,
        r#"data: {"event":"message","task_id":"chat-task-1","conversation_id":"conversation-1","answer":"chat answer"}"#,
        r#"data: {"event":"message_end","task_id":"chat-task-1","conversation_id":"conversation-1"}"#,
    ]
    .join("\n\n");
    Mock::given(method("POST"))
        .and(path("/chat-messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(format!("{body}\n\n"), "text/event-stream"),
        )
        .mount(dify)
        .await;
}

async fn mount_workflow_human_input_stream(dify: &MockServer) {
    let initial = [
        r#"data: {"event":"workflow_started","task_id":"workflow-task-1","workflow_run_id":"workflow-run-1","data":{"id":"workflow-run-1"}}"#,
        r#"data: {"event":"node_started","task_id":"workflow-task-1","workflow_run_id":"workflow-run-1","data":{"id":"node-run-1","node_id":"research","node_type":"llm","title":"Research","index":1}}"#,
        r#"data: {"event":"node_finished","task_id":"workflow-task-1","workflow_run_id":"workflow-run-1","data":{"id":"node-run-1","node_id":"research","node_type":"llm","title":"Research","index":1,"status":"succeeded"}}"#,
        r#"data: {"event":"human_input_required","task_id":"workflow-task-1","workflow_run_id":"workflow-run-1","data":{"form_id":"review-form","form_token":"form-token-1","node_title":"Review result","form_content":"Approve the generated report","inputs":[{"type":"paragraph","output_variable_name":"comment","label":"Comment","required":true}],"actions":[{"id":"approve","title":"Approve"},{"id":"reject","title":"Reject"}]}}"#,
        r#"data: {"event":"workflow_paused","task_id":"workflow-task-1","workflow_run_id":"workflow-run-1","data":{"id":"workflow-run-1"}}"#,
    ]
    .join("\n\n");
    Mock::given(method("POST"))
        .and(path("/workflows/run"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(format!("{initial}\n\n"), "text/event-stream"),
        )
        .mount(dify)
        .await;

    Mock::given(method("POST"))
        .and(path("/form/human_input/form-token-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"result": "success"})))
        .mount(dify)
        .await;

    let resumed = [
        r#"data: {"event":"human_input_form_filled","task_id":"workflow-task-1","workflow_run_id":"workflow-run-1","data":{"form_id":"review-form"}}"#,
        r#"data: {"event":"node_started","task_id":"workflow-task-1","workflow_run_id":"workflow-run-1","data":{"id":"node-run-2","node_id":"publish","node_type":"template-transform","title":"Publish","index":2}}"#,
        r#"data: {"event":"node_finished","task_id":"workflow-task-1","workflow_run_id":"workflow-run-1","data":{"id":"node-run-2","node_id":"publish","node_type":"template-transform","title":"Publish","index":2,"status":"succeeded"}}"#,
        r#"data: {"event":"workflow_finished","task_id":"workflow-task-1","workflow_run_id":"workflow-run-1","data":{"id":"workflow-run-1","status":"succeeded","total_steps":2,"outputs":{"report":"workflow completed after approval"}}}"#,
    ]
    .join("\n\n");
    Mock::given(method("GET"))
        .and(path("/workflow/workflow-run-1/events"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(format!("{resumed}\n\n"), "text/event-stream"),
        )
        .mount(dify)
        .await;
}
