//! Provider-owned SDK integrations.
//!
//! Each provider folder owns the target endpoints and runtimes it supports.

use std::sync::{Arc, OnceLock};

use crate::{
    managed_agents::adapters::{
        registry::{AdapterRegistryError, AgentAdapterRegistry},
        source::SourceAdapter,
    },
    sdk::{
        agents::AgentRuntime,
        providers::base::{models::ModelEndpoint, runtime::RuntimeAdapterBindings},
    },
};

pub use crate::sdk::providers::base::{
    Provider, ProviderRegistry, ProviderRequest, Transformation,
};

pub mod base;
pub mod import_agents;
// Standalone import provider (not a `providers/<name>/` directory, so build.rs
// does not auto-wire it); opt-in via http/managed_agents/import.rs.
pub mod a2a_import_agents;
pub mod acp_import_agents;
pub mod crewai_import_agents;
pub mod dify_import_agents;
pub mod langgraph_import_agents;
pub mod openai_assistants_import_agents;
pub mod openapi_import_agents;
pub mod opencode_import_agents;

pub(crate) fn agent_adapter_registry() -> Result<AgentAdapterRegistry, AdapterRegistryError> {
    let mut managed_runtimes = RuntimeAdapterBindings::new();
    register_runtime_adapters(&mut managed_runtimes);
    AgentAdapterRegistry::builtin()?
        .with_source_adapters([
            &a2a_import_agents::A2A_IMPORT_AGENTS as &'static dyn SourceAdapter,
            &acp_import_agents::ACP_IMPORT_AGENTS,
            &crewai_import_agents::CREWAI_IMPORT_AGENTS,
            &dify_import_agents::DIFY_IMPORT_AGENTS,
            &langgraph_import_agents::LANGGRAPH_IMPORT_AGENTS,
            &openai_assistants_import_agents::OPENAI_ASSISTANTS_IMPORT_AGENTS,
            &openapi_import_agents::OPENAPI_IMPORT_AGENTS,
            &elastic::import_agents::ELASTIC_IMPORT_AGENTS,
            &opencode_import_agents::OPENCODE_IMPORT_AGENTS,
        ])?
        .with_managed_runtime_adapters(managed_runtimes.into_entries())
}

pub(crate) fn default_agent_adapter_registry() -> Result<Arc<AgentAdapterRegistry>, String> {
    static REGISTRY: OnceLock<Result<Arc<AgentAdapterRegistry>, String>> = OnceLock::new();
    REGISTRY
        .get_or_init(|| {
            agent_adapter_registry()
                .map(Arc::new)
                .map_err(|error| error.to_string())
        })
        .clone()
}

pub(crate) fn model_endpoint(runtime: AgentRuntime) -> Option<Arc<dyn ModelEndpoint>> {
    model_registry().get(runtime)
}

pub(crate) fn model_registry() -> base::models::ModelEndpointRegistry {
    let mut registry = base::models::ModelEndpointRegistry::new();
    register_model_endpoints(&mut registry);
    registry
}

pub mod model {
    pub use crate::sdk::providers::base::{
        Provider, ProviderRegistry, ProviderRequest, Transformation,
    };
}

pub mod transform {
    pub use crate::sdk::providers::base::{
        Provider, ProviderRegistry, ProviderRequest, Transformation,
    };
}

include!(concat!(env!("OUT_DIR"), "/providers_generated.rs"));

#[cfg(test)]
mod tests {
    use crate::{
        managed_agents::adapters::{
            registry::{AdapterRegistryError, AgentAdapterRegistry},
            source::SourceAdapter,
        },
        sdk::{agents::AgentRuntime, providers::base::runtime::RuntimeAdapterBindings},
    };

    use super::agent_adapter_registry;

    #[test]
    fn source_registry_binds_every_builtin_source_adapter() {
        let registry = agent_adapter_registry().unwrap();
        let adapter_ids = registry
            .source_adapters()
            .into_iter()
            .map(SourceAdapter::id)
            .collect::<Vec<_>>();

        assert_eq!(
            adapter_ids,
            [
                "a2a",
                "acp",
                "crewai",
                "dify",
                "elastic",
                "langgraph",
                "openai_assistants",
                "openapi",
                "opencode",
            ]
        );
    }

    #[test]
    fn source_registry_rejects_missing_and_duplicate_implementations() {
        let no_adapters: [&'static dyn SourceAdapter; 0] = [];
        assert!(matches!(
            AgentAdapterRegistry::builtin()
                .unwrap()
                .with_source_adapters(no_adapters)
                .unwrap_err(),
            AdapterRegistryError::MissingSourceAdapter { .. }
        ));

        let a2a = &super::a2a_import_agents::A2A_IMPORT_AGENTS as &'static dyn SourceAdapter;
        assert!(matches!(
            AgentAdapterRegistry::builtin()
                .unwrap()
                .with_source_adapters([a2a, a2a])
                .unwrap_err(),
            AdapterRegistryError::DuplicateSourceAdapter { .. }
        ));
    }

    #[test]
    fn managed_runtime_registry_covers_the_static_catalog_by_enum_and_id() {
        let registry = agent_adapter_registry().unwrap();

        for catalog_entry in AgentRuntime::catalog() {
            assert!(
                registry
                    .managed_runtime_adapter(catalog_entry.runtime)
                    .is_some(),
                "{}",
                catalog_entry.id
            );
            let registered = registry
                .managed_runtime_entry(catalog_entry.id)
                .unwrap_or_else(|| panic!("missing runtime {}", catalog_entry.id));
            assert_eq!(registered.runtime, catalog_entry.runtime);
            assert_eq!(registered.id, catalog_entry.id);
        }
    }

    #[test]
    fn managed_runtime_protocol_versions_remain_unverified_until_negotiated() {
        let registry = agent_adapter_registry().unwrap();

        for catalog_entry in AgentRuntime::catalog() {
            let registered = registry
                .managed_runtime_entry(catalog_entry.id)
                .unwrap_or_else(|| panic!("missing runtime {}", catalog_entry.id));
            assert_eq!(
                registered.adapter.protocol_version(),
                "unverified",
                "{}",
                catalog_entry.id
            );
        }
    }

    #[test]
    fn federated_api_specs_are_not_managed_runtime_aliases() {
        let registry = agent_adapter_registry().unwrap();

        for api_spec in [
            "a2a_v1",
            "acp_legacy",
            "dify_app",
            "openapi_rest",
            "langgraph_assistant",
            "crewai_crew",
        ] {
            assert!(
                registry.managed_runtime_entry(api_spec).is_none(),
                "{api_spec}"
            );
        }
    }

    #[test]
    fn managed_runtime_registry_rejects_missing_and_duplicate_implementations() {
        assert!(matches!(
            AgentAdapterRegistry::builtin()
                .unwrap()
                .with_managed_runtime_adapters([])
                .unwrap_err(),
            AdapterRegistryError::MissingManagedRuntimeAdapter { .. }
        ));

        let mut bindings = RuntimeAdapterBindings::new();
        bindings.register(
            AgentRuntime::ClaudeManagedAgents,
            super::anthropic::runtime::RUNTIME_ID,
            super::anthropic::runtime::ClaudeManagedAgentsRuntime,
        );
        bindings.register(
            AgentRuntime::ClaudeManagedAgents,
            super::anthropic::runtime::RUNTIME_ID,
            super::anthropic::runtime::ClaudeManagedAgentsRuntime,
        );
        assert!(matches!(
            AgentAdapterRegistry::builtin()
                .unwrap()
                .with_managed_runtime_adapters(bindings.into_entries())
                .unwrap_err(),
            AdapterRegistryError::DuplicateManagedRuntimeAdapter { .. }
        ));
    }
}
