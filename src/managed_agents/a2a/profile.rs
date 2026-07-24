use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::managed_agents::adapters::source::{
    NegotiatedProtocolInterface, NegotiatedSourceProfile,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum A2aProtocolVersion {
    #[serde(rename = "0.3")]
    V0_3,
    #[serde(rename = "1.0")]
    V1_0,
}

impl A2aProtocolVersion {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V0_3 => "0.3",
            Self::V1_0 => "1.0",
        }
    }
}

impl fmt::Display for A2aProtocolVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for A2aProtocolVersion {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "0.3" | "0.3.0" => Ok(Self::V0_3),
            "1.0" | "1.0.0" => Ok(Self::V1_0),
            value => Err(format!("不受支持的 A2A 协议版本 `{value}`")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum A2aBinding {
    #[serde(rename = "JSONRPC")]
    JsonRpc,
    #[serde(rename = "HTTP+JSON")]
    HttpJson,
    #[serde(rename = "GRPC")]
    Grpc,
    Custom(String),
}

impl A2aBinding {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_uppercase().as_str() {
            "JSONRPC" => Ok(Self::JsonRpc),
            "HTTP+JSON" => Ok(Self::HttpJson),
            "GRPC" => Ok(Self::Grpc),
            _ if value.trim().starts_with("https://") => Ok(Self::Custom(value.trim().to_owned())),
            _ => Err(format!("unsupported A2A protocol binding `{value}`")),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::JsonRpc => "JSONRPC",
            Self::HttpJson => "HTTP+JSON",
            Self::Grpc => "GRPC",
            Self::Custom(value) => value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2aInterface {
    pub url: String,
    pub binding: A2aBinding,
    pub protocol_version: A2aProtocolVersion,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct A2aSelectionPolicy {
    pub preferred_version: A2aProtocolVersion,
    pub supported_bindings: Vec<A2aBinding>,
    pub allow_legacy_0_3: bool,
}

impl Default for A2aSelectionPolicy {
    fn default() -> Self {
        Self {
            preferred_version: A2aProtocolVersion::V1_0,
            supported_bindings: vec![A2aBinding::JsonRpc],
            allow_legacy_0_3: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2aNegotiatedProfile {
    pub protocol: String,
    pub protocol_version: A2aProtocolVersion,
    pub protocol_binding: A2aBinding,
    pub interface_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    pub card_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_version: Option<String>,
    pub selection_reason: String,
    pub advertised_interfaces: Vec<A2aInterface>,
    pub streaming: bool,
    pub push_notifications: bool,
    pub extended_agent_card: bool,
    pub extensions: Vec<String>,
    pub required_extensions: Vec<String>,
}

impl A2aNegotiatedProfile {
    pub(crate) fn select(
        raw_card: &[u8],
        agent_version: Option<String>,
        interfaces: Vec<A2aInterface>,
        streaming: bool,
        push_notifications: bool,
        extended_agent_card: bool,
        extensions: Vec<String>,
        required_extensions: Vec<String>,
        policy: &A2aSelectionPolicy,
    ) -> Result<Self, String> {
        let selected = select_interface(&interfaces, policy)?;
        let selection_reason = if selected.protocol_version == policy.preferred_version {
            "preferred_version"
        } else {
            "legacy_only_compatible_version"
        };
        Ok(Self {
            protocol: "a2a".to_owned(),
            protocol_version: selected.protocol_version,
            protocol_binding: selected.binding.clone(),
            interface_url: selected.url.clone(),
            tenant: selected.tenant.clone(),
            card_digest: format!("{:x}", Sha256::digest(raw_card)),
            agent_version,
            selection_reason: selection_reason.to_owned(),
            advertised_interfaces: interfaces,
            streaming,
            push_notifications,
            extended_agent_card,
            extensions,
            required_extensions,
        })
    }
}

impl From<A2aNegotiatedProfile> for NegotiatedSourceProfile {
    fn from(profile: A2aNegotiatedProfile) -> Self {
        Self {
            protocol: profile.protocol,
            protocol_version: profile.protocol_version.to_string(),
            protocol_binding: profile.protocol_binding.as_str().to_owned(),
            interface_url: profile.interface_url,
            tenant: profile.tenant,
            document_digest: profile.card_digest,
            agent_version: profile.agent_version,
            selection_reason: profile.selection_reason,
            advertised_interfaces: profile
                .advertised_interfaces
                .into_iter()
                .map(|interface| NegotiatedProtocolInterface {
                    url: interface.url,
                    protocol_version: interface.protocol_version.to_string(),
                    protocol_binding: interface.binding.as_str().to_owned(),
                    tenant: interface.tenant,
                })
                .collect(),
            streaming: profile.streaming,
            push_notifications: profile.push_notifications,
            extended_agent_card: profile.extended_agent_card,
            extensions: profile.extensions,
            required_extensions: profile.required_extensions,
        }
    }
}

fn select_interface<'a>(
    interfaces: &'a [A2aInterface],
    policy: &A2aSelectionPolicy,
) -> Result<&'a A2aInterface, String> {
    let supported = |interface: &&A2aInterface| {
        policy
            .supported_bindings
            .iter()
            .any(|binding| binding == &interface.binding)
    };
    if let Some(interface) = interfaces
        .iter()
        .filter(supported)
        .find(|interface| interface.protocol_version == policy.preferred_version)
    {
        return Ok(interface);
    }
    if policy.allow_legacy_0_3 {
        if let Some(interface) = interfaces
            .iter()
            .filter(supported)
            .find(|interface| interface.protocol_version == A2aProtocolVersion::V0_3)
        {
            return Ok(interface);
        }
    }
    Err("Agent Card does not expose an allowed A2A version and binding".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interface(version: A2aProtocolVersion, binding: A2aBinding) -> A2aInterface {
        A2aInterface {
            url: format!("https://agent.example/{version}/{binding:?}"),
            binding,
            protocol_version: version,
            tenant: None,
        }
    }

    #[test]
    fn selects_preferred_version_before_card_order() {
        let profile = A2aNegotiatedProfile::select(
            b"card",
            None,
            vec![
                interface(A2aProtocolVersion::V0_3, A2aBinding::JsonRpc),
                interface(A2aProtocolVersion::V1_0, A2aBinding::JsonRpc),
            ],
            true,
            false,
            false,
            Vec::new(),
            Vec::new(),
            &A2aSelectionPolicy::default(),
        )
        .unwrap();

        assert_eq!(profile.protocol_version, A2aProtocolVersion::V1_0);
        assert_eq!(profile.selection_reason, "preferred_version");
    }

    #[test]
    fn rejects_legacy_when_policy_disallows_it() {
        let policy = A2aSelectionPolicy {
            allow_legacy_0_3: false,
            ..Default::default()
        };
        let error = A2aNegotiatedProfile::select(
            b"card",
            None,
            vec![interface(A2aProtocolVersion::V0_3, A2aBinding::JsonRpc)],
            false,
            false,
            false,
            Vec::new(),
            Vec::new(),
            &policy,
        )
        .unwrap_err();

        assert!(error.contains("allowed A2A version"));
    }
}
