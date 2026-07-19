use std::{collections::HashMap, sync::Arc};

use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use futures_util::StreamExt;
use litellm_rust::{
    agents::config::E2bSandboxParams,
    db::managed_agents::pool as managed_agents_pool,
    http::routes::router,
    proxy::{
        config::{GatewayConfig, GeneralSettings},
        state::AppState,
    },
    sdk::{providers::ProviderRegistry, routing::Router as ModelRouter},
};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use wiremock::{
    matchers::{header as header_matcher, method, path},
    Mock, MockServer, ResponseTemplate,
};

mod db;
#[allow(dead_code, unused_imports)]
pub mod flows;

use db::reset_tables;

/// Starts a wiremock server bound to this host's outbound-routable address
/// instead of loopback. The agent-import SSRF guard
/// (`validate_connector_endpoint`) rejects 127.0.0.1/localhost by design —
/// real import-provider flows (discover/import/sync against Dify, A2A, etc.)
/// must exercise that same guard, so tests that need a live, reachable mock
/// endpoint bind here rather than to `MockServer::start()`'s default loopback
/// listener. The address is discovered via a UDP "connect" (no packets are
/// actually sent — it's a local routing-table lookup), the same trick used to
/// find a host's own LAN IP without hardcoding one.
pub async fn start_reachable_mock_server() -> MockServer {
    let probe = std::net::UdpSocket::bind("0.0.0.0:0").expect("bind probe socket");
    probe.connect("8.8.8.8:80").expect("resolve local route");
    let local_ip = probe.local_addr().expect("read local addr").ip();
    let listener =
        std::net::TcpListener::bind((local_ip, 0)).expect("bind mock server to routable address");
    MockServer::builder().listener(listener).start().await
}

pub struct AppFixture {
    pub app: axum::Router,
    pub state: Arc<AppState>,
    pub(crate) pool: PgPool,
    _e2b: MockServer,
    _litellm: Option<MockServer>,
}

impl AppFixture {
    pub async fn new() -> Option<Self> {
        let database_url = isolated_database_url().await?;
        let pool = managed_agents_pool::connect(&database_url).await.unwrap();
        managed_agents_pool::migrate(&pool).await.unwrap();
        reset_tables(&pool).await;
        let e2b = mock_e2b().await;
        let state = build_state(pool.clone(), e2b.uri(), None);
        Some(Self {
            app: router(state.clone()),
            state,
            pool,
            _e2b: e2b,
            _litellm: None,
        })
    }

    pub async fn new_with_litellm_key_info() -> Option<Self> {
        let database_url = isolated_database_url().await?;
        let pool = managed_agents_pool::connect(&database_url).await.unwrap();
        managed_agents_pool::migrate(&pool).await.unwrap();
        reset_tables(&pool).await;
        let e2b = mock_e2b().await;
        let litellm = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/key/info"))
            .and(header_matcher("authorization", "Bearer external-test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "info": { "user_id": "external-user" }
            })))
            .mount(&litellm)
            .await;
        let state = build_state(pool.clone(), e2b.uri(), Some(litellm.uri()));
        Some(Self {
            app: router(state.clone()),
            state,
            pool,
            _e2b: e2b,
            _litellm: Some(litellm),
        })
    }
}

fn test_database_url() -> Option<String> {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())?;
    let database_name = database_name(&database_url);
    assert!(
        database_name.ends_with("_test"),
        "TEST_DATABASE_URL must reference a database whose name ends with _test"
    );
    Some(database_url)
}

fn database_name(url: &str) -> &str {
    url.split('?')
        .next()
        .and_then(|url| url.rsplit('/').next())
        .unwrap_or_default()
}

/// Resolves the database this test binary should use. `cargo test` runs test
/// binaries in parallel against one `TEST_DATABASE_URL`, and `DB_TEST_LOCK` +
/// `reset_tables` only serialize/clean *within* a binary — so a `reset_tables`
/// in one binary can truncate another binary's tables mid-test. To isolate,
/// each binary provisions its own fresh database (named from the binary's
/// identity) once per process. If the test role can't `CREATE DATABASE` (e.g.
/// a locked-down local setup), this falls back to the shared base database —
/// no worse than before.
async fn isolated_database_url() -> Option<String> {
    let base = test_database_url()?;
    static PER_BINARY: tokio::sync::OnceCell<Option<String>> = tokio::sync::OnceCell::const_new();
    let resolved = PER_BINARY
        .get_or_init(|| async { provision_per_binary_db(&base).await })
        .await;
    Some(resolved.clone().unwrap_or(base))
}

async fn provision_per_binary_db(base: &str) -> Option<String> {
    use sqlx::Connection;
    let base_db = database_name(base);
    let name = per_binary_db_name(base_db);
    if name == base_db {
        return None;
    }
    let mut conn = sqlx::PgConnection::connect(base).await.ok()?;
    // Best-effort drop so re-runs start clean; ignore failure (may not exist).
    let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"))
        .execute(&mut conn)
        .await;
    let created = sqlx::query(&format!("CREATE DATABASE \"{name}\""))
        .execute(&mut conn)
        .await
        .is_ok();
    let _ = conn.close().await;
    created.then(|| replace_database_name(base, base_db, &name))
}

fn per_binary_db_name(base_db: &str) -> String {
    use std::hash::{Hash, Hasher};
    let exe = std::env::current_exe().ok();
    let stem = exe
        .as_ref()
        .and_then(|path| path.file_stem())
        .and_then(|stem| stem.to_str())
        .unwrap_or("bin");
    // Strip cargo's per-build hash suffix so the name is stable across builds.
    let target = stem.rsplit_once('-').map_or(stem, |(name, _)| name);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    target.hash(&mut hasher);
    format!("{base_db}_{:08x}", hasher.finish() as u32)
}

fn replace_database_name(base: &str, base_db: &str, new_db: &str) -> String {
    let (head, query) = match base.split_once('?') {
        Some((head, query)) => (head, Some(query)),
        None => (base, None),
    };
    let prefix = head.rsplit_once('/').map_or(head, |(prefix, _)| prefix);
    debug_assert_eq!(database_name(head), base_db);
    match query {
        Some(query) => format!("{prefix}/{new_db}?{query}"),
        None => format!("{prefix}/{new_db}"),
    }
}

fn build_state(
    pool: PgPool,
    e2b_api_base: String,
    litellm_base_url: Option<String>,
) -> Arc<AppState> {
    let config = GatewayConfig {
        model_list: Vec::new(),
        mcp_servers: Default::default(),
        general_settings: GeneralSettings {
            master_key: Some("sk-local".to_owned()),
            public_base_url: Some("http://localhost".to_owned()),
            database_url: Some("postgres://test".to_owned()),
            litellm_base_url,
            sandbox_choice: Some("e2b".to_owned()),
            e2b_sandbox_params: E2bSandboxParams {
                e2b_api_key: Some("e2b-test".to_owned()),
                e2b_template: "litellm-4gb".to_owned(),
                timeout_seconds: 1800,
                workspace_dir: "/home/user/workspace".to_owned(),
                e2b_api_base,
                envs: Default::default(),
            },
            ..Default::default()
        },
        agents: Vec::new(),
    };
    let http = AppState::build_http_client().unwrap();
    Arc::new(AppState::new(config, empty_router(), http, HashMap::new(), Some(pool)).unwrap())
}

fn empty_router() -> ModelRouter {
    ModelRouter::from_config(
        &GatewayConfig {
            model_list: Vec::new(),
            mcp_servers: Default::default(),
            general_settings: GeneralSettings::default(),
            agents: Vec::new(),
        },
        &ProviderRegistry::new(),
    )
    .unwrap()
}

pub async fn request_json(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> Value {
    let response = request(
        app,
        method,
        uri,
        body.map(|value| value.to_string()),
        "application/json",
    )
    .await;
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(
        status.is_success(),
        "{} {} returned {}: {}",
        method,
        uri,
        status,
        String::from_utf8_lossy(&body)
    );
    serde_json::from_slice(&body).unwrap_or_else(|_| json!({}))
}

pub async fn request_json_raw(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, String) {
    let response = request(
        app,
        method,
        uri,
        body.map(|value| value.to_string()),
        "application/json",
    )
    .await;
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&body).to_string())
}

pub async fn request_json_raw_with_key(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    key: &str,
) -> (StatusCode, String) {
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    body.map(|value| value.to_string()).unwrap_or_default(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&body).to_string())
}

pub async fn read_events_until_completed(
    app: axum::Router,
    event_url: &str,
    session_id: &str,
) -> String {
    let response = request(
        app,
        "GET",
        &format!("{event_url}?key=sk-local"),
        None,
        "application/json",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = response.into_body().into_data_stream();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let mut body = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.unwrap();
            body.push_str(std::str::from_utf8(&chunk).unwrap());
            if body
                .lines()
                .any(|line| line.contains(session_id) && line.contains("\"type\":\"session.idle\""))
            {
                break;
            }
        }
        body
    })
    .await
    .unwrap()
}

pub async fn request_raw(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Option<String>,
    content_type: &str,
    expected: StatusCode,
) -> String {
    let response = request(app, method, uri, body, content_type).await;
    assert_eq!(response.status(), expected);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    String::from_utf8(body.to_vec()).unwrap()
}

#[allow(dead_code)]
pub async fn request_with_headers(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: String,
    content_type: &str,
    headers: &[(&str, String)],
    expected: StatusCode,
) -> String {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, "Bearer sk-local")
        .header(header::CONTENT_TYPE, content_type);
    for (name, value) in headers {
        builder = builder.header(*name, value);
    }
    let response = app
        .oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), expected);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    String::from_utf8(body.to_vec()).unwrap()
}

async fn mock_e2b() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sandboxes"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "templateID": "litellm-4gb",
            "sandboxID": "sbx_managed_test",
            "clientID": "client_test",
            "envdVersion": "test",
            "alias": "base",
            "envdAccessToken": "envd-test",
            "trafficAccessToken": "traffic-test",
            "domain": server.uri()
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/process.Process/Start"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(connect_json_frames(&[
            br#"{"event":{"start":{"pid":1470}}}"#,
            br#"{"stdout":"eyJ0eXBlIjoic3RyZWFtX2V2ZW50Iiwic2Vzc2lvbl9pZCI6InNieF9tYW5hZ2VkX3Rlc3QiLCJldmVudCI6eyJ0eXBlIjoiY29udGVudF9ibG9ja19kZWx0YSIsImluZGV4IjowLCJkZWx0YSI6eyJ0eXBlIjoidGV4dF9kZWx0YSIsInRleHQiOiJoZWxsbyAifX19Cg=="}"#,
            br#"{"stdout":"eyJ0eXBlIjoic3RyZWFtX2V2ZW50Iiwic2Vzc2lvbl9pZCI6InNieF9tYW5hZ2VkX3Rlc3QiLCJldmVudCI6eyJ0eXBlIjoiY29udGVudF9ibG9ja19kZWx0YSIsImluZGV4IjowLCJkZWx0YSI6eyJ0eXBlIjoidGV4dF9kZWx0YSIsInRleHQiOiJmcm9tIG1hbmFnZWQgYWdlbnRcbiJ9fX0K"}"#,
            br#"{"stdout":"eyJ0eXBlIjoicmVzdWx0Iiwic3VidHlwZSI6InN1Y2Nlc3MiLCJzZXNzaW9uX2lkIjoic2J4X21hbmFnZWRfdGVzdCIsImR1cmF0aW9uX21zIjoxLCJkdXJhdGlvbl9hcGlfbXMiOjEsImlzX2Vycm9yIjpmYWxzZSwibnVtX3R1cm5zIjoxLCJ0b3RhbF9jb3N0X3VzZCI6MCwidXNhZ2UiOnt9LCJyZXN1bHQiOiJoZWxsbyBmcm9tIG1hbmFnZWQgYWdlbnRcbiJ9Cg=="}"#,
            br#"{"event":{"end":{"exited":true,"status":"exit status 0"}}}"#,
        ])))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/sandboxes/sbx_managed_test"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    server
}

fn connect_json_frames(payloads: &[&[u8]]) -> Vec<u8> {
    let mut frames = Vec::new();
    for payload in payloads {
        frames.push(0);
        frames.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frames.extend_from_slice(payload);
    }
    frames
}

async fn request(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Option<String>,
    content_type: &str,
) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, "Bearer sk-local")
            .header(header::CONTENT_TYPE, content_type)
            .body(Body::from(body.unwrap_or_default()))
            .unwrap(),
    )
    .await
    .unwrap()
}
