use super::{request_json, AppFixture};

pub async fn assert_agent_runtime_catalog(fixture: &AppFixture) {
    let response = request_json(fixture.app.clone(), "GET", "/api/agent-runtimes", None).await;
    let runtimes = response["runtimes"].as_array().unwrap();
    let ids: Vec<_> = runtimes
        .iter()
        .map(|runtime| runtime["id"].as_str().unwrap())
        .collect();
    // OAP hides direct closed-vendor runtime connections (claude_managed_agents,
    // cursor, gemini_antigravity — see src/site_config.rs) from this listing.
    // They remain fully resolvable internally (open harnesses like opencode
    // depend on the claude_managed_agents protocol adapter staying intact),
    // just not offered as a UI connection option.
    assert_eq!(ids, vec!["elastic_agent_builder"]);
    assert_eq!(
        runtime(runtimes, "elastic_agent_builder")["credential_provider_id"],
        "elastic"
    );
}

fn runtime<'a>(runtimes: &'a [serde_json::Value], id: &str) -> &'a serde_json::Value {
    runtimes.iter().find(|runtime| runtime["id"] == id).unwrap()
}
