use super::descriptor::{AdapterAliases, AdapterDescriptor, AdapterFacets, AdapterId};
use super::invocation::InvocationAdapter;
use super::source::SourceAdapter;
use crate::sdk::{
    agents::AgentRuntime,
    providers::base::runtime::{RuntimeAdapter, RuntimeEntry},
};
use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::Arc,
};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterFacet {
    Source,
    Invocation,
    ManagedRuntime,
}

impl fmt::Display for AdapterFacet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Source => "source",
            Self::Invocation => "invocation",
            Self::ManagedRuntime => "managed_runtime",
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AdapterRegistryError {
    #[error("adapter descriptor field `{field}` must not be empty")]
    EmptyField { field: &'static str },
    #[error("adapter `{adapter_id}` must declare at least one facet")]
    MissingFacet { adapter_id: AdapterId },
    #[error("adapter `{adapter_id}` must declare at least one protocol version")]
    MissingProtocolVersion { adapter_id: AdapterId },
    #[error("adapter `{adapter_id}` declares protocol version `{version}` more than once")]
    DuplicateProtocolVersion {
        adapter_id: AdapterId,
        version: String,
    },
    #[error("adapter `{adapter_id}` declares {facet} aliases without enabling that facet")]
    AliasWithoutFacet {
        adapter_id: AdapterId,
        facet: AdapterFacet,
    },
    #[error("adapter `{adapter_id}` enables the {facet} facet without declaring an alias")]
    FacetWithoutAlias {
        adapter_id: AdapterId,
        facet: AdapterFacet,
    },
    #[error("adapter `{adapter_id}` declares an empty {facet} alias")]
    EmptyAlias {
        adapter_id: AdapterId,
        facet: AdapterFacet,
    },
    #[error("adapter id `{adapter_id}` is registered more than once")]
    DuplicateAdapterId { adapter_id: AdapterId },
    #[error(
        "{facet} alias `{alias}` is registered by both `{existing_adapter_id}` and `{incoming_adapter_id}`"
    )]
    DuplicateAlias {
        facet: AdapterFacet,
        alias: String,
        existing_adapter_id: AdapterId,
        incoming_adapter_id: AdapterId,
    },
    #[error("source adapter `{adapter_id}` has no matching registry descriptor")]
    UnknownSourceAdapter { adapter_id: AdapterId },
    #[error("adapter `{adapter_id}` does not enable the source facet")]
    SourceFacetNotEnabled { adapter_id: AdapterId },
    #[error("source adapter `{adapter_id}` is bound more than once")]
    DuplicateSourceAdapter { adapter_id: AdapterId },
    #[error("source adapter `{adapter_id}` identity alias `{alias}` is not declared")]
    MissingSourceAlias {
        adapter_id: AdapterId,
        alias: String,
    },
    #[error("source adapter `{adapter_id}` protocol version `{version}` is not declared")]
    MissingSourceProtocolVersion {
        adapter_id: AdapterId,
        version: String,
    },
    #[error("adapter `{adapter_id}` enables the source facet but has no implementation")]
    MissingSourceAdapter { adapter_id: AdapterId },
    #[error("invocation adapter `{adapter_id}` has no matching registry descriptor")]
    UnknownInvocationAdapter { adapter_id: AdapterId },
    #[error("adapter `{adapter_id}` does not enable the invocation facet")]
    InvocationFacetNotEnabled { adapter_id: AdapterId },
    #[error("invocation adapter `{adapter_id}` is bound more than once")]
    DuplicateInvocationAdapter { adapter_id: AdapterId },
    #[error("invocation adapter `{adapter_id}` alias `{alias}` is not declared")]
    MissingInvocationAlias {
        adapter_id: AdapterId,
        alias: String,
    },
    #[error("invocation adapter `{adapter_id}` protocol version `{version}` is not declared")]
    MissingInvocationProtocolVersion {
        adapter_id: AdapterId,
        version: String,
    },
    #[error("adapter `{adapter_id}` enables the invocation facet but has no implementation")]
    MissingInvocationAdapter { adapter_id: AdapterId },
    #[error("managed runtime alias `{alias}` has no matching registry descriptor")]
    UnknownManagedRuntime { alias: String },
    #[error("managed runtime adapter `{adapter_id}` is bound more than once")]
    DuplicateManagedRuntimeAdapter { adapter_id: AdapterId },
    #[error("managed runtime adapter `{adapter_id}` protocol version `{version}` is not declared")]
    MissingManagedRuntimeProtocolVersion {
        adapter_id: AdapterId,
        version: String,
    },
    #[error("adapter `{adapter_id}` enables the managed runtime facet but has no implementation")]
    MissingManagedRuntimeAdapter { adapter_id: AdapterId },
}

#[derive(Default)]
pub struct AgentAdapterRegistry {
    descriptors: HashMap<AdapterId, Arc<AdapterDescriptor>>,
    source_aliases: HashMap<String, AdapterId>,
    source_adapters: HashMap<AdapterId, &'static dyn SourceAdapter>,
    invocation_aliases: HashMap<String, AdapterId>,
    invocation_adapters: HashMap<AdapterId, &'static dyn InvocationAdapter>,
    managed_runtime_aliases: HashMap<String, AdapterId>,
    managed_runtime_adapters: HashMap<AdapterId, RuntimeEntry>,
}

impl fmt::Debug for AgentAdapterRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentAdapterRegistry")
            .field("descriptors", &self.descriptors.len())
            .field("source_aliases", &self.source_aliases.len())
            .field("source_adapters", &self.source_adapters.len())
            .field("invocation_aliases", &self.invocation_aliases.len())
            .field("invocation_adapters", &self.invocation_adapters.len())
            .field(
                "managed_runtime_aliases",
                &self.managed_runtime_aliases.len(),
            )
            .field(
                "managed_runtime_adapters",
                &self.managed_runtime_adapters.len(),
            )
            .finish()
    }
}

impl AgentAdapterRegistry {
    pub fn builtin() -> Result<Self, AdapterRegistryError> {
        Self::from_descriptors(builtin_descriptors())
    }

    pub fn from_descriptors(
        descriptors: impl IntoIterator<Item = AdapterDescriptor>,
    ) -> Result<Self, AdapterRegistryError> {
        let mut registry = Self::default();
        for descriptor in descriptors {
            registry.register(descriptor)?;
        }
        Ok(registry)
    }

    pub fn with_source_adapters(
        mut self,
        adapters: impl IntoIterator<Item = &'static dyn SourceAdapter>,
    ) -> Result<Self, AdapterRegistryError> {
        for adapter in adapters {
            self.bind_source_adapter(adapter)?;
        }
        self.validate_source_bindings()?;
        Ok(self)
    }

    pub(crate) fn with_invocation_adapters(
        mut self,
        adapters: impl IntoIterator<Item = &'static dyn InvocationAdapter>,
    ) -> Result<Self, AdapterRegistryError> {
        for adapter in adapters {
            self.bind_invocation_adapter(adapter)?;
        }
        self.validate_invocation_bindings()?;
        Ok(self)
    }

    pub(crate) fn with_managed_runtime_adapters(
        mut self,
        entries: impl IntoIterator<Item = RuntimeEntry>,
    ) -> Result<Self, AdapterRegistryError> {
        for entry in entries {
            self.bind_managed_runtime_adapter(entry)?;
        }
        self.validate_managed_runtime_bindings()?;
        Ok(self)
    }

    pub fn descriptor(&self, adapter_id: &str) -> Option<&AdapterDescriptor> {
        self.descriptors
            .get(&AdapterId::new(adapter_id))
            .map(Arc::as_ref)
    }

    pub fn resolve(&self, facet: AdapterFacet, alias: &str) -> Option<&AdapterDescriptor> {
        let index = match facet {
            AdapterFacet::Source => &self.source_aliases,
            AdapterFacet::Invocation => &self.invocation_aliases,
            AdapterFacet::ManagedRuntime => &self.managed_runtime_aliases,
        };
        index
            .get(alias)
            .and_then(|adapter_id| self.descriptors.get(adapter_id))
            .map(Arc::as_ref)
    }

    pub fn source(&self, alias: &str) -> Option<&AdapterDescriptor> {
        self.resolve(AdapterFacet::Source, alias)
    }

    pub fn source_adapter(&self, alias: &str) -> Option<&'static dyn SourceAdapter> {
        self.source_aliases
            .get(alias)
            .and_then(|adapter_id| self.source_adapters.get(adapter_id))
            .copied()
    }

    pub fn source_adapters(&self) -> Vec<&'static dyn SourceAdapter> {
        let mut entries = self.source_adapters.iter().collect::<Vec<_>>();
        entries.sort_by(|(left, _), (right, _)| left.as_str().cmp(right.as_str()));
        entries.into_iter().map(|(_, adapter)| *adapter).collect()
    }

    pub fn invocation(&self, alias: &str) -> Option<&AdapterDescriptor> {
        self.resolve(AdapterFacet::Invocation, alias)
    }

    pub(crate) fn invocation_adapter(&self, alias: &str) -> Option<&'static dyn InvocationAdapter> {
        self.invocation_aliases
            .get(alias)
            .and_then(|adapter_id| self.invocation_adapters.get(adapter_id))
            .copied()
    }

    pub fn managed_runtime(&self, alias: &str) -> Option<&AdapterDescriptor> {
        self.resolve(AdapterFacet::ManagedRuntime, alias)
    }

    pub(crate) fn managed_runtime_entry(&self, alias: &str) -> Option<&RuntimeEntry> {
        self.managed_runtime_aliases
            .get(alias)
            .and_then(|adapter_id| self.managed_runtime_adapters.get(adapter_id))
    }

    pub(crate) fn managed_runtime_adapter(
        &self,
        runtime: AgentRuntime,
    ) -> Option<Arc<dyn RuntimeAdapter>> {
        self.managed_runtime_adapters
            .values()
            .find(|entry| entry.runtime == runtime)
            .map(|entry| entry.adapter.clone())
    }

    pub fn descriptors(&self) -> Vec<&AdapterDescriptor> {
        let mut descriptors = self
            .descriptors
            .values()
            .map(Arc::as_ref)
            .collect::<Vec<_>>();
        descriptors.sort_by(|left, right| left.adapter_id.as_str().cmp(right.adapter_id.as_str()));
        descriptors
    }

    fn register(&mut self, descriptor: AdapterDescriptor) -> Result<(), AdapterRegistryError> {
        validate_descriptor(&descriptor)?;

        let adapter_id = descriptor.adapter_id.clone();
        if self.descriptors.contains_key(&adapter_id) {
            return Err(AdapterRegistryError::DuplicateAdapterId { adapter_id });
        }

        register_aliases(
            &mut self.source_aliases,
            AdapterFacet::Source,
            &adapter_id,
            &descriptor.aliases.source,
        )?;
        register_aliases(
            &mut self.invocation_aliases,
            AdapterFacet::Invocation,
            &adapter_id,
            &descriptor.aliases.invocation,
        )?;
        register_aliases(
            &mut self.managed_runtime_aliases,
            AdapterFacet::ManagedRuntime,
            &adapter_id,
            &descriptor.aliases.managed_runtime,
        )?;

        self.descriptors.insert(adapter_id, Arc::new(descriptor));
        Ok(())
    }

    fn bind_source_adapter(
        &mut self,
        adapter: &'static dyn SourceAdapter,
    ) -> Result<(), AdapterRegistryError> {
        let adapter_id = AdapterId::new(adapter.id());
        let Some(descriptor) = self.descriptors.get(&adapter_id) else {
            return Err(AdapterRegistryError::UnknownSourceAdapter { adapter_id });
        };
        if !descriptor.facets.source {
            return Err(AdapterRegistryError::SourceFacetNotEnabled { adapter_id });
        }
        for alias in [adapter.id(), adapter.api_spec()] {
            if !descriptor
                .aliases
                .source
                .iter()
                .any(|candidate| candidate == alias)
            {
                return Err(AdapterRegistryError::MissingSourceAlias {
                    adapter_id,
                    alias: alias.to_owned(),
                });
            }
        }
        if !descriptor
            .supported_versions
            .iter()
            .any(|version| version.as_str() == adapter.protocol_version())
        {
            return Err(AdapterRegistryError::MissingSourceProtocolVersion {
                adapter_id,
                version: adapter.protocol_version().to_owned(),
            });
        }
        if self.source_adapters.contains_key(&adapter_id) {
            return Err(AdapterRegistryError::DuplicateSourceAdapter { adapter_id });
        }
        self.source_adapters.insert(adapter_id, adapter);
        Ok(())
    }

    fn validate_source_bindings(&self) -> Result<(), AdapterRegistryError> {
        for descriptor in self.descriptors.values() {
            if descriptor.facets.source
                && !self.source_adapters.contains_key(&descriptor.adapter_id)
            {
                return Err(AdapterRegistryError::MissingSourceAdapter {
                    adapter_id: descriptor.adapter_id.clone(),
                });
            }
        }
        Ok(())
    }

    fn bind_invocation_adapter(
        &mut self,
        adapter: &'static dyn InvocationAdapter,
    ) -> Result<(), AdapterRegistryError> {
        let adapter_id = AdapterId::new(adapter.adapter_id());
        let Some(descriptor) = self.descriptors.get(&adapter_id) else {
            return Err(AdapterRegistryError::UnknownInvocationAdapter { adapter_id });
        };
        if !descriptor.facets.invocation {
            return Err(AdapterRegistryError::InvocationFacetNotEnabled { adapter_id });
        }
        if !descriptor
            .aliases
            .invocation
            .iter()
            .any(|alias| alias == adapter.protocol_alias())
        {
            return Err(AdapterRegistryError::MissingInvocationAlias {
                adapter_id,
                alias: adapter.protocol_alias().to_owned(),
            });
        }
        if !descriptor
            .supported_versions
            .iter()
            .any(|version| version.as_str() == adapter.protocol_version())
        {
            return Err(AdapterRegistryError::MissingInvocationProtocolVersion {
                adapter_id,
                version: adapter.protocol_version().to_owned(),
            });
        }
        if self.invocation_adapters.contains_key(&adapter_id) {
            return Err(AdapterRegistryError::DuplicateInvocationAdapter { adapter_id });
        }
        self.invocation_adapters.insert(adapter_id, adapter);
        Ok(())
    }

    fn validate_invocation_bindings(&self) -> Result<(), AdapterRegistryError> {
        for descriptor in self.descriptors.values() {
            if descriptor.facets.invocation
                && !self
                    .invocation_adapters
                    .contains_key(&descriptor.adapter_id)
            {
                return Err(AdapterRegistryError::MissingInvocationAdapter {
                    adapter_id: descriptor.adapter_id.clone(),
                });
            }
        }
        Ok(())
    }

    fn bind_managed_runtime_adapter(
        &mut self,
        entry: RuntimeEntry,
    ) -> Result<(), AdapterRegistryError> {
        let Some(adapter_id) = self.managed_runtime_aliases.get(entry.id).cloned() else {
            return Err(AdapterRegistryError::UnknownManagedRuntime {
                alias: entry.id.to_owned(),
            });
        };
        let Some(descriptor) = self.descriptors.get(&adapter_id) else {
            return Err(AdapterRegistryError::UnknownManagedRuntime {
                alias: entry.id.to_owned(),
            });
        };
        if !descriptor
            .supported_versions
            .iter()
            .any(|version| version.as_str() == entry.adapter.protocol_version())
        {
            return Err(AdapterRegistryError::MissingManagedRuntimeProtocolVersion {
                adapter_id,
                version: entry.adapter.protocol_version().to_owned(),
            });
        }
        if self.managed_runtime_adapters.contains_key(&adapter_id) {
            return Err(AdapterRegistryError::DuplicateManagedRuntimeAdapter { adapter_id });
        }
        self.managed_runtime_adapters.insert(adapter_id, entry);
        Ok(())
    }

    fn validate_managed_runtime_bindings(&self) -> Result<(), AdapterRegistryError> {
        for descriptor in self.descriptors.values() {
            if descriptor.facets.managed_runtime
                && !self
                    .managed_runtime_adapters
                    .contains_key(&descriptor.adapter_id)
            {
                return Err(AdapterRegistryError::MissingManagedRuntimeAdapter {
                    adapter_id: descriptor.adapter_id.clone(),
                });
            }
        }
        Ok(())
    }
}

fn validate_descriptor(descriptor: &AdapterDescriptor) -> Result<(), AdapterRegistryError> {
    if descriptor.adapter_id.as_str().trim().is_empty() {
        return Err(AdapterRegistryError::EmptyField {
            field: "adapter_id",
        });
    }
    if descriptor.display_name.trim().is_empty() {
        return Err(AdapterRegistryError::EmptyField {
            field: "display_name",
        });
    }
    if descriptor.protocol.as_str().trim().is_empty() {
        return Err(AdapterRegistryError::EmptyField { field: "protocol" });
    }
    if !descriptor.facets.any() {
        return Err(AdapterRegistryError::MissingFacet {
            adapter_id: descriptor.adapter_id.clone(),
        });
    }
    if descriptor.supported_versions.is_empty() {
        return Err(AdapterRegistryError::MissingProtocolVersion {
            adapter_id: descriptor.adapter_id.clone(),
        });
    }

    let mut versions = HashSet::new();
    for version in &descriptor.supported_versions {
        let version = version.as_str().trim();
        if version.is_empty() {
            return Err(AdapterRegistryError::EmptyField {
                field: "supported_versions",
            });
        }
        if !versions.insert(version) {
            return Err(AdapterRegistryError::DuplicateProtocolVersion {
                adapter_id: descriptor.adapter_id.clone(),
                version: version.to_owned(),
            });
        }
    }

    validate_facet_aliases(
        descriptor,
        AdapterFacet::Source,
        descriptor.facets.source,
        &descriptor.aliases.source,
    )?;
    validate_facet_aliases(
        descriptor,
        AdapterFacet::Invocation,
        descriptor.facets.invocation,
        &descriptor.aliases.invocation,
    )?;
    validate_facet_aliases(
        descriptor,
        AdapterFacet::ManagedRuntime,
        descriptor.facets.managed_runtime,
        &descriptor.aliases.managed_runtime,
    )?;

    Ok(())
}

fn validate_facet_aliases(
    descriptor: &AdapterDescriptor,
    facet: AdapterFacet,
    enabled: bool,
    aliases: &[String],
) -> Result<(), AdapterRegistryError> {
    if !enabled && !aliases.is_empty() {
        return Err(AdapterRegistryError::AliasWithoutFacet {
            adapter_id: descriptor.adapter_id.clone(),
            facet,
        });
    }
    if enabled && aliases.is_empty() {
        return Err(AdapterRegistryError::FacetWithoutAlias {
            adapter_id: descriptor.adapter_id.clone(),
            facet,
        });
    }
    if aliases.iter().any(|alias| alias.trim().is_empty()) {
        return Err(AdapterRegistryError::EmptyAlias {
            adapter_id: descriptor.adapter_id.clone(),
            facet,
        });
    }
    Ok(())
}

fn register_aliases(
    index: &mut HashMap<String, AdapterId>,
    facet: AdapterFacet,
    adapter_id: &AdapterId,
    aliases: &[String],
) -> Result<(), AdapterRegistryError> {
    for alias in aliases {
        if let Some(existing_adapter_id) = index.get(alias) {
            return Err(AdapterRegistryError::DuplicateAlias {
                facet,
                alias: alias.clone(),
                existing_adapter_id: existing_adapter_id.clone(),
                incoming_adapter_id: adapter_id.clone(),
            });
        }
        index.insert(alias.clone(), adapter_id.clone());
    }
    Ok(())
}

fn aliases(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn builtin_descriptors() -> Vec<AdapterDescriptor> {
    vec![
        AdapterDescriptor::unverified(
            "a2a",
            "A2A",
            "a2a",
            AdapterAliases {
                source: aliases(&["a2a", "a2a_v1"]),
                invocation: aliases(&["a2a_v1"]),
                ..Default::default()
            },
            AdapterFacets {
                source: true,
                invocation: true,
                event_normalizer: true,
                ..Default::default()
            },
        )
        .with_supported_versions(["unverified", "0.3", "1.0"]),
        AdapterDescriptor::unverified(
            "acp",
            "ACP",
            "acp",
            AdapterAliases {
                source: aliases(&["acp", "acp_legacy"]),
                invocation: aliases(&["acp_legacy"]),
                ..Default::default()
            },
            AdapterFacets {
                source: true,
                invocation: true,
                event_normalizer: true,
                ..Default::default()
            },
        ),
        AdapterDescriptor::unverified(
            "crewai",
            "CrewAI",
            "crewai",
            AdapterAliases {
                source: aliases(&["crewai", "crewai_crew"]),
                invocation: aliases(&["crewai_crew"]),
                ..Default::default()
            },
            AdapterFacets {
                source: true,
                invocation: true,
                event_normalizer: true,
                ..Default::default()
            },
        ),
        AdapterDescriptor::unverified(
            "dify",
            "Dify",
            "dify",
            AdapterAliases {
                source: aliases(&["dify", "dify_app"]),
                invocation: aliases(&["dify_app"]),
                ..Default::default()
            },
            AdapterFacets {
                source: true,
                invocation: true,
                event_normalizer: true,
                ..Default::default()
            },
        ),
        AdapterDescriptor::unverified(
            "langgraph",
            "LangGraph",
            "langgraph",
            AdapterAliases {
                source: aliases(&["langgraph", "langgraph_assistant"]),
                invocation: aliases(&["langgraph_assistant"]),
                ..Default::default()
            },
            AdapterFacets {
                source: true,
                invocation: true,
                event_normalizer: true,
                ..Default::default()
            },
        ),
        AdapterDescriptor::unverified(
            "openai_assistants",
            "OpenAI Assistants",
            "openai_assistants",
            AdapterAliases {
                source: aliases(&["openai_assistants", "openai_assistant"]),
                ..Default::default()
            },
            AdapterFacets {
                source: true,
                ..Default::default()
            },
        )
        .with_supported_versions(["assistants=v2"]),
        AdapterDescriptor::unverified(
            "openapi",
            "OpenAPI",
            "openapi",
            AdapterAliases {
                source: aliases(&["openapi", "openapi_rest"]),
                invocation: aliases(&["openapi_rest"]),
                ..Default::default()
            },
            AdapterFacets {
                source: true,
                invocation: true,
                event_normalizer: true,
                ..Default::default()
            },
        ),
        AdapterDescriptor::unverified(
            "elastic",
            "Elastic Agent Builder",
            "elastic_agent_builder",
            AdapterAliases {
                source: aliases(&["elastic", "elastic_agent_builder"]),
                managed_runtime: aliases(&["elastic_agent_builder"]),
                ..Default::default()
            },
            AdapterFacets {
                source: true,
                managed_runtime: true,
                ..Default::default()
            },
        ),
        AdapterDescriptor::unverified(
            "opencode",
            "OpenCode",
            "claude_managed_agents",
            AdapterAliases {
                source: aliases(&["opencode", "claude_managed_agents"]),
                ..Default::default()
            },
            AdapterFacets {
                source: true,
                ..Default::default()
            },
        ),
        AdapterDescriptor::unverified(
            "claude_managed_agents",
            "Claude Managed Agents",
            "claude_managed_agents",
            AdapterAliases {
                managed_runtime: aliases(&["claude_managed_agents"]),
                ..Default::default()
            },
            AdapterFacets {
                managed_runtime: true,
                ..Default::default()
            },
        ),
        AdapterDescriptor::unverified(
            "cursor",
            "Cursor",
            "cursor",
            AdapterAliases {
                managed_runtime: aliases(&["cursor"]),
                ..Default::default()
            },
            AdapterFacets {
                managed_runtime: true,
                ..Default::default()
            },
        ),
        AdapterDescriptor::unverified(
            "gemini_antigravity",
            "Gemini Antigravity",
            "gemini_antigravity",
            AdapterAliases {
                managed_runtime: aliases(&["gemini_antigravity"]),
                ..Default::default()
            },
            AdapterFacets {
                managed_runtime: true,
                ..Default::default()
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_agents::adapters::descriptor::{ProtocolId, ProtocolVersion};

    fn descriptor(
        adapter_id: &str,
        aliases: AdapterAliases,
        facets: AdapterFacets,
    ) -> AdapterDescriptor {
        AdapterDescriptor {
            adapter_id: AdapterId::new(adapter_id),
            display_name: adapter_id.to_owned(),
            protocol: ProtocolId::new(adapter_id),
            supported_versions: vec![ProtocolVersion::new("test")],
            aliases,
            facets,
        }
    }

    #[test]
    fn builtin_registry_resolves_source_aliases() {
        let registry = AgentAdapterRegistry::builtin().unwrap();

        for (alias, adapter_id) in [
            ("a2a", "a2a"),
            ("a2a_v1", "a2a"),
            ("acp", "acp"),
            ("acp_legacy", "acp"),
            ("crewai", "crewai"),
            ("crewai_crew", "crewai"),
            ("dify", "dify"),
            ("dify_app", "dify"),
            ("langgraph", "langgraph"),
            ("langgraph_assistant", "langgraph"),
            ("openai_assistants", "openai_assistants"),
            ("openai_assistant", "openai_assistants"),
            ("openapi", "openapi"),
            ("openapi_rest", "openapi"),
            ("elastic", "elastic"),
            ("elastic_agent_builder", "elastic"),
            ("opencode", "opencode"),
            ("claude_managed_agents", "opencode"),
        ] {
            assert_eq!(
                registry
                    .source(alias)
                    .map(|descriptor| descriptor.adapter_id.as_str()),
                Some(adapter_id),
                "incorrect source alias {alias}"
            );
        }
    }

    #[test]
    fn a2a_declares_concrete_protocol_versions_without_reinterpreting_legacy_aliases() {
        let registry = AgentAdapterRegistry::builtin().unwrap();
        let descriptor = registry.descriptor("a2a").unwrap();
        let versions = descriptor
            .supported_versions
            .iter()
            .map(|version| version.as_str())
            .collect::<Vec<_>>();

        assert_eq!(descriptor.protocol.as_str(), "a2a");
        assert_eq!(versions, ["unverified", "0.3", "1.0"]);
        assert!(descriptor.aliases.invocation.contains(&"a2a_v1".to_owned()));
    }

    #[test]
    fn builtin_registry_resolves_invocation_and_managed_runtime_aliases() {
        let registry = AgentAdapterRegistry::builtin().unwrap();

        for (alias, adapter_id) in [
            ("a2a_v1", "a2a"),
            ("acp_legacy", "acp"),
            ("crewai_crew", "crewai"),
            ("dify_app", "dify"),
            ("langgraph_assistant", "langgraph"),
            ("openapi_rest", "openapi"),
        ] {
            assert_eq!(
                registry
                    .invocation(alias)
                    .map(|descriptor| descriptor.adapter_id.as_str()),
                Some(adapter_id),
                "incorrect invocation alias {alias}"
            );
        }
        for (alias, adapter_id) in [
            ("claude_managed_agents", "claude_managed_agents"),
            ("cursor", "cursor"),
            ("gemini_antigravity", "gemini_antigravity"),
            ("elastic_agent_builder", "elastic"),
        ] {
            assert_eq!(
                registry
                    .managed_runtime(alias)
                    .map(|descriptor| descriptor.adapter_id.as_str()),
                Some(adapter_id),
                "incorrect managed runtime alias {alias}"
            );
        }
    }

    #[test]
    fn aliases_are_scoped_by_facet() {
        let registry = AgentAdapterRegistry::builtin().unwrap();

        assert_eq!(
            registry
                .source("claude_managed_agents")
                .unwrap()
                .adapter_id
                .as_str(),
            "opencode"
        );
        assert_eq!(
            registry
                .managed_runtime("claude_managed_agents")
                .unwrap()
                .adapter_id
                .as_str(),
            "claude_managed_agents"
        );
    }

    #[test]
    fn duplicate_alias_within_a_facet_is_rejected() {
        let descriptors = [
            descriptor(
                "one",
                AdapterAliases {
                    source: aliases(&["shared"]),
                    ..Default::default()
                },
                AdapterFacets {
                    source: true,
                    ..Default::default()
                },
            ),
            descriptor(
                "two",
                AdapterAliases {
                    source: aliases(&["shared"]),
                    ..Default::default()
                },
                AdapterFacets {
                    source: true,
                    ..Default::default()
                },
            ),
        ];

        assert!(matches!(
            AgentAdapterRegistry::from_descriptors(descriptors).unwrap_err(),
            AdapterRegistryError::DuplicateAlias {
                facet: AdapterFacet::Source,
                ..
            }
        ));
    }

    #[test]
    fn duplicate_alias_across_facets_is_allowed() {
        let registry = AgentAdapterRegistry::from_descriptors([
            descriptor(
                "source",
                AdapterAliases {
                    source: aliases(&["shared"]),
                    ..Default::default()
                },
                AdapterFacets {
                    source: true,
                    ..Default::default()
                },
            ),
            descriptor(
                "runtime",
                AdapterAliases {
                    managed_runtime: aliases(&["shared"]),
                    ..Default::default()
                },
                AdapterFacets {
                    managed_runtime: true,
                    ..Default::default()
                },
            ),
        ])
        .unwrap();

        assert_eq!(
            registry.source("shared").unwrap().adapter_id.as_str(),
            "source"
        );
        assert_eq!(
            registry
                .managed_runtime("shared")
                .unwrap()
                .adapter_id
                .as_str(),
            "runtime"
        );
    }

    #[test]
    fn invalid_descriptors_fail_startup_validation() {
        let mut missing_version = descriptor(
            "missing-version",
            AdapterAliases {
                source: aliases(&["missing-version"]),
                ..Default::default()
            },
            AdapterFacets {
                source: true,
                ..Default::default()
            },
        );
        missing_version.supported_versions.clear();
        assert!(matches!(
            AgentAdapterRegistry::from_descriptors([missing_version]).unwrap_err(),
            AdapterRegistryError::MissingProtocolVersion { .. }
        ));

        let alias_without_facet = descriptor(
            "alias-without-facet",
            AdapterAliases {
                source: aliases(&["alias-without-facet-source"]),
                invocation: aliases(&["alias-without-facet"]),
                ..Default::default()
            },
            AdapterFacets {
                source: true,
                ..Default::default()
            },
        );
        assert!(matches!(
            AgentAdapterRegistry::from_descriptors([alias_without_facet]).unwrap_err(),
            AdapterRegistryError::AliasWithoutFacet {
                facet: AdapterFacet::Invocation,
                ..
            }
        ));
    }
}
