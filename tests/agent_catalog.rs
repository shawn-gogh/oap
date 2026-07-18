#[path = "managed_agents_support/mod.rs"]
#[allow(dead_code)]
mod support;

use axum::http::StatusCode;
use serde_json::{json, Value};
use support::{request_json, request_json_raw_with_key, AppFixture};

#[tokio::test]
async fn catalog_searches_safe_metadata_and_reports_real_consumers() {
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping agent catalog test: TEST_DATABASE_URL is not set");
        return;
    };
    let consumer_key = create_consumer(&fixture).await;
    let visible_id = create_catalog_agent(
        &fixture,
        "财务分析",
        json!({
            "tags": ["财务", "分析"],
            "capabilities": ["报表生成"],
        }),
        json!(["sql"]),
    )
    .await;
    grant_and_record_use(&fixture, &visible_id).await;
    let unavailable_id = create_catalog_agent(
        &fixture,
        "研发助手",
        json!({"tags": ["研发"]}),
        json!(["read"]),
    )
    .await;
    create_hidden_agents(&fixture).await;

    let response = catalog(&fixture, &consumer_key, "/api/agent-catalog").await;
    assert_catalog_visibility(&response, &visible_id, &unavailable_id);
    let filtered = catalog(
        &fixture,
        &consumer_key,
        "/api/agent-catalog?tag=%E8%B4%A2%E5%8A%A1&capability=SQL",
    )
    .await;
    assert_eq!(filtered["agents"].as_array().unwrap().len(), 1);
    assert_eq!(filtered["agents"][0]["id"], visible_id);
    assert_invalid_tags_are_rejected(&fixture, &visible_id).await;
}

async fn create_consumer(fixture: &AppFixture) -> String {
    litellm_rust::db::managed_agents::users::repository::create(
        &fixture.pool,
        "catalog-user",
        "目录用户",
        None,
    )
    .await
    .unwrap();
    litellm_rust::db::managed_agents::api_keys::repository::create(
        &fixture.pool,
        Some("catalog-key"),
        Some("catalog-user"),
        Some("user"),
    )
    .await
    .unwrap()
    .key
}

async fn create_catalog_agent(
    fixture: &AppFixture,
    name: &str,
    config: Value,
    tools: Value,
) -> String {
    let agent = request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents",
        Some(json!({
            "name": name,
            "owner_id": "catalog-owner",
            "description": format!("{name}的公开说明"),
            "model": "test-model",
            "system": "sensitive system prompt",
            "tools": tools,
            "config": config,
        })),
    )
    .await;
    let id = agent["id"].as_str().unwrap().to_owned();
    litellm_rust::db::managed_agents::registry::repository::set_status(
        &fixture.pool,
        &id,
        "active",
    )
    .await
    .unwrap();
    id
}

async fn grant_and_record_use(fixture: &AppFixture, agent_id: &str) {
    litellm_rust::db::managed_agents::agent_grants::repository::upsert(
        &fixture.pool,
        agent_id,
        "catalog-user",
        "use",
        None,
        "catalog-owner",
    )
    .await
    .unwrap();
    litellm_rust::db::managed_agents::sessions::repository::create(
        &fixture.pool,
        "opencode",
        Some(agent_id),
        "真实消费会话",
        Some("UTC"),
        Some("catalog-user"),
        None,
    )
    .await
    .unwrap();
}

async fn create_hidden_agents(fixture: &AppFixture) {
    let governed = create_catalog_agent(
        fixture,
        "未发布外部智能体",
        json!({"tags": ["隐藏"]}),
        json!([]),
    )
    .await;
    litellm_rust::db::managed_agents::governance::record_import(
        &fixture.pool,
        litellm_rust::db::managed_agents::governance::ImportedSource {
            agent_id: &governed,
            owner_id: "catalog-owner",
            provider: "external-test",
            endpoint: "https://runtime.example.test",
            external_agent_id: "hidden-external",
            source_hash: "source-v1",
            credential_scope: "personal",
            credential_name: None,
        },
    )
    .await
    .unwrap();
    request_json(
        fixture.app.clone(),
        "POST",
        "/api/agents",
        Some(json!({
            "name": "草稿智能体",
            "owner_id": "catalog-owner",
            "model": "test-model",
        })),
    )
    .await;
}

async fn catalog(fixture: &AppFixture, key: &str, path: &str) -> Value {
    let (status, body) =
        request_json_raw_with_key(fixture.app.clone(), "GET", path, None, key).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    serde_json::from_str(&body).unwrap()
}

fn assert_catalog_visibility(catalog: &Value, visible_id: &str, unavailable_id: &str) {
    let agents = catalog["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 2);
    let visible = agents
        .iter()
        .find(|agent| agent["id"] == visible_id)
        .unwrap();
    assert_eq!(visible["can_use"], true);
    assert_eq!(visible["access"], "granted");
    assert_eq!(visible["consumers"][0]["display_name"], "目录用户");
    assert_eq!(visible["session_count"], 1);
    assert_eq!(visible["capabilities"], json!(["sql", "报表生成"]));
    assert!(visible.get("system").is_none());
    assert!(visible.get("config").is_none());
    let unavailable = agents
        .iter()
        .find(|agent| agent["id"] == unavailable_id)
        .unwrap();
    assert_eq!(unavailable["can_use"], false);
}

async fn assert_invalid_tags_are_rejected(fixture: &AppFixture, agent_id: &str) {
    let (status, _) = request_json_raw_with_key(
        fixture.app.clone(),
        "PATCH",
        &format!("/api/agents/{agent_id}"),
        Some(json!({"config": {"tags": "not-an-array"}})),
        "sk-local",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
