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
