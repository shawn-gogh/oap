use std::{future::Future, pin::Pin};

use types::{
    ApprovalResult, ArtifactReference, CanonicalRuntimeEvent, ConnectorContext, InvocationBinding,
    InvocationEnvelope, InvocationState, NegotiatedCapabilities, RuntimeCapabilityProfile,
};

pub mod artifacts;
pub mod cloudevents;
pub mod descriptor;
pub(crate) mod invocation;
pub mod mcp_grants;
pub mod platform_identity;
pub mod registry;
pub mod source;
pub mod telemetry;
pub mod types;

pub type AdapterFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AdapterError>> + Send + 'a>>;

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("adapter configuration is invalid: {0}")]
    InvalidConfiguration(String),
    #[error("adapter does not support {0}")]
    Unsupported(&'static str),
    #[error("adapter authentication failed")]
    Authentication,
    #[error("adapter request failed: {0}")]
    Transport(String),
    #[error("adapter response could not be decoded: {0}")]
    Decode(String),
    #[error("remote invocation state is unknown: {0}")]
    StateUnknown(String),
    #[error("external identity requires an administrator mapping: {0}")]
    UnmappedIdentity(String),
    #[error("external identity is blocked: {0}")]
    BlockedIdentity(String),
    #[error("adapter storage request failed: {0}")]
    Storage(String),
}

pub trait RuntimeAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn protocol(&self) -> &'static str;
    fn protocol_version(&self) -> &'static str;

    fn negotiate<'a>(
        &'a self,
        connector: &'a ConnectorContext,
        requested: &'a RuntimeCapabilityProfile,
    ) -> AdapterFuture<'a, NegotiatedCapabilities>;

    fn invoke<'a>(
        &'a self,
        connector: &'a ConnectorContext,
        envelope: &'a InvocationEnvelope,
    ) -> AdapterFuture<'a, InvocationBinding>;

    fn events<'a>(
        &'a self,
        connector: &'a ConnectorContext,
        binding: &'a InvocationBinding,
    ) -> AdapterFuture<'a, Vec<CanonicalRuntimeEvent>>;

    fn state<'a>(
        &'a self,
        connector: &'a ConnectorContext,
        binding: &'a InvocationBinding,
    ) -> AdapterFuture<'a, InvocationState>;

    fn resolve_approval<'a>(
        &'a self,
        connector: &'a ConnectorContext,
        binding: &'a InvocationBinding,
        result: &'a ApprovalResult,
    ) -> AdapterFuture<'a, ()>;

    fn cancel<'a>(
        &'a self,
        connector: &'a ConnectorContext,
        binding: &'a InvocationBinding,
    ) -> AdapterFuture<'a, InvocationState>;

    fn abort<'a>(
        &'a self,
        _connector: &'a ConnectorContext,
        _binding: &'a InvocationBinding,
    ) -> AdapterFuture<'a, InvocationState> {
        Box::pin(async { Err(AdapterError::Unsupported("abort")) })
    }
}

pub trait CredentialAdapter: Send + Sync {
    fn resolve<'a>(
        &'a self,
        owner_id: &'a str,
        credential_name: &'a str,
    ) -> AdapterFuture<'a, types::CredentialLease>;
}

pub trait EventAdapter: Send + Sync {
    fn normalize<'a>(
        &'a self,
        context: &'a types::EventNormalizationContext,
        raw: &'a serde_json::Value,
    ) -> AdapterFuture<'a, Vec<CanonicalRuntimeEvent>>;
}

pub trait ArtifactAdapter: Send + Sync {
    fn persist<'a>(
        &'a self,
        session_id: &'a str,
        turn_id: &'a str,
        artifact: &'a ArtifactReference,
    ) -> AdapterFuture<'a, ArtifactReference>;
}

pub trait PolicyAdapter: Send + Sync {
    fn evaluate<'a>(
        &'a self,
        request: &'a types::CapabilityGrantRequest,
    ) -> AdapterFuture<'a, types::CapabilityGrantDecision>;
}

pub trait IdentityAdapter: Send + Sync {
    fn resolve<'a>(
        &'a self,
        identity: &'a types::ExternalIdentity,
    ) -> AdapterFuture<'a, types::PlatformIdentity>;
}

pub trait McpAdapter: Send + Sync {
    fn project_grant<'a>(
        &'a self,
        grant: &'a types::McpCapabilityGrant,
    ) -> AdapterFuture<'a, serde_json::Value>;
}

pub trait TelemetryAdapter: Send + Sync {
    fn invocation_started<'a>(
        &'a self,
        context: &'a types::TelemetryContext,
    ) -> AdapterFuture<'a, types::TelemetryContext>;

    fn invocation_finished<'a>(
        &'a self,
        context: &'a types::TelemetryContext,
        state: &'a InvocationState,
    ) -> AdapterFuture<'a, ()>;
}
