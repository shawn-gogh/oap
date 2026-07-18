#[allow(dead_code)]
#[path = "managed_agents_support/mod.rs"]
mod support;

use litellm_rust::db::managed_agents::{
    audit,
    registry::{repository, revisions, schema::CreateManagedAgent},
};
use serde_json::json;
use support::{request_json, AppFixture};

static DB_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn revision_diff_and_agent_audit_timeline_against_postgres() {
    let _guard = DB_TEST_LOCK.lock().await;
    let Some(fixture) = AppFixture::new().await else {
        eprintln!("skipping agent observability test: TEST_DATABASE_URL is not set");
        return;
    };
    let agent = repository::create(
        &fixture.pool,
        CreateManagedAgent {
            name: "reviewed-agent".to_owned(),
            owner_id: "admin".to_owned(),
            description: Some("before".to_owned()),
            runtime: Some("opencode".to_owned()),
            harness: Some("opencode".to_owned()),
            prompt: None,
            tools: Some(json!(["read"])),
            schedule: None,
            vault_keys: None,
            setup_commands: None,
            max_runtime_minutes: None,
            on_failure: None,
            config: None,
            model: Some("model-a".to_owned()),
            system: None,
            skill_ids: None,
            rule_ids: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        revisions::record(&fixture.pool, &agent, Some("admin"))
            .await
            .unwrap(),
        1
    );
    let mut candidate = agent.clone();
    candidate.model = "model-b".to_owned();
    candidate.tools = json!(["read", "bash"]);
    assert_eq!(
        revisions::record(&fixture.pool, &candidate, Some("admin"))
            .await
            .unwrap(),
        2
    );
    audit::record(
        &fixture.pool,
        "admin",
        "agent.governance.publish_requested",
        "agent",
        &agent.id,
        json!({"revision": 2}),
    )
    .await
    .unwrap();

    let diff = request_json(
        fixture.app.clone(),
        "GET",
        &format!("/api/agents/{}/revisions/1/diff/2", agent.id),
        None,
    )
    .await;
    assert_eq!(diff["from_version"], 1);
    assert_eq!(diff["to_version"], 2);
    assert_eq!(diff["highest_risk"], "high");
    assert!(diff["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| finding["field_path"] == "tools" && finding["risk"] == "high"));

    let timeline = request_json(
        fixture.app,
        "GET",
        &format!("/api/agents/{}/audit", agent.id),
        None,
    )
    .await;
    assert_eq!(
        timeline["events"][0]["action"],
        "agent.governance.publish_requested"
    );
    assert_eq!(timeline["events"][0]["metadata"]["revision"], 2);
}
