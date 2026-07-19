#[path = "managed_agents_support/mod.rs"]
mod support;

use serde_json::{json, Value};
use support::{
    flows, request_json, request_json_raw, request_json_raw_with_key, request_with_headers,
    AppFixture,
};

static DB_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn invocation_telemetry_persists_and_continues_w3c_context_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping telemetry integration test: TEST_DATABASE_URL is not set");
        return;
    };
    let session = litellm_rust::db::managed_agents::sessions::repository::create(
        &fixture.pool,
        "claude-code",
        None,
        "Telemetry boundary test",
        None,
        Some("admin"),
        None,
    )
    .await
    .unwrap();
    let parent = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    let created = litellm_rust::db::managed_agents::session_control::repository::create_or_get(
        &fixture.pool,
        litellm_rust::db::managed_agents::session_control::repository::NewTurn {
            session_id: &session.id,
            request_id: "telemetry-request",
            model: Some("test-model"),
            agent_id: None,
            runtime: None,
            protocol: "a2a",
            protocol_version: "1.0",
            adapter_id: "a2a_v1",
            traceparent: Some(parent),
            tracestate: Some("vendor=value"),
        },
    )
    .await
    .unwrap();
    let invocation = &created.snapshot.invocations[0];
    let telemetry = &invocation.metadata["telemetry"];
    assert_eq!(telemetry["trace_id"], "4bf92f3577b34da6a3ce929d0e0e4736");
    assert_eq!(telemetry["parent_traceparent"], parent);
    assert_eq!(telemetry["adapter_id"], "a2a_v1");
    let propagated = telemetry["traceparent"].as_str().unwrap();
    assert!(propagated.starts_with("00-4bf92f3577b34da6a3ce929d0e0e4736-"));
    assert_ne!(propagated, parent);
    let headers =
        litellm_rust::managed_agents::adapters::telemetry::trace_headers(&invocation.metadata)
            .unwrap();
    assert_eq!(headers.0, propagated);
    assert_eq!(headers.1.as_deref(), Some("vendor=value"));

    litellm_rust::db::managed_agents::session_control::repository::transition(
        &fixture.pool,
        &created.snapshot.turn.id,
        "running",
        None,
    )
    .await
    .unwrap();
    litellm_rust::db::managed_agents::session_control::repository::transition(
        &fixture.pool,
        &created.snapshot.turn.id,
        "completed",
        None,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn canonical_external_artifacts_are_unverified_idempotent_and_owner_scoped() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping artifact integration test: TEST_DATABASE_URL is not set");
        return;
    };
    let session = litellm_rust::db::managed_agents::sessions::repository::create(
        &fixture.pool,
        "claude-code",
        None,
        "Artifact boundary test",
        None,
        Some("admin"),
        None,
    )
    .await
    .unwrap();
    let turn = litellm_rust::db::managed_agents::session_control::repository::create_or_get(
        &fixture.pool,
        litellm_rust::db::managed_agents::session_control::repository::NewTurn {
            session_id: &session.id,
            request_id: "artifact-boundary-request",
            model: Some("test-model"),
            agent_id: None,
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
    let endpoint = format!(
        "/api/sessions/{}/turns/{}/artifacts",
        session.id, turn.snapshot.turn.id
    );
    let input = json!({
        "id": "source-report-1",
        "name": "assessment.json",
        "media_type": "application/json",
        "digest": format!("sha256:{}", "a".repeat(64)),
        "size_bytes": 42,
        "uri": "https://intelligence.example/reports/42",
        "metadata": {"classification": "open-source"}
    });
    let created = request_json(fixture.app.clone(), "POST", &endpoint, Some(input.clone())).await;
    assert_eq!(created["status"], "unverified_external");
    assert_eq!(created["download_url"], Value::Null);
    assert_eq!(
        created["external_reference_url"],
        "https://intelligence.example/reports/42"
    );
    assert_eq!(created["invocation_id"], turn.snapshot.invocations[0].id);
    assert_eq!(created["metadata"]["name"], "assessment.json");

    let duplicate = request_json(fixture.app.clone(), "POST", &endpoint, Some(input.clone())).await;
    assert_eq!(duplicate["id"], created["id"]);
    let listed = request_json(
        fixture.app.clone(),
        "GET",
        &format!("/api/sessions/{}/artifacts", session.id),
        None,
    )
    .await;
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["id"], created["id"]);

    let mut changed = input;
    changed["digest"] = json!(format!("sha256:{}", "b".repeat(64)));
    let (status, _) = request_json_raw(fixture.app.clone(), "POST", &endpoint, Some(changed)).await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);

    let stored = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
        r#"
        SELECT status, storage_backend, object_bucket, object_key
        FROM "LiteLLM_ManagedArtifactsTable"
        WHERE id = $1
        "#,
    )
    .bind(created["id"].as_str().unwrap())
    .fetch_one(&fixture.pool)
    .await
    .unwrap();
    assert_eq!(stored.0, "unverified_external");
    assert_eq!(stored.1, "external_reference");
    assert!(stored.2.is_none());
    assert!(stored.3.is_none());
}

#[tokio::test]
async fn mcp_grants_are_invocation_scoped_and_revoked_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping MCP invocation grant integration test: TEST_DATABASE_URL is not set");
        return;
    };
    let agent = create_test_agent(
        &fixture,
        json!({
            "name": "mcp-grant-agent",
            "owner_id": "mcp-owner",
            "model": "test-model",
            "system": "test",
            "tools": [],
            "config": {
                "platform_mcp_ids": ["read_platform_session"]
            }
        }),
    )
    .await;
    let agent_id = agent["id"].as_str().unwrap();
    let session = litellm_rust::db::managed_agents::sessions::repository::create(
        &fixture.pool,
        "claude-code",
        Some(agent_id),
        "MCP grant test",
        None,
        Some("mcp-owner"),
        None,
    )
    .await
    .unwrap();
    let created = litellm_rust::db::managed_agents::session_control::repository::create_or_get(
        &fixture.pool,
        litellm_rust::db::managed_agents::session_control::repository::NewTurn {
            session_id: &session.id,
            request_id: "mcp-grant-request",
            model: Some("test-model"),
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
    let invocation_id = &created.snapshot.invocations[0].id;
    let grant =
        litellm_rust::db::managed_agents::mcp_invocation_grants::repository::active_for_invocation(
            &fixture.pool,
            invocation_id,
            "platform",
        )
        .await
        .unwrap()
        .expect("active grant");
    assert!(grant.allows("read_platform_session"));
    assert!(!grant.allows("create_managed_agent"));

    let (capability_token, _) =
        litellm_rust::db::managed_agents::sources::repository::issue_capability_token(
            &fixture.pool,
            &session.id,
            agent_id,
            json!({"mcp": true}),
            60_000,
        )
        .await
        .unwrap();
    let endpoint = format!("/mcp/platform/{agent_id}?session_id={}", session.id);
    let (status, listed) = request_json_raw_with_key(
        fixture.app.clone(),
        "POST",
        &endpoint,
        Some(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})),
        &capability_token,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let listed = serde_json::from_str::<Value>(&listed).unwrap();
    let tools = listed["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "read_platform_session");
    let (status, denied) = request_json_raw_with_key(
        fixture.app.clone(),
        "POST",
        &endpoint,
        Some(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {"name": "create_managed_agent", "arguments": {}}
        })),
        &capability_token,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let denied = serde_json::from_str::<Value>(&denied).unwrap();
    assert_eq!(denied["error"]["code"], -32602);

    let (status, _) = request_json_raw_with_key(
        fixture.app.clone(),
        "POST",
        &format!("/mcp/platform/{agent_id}"),
        Some(json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list"})),
        &capability_token,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::UNAUTHORIZED);

    litellm_rust::db::managed_agents::session_control::repository::transition(
        &fixture.pool,
        &created.snapshot.turn.id,
        "cancelled",
        None,
    )
    .await
    .unwrap();
    assert!(
        litellm_rust::db::managed_agents::mcp_invocation_grants::repository::active_for_invocation(
            &fixture.pool,
            invocation_id,
            "platform",
        )
        .await
        .unwrap()
        .is_none()
    );
    let revoked = request_json(
        fixture.app.clone(),
        "POST",
        &endpoint,
        Some(json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"})),
    )
    .await;
    assert_eq!(revoked["error"]["code"], -32001);
}

#[tokio::test]
async fn cloud_events_ingress_dedup_and_egress_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping CloudEvents integration test: TEST_DATABASE_URL is not set");
        return;
    };
    let session = litellm_rust::db::managed_agents::sessions::repository::create(
        &fixture.pool,
        "claude-code",
        None,
        "CloudEvents test",
        None,
        Some("cloud-owner"),
        None,
    )
    .await
    .unwrap();
    let event = json!({
        "specversion": "1.0",
        "id": "delivery-1",
        "source": "urn:example:agent-runtime",
        "type": "com.example.agent.completed",
        "subject": "task/42",
        "time": "2026-07-16T12:00:00Z",
        "datacontenttype": "application/json",
        "traceparent": "00-test-trace-test-span-01",
        "data": {"result": "done"}
    });
    let endpoint = format!("/api/sessions/{}/cloudevents", session.id);
    let first = request_with_headers(
        fixture.app.clone(),
        "POST",
        &endpoint,
        event.to_string(),
        "application/cloudevents+json",
        &[],
        axum::http::StatusCode::ACCEPTED,
    )
    .await;
    assert!(!serde_json::from_str::<Value>(&first).unwrap()["duplicate"]
        .as_bool()
        .unwrap());
    let replay = request_with_headers(
        fixture.app.clone(),
        "POST",
        &endpoint,
        event.to_string(),
        "application/cloudevents+json",
        &[],
        axum::http::StatusCode::ACCEPTED,
    )
    .await;
    assert!(serde_json::from_str::<Value>(&replay).unwrap()["duplicate"]
        .as_bool()
        .unwrap());

    let mut changed = event.clone();
    changed["data"] = json!({"result": "tampered"});
    request_with_headers(
        fixture.app.clone(),
        "POST",
        &endpoint,
        changed.to_string(),
        "application/cloudevents+json",
        &[],
        axum::http::StatusCode::BAD_REQUEST,
    )
    .await;

    let stored = litellm_rust::db::managed_agents::runtime_events::repository::list(
        &fixture.pool,
        &session.id,
    )
    .await
    .unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0]["type"], "external.event.received");
    assert_eq!(
        stored[0]["raw"]["traceparent"],
        "00-test-trace-test-span-01"
    );

    let exported = request_json(fixture.app.clone(), "GET", &endpoint, None).await;
    assert_eq!(exported[0]["specversion"], "1.0");
    assert_eq!(
        exported[0]["type"],
        "io.lap.runtime.external.event.received"
    );
    assert_eq!(exported[0]["data"]["type"], "external.event.received");

    let receipts: Vec<(String, i32)> = sqlx::query_as(
        r#"
        SELECT direction, delivery_count
        FROM "LiteLLM_CloudEventReceiptsTable"
        WHERE session_id = $1
        ORDER BY direction
        "#,
    )
    .bind(&session.id)
    .fetch_all(&fixture.pool)
    .await
    .unwrap();
    assert_eq!(
        receipts,
        vec![("egress".to_owned(), 1), ("ingress".to_owned(), 2)]
    );
}

#[tokio::test]
async fn external_identity_requires_explicit_mapping_against_postgres() {
    use litellm_rust::managed_agents::adapters::{
        platform_identity::DatabaseIdentityAdapter, types::ExternalIdentity, AdapterError,
        IdentityAdapter,
    };

    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping identity mapping integration test: TEST_DATABASE_URL is not set");
        return;
    };
    litellm_rust::db::managed_agents::users::repository::ensure(&fixture.pool, "mapped-user")
        .await
        .unwrap();
    let adapter = DatabaseIdentityAdapter::new(fixture.pool.clone());
    let identity = ExternalIdentity {
        issuer: "https://identity.example.test".to_owned(),
        subject: "external-subject-1".to_owned(),
        audience: Some("agent-platform".to_owned()),
        claims: json!({"email": "sensitive@example.test", "roles": ["operator"]}),
    };

    let pending_id = match adapter.resolve(&identity).await {
        Err(AdapterError::UnmappedIdentity(mapping_id)) => mapping_id,
        result => panic!("expected pending identity, got {result:?}"),
    };
    let pending = litellm_rust::db::managed_agents::identity_mappings::repository::get(
        &fixture.pool,
        &pending_id,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(pending.status, "pending");
    assert!(pending.claims_digest.starts_with("sha256:"));
    assert!(!pending
        .evidence
        .to_string()
        .contains("sensitive@example.test"));

    let user_key = litellm_rust::db::managed_agents::api_keys::repository::create(
        &fixture.pool,
        Some("identity-non-admin"),
        Some("mapped-user"),
        Some("user"),
    )
    .await
    .unwrap()
    .key;
    let (status, _) = request_json_raw_with_key(
        fixture.app.clone(),
        "GET",
        "/api/identity-mappings",
        None,
        &user_key,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::FORBIDDEN);

    let listed = request_json(
        fixture.app.clone(),
        "GET",
        "/api/identity-mappings?status=pending",
        None,
    )
    .await;
    assert_eq!(listed["mappings"][0]["id"], pending.id);
    let bound = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/identity-mappings/{}/bind", pending.id),
        Some(json!({"user_id": "mapped-user"})),
    )
    .await;
    assert_eq!(bound["status"], "active");
    assert_eq!(bound["platform_user_id"], "mapped-user");
    let resolved = adapter.resolve(&identity).await.expect("identity resolves");
    assert_eq!(resolved.user_id, "mapped-user");
    assert_eq!(resolved.agent_id, None);
    assert_eq!(
        resolved.mapping_evidence["mapping_id"].as_str(),
        Some(pending_id.as_str())
    );

    let blocked = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/identity-mappings/{}/block", pending.id),
        None,
    )
    .await;
    assert_eq!(blocked["status"], "blocked");
    assert!(blocked["platform_user_id"].is_null());
    assert!(matches!(
        adapter.resolve(&identity).await,
        Err(AdapterError::BlockedIdentity(_))
    ));
}

#[tokio::test]
async fn session_turn_idempotency_approval_and_cancel_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping session control integration test: TEST_DATABASE_URL is not set");
        return;
    };
    let session = litellm_rust::db::managed_agents::sessions::repository::create(
        &fixture.pool,
        "claude-code",
        None,
        "turn state test",
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let input = || litellm_rust::db::managed_agents::session_control::repository::NewTurn {
        session_id: &session.id,
        request_id: "req_idempotent",
        model: Some("test-model"),
        agent_id: None,
        runtime: None,
        protocol: "platform",
        protocol_version: "1",
        adapter_id: "platform",
        traceparent: None,
        tracestate: None,
    };
    let created = litellm_rust::db::managed_agents::session_control::repository::create_or_get(
        &fixture.pool,
        input(),
    )
    .await
    .unwrap();
    assert!(created.created);
    assert_eq!(created.snapshot.invocations[0].protocol_version, "1");
    let duplicate = litellm_rust::db::managed_agents::session_control::repository::create_or_get(
        &fixture.pool,
        input(),
    )
    .await
    .unwrap();
    assert!(!duplicate.created);
    assert_eq!(created.snapshot.turn.id, duplicate.snapshot.turn.id);

    let invocation_id = &created.snapshot.invocations[0].id;
    let lease = litellm_rust::db::managed_agents::credential_leases::repository::issue(
        &fixture.pool,
        litellm_rust::db::managed_agents::credential_leases::repository::NewCredentialLease {
            owner_id: "lease-owner",
            session_id: &session.id,
            turn_id: &created.snapshot.turn.id,
            invocation_id,
            credential_name: "provider:test",
            adapter_id: "platform",
            purpose: "agent_runtime",
            ttl_ms: 60_000,
            metadata: json!({"protocol": "platform"}),
        },
    )
    .await
    .unwrap();
    assert!(
        litellm_rust::db::managed_agents::credential_leases::repository::mark_resolved(
            &fixture.pool,
            &lease.id,
            "lease-owner",
            litellm_rust::db::managed_agents::now_ms(),
        )
        .await
        .unwrap()
    );

    let turn_id = created.snapshot.turn.id;
    litellm_rust::db::managed_agents::session_control::repository::transition(
        &fixture.pool,
        &turn_id,
        "running",
        None,
    )
    .await
    .unwrap();
    let approval = litellm_rust::db::managed_agents::inbox::repository::create_approval(
        &fixture.pool,
        "runtime_permission",
        "test permission".to_owned(),
        Some(session.id.clone()),
        None,
        None,
        Some(json!({"request_id": "provider-request"})),
    )
    .await
    .unwrap();
    assert_eq!(approval.turn_id.as_deref(), Some(turn_id.as_str()));
    assert_eq!(approval.request_id.as_deref(), Some("req_idempotent"));
    assert_eq!(
        litellm_rust::db::managed_agents::session_control::repository::active_turn(
            &fixture.pool,
            &session.id,
        )
        .await
        .unwrap()
        .unwrap()
        .turn
        .status,
        "waiting_approval"
    );

    litellm_rust::db::managed_agents::session_control::repository::transition(
        &fixture.pool,
        &turn_id,
        "running",
        None,
    )
    .await
    .unwrap();
    litellm_rust::db::managed_agents::session_control::repository::transition(
        &fixture.pool,
        &turn_id,
        "cancelling",
        None,
    )
    .await
    .unwrap();
    litellm_rust::db::managed_agents::session_control::repository::transition(
        &fixture.pool,
        &turn_id,
        "cancelled",
        None,
    )
    .await
    .unwrap();
    assert!(
        litellm_rust::db::managed_agents::session_control::repository::active_turn(
            &fixture.pool,
            &session.id,
        )
        .await
        .unwrap()
        .is_none()
    );
    let lease = litellm_rust::db::managed_agents::credential_leases::repository::get(
        &fixture.pool,
        &lease.id,
    )
    .await
    .unwrap()
    .unwrap();
    assert!(lease.last_resolved_at.is_some());
    assert!(lease.revoked_at.is_some());
    let events = litellm_rust::db::managed_agents::session_control::repository::list_events(
        &fixture.pool,
        &session.id,
        0,
    )
    .await
    .unwrap();
    assert_eq!(events.first().unwrap().event_type, "turn.accepted");
    assert_eq!(events.last().unwrap().event_type, "turn.cancelled");

    let recovered = litellm_rust::db::managed_agents::session_control::repository::create_or_get(
        &fixture.pool,
        litellm_rust::db::managed_agents::session_control::repository::NewTurn {
            session_id: &session.id,
            request_id: "req_recovery",
            model: Some("test-model"),
            agent_id: None,
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
    litellm_rust::db::managed_agents::session_control::repository::transition(
        &fixture.pool,
        &recovered.snapshot.turn.id,
        "running",
        None,
    )
    .await
    .unwrap();
    litellm_rust::db::managed_agents::sessions::repository::set_status(
        &fixture.pool,
        &session.id,
        "idle",
    )
    .await
    .unwrap();
    assert_eq!(
        litellm_rust::http::sessions::recovery::run_once(
            fixture.state.clone(),
            litellm_rust::db::managed_agents::now_ms(),
        )
        .await
        .unwrap(),
        1
    );
    let recovered = litellm_rust::db::managed_agents::session_control::repository::get_turn(
        &fixture.pool,
        &session.id,
        &recovered.snapshot.turn.id,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(recovered.turn.status, "completed");
}

#[tokio::test]
async fn permission_matrix_and_not_found_isolation_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new_with_litellm_key_info().await else {
        eprintln!("skipping permission integration test: TEST_DATABASE_URL is not set");
        return;
    };

    let alice = litellm_rust::db::managed_agents::users::repository::create(
        &fixture.pool,
        "alice",
        "Alice",
        Some("alice@example.com"),
    )
    .await
    .unwrap();
    let bob = litellm_rust::db::managed_agents::users::repository::create(
        &fixture.pool,
        "bob",
        "Bob",
        Some("bob@example.com"),
    )
    .await
    .unwrap();
    let bob_key = litellm_rust::db::managed_agents::api_keys::repository::create(
        &fixture.pool,
        Some("bob-test"),
        Some(&bob.id),
        Some("user"),
    )
    .await
    .unwrap()
    .key;

    let agent = create_owned_agent(&fixture, "private-agent", &alice.id).await;
    let other_agent = create_owned_agent(&fixture, "other-agent", &alice.id).await;
    let session = litellm_rust::db::managed_agents::sessions::repository::create(
        &fixture.pool,
        "claude-code",
        Some(&agent),
        "private session",
        None,
        Some(&alice.id),
        None,
    )
    .await
    .unwrap();

    assert_resources_hidden(&fixture, &bob_key, &agent, &session.id).await;

    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent}/grants"),
        Some(json!({ "user_id": bob.id, "permission": "use" })),
    )
    .await;
    assert_status(
        &fixture,
        &bob_key,
        "GET",
        &format!("/api/agents/{agent}"),
        None,
        axum::http::StatusCode::OK,
    )
    .await;
    assert_status(
        &fixture,
        &bob_key,
        "PATCH",
        &format!("/api/agents/{agent}"),
        Some(json!({ "description": "blocked" })),
        axum::http::StatusCode::NOT_FOUND,
    )
    .await;

    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent}/grants"),
        Some(json!({ "user_id": bob.id, "permission": "edit" })),
    )
    .await;
    assert_status(
        &fixture,
        &bob_key,
        "PATCH",
        &format!("/api/agents/{agent}"),
        Some(json!({ "description": "direct edit" })),
        axum::http::StatusCode::OK,
    )
    .await;
    request_json(
        fixture.app.clone(),
        "DELETE",
        &format!("/api/agents/{agent}/grants/{}", bob.id),
        None,
    )
    .await;

    let use_group = litellm_rust::db::managed_agents::groups::repository::create(
        &fixture.pool,
        "analysts",
        None,
        "admin",
    )
    .await
    .unwrap();
    let edit_group = litellm_rust::db::managed_agents::groups::repository::create(
        &fixture.pool,
        "maintainers",
        None,
        "admin",
    )
    .await
    .unwrap();
    for group in [&use_group, &edit_group] {
        litellm_rust::db::managed_agents::groups::members::upsert(
            &fixture.pool,
            &group.id,
            &bob.id,
            "member",
            "admin",
        )
        .await
        .unwrap();
    }
    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent}/group-grants"),
        Some(json!({ "group_id": use_group.id, "permission": "use" })),
    )
    .await;
    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent}/group-grants"),
        Some(json!({ "group_id": edit_group.id, "permission": "edit" })),
    )
    .await;
    assert_status(
        &fixture,
        &bob_key,
        "PATCH",
        &format!("/api/agents/{agent}"),
        Some(json!({ "description": "stacked edit" })),
        axum::http::StatusCode::OK,
    )
    .await;

    litellm_rust::db::managed_agents::groups::members::delete(
        &fixture.pool,
        &edit_group.id,
        &bob.id,
    )
    .await
    .unwrap();
    assert_status(
        &fixture,
        &bob_key,
        "GET",
        &format!("/api/agents/{agent}"),
        None,
        axum::http::StatusCode::OK,
    )
    .await;
    assert_status(
        &fixture,
        &bob_key,
        "PATCH",
        &format!("/api/agents/{agent}"),
        Some(json!({ "description": "no edit" })),
        axum::http::StatusCode::NOT_FOUND,
    )
    .await;

    litellm_rust::db::managed_agents::groups::repository::update_status(
        &fixture.pool,
        &use_group.id,
        "disabled",
    )
    .await
    .unwrap();
    assert_resources_hidden(&fixture, &bob_key, &agent, &session.id).await;

    litellm_rust::db::managed_agents::users::repository::update_status(
        &fixture.pool,
        &bob.id,
        "disabled",
    )
    .await
    .unwrap();
    assert_status(
        &fixture,
        &bob_key,
        "GET",
        &format!("/api/agents/{agent}"),
        None,
        axum::http::StatusCode::UNAUTHORIZED,
    )
    .await;

    assert_status(
        &fixture,
        "external-test-key",
        "GET",
        "/api/auth/me",
        None,
        axum::http::StatusCode::OK,
    )
    .await;
    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent}/grants"),
        Some(json!({ "user_id": "external-user", "permission": "use" })),
    )
    .await;
    assert_status(
        &fixture,
        "external-test-key",
        "GET",
        &format!("/api/agents/{agent}"),
        None,
        axum::http::StatusCode::OK,
    )
    .await;
    assert_status(
        &fixture,
        "external-test-key",
        "GET",
        &format!("/api/agents/{other_agent}"),
        None,
        axum::http::StatusCode::NOT_FOUND,
    )
    .await;
    assert_status(
        &fixture,
        "external-test-key",
        "GET",
        &format!("/session/{}", session.id),
        None,
        axum::http::StatusCode::NOT_FOUND,
    )
    .await;
    assert_status(
        &fixture,
        "external-test-key",
        "GET",
        &format!("/session/{}/workspace/files", session.id),
        None,
        axum::http::StatusCode::NOT_FOUND,
    )
    .await;
}

#[tokio::test]
async fn imported_agent_governance_publish_and_rollback_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping governance integration test: TEST_DATABASE_URL is not set");
        return;
    };

    let owner = litellm_rust::db::managed_agents::users::repository::create(
        &fixture.pool,
        "import-owner",
        "导入负责人",
        Some("import-owner@example.com"),
    )
    .await
    .unwrap();
    let owner_key = litellm_rust::db::managed_agents::api_keys::repository::create(
        &fixture.pool,
        Some("import-owner-test"),
        Some(&owner.id),
        Some("user"),
    )
    .await
    .unwrap()
    .key;
    let agent_id = create_owned_agent(&fixture, "external-managed-agent", &owner.id).await;

    let credential_name = format!("import:{agent_id}");
    litellm_rust::db::credentials::upsert_personal(
        &fixture.pool,
        &credential_name,
        &owner.id,
        json!({ "api_key": "owner-secret" }),
        json!({ "purpose": "imported_runtime" }),
        &owner.id,
    )
    .await
    .unwrap();
    assert!(litellm_rust::db::credentials::get_personal_by_name(
        &fixture.pool,
        &credential_name,
        &owner.id,
    )
    .await
    .unwrap()
    .is_some());
    assert!(litellm_rust::db::credentials::get_personal_by_name(
        &fixture.pool,
        &credential_name,
        "another-user",
    )
    .await
    .unwrap()
    .is_none());
    assert!(
        litellm_rust::db::credentials::get_by_name(&fixture.pool, &credential_name)
            .await
            .unwrap()
            .is_none()
    );

    let imported = litellm_rust::db::managed_agents::governance::record_import(
        &fixture.pool,
        litellm_rust::db::managed_agents::governance::ImportedSource {
            agent_id: &agent_id,
            owner_id: &owner.id,
            provider: "external-test",
            endpoint: "https://runtime.example.test",
            external_agent_id: "external-1",
            source_hash: "source-v1",
            credential_scope: "personal",
            credential_name: Some(&credential_name),
        },
    )
    .await
    .unwrap();
    assert_eq!(imported.source_version, 1);

    let tested = request_json_with_key(
        &fixture,
        &owner_key,
        "POST",
        &format!("/api/agents/{agent_id}/governance/test"),
        Some(json!({})),
    )
    .await;
    assert_eq!(tested["governance"]["lifecycle_status"], "tested");
    assert_eq!(tested["governance"]["runtime_health"], "healthy");

    let requested = request_json_with_key(
        &fixture,
        &owner_key,
        "POST",
        &format!("/api/agents/{agent_id}/governance/request-publish"),
        Some(json!({})),
    )
    .await;
    let approval_id = requested["approval"]["id"].as_str().unwrap();
    assert_status(
        &fixture,
        &owner_key,
        "POST",
        &format!("/api/approvals/{approval_id}/accept"),
        Some(json!({ "arguments": null })),
        axum::http::StatusCode::BAD_REQUEST,
    )
    .await;
    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/approvals/{approval_id}/accept"),
        Some(json!({ "arguments": null })),
    )
    .await;
    let first_published = request_json_with_key(
        &fixture,
        &owner_key,
        "GET",
        &format!("/api/agents/{agent_id}/governance"),
        None,
    )
    .await;
    assert_eq!(
        first_published["governance"]["lifecycle_status"],
        "published"
    );
    assert_eq!(first_published["governance"]["published_revision"], 1);

    request_json_with_key(
        &fixture,
        &owner_key,
        "PATCH",
        &format!("/api/agents/{agent_id}"),
        Some(json!({ "description": "第二个外部版本" })),
    )
    .await;
    let imported_v2 = litellm_rust::db::managed_agents::governance::record_import(
        &fixture.pool,
        litellm_rust::db::managed_agents::governance::ImportedSource {
            agent_id: &agent_id,
            owner_id: &owner.id,
            provider: "external-test",
            endpoint: "https://runtime.example.test",
            external_agent_id: "external-1",
            source_hash: "source-v2",
            credential_scope: "personal",
            credential_name: Some(&credential_name),
        },
    )
    .await
    .unwrap();
    assert_eq!(imported_v2.source_version, 2);

    request_json_with_key(
        &fixture,
        &owner_key,
        "POST",
        &format!("/api/agents/{agent_id}/governance/test"),
        Some(json!({})),
    )
    .await;
    let requested_v2 = request_json_with_key(
        &fixture,
        &owner_key,
        "POST",
        &format!("/api/agents/{agent_id}/governance/request-publish"),
        Some(json!({})),
    )
    .await;
    let approval_v2 = requested_v2["approval"]["id"].as_str().unwrap();
    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/approvals/{approval_v2}/accept"),
        Some(json!({ "arguments": null })),
    )
    .await;

    // Rollback must restore configuration only — it must never re-activate a
    // paused/stopped agent by itself (that is activate's job, with preflight).
    litellm_rust::db::managed_agents::registry::repository::set_status(
        &fixture.pool,
        &agent_id,
        "paused",
    )
    .await
    .unwrap();
    let rolled_back = request_json_with_key(
        &fixture,
        &owner_key,
        "POST",
        &format!("/api/agents/{agent_id}/governance/rollback"),
        Some(json!({})),
    )
    .await;
    assert_eq!(rolled_back["governance"]["lifecycle_status"], "rolled_back");
    assert_eq!(rolled_back["restored_from_revision"], 1);
    assert_ne!(rolled_back["agent"]["description"], "第二个外部版本");
    assert_eq!(rolled_back["agent"]["status"], "paused");
}

#[tokio::test]
async fn governance_state_machine_gates_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping governance gate integration test: TEST_DATABASE_URL is not set");
        return;
    };

    // --- Emergency-stopped governed agent: resume and new interaction are
    // both refused until it goes back through the governance pipeline.
    let governed = create_test_agent(
        &fixture,
        json!({
            "name": "gate-agent",
            "owner_id": "gate-owner",
            "model": "test-model",
            "system": "gate"
        }),
    )
    .await;
    let governed_id = governed["id"].as_str().unwrap();
    litellm_rust::db::managed_agents::governance::record_import(
        &fixture.pool,
        litellm_rust::db::managed_agents::governance::ImportedSource {
            agent_id: governed_id,
            owner_id: "gate-owner",
            provider: "external-test",
            endpoint: "https://runtime.example.test",
            external_agent_id: "gate-external-1",
            source_hash: "gate-v1",
            credential_scope: "personal",
            credential_name: None,
        },
    )
    .await
    .unwrap();
    litellm_rust::db::managed_agents::registry::repository::set_status(
        &fixture.pool,
        governed_id,
        "active",
    )
    .await
    .unwrap();
    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{governed_id}/emergency-stop"),
        Some(json!({})),
    )
    .await;

    let (resume_status, resume_body) = request_json_raw(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{governed_id}/resume"),
        Some(json!({})),
    )
    .await;
    assert_eq!(resume_status, axum::http::StatusCode::BAD_REQUEST);
    assert!(resume_body.contains("治理挂起"), "got: {resume_body}");

    let governed_session = litellm_rust::db::managed_agents::sessions::repository::create(
        &fixture.pool,
        "claude-code",
        Some(governed_id),
        "gate session",
        None,
        Some("gate-owner"),
        None,
    )
    .await
    .unwrap();
    let (message_status, message_body) = request_json_raw(
        fixture.app.clone(),
        "POST",
        &format!("/session/{}/message", governed_session.id),
        Some(json!({
            "model": { "modelID": "test-model" },
            "parts": [{ "type": "text", "text": "hi" }]
        })),
    )
    .await;
    assert_eq!(message_status, axum::http::StatusCode::BAD_REQUEST);
    assert!(message_body.contains("紧急停止"), "got: {message_body}");

    // --- Plain paused agent (no governance): new prompts are refused while
    // paused, and resume works because there is no governance suspension.
    let native = create_test_agent(
        &fixture,
        json!({
            "name": "native-gate-agent",
            "owner_id": "gate-owner",
            "model": "test-model",
            "system": "native"
        }),
    )
    .await;
    let native_id = native["id"].as_str().unwrap();
    litellm_rust::db::managed_agents::registry::repository::set_status(
        &fixture.pool,
        native_id,
        "active",
    )
    .await
    .unwrap();
    request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{native_id}/pause"),
        Some(json!({})),
    )
    .await;
    let native_session = litellm_rust::db::managed_agents::sessions::repository::create(
        &fixture.pool,
        "claude-code",
        Some(native_id),
        "native gate session",
        None,
        Some("gate-owner"),
        None,
    )
    .await
    .unwrap();
    let (paused_status, paused_body) = request_json_raw(
        fixture.app.clone(),
        "POST",
        &format!("/session/{}/message", native_session.id),
        Some(json!({
            "model": { "modelID": "test-model" },
            "parts": [{ "type": "text", "text": "hi" }]
        })),
    )
    .await;
    assert_eq!(paused_status, axum::http::StatusCode::BAD_REQUEST);
    assert!(paused_body.contains("已暂停"), "got: {paused_body}");
    let resumed = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{native_id}/resume"),
        Some(json!({})),
    )
    .await;
    assert_eq!(resumed["status"], "active");
}

#[tokio::test]
async fn import_blocking_and_drift_review_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping import gate integration test: TEST_DATABASE_URL is not set");
        return;
    };

    // A blocking issue (empty identity) must reject the import outright.
    let (blocked_status, blocked_body) = request_json_raw(
        fixture.app.clone(),
        "POST",
        "/api/agents/import/elastic",
        Some(json!({
            "endpoint": "https://example.com",
            "credential_mode": "byo",
            "agents": [{ "external_id": "   " }]
        })),
    )
    .await;
    assert_eq!(blocked_status, axum::http::StatusCode::BAD_REQUEST);
    assert!(blocked_body.contains("无法导入"), "got: {blocked_body}");

    // First import lands; re-importing a changed definition must not touch the
    // agent — it becomes a drift candidate awaiting review.
    let first = request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents/import/elastic",
        Some(json!({
            "endpoint": "https://example.com",
            "credential_mode": "byo",
            "agents": [{ "external_id": "drift-ext-1", "name": "Drift Agent", "raw": { "a": 1 } }]
        })),
    )
    .await;
    assert_eq!(first["results"][0]["status"], "imported");
    let imported_id = first["results"][0]["agent_id"].as_str().unwrap().to_owned();

    let second = request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents/import/elastic",
        Some(json!({
            "endpoint": "https://example.com",
            "credential_mode": "byo",
            "agents": [{ "external_id": "drift-ext-1", "name": "Drift Agent", "raw": { "a": 2 } }]
        })),
    )
    .await;
    assert_eq!(second["results"][0]["status"], "drift_pending");
    assert!(second["results"][0]["snapshot_id"].as_str().is_some());

    let row =
        litellm_rust::db::managed_agents::registry::repository::get(&fixture.pool, &imported_id)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(
        row.config.pointer("/source/raw/a"),
        Some(&json!(1)),
        "re-import must not overwrite the approved configuration"
    );
    let source = litellm_rust::db::managed_agents::sources::repository::get_source_by_agent(
        &fixture.pool,
        &imported_id,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(source.sync_state, "drifted");
    assert!(source.candidate_snapshot_id.is_some());
}

async fn request_json_with_key(
    fixture: &AppFixture,
    key: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> Value {
    let (status, response) =
        request_json_raw_with_key(fixture.app.clone(), method, path, body, key).await;
    assert!(status.is_success(), "{method} {path}: {status} {response}");
    serde_json::from_str(&response).unwrap_or_else(|_| json!({}))
}

async fn create_owned_agent(fixture: &AppFixture, name: &str, owner_id: &str) -> String {
    request_json(
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
    .await["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn assert_resources_hidden(
    fixture: &AppFixture,
    key: &str,
    agent_id: &str,
    session_id: &str,
) {
    for path in [
        format!("/api/agents/{agent_id}"),
        format!("/api/agents/{agent_id}/workspace/files"),
        format!("/session/{session_id}"),
        format!("/session/{session_id}/workspace/files"),
        format!("/session/{session_id}/workspace/browse"),
        format!("/session/{session_id}/workspace/folders"),
        format!("/session/{session_id}/workspace/trash"),
    ] {
        assert_status(
            fixture,
            key,
            "GET",
            &path,
            None,
            axum::http::StatusCode::NOT_FOUND,
        )
        .await;
    }
    for (path, body) in [
        (
            format!("/session/{session_id}/workspace/files/move"),
            json!({ "source_path": "a.txt", "destination_path": "b.txt" }),
        ),
        (
            format!("/session/{session_id}/workspace/files/copy"),
            json!({ "source_path": "a.txt", "destination_path": "b.txt" }),
        ),
        (
            format!("/session/{session_id}/workspace/files/batch-delete"),
            json!({ "paths": ["a.txt"] }),
        ),
        (
            format!("/session/{session_id}/workspace/folders"),
            json!({ "path": "private" }),
        ),
        (
            format!("/session/{session_id}/workspace/files/batch-transfer"),
            json!({
                "source_paths": ["a.txt"],
                "destination_directory": "private",
                "operation": "move"
            }),
        ),
        (
            format!("/session/{session_id}/workspace/trash"),
            json!({ "paths": ["a.txt"] }),
        ),
        (
            format!("/session/{session_id}/workspace/trash/restore"),
            json!({ "ids": ["abc123"] }),
        ),
        (
            format!("/session/{session_id}/workspace/trash/delete"),
            json!({ "ids": ["abc123"] }),
        ),
        (
            format!("/session/{session_id}/workspace/trash/empty"),
            json!({}),
        ),
    ] {
        assert_status(
            fixture,
            key,
            "POST",
            &path,
            Some(body),
            axum::http::StatusCode::NOT_FOUND,
        )
        .await;
    }
}

async fn assert_status(
    fixture: &AppFixture,
    key: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
    expected: axum::http::StatusCode,
) {
    let (status, response) =
        request_json_raw_with_key(fixture.app.clone(), method, path, body, key).await;
    assert_eq!(status, expected, "{method} {path}: {response}");
}

#[tokio::test]
async fn mcp_proxy_base_url_setting_round_trip_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping managed agent integration test: TEST_DATABASE_URL is not set");
        return;
    };

    assert_initial_proxy_base_url(&fixture).await;
    assert_saved_proxy_base_url(&fixture).await;
    assert_invalid_proxy_base_url_rejected(&fixture).await;
    assert_cleared_proxy_base_url(&fixture).await;
}

async fn assert_initial_proxy_base_url(fixture: &AppFixture) {
    let initial = request_json(
        fixture.app.clone(),
        "GET",
        "/v1/mcp/settings/proxy-base-url",
        None,
    )
    .await;
    assert_eq!(initial["proxy_base_url"], "http://localhost");
    assert_eq!(initial["source"], "config");
}

async fn assert_saved_proxy_base_url(fixture: &AppFixture) {
    let saved = request_json(
        fixture.app.clone(),
        "PUT",
        "/v1/mcp/settings/proxy-base-url",
        Some(json!({ "proxy_base_url": "https://gateway.example.com/" })),
    )
    .await;
    assert_eq!(saved["proxy_base_url"], "https://gateway.example.com");
    assert_eq!(saved["source"], "database");
    assert_eq!(
        litellm_rust::http::platform_mcps::platform_mcp_url(&fixture.state, "agent_test", None)
            .unwrap(),
        "https://gateway.example.com/mcp/platform/agent_test"
    );
}

async fn assert_invalid_proxy_base_url_rejected(fixture: &AppFixture) {
    let (status, body) = request_json_raw(
        fixture.app.clone(),
        "PUT",
        "/v1/mcp/settings/proxy-base-url",
        Some(json!({ "proxy_base_url": "localhost:4000" })),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert!(body.contains("absolute http(s) URL"));
}

async fn assert_cleared_proxy_base_url(fixture: &AppFixture) {
    let cleared = request_json(
        fixture.app.clone(),
        "PUT",
        "/v1/mcp/settings/proxy-base-url",
        Some(json!({ "proxy_base_url": null })),
    )
    .await;
    assert_eq!(cleared["proxy_base_url"], "http://localhost");
    assert_eq!(cleared["source"], "config");
    assert_eq!(
        litellm_rust::http::platform_mcps::platform_mcp_url(&fixture.state, "agent_test", None)
            .unwrap(),
        "http://localhost/mcp/platform/agent_test"
    );
}

#[tokio::test]
async fn managed_agent_endpoints_round_trip_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping managed agent integration test: TEST_DATABASE_URL is not set");
        return;
    };

    flows::assert_agent_runtime_catalog(&fixture).await;
    let agent_id = flows::create_agent(&fixture).await;
    flows::exercise_agent_lifecycle(&fixture, &agent_id).await;
    flows::exercise_routines(&fixture, &agent_id).await;
    flows::exercise_agent_runtime_update(&fixture, &agent_id).await;
    flows::exercise_memory(&fixture, &agent_id).await;
    flows::exercise_platform_mcps(&fixture, &agent_id).await;
    flows::exercise_rules(&fixture, &agent_id).await;
    flows::exercise_runs(&fixture, &agent_id).await;
    flows::exercise_sessions(&fixture).await;
    flows::exercise_skills(&fixture).await;
    flows::exercise_inbox(&fixture).await;

    request_json(
        fixture.app.clone(),
        "DELETE",
        &format!("/api/agents/{agent_id}"),
        None,
    )
    .await;
}

#[tokio::test]
async fn task_retry_isolation_and_sandbox_cancel_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping managed agent integration test: TEST_DATABASE_URL is not set");
        return;
    };
    let agent_id = flows::create_agent(&fixture).await;
    let task = litellm_rust::db::managed_agents::tasks::repository::create(
        &fixture.pool,
        litellm_rust::db::managed_agents::tasks::schema::NewTask {
            agent_id: &agent_id,
            application_version: 1,
            source: "test",
            source_id: None,
            title: "Retry repository behavior",
            input: json!({"request": "retry me"}),
            created_by: "user-1",
            completion_criteria: vec!["A deliverable exists".to_owned()],
        },
    )
    .await
    .unwrap();
    let first = litellm_rust::db::managed_agents::sessions::repository::create(
        &fixture.pool,
        "claude-code",
        Some(&agent_id),
        "attempt one",
        None,
        Some("user-1"),
        Some(&task.id),
    )
    .await
    .unwrap();
    assert_eq!(first.attempt_number, 1);
    litellm_rust::db::managed_agents::tasks::artifacts::create(
        &fixture.pool,
        litellm_rust::db::managed_agents::tasks::schema::NewArtifact {
            task_id: &task.id,
            session_id: Some(&first.id),
            run_id: None,
            artifact_type: "session_output",
            name: "Attempt one output",
            content: Some(json!({"text": "incomplete"})),
            location: None,
            dedupe_key: Some("attempt-one-output"),
            created_by: "system",
        },
    )
    .await
    .unwrap();
    litellm_rust::db::managed_agents::tasks::acceptance::record(
        &fixture.pool,
        &task.id,
        0,
        None,
        "failed",
        Some("first attempt was incomplete"),
        "user-1",
    )
    .await
    .unwrap();
    litellm_rust::db::managed_agents::tasks::repository::fail(
        &fixture.pool,
        &task.id,
        "first attempt failed",
    )
    .await
    .unwrap();
    let reopened = litellm_rust::db::managed_agents::tasks::repository::prepare_retry(
        &fixture.pool,
        &task.id,
        2,
    )
    .await
    .unwrap();
    assert_eq!(reopened.status, "queued");
    assert_eq!(reopened.current_attempt_number, 2);
    assert_eq!(reopened.completed_at, None);
    assert_eq!(reopened.failure_reason, None);
    let checks = litellm_rust::db::managed_agents::tasks::acceptance::list(&fixture.pool, &task.id)
        .await
        .unwrap();
    assert_eq!(checks[0].verdict, "pending");
    assert_eq!(checks[0].evidence, None);
    let all_checks =
        litellm_rust::db::managed_agents::tasks::acceptance::list_all(&fixture.pool, &task.id)
            .await
            .unwrap();
    assert_eq!(all_checks.len(), 2);
    assert_eq!(all_checks[0].attempt_number, 2);
    assert_eq!(all_checks[0].verdict, "pending");
    assert_eq!(all_checks[1].attempt_number, 1);
    assert_eq!(all_checks[1].verdict, "failed");
    assert_eq!(
        all_checks[1].evidence.as_deref(),
        Some("first attempt was incomplete")
    );
    assert!(
        litellm_rust::db::managed_agents::tasks::artifacts::list_for_attempt(
            &fixture.pool,
            &task.id,
            2,
        )
        .await
        .unwrap()
        .is_empty()
    );
    assert_eq!(
        litellm_rust::db::managed_agents::tasks::artifacts::list_for_attempt(
            &fixture.pool,
            &task.id,
            1,
        )
        .await
        .unwrap()
        .len(),
        1
    );
    let second = litellm_rust::db::managed_agents::sessions::repository::create(
        &fixture.pool,
        "claude-code",
        Some(&agent_id),
        "attempt two",
        None,
        Some("user-1"),
        Some(&task.id),
    )
    .await
    .unwrap();
    assert_eq!(second.attempt_number, 2);
    let attempts = litellm_rust::db::managed_agents::sessions::repository::list_for_task(
        &fixture.pool,
        &task.id,
    )
    .await
    .unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].attempt_number, 2);
    litellm_rust::db::managed_agents::tasks::repository::fail_for_session(
        &fixture.pool,
        &first.id,
        "late failure from attempt one",
    )
    .await
    .unwrap();
    let still_current = litellm_rust::db::managed_agents::tasks::repository::get(
        &fixture.pool,
        &agent_id,
        &task.id,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(still_current.status, "queued");
    assert_eq!(still_current.current_attempt_number, 2);
    litellm_rust::db::managed_agents::tasks::repository::fail_for_session(
        &fixture.pool,
        &second.id,
        "second attempt failed",
    )
    .await
    .unwrap();
    assert!(
        litellm_rust::db::managed_agents::tasks::repository::prepare_retry(
            &fixture.pool,
            &task.id,
            2,
        )
        .await
        .is_err()
    );

    let run_task = litellm_rust::db::managed_agents::tasks::repository::create(
        &fixture.pool,
        litellm_rust::db::managed_agents::tasks::schema::NewTask {
            agent_id: &agent_id,
            application_version: 1,
            source: "test",
            source_id: None,
            title: "Cancel sandbox attempt",
            input: json!({"request": "cancel sandbox"}),
            created_by: "user-1",
            completion_criteria: Vec::new(),
        },
    )
    .await
    .unwrap();
    let run = litellm_rust::db::managed_agents::runs::repository::create(
        &fixture.pool,
        &agent_id,
        None,
        Some(&run_task.id),
        litellm_rust::db::managed_agents::runs::schema::CreateRun {
            session_id: None,
            config_overrides: None,
            prompt: None,
        },
    )
    .await
    .unwrap();
    litellm_rust::db::managed_agents::runs::repository::set_running(
        &fixture.pool,
        &run.id,
        Some("sbx_managed_test"),
    )
    .await
    .unwrap();
    litellm_rust::db::managed_agents::tasks::repository::mark_running_for_run(
        &fixture.pool,
        &run.id,
    )
    .await
    .unwrap();
    let cancelled_run = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/tasks/{}/cancel", run_task.id),
        None,
    )
    .await;
    assert_eq!(cancelled_run["task"]["status"], "cancelled");
    assert_eq!(cancelled_run["run_id"], run.id);
    assert_eq!(cancelled_run["interruption"], "sandbox_terminated");
    litellm_rust::db::managed_agents::runs::repository::complete(&fixture.pool, &run.id)
        .await
        .unwrap();
    let sealed_run =
        litellm_rust::db::managed_agents::runs::repository::get(&fixture.pool, &agent_id, &run.id)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(sealed_run.status, "cancelled");
}

#[tokio::test]
async fn task_cancel_seals_current_attempt_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping managed agent integration test: TEST_DATABASE_URL is not set");
        return;
    };
    let agent_id = flows::create_agent(&fixture).await;
    let task = litellm_rust::db::managed_agents::tasks::repository::create(
        &fixture.pool,
        litellm_rust::db::managed_agents::tasks::schema::NewTask {
            agent_id: &agent_id,
            application_version: 1,
            source: "test",
            source_id: None,
            title: "Cancel current attempt",
            input: json!({"request": "cancel me"}),
            created_by: "user-1",
            completion_criteria: vec!["A deliverable exists".to_owned()],
        },
    )
    .await
    .unwrap();
    let session = litellm_rust::db::managed_agents::sessions::repository::create(
        &fixture.pool,
        "claude-code",
        Some(&agent_id),
        "cancel attempt",
        None,
        Some("user-1"),
        Some(&task.id),
    )
    .await
    .unwrap();
    litellm_rust::db::managed_agents::tasks::repository::mark_running_for_session(
        &fixture.pool,
        &session.id,
    )
    .await
    .unwrap();
    let cancelled = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/tasks/{}/cancel", task.id),
        None,
    )
    .await;
    assert_eq!(cancelled["task"]["status"], "cancelled");
    assert_eq!(cancelled["session_id"], session.id);
    assert_eq!(cancelled["interruption"], "cooperative");
    let cancelled_session =
        litellm_rust::db::managed_agents::sessions::repository::get(&fixture.pool, &session.id)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(cancelled_session.status, "cancelled");
    litellm_rust::db::managed_agents::sessions::repository::set_status(
        &fixture.pool,
        &session.id,
        "idle",
    )
    .await
    .unwrap();
    litellm_rust::db::managed_agents::tasks::artifacts::create(
        &fixture.pool,
        litellm_rust::db::managed_agents::tasks::schema::NewArtifact {
            task_id: &task.id,
            session_id: Some(&session.id),
            run_id: None,
            artifact_type: "session_output",
            name: "Late output",
            content: Some(json!({"text": "too late"})),
            location: None,
            dedupe_key: Some("late-output"),
            created_by: "system",
        },
    )
    .await
    .unwrap();
    litellm_rust::db::managed_agents::tasks::acceptance::record(
        &fixture.pool,
        &task.id,
        0,
        None,
        "passed",
        Some("late acceptance"),
        "user-1",
    )
    .await
    .unwrap();
    litellm_rust::db::managed_agents::tasks::acceptance::reconcile(&fixture.pool, &task.id)
        .await
        .unwrap();
    litellm_rust::db::managed_agents::tasks::repository::fail_for_session(
        &fixture.pool,
        &session.id,
        "late failure",
    )
    .await
    .unwrap();
    let sealed = litellm_rust::db::managed_agents::tasks::repository::get(
        &fixture.pool,
        &agent_id,
        &task.id,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(sealed.status, "cancelled");
    let sealed_session =
        litellm_rust::db::managed_agents::sessions::repository::get(&fixture.pool, &session.id)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(sealed_session.status, "cancelled");
    assert!(
        litellm_rust::db::managed_agents::tasks::repository::prepare_retry(
            &fixture.pool,
            &task.id,
            3,
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn task_timeout_sweep_seals_attempt_and_refreshes_deadline_on_retry_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping managed agent integration test: TEST_DATABASE_URL is not set");
        return;
    };
    let agent_id = flows::create_agent(&fixture).await;
    let task = litellm_rust::db::managed_agents::tasks::repository::create(
        &fixture.pool,
        litellm_rust::db::managed_agents::tasks::schema::NewTask {
            agent_id: &agent_id,
            application_version: 1,
            source: "test",
            source_id: None,
            title: "Timeout current attempt",
            input: json!({"request": "take too long"}),
            created_by: "user-1",
            completion_criteria: vec!["A deliverable exists".to_owned()],
        },
    )
    .await
    .unwrap();
    assert!(task
        .deadline_at
        .is_some_and(|deadline| deadline > task.created_at));
    let session = litellm_rust::db::managed_agents::sessions::repository::create(
        &fixture.pool,
        "claude-code",
        Some(&agent_id),
        "timeout attempt",
        None,
        Some("user-1"),
        Some(&task.id),
    )
    .await
    .unwrap();
    litellm_rust::db::managed_agents::tasks::repository::mark_running_for_session(
        &fixture.pool,
        &session.id,
    )
    .await
    .unwrap();
    let now = litellm_rust::db::managed_agents::now_ms();
    sqlx::query(r#"UPDATE "LiteLLM_ManagedAgentTasksTable" SET deadline_at = $2 WHERE id = $1"#)
        .bind(&task.id)
        .bind(now - 1)
        .execute(&fixture.pool)
        .await
        .unwrap();
    assert_eq!(
        litellm_rust::http::managed_agents::tasks::timeout::run_due_once(
            fixture.state.clone(),
            now,
        )
        .await
        .unwrap(),
        1
    );
    let timed_out = litellm_rust::db::managed_agents::tasks::repository::get(
        &fixture.pool,
        &agent_id,
        &task.id,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(timed_out.status, "failed");
    assert_eq!(timed_out.failure_code.as_deref(), Some("timeout"));
    assert_eq!(timed_out.completed_at, Some(now));
    let timed_out_session =
        litellm_rust::db::managed_agents::sessions::repository::get(&fixture.pool, &session.id)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(timed_out_session.status, "timed_out");
    litellm_rust::db::managed_agents::sessions::repository::set_status(
        &fixture.pool,
        &session.id,
        "idle",
    )
    .await
    .unwrap();
    let still_timed_out =
        litellm_rust::db::managed_agents::sessions::repository::get(&fixture.pool, &session.id)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(still_timed_out.status, "timed_out");
    let retried = litellm_rust::db::managed_agents::tasks::repository::prepare_retry(
        &fixture.pool,
        &task.id,
        3,
    )
    .await
    .unwrap();
    assert_eq!(retried.current_attempt_number, 2);
    assert_eq!(retried.failure_code, None);
    assert!(retried.deadline_at.is_some_and(|deadline| deadline > now));
    let retry_session = litellm_rust::db::managed_agents::sessions::repository::create(
        &fixture.pool,
        "claude-code",
        Some(&agent_id),
        "retry verifying",
        None,
        Some("user-1"),
        Some(&task.id),
    )
    .await
    .unwrap();
    litellm_rust::db::managed_agents::tasks::repository::mark_verifying_for_session(
        &fixture.pool,
        &retry_session.id,
    )
    .await
    .unwrap();
    let verifying = litellm_rust::db::managed_agents::tasks::repository::get(
        &fixture.pool,
        &agent_id,
        &task.id,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(verifying.status, "verifying");
    assert_eq!(verifying.deadline_at, None);
    assert_eq!(
        litellm_rust::http::managed_agents::tasks::timeout::run_due_once(
            fixture.state.clone(),
            now,
        )
        .await
        .unwrap(),
        0
    );
}

#[tokio::test]
async fn rejects_invalid_file_base64_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping managed agent integration test: TEST_DATABASE_URL is not set");
        return;
    };

    support::request_raw(
        fixture.app.clone(),
        "POST",
        "/api/agents/import/bundle",
        Some(
            json!({
                "filename": "bad.zip",
                "content_base64": "not base64 !!!"
            })
            .to_string(),
        ),
        "application/json",
        axum::http::StatusCode::BAD_REQUEST,
    )
    .await;
}

#[tokio::test]
async fn runtime_model_discovery_requires_credentials_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping managed agent integration test: TEST_DATABASE_URL is not set");
        return;
    };

    let (status, body) = request_json_raw(
        fixture.app.clone(),
        "GET",
        "/v1/models?runtime=cursor",
        None,
    )
    .await;

    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert!(body.contains("Cursor provider credentials are not configured"));
}

#[tokio::test]
async fn gemini_runtime_models_are_unsupported_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping managed agent integration test: TEST_DATABASE_URL is not set");
        return;
    };

    let (status, body) = request_json_raw(
        fixture.app.clone(),
        "GET",
        "/v1/models?runtime=gemini_antigravity",
        None,
    )
    .await;

    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    assert!(body.contains("model discovery is not supported for runtime: gemini_antigravity"));
}

#[tokio::test]
async fn runtime_agent_create_keeps_legacy_harness_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping managed agent integration test: TEST_DATABASE_URL is not set");
        return;
    };

    let created = create_test_agent(
        &fixture,
        json!({
            "name": "runtime-agent",
            "owner_id": "user-1",
            "runtime": "claude_managed_agents",
            "harness": "claude_managed_agents"
        }),
    )
    .await;
    assert_eq!(created["harness"], "claude-code");
    assert!(created["tools"].is_null());
    assert_eq!(created["config"]["runtime"], "claude_managed_agents");
}

#[tokio::test]
async fn runtime_agent_create_preserves_tool_config_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping managed agent integration test: TEST_DATABASE_URL is not set");
        return;
    };

    assert_explicit_empty_tools_preserved(&fixture).await;
    assert_top_level_tools_override_config_tools(&fixture).await;
    assert_invalid_config_normalized(&fixture).await;
}

async fn assert_explicit_empty_tools_preserved(fixture: &AppFixture) {
    let explicit_empty_tools = create_test_agent(
        fixture,
        json!({
            "name": "empty-tools-agent",
            "owner_id": "user-1",
            "runtime": "claude_managed_agents",
            "tools": []
        }),
    )
    .await;
    assert_eq!(explicit_empty_tools["tools"], json!([]));
    assert_eq!(
        explicit_empty_tools["config"]["runtime"],
        "claude_managed_agents"
    );
    assert_eq!(explicit_empty_tools["config"]["tools"], json!([]));
}

async fn assert_top_level_tools_override_config_tools(fixture: &AppFixture) {
    let overriding_tools = create_test_agent(
        fixture,
        json!({
            "name": "overriding-tools-agent",
            "owner_id": "user-1",
            "runtime": "claude_managed_agents",
            "tools": [],
            "config": { "tools": [{ "type": "bash" }] }
        }),
    )
    .await;
    assert_eq!(overriding_tools["tools"], json!([]));
    assert_eq!(overriding_tools["config"]["tools"], json!([]));
}

async fn assert_invalid_config_normalized(fixture: &AppFixture) {
    let normalized_config = create_test_agent(
        fixture,
        json!({
            "name": "normalized-config-agent",
            "owner_id": "user-1",
            "runtime": "claude_managed_agents",
            "tools": [],
            "config": "invalid"
        }),
    )
    .await;
    assert_eq!(
        normalized_config["config"]["runtime"],
        "claude_managed_agents"
    );
    assert_eq!(normalized_config["config"]["tools"], json!([]));
}

#[tokio::test]
async fn claude_runtime_session_reuses_gateway_mcp_vault_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping managed agent integration test: TEST_DATABASE_URL is not set");
        return;
    };

    flows::exercise_claude_gateway_mcp_vault(&fixture).await;
}

async fn create_test_agent(fixture: &AppFixture, body: Value) -> Value {
    request_json(fixture.app.clone(), "POST", "/api/agents", Some(body)).await
}

#[tokio::test]
async fn tool_permission_decisions_leave_pending_state_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping managed agent integration test: TEST_DATABASE_URL is not set");
        return;
    };

    for (decision, expected_status) in [("accept", "accepted"), ("reject", "rejected")] {
        let item = litellm_rust::db::managed_agents::inbox::repository::create_approval(
            &fixture.pool,
            "tool_permission",
            "工具权限请求：bash".to_owned(),
            None,
            None,
            None,
            Some(json!({ "request_id": format!("request-{decision}") })),
        )
        .await
        .unwrap();

        let response = request_json(
            fixture.app.clone(),
            "POST",
            &format!("/api/approvals/{}/{decision}", item.id),
            Some(json!({})),
        )
        .await;
        assert_eq!(response["live"], true);

        let decided =
            litellm_rust::db::managed_agents::inbox::repository::get(&fixture.pool, &item.id)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(decided.status, expected_status);
    }

    let pending = litellm_rust::db::managed_agents::inbox::repository::pending_approvals(
        &fixture.pool,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(pending.is_empty());
}

#[tokio::test]
async fn persisted_runtime_permission_flow_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping managed agent integration test: TEST_DATABASE_URL is not set");
        return;
    };

    let agent_body = json!({
        "name": "transient-tester",
        "description": "test agent",
        "model": "claude-sonnet-4-6",
        "mcp_server_ids": []
    });
    let agent = create_test_agent(&fixture, agent_body).await;
    let agent_id = agent["id"].as_str().unwrap();
    let provider_session_id = format!(
        "runtime-permission-test-{}",
        litellm_rust::db::managed_agents::now_ms()
    );

    let session = litellm_rust::db::managed_agents::sessions::repository::create_runtime(
        &fixture.pool,
        litellm_rust::db::managed_agents::sessions::repository::CreateRuntimeSession {
            runtime: "opencode",
            agent_id: Some(agent_id),
            title: "runtime permission test",
            timezone: None,
            runtime_agent_ref_id: None,
            environment: json!({}),
            provider_session_id: Some(&provider_session_id),
            provider_run_id: None,
            owner_id: Some("admin"),
            task_id: None,
        },
    )
    .await
    .unwrap();
    let session_id = session.id;

    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/api/tool-approvals")
        .header("content-type", "application/json")
        .header("authorization", "Bearer sk-local");
    let body = json!({
        "session_id": &provider_session_id,
        "request_id": "req-abc",
        "permission": "Permission.Service.ask",
        "patterns": ["*.txt"],
        "metadata": {}
    });

    use tower::ServiceExt;
    let req_body = axum::body::Body::from(serde_json::to_vec(&body).unwrap());
    let response = fixture
        .app
        .clone()
        .oneshot(req.body(req_body).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let res_body = axum::body::to_bytes(response.into_body(), 10000)
        .await
        .unwrap();
    let res_json: serde_json::Value = serde_json::from_slice(&res_body).unwrap();
    let approval_id = res_json["id"].as_str().unwrap();

    let list_response = request_json(
        fixture.app.clone(),
        "GET",
        &format!("/api/approvals?session_id={session_id}"),
        None,
    )
    .await;
    let approvals = list_response["approvals"].as_array().unwrap();
    let pending = approvals
        .iter()
        .find(|approval| approval["id"] == approval_id)
        .unwrap();
    assert_eq!(pending["kind"], "runtime_permission");
    assert_eq!(pending["enforcement_owner"], "runtime");
    assert_eq!(pending["effect_handler"], "runtime_permission");
    assert!(pending["expires_at"].as_i64().is_some());

    let accept_response = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/approvals/{approval_id}/accept"),
        Some(json!({ "scope": "once" })),
    )
    .await;
    assert_eq!(accept_response["live"], true);
    assert_eq!(accept_response["delivery_status"], "delivery_failed");

    let list_response_after = request_json(
        fixture.app.clone(),
        "GET",
        &format!("/api/approvals?session_id={session_id}"),
        None,
    )
    .await;
    let approvals_after = list_response_after["approvals"].as_array().unwrap();
    assert!(!approvals_after.iter().any(|appr| appr["id"] == approval_id));
}

#[tokio::test]
async fn egress_whitelist_and_unlisted_egress_flow_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping managed agent integration test: TEST_DATABASE_URL is not set");
        return;
    };

    litellm_rust::db::managed_agents::settings::repository::set_outbound_domain_whitelist(
        &fixture.pool,
        Some("google.com, *.github.com"),
        "test-actor",
    )
    .await
    .unwrap();

    let agent_body = json!({
        "name": "egress-tester",
        "description": "test agent",
        "model": "claude-sonnet-4-6",
        "mcp_server_ids": []
    });
    let agent = create_test_agent(&fixture, agent_body).await;
    let agent_id = agent["id"].as_str().unwrap();
    let provider_session_id = format!(
        "egress-approval-test-{}",
        litellm_rust::db::managed_agents::now_ms()
    );

    let session = litellm_rust::db::managed_agents::sessions::repository::create_runtime(
        &fixture.pool,
        litellm_rust::db::managed_agents::sessions::repository::CreateRuntimeSession {
            runtime: "opencode",
            agent_id: Some(agent_id),
            title: "egress approval test",
            timezone: None,
            runtime_agent_ref_id: None,
            environment: json!({}),
            provider_session_id: Some(&provider_session_id),
            provider_run_id: None,
            owner_id: Some("admin"),
            task_id: None,
        },
    )
    .await
    .unwrap();
    let session_id = session.id;

    let req1 = axum::http::Request::builder()
        .method("POST")
        .uri("/api/tool-approvals")
        .header("content-type", "application/json")
        .header("authorization", "Bearer sk-local");
    let body1 = json!({
        "session_id": &provider_session_id,
        "request_id": "req-1",
        "permission": "web_request",
        "patterns": ["https://google.com/search"],
        "metadata": {}
    });
    use tower::ServiceExt;
    let req_body1 = axum::body::Body::from(serde_json::to_vec(&body1).unwrap());
    let response1 = fixture
        .app
        .clone()
        .oneshot(req1.body(req_body1).unwrap())
        .await
        .unwrap();
    assert_eq!(response1.status(), axum::http::StatusCode::OK);

    let list_response = request_json(
        fixture.app.clone(),
        "GET",
        &format!("/api/approvals?session_id={session_id}"),
        None,
    )
    .await;
    let approvals = list_response["approvals"].as_array().unwrap();
    assert!(approvals.is_empty());

    let req2 = axum::http::Request::builder()
        .method("POST")
        .uri("/api/tool-approvals")
        .header("content-type", "application/json")
        .header("authorization", "Bearer sk-local");
    let body2 = json!({
        "session_id": &provider_session_id,
        "request_id": "req-2",
        "permission": "web_request",
        "patterns": ["https://malicious.com/steal"],
        "metadata": {}
    });
    let req_body2 = axum::body::Body::from(serde_json::to_vec(&body2).unwrap());
    let response2 = fixture
        .app
        .clone()
        .oneshot(req2.body(req_body2).unwrap())
        .await
        .unwrap();
    assert_eq!(response2.status(), axum::http::StatusCode::OK);

    let res_body2 = axum::body::to_bytes(response2.into_body(), 10000)
        .await
        .unwrap();
    let res_json2: serde_json::Value = serde_json::from_slice(&res_body2).unwrap();
    let approval_id = res_json2["id"].as_str().unwrap();

    let list_response2 = request_json(
        fixture.app.clone(),
        "GET",
        &format!("/api/approvals?session_id={session_id}"),
        None,
    )
    .await;
    let approvals2 = list_response2["approvals"].as_array().unwrap();
    assert_eq!(approvals2.len(), 1);
    assert_eq!(approvals2[0]["id"], approval_id);
    assert_eq!(approvals2[0]["kind"], "data_egress");
    assert_eq!(approvals2[0]["required_role"], "approver");

    let egress_item =
        litellm_rust::db::managed_agents::inbox::repository::get(&fixture.pool, approval_id)
            .await
            .unwrap()
            .unwrap();
    let non_admin = litellm_rust::proxy::auth::master_key::AuthContext::user("admin");
    assert!(
        !litellm_rust::http::managed_agents::inbox::approvals::can_decide(
            &fixture.pool,
            &non_admin,
            &egress_item,
        )
        .await
        .unwrap()
    );

    let accept_response = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/approvals/{approval_id}/accept"),
        Some(json!({ "scope": "once" })),
    )
    .await;
    assert_eq!(accept_response["live"], true);
    assert_eq!(accept_response["delivery_status"], "delivery_failed");

    let list_response3 = request_json(
        fixture.app.clone(),
        "GET",
        &format!("/api/approvals?session_id={session_id}"),
        None,
    )
    .await;
    let approvals3 = list_response3["approvals"].as_array().unwrap();
    assert!(approvals3.is_empty());
}

#[tokio::test]
async fn approval_timeout_persists_denial_delivery_and_escalation_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping managed agent integration test: TEST_DATABASE_URL is not set");
        return;
    };

    let agent = create_test_agent(
        &fixture,
        json!({
            "name": "approval-timeout-tester",
            "description": "test agent",
            "model": "claude-sonnet-4-6",
            "mcp_server_ids": []
        }),
    )
    .await;
    let agent_id = agent["id"].as_str().unwrap();
    let session = litellm_rust::db::managed_agents::sessions::repository::create_runtime(
        &fixture.pool,
        litellm_rust::db::managed_agents::sessions::repository::CreateRuntimeSession {
            runtime: "opencode",
            agent_id: Some(agent_id),
            title: "approval timeout test",
            timezone: None,
            runtime_agent_ref_id: None,
            environment: json!({}),
            provider_session_id: Some("timeout-provider-session"),
            provider_run_id: None,
            owner_id: Some("admin"),
            task_id: None,
        },
    )
    .await
    .unwrap();
    let runtime_permission = litellm_rust::db::managed_agents::inbox::repository::create_approval(
        &fixture.pool,
        "runtime_permission",
        "runtime permission timeout".to_owned(),
        Some(session.id),
        Some(agent_id.to_owned()),
        None,
        Some(json!({ "request_id": "timeout-request" })),
    )
    .await
    .unwrap();
    let business = litellm_rust::db::managed_agents::inbox::repository::create_approval(
        &fixture.pool,
        "business_decision",
        "business escalation".to_owned(),
        None,
        Some(agent_id.to_owned()),
        None,
        Some(json!({})),
    )
    .await
    .unwrap();
    let now = litellm_rust::db::managed_agents::now_ms();
    sqlx::query(
        r#"UPDATE "LiteLLM_ManagedAgentInboxItemsTable" SET expires_at = $2 WHERE id = $1"#,
    )
    .bind(&runtime_permission.id)
    .bind(now - 1)
    .execute(&fixture.pool)
    .await
    .unwrap();
    sqlx::query(
        r#"UPDATE "LiteLLM_ManagedAgentInboxItemsTable" SET escalate_at = $2 WHERE id = $1"#,
    )
    .bind(&business.id)
    .bind(now - 1)
    .execute(&fixture.pool)
    .await
    .unwrap();

    let expired = litellm_rust::http::managed_agents::inbox::timeout::run_due_once(
        fixture.state.clone(),
        now,
    )
    .await
    .unwrap();
    assert_eq!(expired, 1);

    let timed_out = litellm_rust::db::managed_agents::inbox::repository::get(
        &fixture.pool,
        &runtime_permission.id,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(timed_out.status, "expired");
    assert_eq!(timed_out.delivery_status, "delivery_failed");
    assert_eq!(timed_out.delivery_attempts, 1);

    let escalated =
        litellm_rust::db::managed_agents::inbox::repository::get(&fixture.pool, &business.id)
            .await
            .unwrap()
            .unwrap();
    assert!(escalated.escalated_at.is_some());
    assert_eq!(escalated.escalation_role.as_deref(), Some("group_admin"));
}

#[tokio::test]
async fn agent_soft_delete_restore_and_cleanup_flow_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping managed agent integration test: TEST_DATABASE_URL is not set");
        return;
    };

    let agent_body = json!({
        "name": "soft-delete-tester",
        "description": "test agent",
        "model": "claude-sonnet-4-6",
        "mcp_server_ids": []
    });
    let agent = create_test_agent(&fixture, agent_body).await;
    let agent_id = agent["id"].as_str().unwrap();

    let list_response = request_json(fixture.app.clone(), "GET", "/api/agents", None).await;
    let agents = list_response["agents"].as_array().unwrap();
    assert!(agents.iter().any(|a| a["id"] == agent_id));

    let delete_response = request_json(
        fixture.app.clone(),
        "DELETE",
        &format!("/api/agents/{agent_id}"),
        None,
    )
    .await;
    assert_eq!(delete_response["ok"], true);

    let list_response_after = request_json(fixture.app.clone(), "GET", "/api/agents", None).await;
    let agents_after = list_response_after["agents"].as_array().unwrap();
    assert!(!agents_after.iter().any(|a| a["id"] == agent_id));

    let db_agent =
        litellm_rust::db::managed_agents::registry::repository::get(&fixture.pool, agent_id)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(db_agent.status, "archived_pending_delete");
    assert!(db_agent.config.get("deleted_at").is_some());

    let restore_response = request_json(
        fixture.app.clone(),
        "POST",
        &format!("/api/agents/{agent_id}/restore"),
        None,
    )
    .await;
    assert_eq!(restore_response["ok"], true);

    let list_response_restored =
        request_json(fixture.app.clone(), "GET", "/api/agents", None).await;
    let agents_restored = list_response_restored["agents"].as_array().unwrap();
    assert!(agents_restored.iter().any(|a| a["id"] == agent_id));

    let _ = request_json(
        fixture.app.clone(),
        "DELETE",
        &format!("/api/agents/{agent_id}"),
        None,
    )
    .await;

    let eight_days_ago = litellm_rust::db::managed_agents::now_ms() - (8 * 24 * 60 * 60 * 1000);
    sqlx::query(
        r#"
        UPDATE "LiteLLM_ManagedAgentsTable"
        SET config = jsonb_set(config, '{deleted_at}', to_jsonb($2::BIGINT), true)
        WHERE id = $1
        "#,
    )
    .bind(agent_id)
    .bind(eight_days_ago)
    .execute(&fixture.pool)
    .await
    .unwrap();

    litellm_rust::http::managed_agents::registry::cleanup::run_cleanup_once(&fixture.state)
        .await
        .unwrap();

    let deleted_agent =
        litellm_rust::db::managed_agents::registry::repository::get(&fixture.pool, agent_id)
            .await
            .unwrap();
    assert!(deleted_agent.is_none());
}

#[tokio::test]
async fn dify_agent_governance_pipeline_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping Dify governance integration test: TEST_DATABASE_URL is not set");
        return;
    };
    flows::exercise_dify_governance(&fixture).await;
}

#[tokio::test]
async fn a2a_agent_governance_pipeline_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping A2A governance integration test: TEST_DATABASE_URL is not set");
        return;
    };
    flows::exercise_a2a_governance(&fixture).await;
}
