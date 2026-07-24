use serde_json::json;

use super::*;
use crate::sdk::providers::{
    a2a_import_agents::A2A_IMPORT_AGENTS, elastic::import_agents::ELASTIC_IMPORT_AGENTS,
    opencode_import_agents::OPENCODE_IMPORT_AGENTS,
};

fn source_registry() -> AgentAdapterRegistry {
    crate::sdk::providers::agent_adapter_registry().unwrap()
}

#[test]
fn elastic_agent_config_uses_runtime_api_spec() {
    let agent = ImportAgent {
        external_id: "elastic-ai-agent".to_owned(),
        name: Some("Elastic AI Agent".to_owned()),
        description: None,
        model: None,
        raw: Some(json!({ "id": "elastic-ai-agent" })),
    };

    let config = agent_config(
        &ELASTIC_IMPORT_AGENTS,
        "https://example.elastic-cloud.com",
        &agent,
        &CredentialMode::Shared,
        Some("provider:elastic:agent:elastic-ai-agent".to_owned()),
        "elastic_agent_builder",
    );

    assert_eq!(config["runtime"], "elastic_agent_builder");
    assert_eq!(config["elastic_agent_id"], "elastic-ai-agent");
    assert_eq!(config["source"]["kind"], "external_agent");
    assert_eq!(config["source"]["provider"], "elastic");
    assert_eq!(config["source"]["provider_name"], "Elastic");
    assert_eq!(config["source"]["api_spec"], "elastic_agent_builder");
    assert_eq!(config["runtime_capabilities"]["session_workspace"], false);
}

#[test]
fn opencode_agent_config_declares_session_workspace() {
    let agent = ImportAgent {
        external_id: "reviewer".to_owned(),
        name: Some("Reviewer".to_owned()),
        description: None,
        model: None,
        raw: Some(json!({ "id": "reviewer" })),
    };
    let config = agent_config(
        &OPENCODE_IMPORT_AGENTS,
        "https://example.com",
        &agent,
        &CredentialMode::Byo,
        None,
        "local-opencode",
    );

    assert_eq!(config["runtime_capabilities"]["session_workspace"], true);
    assert_eq!(config["interaction_profile"]["schema_version"], 1);
    assert_eq!(
        config["interaction_profile"]["accepted_input_types"][0],
        "application/json"
    );
}

#[test]
fn imported_agent_config_persists_provider_interaction_contract() {
    let agent = ImportAgent {
        external_id: "remote-a2a".to_owned(),
        name: Some("Remote A2A".to_owned()),
        description: None,
        model: None,
        raw: Some(json!({"name": "Remote A2A"})),
    };

    let config = agent_config(
        &A2A_IMPORT_AGENTS,
        "https://a2a.example.com",
        &agent,
        &CredentialMode::Shared,
        None,
        "a2a",
    );

    assert_eq!(
        config["interaction_profile"]["execution_mode"],
        "async_poll"
    );
    assert_eq!(config["interaction_profile"]["progress_mode"], "status");
    assert_eq!(
        config["interaction_profile"]["input_schema"]["required"][0],
        "message"
    );
}

#[test]
fn elastic_import_provider_resolves_by_api_spec() {
    let registry = source_registry();
    let provider = provider_for_id(&registry, "elastic_agent_builder").unwrap();

    assert_eq!(provider.id(), "elastic");
    assert_eq!(provider.api_spec(), "elastic_agent_builder");
}

#[test]
fn import_provider_lookup_preserves_provider_and_api_spec_aliases() {
    let registry = source_registry();
    let expected = [
        ("a2a", "a2a_v1"),
        ("acp", "acp_legacy"),
        ("crewai", "crewai_crew"),
        ("dify", "dify_app"),
        ("langgraph", "langgraph_assistant"),
        ("openai_assistants", "openai_assistant"),
        ("openapi", "openapi_rest"),
        ("elastic", "elastic_agent_builder"),
        ("opencode", "claude_managed_agents"),
    ];

    for (provider_id, api_spec) in expected {
        let by_provider = provider_for_id(&registry, provider_id).unwrap();
        let by_api_spec = provider_for_id(&registry, api_spec).unwrap();

        assert_eq!(by_provider.id(), provider_id);
        assert_eq!(by_provider.api_spec(), api_spec);
        assert_eq!(by_api_spec.id(), provider_id);
        assert_eq!(by_api_spec.api_spec(), api_spec);
    }
}

#[test]
fn imported_source_keeps_adapter_protocol_and_runtime_identities_separate() {
    let agent = ImportAgent {
        external_id: "remote-a2a".to_owned(),
        name: Some("Remote A2A".to_owned()),
        description: None,
        model: None,
        raw: Some(json!({"name": "Remote A2A"})),
    };

    let config = agent_config(
        &A2A_IMPORT_AGENTS,
        "https://a2a.example.com",
        &agent,
        &CredentialMode::Byo,
        None,
        "a2a_v1",
    );

    assert_eq!(config["runtime"], "a2a_v1");
    assert_eq!(config["source"]["provider"], "a2a");
    assert_eq!(config["source"]["api_spec"], "a2a_v1");
    assert_eq!(A2A_IMPORT_AGENTS.protocol_version(), "unverified");
}

#[test]
fn unknown_import_provider_is_rejected() {
    let registry = source_registry();
    let error = match provider_for_id(&registry, "unknown-provider") {
        Ok(_) => panic!("unknown provider unexpectedly resolved"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("import provider not found"));
}

#[test]
fn import_owner_defaults_to_authenticated_user() {
    let input = ImportAgentsRequest {
        endpoint: "https://example.com".to_owned(),
        api_key: None,
        credential_mode: CredentialMode::Byo,
        owner_id: None,
        agents: vec![],
    };
    let auth = AuthContext {
        user_id: "alice".to_owned(),
        is_admin: false,
        role: "importer".to_owned(),
    };

    assert_eq!(owner_id_for_import(&input, &auth), "alice");
}

#[test]
fn non_admin_import_cannot_claim_owner() {
    let input = ImportAgentsRequest {
        endpoint: "https://example.com".to_owned(),
        api_key: None,
        credential_mode: CredentialMode::Byo,
        owner_id: Some("bob".to_owned()),
        agents: vec![],
    };
    let auth = AuthContext {
        user_id: "alice".to_owned(),
        is_admin: false,
        role: "importer".to_owned(),
    };

    assert_eq!(owner_id_for_import(&input, &auth), "alice");
}

#[test]
fn admin_import_can_claim_owner() {
    let input = ImportAgentsRequest {
        endpoint: "https://example.com".to_owned(),
        api_key: None,
        credential_mode: CredentialMode::Byo,
        owner_id: Some("bob".to_owned()),
        agents: vec![],
    };
    let auth = AuthContext {
        user_id: "admin".to_owned(),
        is_admin: true,
        role: "admin".to_owned(),
    };

    assert_eq!(owner_id_for_import(&input, &auth), "bob");
}

#[test]
fn non_admin_import_cannot_save_shared_credentials() {
    let auth = AuthContext {
        user_id: "alice".to_owned(),
        is_admin: false,
        role: "importer".to_owned(),
    };

    assert!(matches!(
        validate_credential_mode(&CredentialMode::Shared, &auth),
        Err(GatewayError::Unauthorized)
    ));
}

#[test]
fn non_admin_import_can_use_byo_credentials() {
    let auth = AuthContext {
        user_id: "alice".to_owned(),
        is_admin: false,
        role: "importer".to_owned(),
    };

    assert!(validate_credential_mode(&CredentialMode::Byo, &auth).is_ok());
}

#[test]
fn admin_import_can_save_shared_credentials() {
    let auth = AuthContext {
        user_id: "admin".to_owned(),
        is_admin: true,
        role: "admin".to_owned(),
    };

    assert!(validate_credential_mode(&CredentialMode::Shared, &auth).is_ok());
}

#[test]
fn empty_external_id_is_a_blocking_import_issue() {
    let agent = ImportAgent {
        external_id: "   ".to_owned(),
        name: Some("no identity".to_owned()),
        description: None,
        model: None,
        raw: Some(json!({})),
    };

    let issues = import_issues(&ELASTIC_IMPORT_AGENTS, &agent);
    let blocking = blocking_issues(&issues);

    assert_eq!(blocking.len(), 1);
    assert_eq!(blocking[0]["code"], "identity_missing");
}

#[test]
fn a2a_agent_without_runtime_url_is_blocked() {
    let agent = ImportAgent {
        external_id: "remote-agent".to_owned(),
        name: None,
        description: None,
        model: None,
        raw: Some(json!({ "name": "remote-agent" })),
    };

    let issues = import_issues(&A2A_IMPORT_AGENTS, &agent);
    let blocking = blocking_issues(&issues);

    assert_eq!(blocking.len(), 1);
    assert_eq!(blocking[0]["code"], "a2a_runtime_url_missing");
}

#[test]
fn importable_agent_has_no_blocking_issues() {
    let agent = ImportAgent {
        external_id: "remote-agent".to_owned(),
        name: Some("Remote".to_owned()),
        description: None,
        model: None,
        raw: Some(json!({
            "protocolVersion": "0.3",
            "name": "Remote",
            "description": "Remote agent",
            "url": "https://agents.example.com/rpc",
            "preferredTransport": "JSONRPC",
            "version": "1.0.0",
            "capabilities": {},
            "defaultInputModes": ["text/plain"],
            "defaultOutputModes": ["text/plain"],
            "skills": [{
                "id": "remote",
                "name": "Remote",
                "description": "Handles remote work",
                "tags": ["remote"]
            }]
        })),
    };

    let issues = import_issues(&A2A_IMPORT_AGENTS, &agent);

    assert!(blocking_issues(&issues).is_empty());
}

#[test]
fn provider_catalog_exposes_import_capabilities() {
    let registry = source_registry();
    let providers = import_runtime_providers(&registry);
    let opencode = providers
        .iter()
        .find(|provider| provider.id == "opencode")
        .expect("opencode provider");
    let elastic = providers
        .iter()
        .find(|provider| provider.id == "elastic")
        .expect("elastic provider");
    let langgraph = providers
        .iter()
        .find(|provider| provider.id == "langgraph")
        .expect("langgraph provider");
    let crewai = providers
        .iter()
        .find(|provider| provider.id == "crewai")
        .expect("crewai provider");
    let openai = providers
        .iter()
        .find(|provider| provider.id == "openai_assistants")
        .expect("openai assistants provider");

    assert!(opencode.capabilities.discover);
    assert!(opencode.capabilities.remote_import);
    assert!(opencode.capabilities.file_import);
    assert!(opencode.capabilities.bundle_import);
    assert!(opencode.capabilities.continuous_sync);
    assert_eq!(opencode.capabilities.runtime_contract, opencode.api_spec);

    assert!(elastic.capabilities.discover);
    assert!(elastic.capabilities.remote_import);
    assert!(!elastic.capabilities.file_import);
    assert!(!elastic.capabilities.bundle_import);
    assert!(elastic.capabilities.continuous_sync);

    assert!(langgraph.capabilities.incremental_sync);
    assert!(!langgraph.expose_runtime_harness);
    assert!(crewai.capabilities.continuous_sync);
    assert!(!crewai.capabilities.incremental_sync);
    assert!(openai.capabilities.incremental_sync);
    assert!(!openai.expose_runtime_harness);
}
