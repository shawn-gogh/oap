use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AdapterId(String);

impl AdapterId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AdapterId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProtocolId(String);

impl ProtocolId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProtocolId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProtocolVersion(String);

impl ProtocolVersion {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProtocolVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterFacets {
    pub source: bool,
    pub invocation: bool,
    pub managed_runtime: bool,
    pub event_normalizer: bool,
}

impl AdapterFacets {
    pub fn any(self) -> bool {
        self.source || self.invocation || self.managed_runtime || self.event_normalizer
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterAliases {
    pub source: Vec<String>,
    pub invocation: Vec<String>,
    pub managed_runtime: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterDescriptor {
    pub adapter_id: AdapterId,
    pub display_name: String,
    pub protocol: ProtocolId,
    pub supported_versions: Vec<ProtocolVersion>,
    pub aliases: AdapterAliases,
    pub facets: AdapterFacets,
}

impl AdapterDescriptor {
    pub fn unverified(
        adapter_id: impl Into<String>,
        display_name: impl Into<String>,
        protocol: impl Into<String>,
        aliases: AdapterAliases,
        facets: AdapterFacets,
    ) -> Self {
        Self {
            adapter_id: AdapterId::new(adapter_id),
            display_name: display_name.into(),
            protocol: ProtocolId::new(protocol),
            supported_versions: vec![ProtocolVersion::new("unverified")],
            aliases,
            facets,
        }
    }

    pub fn with_supported_versions(
        mut self,
        versions: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.supported_versions = versions
            .into_iter()
            .map(|version| ProtocolVersion::new(version.into()))
            .collect();
        self
    }
}
