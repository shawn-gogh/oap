export interface OpencodeSession {
  id: string;
  title?: string;
  agent?: string;
  agent_id?: string;
  runtime?: AgentRuntimeId;
  runtime_agent_ref_id?: string;
  provider_session_id?: string;
  provider_run_id?: string;
  provider_url?: string;
  status?: string;
  workspace_bucket?: string;
  owner_id?: string;
  task_id?: string;
  attempt_number?: number;
  environment?: Record<string, unknown>;
  /** @deprecated use agent */
  harness?: string;
  time?: { created: number; updated?: number };
  [k: string]: unknown;
}

/** Frozen copy of the agent a session belonged to, stamped into
 *  `environment.deleted_agent` when the agent is deleted. Sessions outlive
 *  agents, so this is the only thing that still names the agent after the
 *  retention sweep removes its row. */
export interface DeletedAgentSnapshot {
  agent_id?: string;
  name?: string;
  model?: string;
  harness?: string;
  runtime?: string | null;
  deleted_at?: number | null;
}

export function deletedAgentSnapshot(
  session: OpencodeSession,
): DeletedAgentSnapshot | null {
  const raw = session.environment?.deleted_agent;
  return raw && typeof raw === "object" ? (raw as DeletedAgentSnapshot) : null;
}

export type AgentRuntimeId = string;
export type BuiltinRuntimeId = "claude_managed_agents" | "cursor" | "gemini_antigravity";
export function isBuiltinRuntime(id: string): id is BuiltinRuntimeId {
  return id === "claude_managed_agents" || id === "cursor" || id === "gemini_antigravity";
}

export interface AgentRuntimeTool {
  id: string;
  name: string;
  description: string;
  enabled_by_default: boolean;
  risk?: string | null;
}

export interface AgentRuntime {
  id: AgentRuntimeId;
  name: string;
  default_api_base: string;
  credential_provider_id: string;
  credential_provider_name: string;
  tools: AgentRuntimeTool[];
  approval_enforcement?: "enforced" | "advisory";
  connected: boolean;
  api_base?: string | null;
  masked_api_key?: string | null;
}

export interface RuntimeHarness {
  alias: string;
  api_spec: string;
  display_name: string;
  api_base: string;
  is_default: boolean;
  connected: boolean;
  masked_api_key?: string | null;
  tools: AgentRuntimeTool[];
  approval_enforcement?: "enforced" | "advisory";
}

export function resolveApiSpec(
  alias: string,
  harnesses: RuntimeHarness[],
): BuiltinRuntimeId | null {
  if (alias === "claude_managed_agents" || alias === "cursor" || alias === "gemini_antigravity") {
    return alias as BuiltinRuntimeId;
  }
  // Return null when harnesses haven't loaded yet or alias is unknown — callers
  // must treat null as "spec not yet known" rather than silently routing to a
  // default spec that may be wrong for this alias.
  const apiSpec = harnesses.find((h) => h.alias === alias)?.api_spec;
  return apiSpec && isBuiltinRuntime(apiSpec) ? apiSpec : null;
}

export interface MessageInfo {
  id?: string;
  role: "user" | "assistant";
  finish?: string;
  tokens?: { input?: number; output?: number; reasoning?: number };
  time?: { created?: number; completed?: number };
  providerID?: string;
  modelID?: string;
  sessionID?: string;
  [k: string]: unknown;
}

interface PartBase {
  id?: string;
  messageID?: string;
  sessionID?: string;
}

export type HarnessMessagePart = PartBase &
  (
    | { type: "text"; text: string }
    | { type: "reasoning"; text: string; time?: { start?: number; end?: number } }
    | { type: "thinking"; text: string; time?: { start?: number; end?: number } }
    | {
        type: "tool";
        tool: string;
        state: {
          status: string;
          input?: unknown;
          output?: unknown;
          error?: unknown;
          [k: string]: unknown;
        };
      }
    | { type: "step-start" }
    | { type: "step-finish"; [k: string]: unknown }
  );

export interface HarnessMessage {
  info: MessageInfo;
  parts: HarnessMessagePart[];
}

export interface Agent {
  id: string;
  name: string;
  model?: string;
  prompt?: string;
  system?: string;
  description?: string;
  harness?: string;
  cron?: string | null;
  timezone?: string | null;
  status?: string;
  owner_id?: string | null;
  /** IDs of DB-backed skills attached to this agent (agents.skill_ids). */
  skill_ids?: string[];
  /** IDs of DB-backed rules attached to this agent (agents.rule_ids). */
  rule_ids?: string[];
  vault_keys?: string[];
  config?: Record<string, unknown>;
  created_at?: number;
  [k: string]: unknown;
}

export interface PlatformMcp {
  id: string;
  name: string;
  description: string;
}

export interface Routine {
  id: string;
  agent_id: string;
  name: string;
  prompt: string;
  cron: string;
  timezone: string;
  status: "active" | "paused" | string;
  last_run_id?: string | null;
  last_session_id?: string | null;
  last_run_at?: number | null;
  created_at: number;
  updated_at: number;
}

export type AgentTaskStatus =
  | "draft"
  | "queued"
  | "running"
  | "waiting_input"
  | "verifying"
  | "succeeded"
  | "failed"
  | "cancelled";

export interface AgentTask {
  id: string;
  agent_id: string;
  application_version: number;
  source: "manual" | "routine" | "api" | "event" | "test" | string;
  source_id?: string | null;
  title: string;
  input_json: Record<string, unknown>;
  status: AgentTaskStatus | string;
  created_by: string;
  created_at: number;
  started_at?: number | null;
  completed_at?: number | null;
  failure_reason?: string | null;
  failure_code?: string | null;
  deadline_at?: number | null;
  current_attempt_number: number;
}

export interface TaskArtifact {
  id: string;
  task_id: string;
  session_id?: string | null;
  run_id?: string | null;
  attempt_number: number;
  artifact_type: string;
  name: string;
  content_json?: unknown;
  location?: string | null;
  created_by: string;
  created_at: number;
}

export interface TaskAcceptanceCheck {
  id: string;
  task_id: string;
  attempt_number: number;
  criterion_index: number;
  criterion: string;
  verdict: "pending" | "passed" | "failed";
  evidence?: string | null;
  checked_by?: string | null;
  checked_at?: number | null;
}

export interface TaskSessionAttempt {
  id: string;
  harness: string;
  runtime?: string | null;
  status: string;
  created_at: number;
  updated_at?: number | null;
  attempt_number: number;
  environment_json: Record<string, unknown>;
}

export interface TaskRunAttempt {
  id: string;
  session_id?: string | null;
  status: string;
  started_at: number;
  finished_at?: number | null;
  error?: string | null;
  attempt_number: number;
}

export interface TaskAttempts {
  sessions: TaskSessionAttempt[];
  runs: TaskRunAttempt[];
  artifacts: TaskArtifact[];
  acceptance_checks: TaskAcceptanceCheck[];
  max_attempts: number;
}

export interface AgentFile {
  agent_id: string;
  path: string;
  encoding?: "utf8" | "base64" | string;
  size_bytes: number;
  created_at: number;
  updated_at: number;
}

/** A file in a session's workspace bucket (see /session/:id/workspace/files). */
export interface WorkspaceFile {
  path: string;
  size_bytes: number;
  updated_at: number | null;
  content_type?: string;
  etag?: string | null;
}

export interface WorkspaceTrashItem {
  id: string;
  paths: string[];
  deleted_at: number;
  expires_at: number;
  size_bytes: number;
  object_count: number;
}

export interface AgentRunStart {
  run_id: string;
  agent_id: string;
  session_id?: string;
  status: string;
  event_url: string;
  logs_url?: string;
}

export interface VaultKeyEntry {
  key: string;
  scope: "global" | "personal";
  updated_at?: number;
  /** "env" if sourced from environment variables */
  source?: string;
}

/** A reusable, DB-backed skill (capability doc) attachable to an agent. */
export interface Skill {
  id: string;
  name: string;
  description: string | null;
  content: string;
  owner_id: string | null;
  created_at: number;
}

/** A reusable, DB-backed Markdown rule attachable to an agent. */
export interface Rule {
  id: string;
  name: string;
  description: string | null;
  content: string;
  owner_id: string | null;
  created_at: number;
  updated_at: number;
}

/** A durable key→value note an agent has stored in its memory. */
export interface Memory {
  id: string;
  agent_id: string;
  key: string;
  value: string;
  always_on?: boolean | number;
  created_at: number;
  updated_at: number;
}

export interface McpServer {
  server_id: string;
  server_name?: string | null;
  alias?: string | null;
  description?: string | null;
  instructions?: string | null;
  url?: string | null;
  transport: string;
  auth_type?: string | null;
  is_byok: boolean;
  byok_description?: string[];
  byok_api_key_help_url?: string | null;
  allowed_tools?: string[];
  available_on_public_internet: boolean;
  approval_status?: string | null;
  status?: string | null;
  created_at?: number | null;
  updated_at?: number | null;
  [k: string]: unknown;
}

export interface SpendLog {
  request_id: string;
  call_type: string;
  api_key: string;
  spend: number;
  total_tokens: number;
  prompt_tokens: number;
  completion_tokens: number;
  start_time: string;
  end_time: string;
  request_duration_ms: number | null;
  model: string;
  model_id: string | null;
  model_group: string | null;
  custom_llm_provider: string | null;
  api_base: string | null;
  user: string | null;
  metadata: Record<string, unknown> | null;
  cache_hit: string | null;
  cache_key: string | null;
  request_tags: unknown[] | Record<string, unknown> | null;
  end_user: string | null;
  requester_ip_address: string | null;
  messages: unknown;
  response: unknown;
  session_id: string | null;
  agent_id: string | null;
  invocation_id: string | null;
  purpose: string;
  status: string | null;
}

export interface AgentMeteringCoverage {
  gateway_metered: number;
  provider_reported: number;
  unmetered: number;
}

export interface AgentUsageMetrics {
  model_calls: number;
  invocations: number;
  total_tokens: number;
  estimated_cost_usd: number;
  average_latency_ms: number | null;
  success_rate: number | null;
}

export interface AgentDailyUsageMetrics extends AgentUsageMetrics {
  date: string;
  coverage: AgentMeteringCoverage;
}

export interface AgentQuotaConfig {
  budget_usd_monthly: number | null;
  max_concurrent_sessions: number | null;
  rate_per_minute: number | null;
}

export interface AgentQuotaStatus {
  config: AgentQuotaConfig;
  month_cost_usd: number;
  month_remaining_usd: number | null;
  month_reset_at: number;
  active_sessions: number;
  requests_this_minute: number;
  minute_reset_at: number;
}

export interface AgentMetrics {
  agent_id: string;
  days: number;
  timezone: "UTC";
  totals: AgentUsageMetrics;
  coverage: AgentMeteringCoverage;
  quota: AgentQuotaStatus;
  daily: AgentDailyUsageMetrics[];
}
