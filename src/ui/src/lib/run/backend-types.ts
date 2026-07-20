// TypeScript mirrors of the real backend's Run response shapes
// (src/http/sessions/run_types.rs, src/db/managed_agents/session_control/
// schema.rs, src/db/managed_agents/artifacts/schema.rs,
// src/db/managed_agents/inbox/schema.rs — snake_case, ground-truthed
// against tests/fixtures/run_contract/*.json). adapt-backend.ts projects
// these into the frontend's own camelCase RunSnapshotV1 (types.ts) — see
// that file's module doc comment for why the two are kept separate rather
// than unified.

export type BackendTurnStatus =
  | "queued"
  | "running"
  | "waiting_input"
  | "waiting_approval"
  | "cancelling"
  | "completed"
  | "failed"
  | "rejected"
  | "cancelled"
  | "timed_out";

// Only `id`/`session_id`/`request_id`/`status` are marked required — the
// rest are typed optional even though the Rust struct always serializes
// them, because some of this repo's own committed fixtures (e.g.
// tests/fixtures/run_contract/run-snapshot-completed-v1.json) omit them.
// Forcing optional access here means adapt-backend.ts can't silently assume
// a field is present.
export interface BackendSessionTurnRow {
  id: string;
  session_id: string;
  request_id: string;
  status: BackendTurnStatus;
  model?: string | null;
  input_json?: unknown;
  input_schema_json?: unknown;
  output_schema_json?: unknown;
  interaction_profile_json?: unknown;
  result_json?: unknown;
  trigger_type?: string;
  retry_of_turn_id?: string | null;
  attempt_number?: number;
  error_json?: { code?: string; message?: string; retryable?: boolean } | null;
  started_at?: number | null;
  completed_at?: number | null;
  created_at?: number;
  updated_at?: number;
}

export interface BackendSessionInvocationRow {
  id: string;
  session_id: string;
  turn_id: string;
  protocol: string;
  adapter_id: string;
  role: string;
  status: BackendTurnStatus;
  remote_session_id?: string | null;
  remote_context_id?: string | null;
  remote_task_id?: string | null;
  resume_cursor?: string | null;
  metadata?: unknown;
  error_json?: unknown;
  started_at?: number | null;
  completed_at?: number | null;
}

export interface BackendSessionOperationRow {
  id: string;
  turn_id: string;
  operation_key: string;
  operation_type: string;
  status: string;
  request_json: unknown;
  result_json: unknown;
  error_json: unknown;
}

export interface BackendInboxItemRow {
  id: string;
  kind: string;
  title: string;
  body: string | null;
  status: string;
  request_id?: string | null;
  turn_id?: string | null;
  invocation_id?: string | null;
  args_json?: unknown;
}

export interface BackendManagedArtifactRow {
  id: string;
  session_id: string;
  turn_id: string;
  invocation_id: string | null;
  task_id: string | null;
  source_artifact_id: string;
  media_type: string;
  digest: string | null;
  size_bytes: number | null;
  status: string;
  metadata: { name?: string; [key: string]: unknown };
  created_at: number;
  verified_at: number | null;
}

export interface BackendInteractionProfileV1 {
  schema_version: number;
  primary_surface: "conversation" | "run" | "workspace";
  execution_mode: "blocking" | "async_poll" | "async_stream";
  input_schema: Record<string, unknown>;
  output_schema: Record<string, unknown>;
  progress_mode: "none" | "status" | "percent" | "steps" | "graph";
  continuation_modes: string[];
  accepted_input_types: string[];
  artifact_media_types: string[];
  supports_retry: boolean;
  supports_checkpoint_resume: boolean;
  supports_child_invocations: boolean;
}

export interface BackendRunSnapshotV1 {
  schema_version: number;
  turn: BackendSessionTurnRow;
  interaction_profile: BackendInteractionProfileV1;
  input: unknown;
  result: unknown;
  invocations: BackendSessionInvocationRow[];
  operations: BackendSessionOperationRow[];
  pending_requests: BackendInboxItemRow[];
  artifacts: BackendManagedArtifactRow[];
  latest_sequence: number;
}

// The smaller shape cancel_turn returns — {turn, invocations} only, per the
// asymmetric-response finding in the Stage 7 plan.
export interface BackendTurnSnapshot {
  turn: BackendSessionTurnRow;
  invocations: BackendSessionInvocationRow[];
}

export interface BackendControlEventV1 {
  schema_version: number;
  type: string;
  sequence: number;
  session_id: string;
  turn_id: string | null;
  invocation_id: string | null;
  request_id: string | null;
  occurred_at: number;
  payload: unknown;
}

export interface BackendArtifactResponse {
  id: string;
  session_id: string;
  turn_id: string;
  invocation_id: string | null;
  task_id: string | null;
  source_artifact_id: string;
  media_type: string;
  digest: string | null;
  size_bytes: number | null;
  status: string;
  metadata: unknown;
  created_at: number;
  verified_at: number | null;
  download_url: string | null;
  external_reference_url: string | null;
}
