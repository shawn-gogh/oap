use litellm_rust::sdk::providers::{
    crewai_import_agents::CREWAI_IMPORT_AGENTS, import_agents::ImportAgentsProvider,
    langgraph_import_agents::LANGGRAPH_IMPORT_AGENTS,
    openai_assistants_import_agents::OPENAI_ASSISTANTS_IMPORT_AGENTS,
};
use serde_json::json;
use wiremock::{
    matchers::{body_json, header, method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn langgraph_discovery_uses_assistant_search_contract() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/assistants/search"))
        .and(header("authorization", "Bearer langgraph-key"))
        .and(header("x-api-key", "langgraph-key"))
        .and(body_json(json!({"limit": 1000, "offset": 0})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "assistant_id": "assistant-1",
            "name": "Research",
            "graph_id": "research",
            "config": {"configurable": {"model": "openai/gpt-4.1"}}
        }])))
        .mount(&server)
        .await;

    let agents = LANGGRAPH_IMPORT_AGENTS
        .discover(&reqwest::Client::new(), &server.uri(), "langgraph-key")
        .await
        .unwrap();

    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].id, "assistant-1");
    assert_eq!(agents[0].model.as_deref(), Some("openai/gpt-4.1"));
}

#[tokio::test]
async fn crewai_discovery_reads_deployment_inputs() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/inputs"))
        .and(header("authorization", "Bearer crew-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "crew_id": "crew-1",
            "name": "Research Crew",
            "inputs": [{"name": "topic", "required": true}]
        })))
        .mount(&server)
        .await;

    let agents = CREWAI_IMPORT_AGENTS
        .discover(&reqwest::Client::new(), &server.uri(), "crew-key")
        .await
        .unwrap();

    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].id, "crew-1");
    assert_eq!(agents[0].raw["inputs"][0]["name"], "topic");
}

#[tokio::test]
async fn openai_discovery_uses_v2_list_contract() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/assistants"))
        .and(header("authorization", "Bearer openai-key"))
        .and(header("openai-beta", "assistants=v2"))
        .and(query_param("order", "asc"))
        .and(query_param("limit", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "list",
            "data": [{
                "id": "asst_1",
                "name": "Research",
                "model": "gpt-4.1",
                "instructions": "Find primary sources."
            }],
            "has_more": false,
            "last_id": "asst_1"
        })))
        .mount(&server)
        .await;

    let agents = OPENAI_ASSISTANTS_IMPORT_AGENTS
        .discover(&reqwest::Client::new(), &server.uri(), "openai-key")
        .await
        .unwrap();

    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].id, "asst_1");
    assert_eq!(
        OPENAI_ASSISTANTS_IMPORT_AGENTS.system_prompt_from_raw("asst_1", &agents[0].raw),
        "Find primary sources."
    );
}
