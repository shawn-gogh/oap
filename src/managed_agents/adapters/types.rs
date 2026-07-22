use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ConnectorContext {
    pub connector_id: String,
    pub owner_id: String,
    pub endpoint: String,
    pub credential: Option<CredentialLease>,
    pub configuration: Value,
}

#[derive(Clone)]
pub struct CredentialLease {
    secret: String,
    pub expires_at: Option<i64>,
}

impl CredentialLease {
    pub fn new(secret: String, expires_at: Option<i64>) -> Self {
        Self { secret, expires_at }
    }

    pub fn expose(&self) -> &str {
        &self.secret
    }
}

impl std::fmt::Debug for CredentialLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CredentialLease")
            .field("secret", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeCapabilityProfile {
    pub streaming: bool,
    pub resumable_events: bool,
    pub idempotent_invocation: bool,
    pub remote_sessions: bool,
    pub remote_tasks: bool,
    pub approvals: bool,
    pub cancel: bool,
    pub abort: bool,
    pub artifacts: bool,
    pub multimodal_input: bool,
    pub mcp: bool,
    pub max_input_bytes: Option<u64>,
    pub max_artifact_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionProfileV1 {
    #[serde(default = "interaction_profile_version")]
    pub schema_version: u16,
    #[serde(default)]
    pub primary_surface: PrimarySurface,
    #[serde(default)]
    pub execution_mode: ExecutionMode,
    #[serde(default = "object_schema")]
    pub input_schema: Value,
    #[serde(default)]
    pub output_schema: Value,
    #[serde(default)]
    pub progress_mode: ProgressMode,
    #[serde(default)]
    pub continuation_modes: Vec<ContinuationMode>,
    #[serde(default)]
    pub accepted_input_types: Vec<String>,
    #[serde(default)]
    pub artifact_media_types: Vec<String>,
    #[serde(default)]
    pub supports_retry: bool,
    #[serde(default)]
    pub supports_checkpoint_resume: bool,
    #[serde(default)]
    pub supports_child_invocations: bool,
}

impl Default for InteractionProfileV1 {
    fn default() -> Self {
        Self {
            schema_version: interaction_profile_version(),
            primary_surface: PrimarySurface::default(),
            execution_mode: ExecutionMode::default(),
            input_schema: object_schema(),
            output_schema: Value::Object(Default::default()),
            progress_mode: ProgressMode::default(),
            continuation_modes: Vec::new(),
            accepted_input_types: vec!["application/json".to_owned(), "text/plain".to_owned()],
            artifact_media_types: Vec::new(),
            supports_retry: true,
            supports_checkpoint_resume: false,
            supports_child_invocations: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimarySurface {
    #[default]
    Conversation,
    Run,
    Workspace,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Blocking,
    AsyncPoll,
    #[default]
    AsyncStream,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressMode {
    #[default]
    None,
    Status,
    Percent,
    Steps,
    Graph,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationMode {
    Input,
    Approval,
    Authentication,
    FileUpload,
    Choice,
}

fn interaction_profile_version() -> u16 {
    1
}

fn object_schema() -> Value {
    serde_json::json!({"type": "object"})
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiatedCapabilities {
    pub declared: RuntimeCapabilityProfile,
    pub verified: RuntimeCapabilityProfile,
    pub granted: RuntimeCapabilityProfile,
    pub findings: Vec<CapabilityFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityFinding {
    pub code: String,
    pub severity: FindingSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Info,
    Warning,
    ApprovalRequired,
    Blocking,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceCursor {
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceHealth {
    pub status: SourceHealthStatus,
    pub detail: Option<String>,
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceHealthStatus {
    Healthy,
    Degraded,
    Unreachable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredAgent {
    pub external_id: String,
    pub name: String,
    pub description: Option<String>,
    pub source_version: Option<String>,
    pub raw_spec: Value,
    pub canonical_spec: Value,
    pub declared_capabilities: RuntimeCapabilityProfile,
    pub findings: Vec<CapabilityFinding>,
    pub next_cursor: Option<SourceCursor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvocationEnvelope {
    pub request_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub invocation_id: String,
    pub agent_id: String,
    pub agent_revision: i32,
    pub input: Value,
    pub capability_token: String,
    pub deadline: i64,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvocationBinding {
    pub remote_agent_id: Option<String>,
    pub remote_session_id: Option<String>,
    pub remote_context_id: Option<String>,
    pub remote_task_id: Option<String>,
    pub resume_cursor: Option<String>,
    pub raw: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvocationState {
    pub status: InvocationStatus,
    pub resume_cursor: Option<String>,
    pub error: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvocationStatus {
    Queued,
    Running,
    WaitingInput,
    WaitingApproval,
    Cancelling,
    Completed,
    Failed,
    Rejected,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalRuntimeEvent {
    pub event_key: String,
    pub event_type: String,
    pub provider_sequence: Option<String>,
    pub resume_cursor: Option<String>,
    pub payload: Value,
    pub raw: Value,
    pub occurred_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventNormalizationContext {
    pub session_id: String,
    pub turn_id: String,
    pub invocation_id: String,
    pub request_id: String,
    pub provider_sequence: Option<String>,
    pub resume_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudEventEnvelope {
    pub specversion: String,
    pub id: String,
    pub source: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub subject: Option<String>,
    pub time: Option<String>,
    #[serde(default = "default_cloud_event_content_type")]
    pub datacontenttype: String,
    pub data: Value,
    #[serde(default, flatten)]
    pub extensions: HashMap<String, Value>,
}

fn default_cloud_event_content_type() -> String {
    "application/json".to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalResult {
    pub approval_id: String,
    pub operation_id: String,
    pub accepted: bool,
    pub feedback: Option<String>,
    pub decided_by: String,
    pub decided_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactReference {
    pub id: Option<String>,
    /// Invocation that produced the artifact. Omit only for primary-invocation
    /// output; child or delegated output should identify its producer.
    #[serde(default)]
    pub invocation_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    pub media_type: String,
    pub digest: Option<String>,
    pub size_bytes: Option<u64>,
    pub uri: Option<String>,
    #[serde(default)]
    pub data_base64: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityGrantRequest {
    pub owner_id: String,
    pub agent_id: String,
    pub agent_revision: i32,
    pub session_id: String,
    pub turn_id: String,
    pub requested: RuntimeCapabilityProfile,
    pub data_classification: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityGrantDecision {
    pub granted: RuntimeCapabilityProfile,
    pub approval_required: bool,
    pub policy_version: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalIdentity {
    pub issuer: String,
    pub subject: String,
    pub audience: Option<String>,
    pub claims: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformIdentity {
    pub user_id: String,
    pub agent_id: Option<String>,
    pub groups: Vec<String>,
    pub mapping_evidence: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCapabilityGrant {
    pub session_id: String,
    pub turn_id: String,
    pub invocation_id: String,
    pub server_ids: Vec<String>,
    pub tool_allowlist: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub allow_all_servers: Vec<String>,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryContext {
    pub trace_id: String,
    pub span_id: String,
    pub traceparent: String,
    pub tracestate: Option<String>,
    pub parent_traceparent: Option<String>,
    pub parent_tracestate: Option<String>,
    pub session_id: String,
    pub turn_id: String,
    pub invocation_id: String,
    pub adapter_id: String,
    pub protocol: String,
    pub remote_correlation_id: Option<String>,
    pub started_at: i64,
    pub attributes: HashMap<String, String>,
}
