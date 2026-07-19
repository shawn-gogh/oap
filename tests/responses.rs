use std::{collections::HashMap, sync::Arc};

use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use litellm_rust::{
    db::managed_agents::{id, now_ms, pool as managed_agents_pool, registry, sessions, spend_logs},
    http::routes::router,
    proxy::{
        config::{GatewayConfig, GeneralSettings, LiteLlmParams, ModelEntry},
        provider_credentials::{self, ProviderCredentialInput},
        state::AppState,
    },
    sdk::{
        providers::{self, ProviderRegistry},
        routing::Router as ModelRouter,
    },
};
use serde_json::json;
use sqlx::PgPool;
use tower::util::ServiceExt;
use wiremock::{
    matchers::{header as header_match, method, path},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn responses_uses_db_backed_openai_credentials() {
    let Some(pool) = test_pool().await else {
        eprintln!("skipping responses integration test: TEST_DATABASE_URL is not set");
        return;
    };
    let upstream = MockServer::start().await;
    mock_responses_upstream(&upstream).await;
    let config = test_config(upstream.uri());
    save_openai_credential(&pool, &config, upstream.uri()).await;
    let agent_id = create_agent(&pool).await;
    let (session_id, invocation_id) =
        create_session_invocation(&pool, &agent_id, "attributed").await;
    create_session_invocation(&pool, &agent_id, "unmetered").await;

    let query_pool = pool.clone();
    let app = router(build_state(config, pool));
    let response = app
        .clone()
        .oneshot(responses_request(&session_id))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["output_text"], "ok");
    assert_spend_attribution(&query_pool, &agent_id, &session_id, &invocation_id).await;
    assert_agent_metrics(app, &agent_id).await;
}

async fn mock_responses_upstream(upstream: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .and(header_match("authorization", "Bearer sk-openai-db"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "resp_test",
            "object": "response",
            "status": "completed",
            "model": "gpt-5.5",
            "output": [],
            "output_text": "ok"
        })))
        .mount(upstream)
        .await;
}

async fn save_openai_credential(pool: &PgPool, config: &GatewayConfig, api_base: String) {
    provider_credentials::save(
        pool,
        config,
        "openai",
        ProviderCredentialInput {
            api_key: "sk-openai-db".to_owned(),
            api_base,
        },
    )
    .await
    .unwrap();
}

fn responses_request(session_id: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/v1/responses")
        .header(header::AUTHORIZATION, "Bearer sk-local")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-request-id", "req-attributed")
        .header("x-lap-session-id", session_id)
        .body(Body::from(
            json!({
                "model": "gpt-5.5",
                "input": "Reply with exactly: ok",
                "max_output_tokens": 16
            })
            .to_string(),
        ))
        .unwrap()
}

async fn create_agent(pool: &PgPool) -> String {
    registry::repository::create(
        pool,
        registry::schema::CreateManagedAgent {
            name: "metrics-test".to_owned(),
            owner_id: "admin".to_owned(),
            description: None,
            runtime: Some("opencode".to_owned()),
            harness: Some("opencode".to_owned()),
            prompt: None,
            tools: None,
            schedule: None,
            vault_keys: None,
            setup_commands: None,
            max_runtime_minutes: None,
            on_failure: None,
            config: None,
            model: Some("gpt-5.5".to_owned()),
            system: None,
            skill_ids: None,
            rule_ids: None,
        },
    )
    .await
    .unwrap()
    .id
}

async fn create_session_invocation(
    pool: &PgPool,
    agent_id: &str,
    request_id: &str,
) -> (String, String) {
    let session = sessions::repository::create(
        pool,
        "opencode",
        Some(agent_id),
        request_id,
        None,
        Some("admin"),
        None,
    )
    .await
    .unwrap();
    let turn_id = id("turn");
    let invocation_id = id("inv");
    let now = now_ms();
    sqlx::query(
        r#"
        INSERT INTO "LiteLLM_SessionTurnsTable"
          (id, session_id, request_id, status, created_at, updated_at)
        VALUES ($1, $2, $3, 'queued', $4, $4)
        "#,
    )
    .bind(&turn_id)
    .bind(&session.id)
    .bind(request_id)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO "LiteLLM_SessionInvocationsTable"
          (id, session_id, turn_id, protocol, protocol_version, adapter_id,
           role, status, created_at, updated_at)
        VALUES ($1, $2, $3, 'native', '1', 'opencode', 'primary', 'queued', $4, $4)
        "#,
    )
    .bind(&invocation_id)
    .bind(&session.id)
    .bind(turn_id)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    (session.id, invocation_id)
}

async fn assert_spend_attribution(
    pool: &PgPool,
    agent_id: &str,
    session_id: &str,
    invocation_id: &str,
) {
    let mut log = None;
    for _ in 0..20 {
        log = spend_logs::repository::get(pool, "req-attributed")
            .await
            .unwrap();
        if log.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let log = log.expect("spend log");
    assert_eq!(log.session_id.as_deref(), Some(session_id));
    assert_eq!(log.agent_id.as_deref(), Some(agent_id));
    assert_eq!(log.invocation_id.as_deref(), Some(invocation_id));
    assert_eq!(log.purpose, "production");
}

async fn assert_agent_metrics(app: axum::Router, agent_id: &str) {
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/agents/{agent_id}/metrics?days=7"))
                .header(header::AUTHORIZATION, "Bearer sk-local")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let metrics: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(metrics["days"], 7);
    assert_eq!(metrics["totals"]["model_calls"], 1);
    assert_eq!(metrics["totals"]["invocations"], 2);
    assert_eq!(metrics["coverage"]["gateway_metered"], 1);
    assert_eq!(metrics["coverage"]["provider_reported"], 0);
    assert_eq!(metrics["coverage"]["unmetered"], 1);
}

async fn test_pool() -> Option<PgPool> {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())?;
    let pool = managed_agents_pool::connect(&database_url).await.unwrap();
    managed_agents_pool::migrate(&pool).await.unwrap();
    sqlx::query(
        r#"DELETE FROM "LiteLLM_CredentialsTable" WHERE credential_name = 'provider:openai'"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(r#"DELETE FROM "LiteLLM_SpendLogs" WHERE request_id = 'req-attributed'"#)
        .execute(&pool)
        .await
        .unwrap();
    Some(pool)
}

fn test_config(api_base: String) -> GatewayConfig {
    GatewayConfig {
        model_list: vec![ModelEntry {
            model_name: "gpt-5.5".to_owned(),
            litellm_params: LiteLlmParams {
                model: "openai/gpt-5.5".to_owned(),
                api_key: None,
                api_base: Some(api_base),
                extra: Default::default(),
            },
        }],
        mcp_servers: Default::default(),
        general_settings: GeneralSettings {
            master_key: Some("sk-local".to_owned()),
            database_url: Some("postgres://test".to_owned()),
            spend_logs_batch_size: 1,
            ..Default::default()
        },
        agents: Vec::new(),
    }
}

fn build_state(config: GatewayConfig, pool: PgPool) -> Arc<AppState> {
    let mut providers = ProviderRegistry::new();
    providers::register_all(&mut providers);
    let model_router = ModelRouter::from_config(&config, &providers).unwrap();
    let http = AppState::build_http_client().unwrap();
    Arc::new(AppState::new(config, model_router, http, HashMap::new(), Some(pool)).unwrap())
}
