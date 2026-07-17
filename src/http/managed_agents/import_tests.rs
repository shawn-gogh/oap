use serde_json::json;

use super::*;

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
}

#[test]
fn elastic_import_provider_resolves_by_api_spec() {
    let provider = provider_for_id("elastic_agent_builder").unwrap();

    assert_eq!(provider.id(), "elastic");
    assert_eq!(provider.api_spec(), "elastic_agent_builder");
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
    };

    assert_eq!(owner_id_for_import(&input, &auth), "bob");
}

#[test]
fn non_admin_import_cannot_save_shared_credentials() {
    let auth = AuthContext {
        user_id: "alice".to_owned(),
        is_admin: false,
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
    };

    assert!(validate_credential_mode(&CredentialMode::Byo, &auth).is_ok());
}

#[test]
fn admin_import_can_save_shared_credentials() {
    let auth = AuthContext {
        user_id: "admin".to_owned(),
        is_admin: true,
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
        raw: Some(json!({ "url": "https://agents.example.com/rpc" })),
    };

    let issues = import_issues(&A2A_IMPORT_AGENTS, &agent);

    assert!(blocking_issues(&issues).is_empty());
}

#[test]
fn provider_catalog_exposes_import_capabilities() {
    let providers = import_runtime_providers();
    let opencode = providers
        .iter()
        .find(|provider| provider.id == "opencode")
        .expect("opencode provider");
    let elastic = providers
        .iter()
        .find(|provider| provider.id == "elastic")
        .expect("elastic provider");

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
}
