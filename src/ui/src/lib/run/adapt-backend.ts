// Projects the real backend's Run response shapes (backend-types.ts) into
// the frontend's own RunSnapshotV1 (types.ts). Pure functions, no I/O —
// real-client.ts calls these after every request. See types.ts's module
// doc comment and the Stage 7 plan's "Design decisions" for why the two
// shapes are kept distinct rather than unified.

import type {
  BackendArtifactResponse,
  BackendInboxItemRow,
  BackendManagedArtifactRow,
  BackendRunSnapshotV1,
  BackendSessionInvocationRow,
  BackendTurnStatus,
} from "./backend-types";
import type {
  RunApproval,
  RunArtifact,
  RunInputRequest,
  RunInvocation,
  RunResult,
  RunSnapshotV1,
  RunStatus,
  RunTrigger,
} from "./types";

// Backend and frontend status enums are the same string set (confirmed
// against src/db/managed_agents/session_control/repository.rs's active/
// terminal lists) — identity, not a lookup table.
function adaptStatus(status: BackendTurnStatus): RunStatus {
  return status;
}

// The backend's trigger_type check constraint allows
// conversation/manual/api/routine/event/delegate/retry — none of which
// match the frontend's originally-guessed RunTrigger union exactly. Best
// -effort mapping; unrecognized values fall back to "user" rather than
// throwing, since this is a display-only field.
function adaptTrigger(triggerType: string | undefined): RunTrigger {
  switch (triggerType) {
    case "retry":
      return "retry";
    case "routine":
      return "schedule";
    case "api":
    case "event":
      return "webhook";
    case "delegate":
      return "resume";
    case "conversation":
    case "manual":
    default:
      return "user";
  }
}

function adaptInvocation(row: BackendSessionInvocationRow): RunInvocation {
  return {
    id: row.id,
    turnId: row.turn_id,
    parentInvocationId: null, // backend rows don't carry a parent pointer yet
    role: row.role === "tool" ? "tool" : "agent",
    label: row.adapter_id || row.protocol,
    status: adaptStatus(row.status),
    startedAt: row.started_at ?? null,
    endedAt: row.completed_at ?? null,
    summary: null,
    raw: row,
  };
}

// name has no backend equivalent (ManagedArtifactRow has no display-name
// column) — synthesized from metadata.name when the runtime happened to
// set one (seen in tests/fixtures/run_contract/artifact-list-v1.json),
// else source_artifact_id, else the id.
function adaptArtifact(row: BackendManagedArtifactRow): RunArtifact {
  const metadataName = typeof row.metadata?.name === "string" ? row.metadata.name : null;
  return {
    id: row.id,
    name: metadataName ?? row.source_artifact_id ?? row.id,
    mediaType: row.media_type,
    sizeBytes: row.size_bytes ?? null,
    // The snapshot's embedded artifact rows never carry a download URL —
    // only GET /api/sessions/{id}/artifacts/{artifactId} does (see
    // adaptArtifactResponse below). Left null here; RunShell's artifact
    // list doesn't render a link yet (that's Stage 5), so this is a
    // no-op gap, not a regression.
    url: null,
    inline: null, // backend never inlines content
  };
}

export function adaptArtifactResponse(row: BackendArtifactResponse): RunArtifact {
  const metadataName =
    row.metadata && typeof row.metadata === "object" && !Array.isArray(row.metadata)
      ? (row.metadata as Record<string, unknown>).name
      : undefined;
  return {
    id: row.id,
    name: typeof metadataName === "string" ? metadataName : row.source_artifact_id || row.id,
    mediaType: row.media_type,
    sizeBytes: row.size_bytes ?? null,
    url: row.download_url ?? row.external_reference_url ?? null,
    inline: null,
  };
}

function adaptResult(value: unknown): RunResult | null {
  if (value === null || value === undefined) return null;
  if (typeof value === "string") return { kind: "text", text: value };
  return { kind: "json", json: value };
}

// Known gap (see Stage 7 plan): the exact shape of a pending input
// request's structured fields (InboxItemRow.args_json) wasn't confirmed.
// Represented as a single generic free-text field until that shape is
// confirmed against a live waiting_input turn.
function adaptPendingInputRequest(item: BackendInboxItemRow): RunInputRequest {
  return {
    id: item.id,
    requestedAt: 0,
    prompt: item.body ?? item.title,
    fields: [{ id: "input", label: item.title, kind: "text", required: true }],
  };
}

function adaptPendingApproval(item: BackendInboxItemRow): RunApproval {
  return {
    id: item.id,
    kind: item.kind,
    title: item.title,
    body: item.body ?? null,
    requestedAt: 0,
    canDecide: item.status === "pending",
  };
}

export function adaptSnapshot(backend: BackendRunSnapshotV1): RunSnapshotV1 {
  const { turn } = backend;
  const status = adaptStatus(turn.status);
  const isTerminal = [
    "completed",
    "failed",
    "rejected",
    "cancelled",
    "timed_out",
  ].includes(status);

  // pending_requests mixes input-requests and approvals; the backend
  // fixture (run-snapshot-waiting-input-v1.json) confirms kind:"input" for
  // the former — everything else is treated as an approval.
  const inputRequestItem = backend.pending_requests.find((item) => item.kind === "input");
  const approvalItem = backend.pending_requests.find((item) => item.kind !== "input");

  return {
    version: "v1",
    runId: turn.id,
    sessionId: turn.session_id,
    agentId: null, // not present on SessionTurnRow; caller may overlay if known
    agentName: turn.session_id,
    providerName: backend.invocations[0]?.protocol ?? null,
    status,
    trigger: adaptTrigger(turn.trigger_type),
    createdAt: turn.created_at ?? 0,
    updatedAt: turn.updated_at ?? turn.created_at ?? 0,
    startedAt: turn.started_at ?? null,
    endedAt: turn.completed_at ?? null,
    interactionProfile: {
      version: "v1",
      supportsCancel: !isTerminal,
      supportsRetry: backend.interaction_profile.supports_retry,
      supportsStreaming: backend.interaction_profile.execution_mode === "async_stream",
      inputSchema:
        Object.keys(backend.interaction_profile.input_schema ?? {}).length > 0
          ? backend.interaction_profile.input_schema
          : null,
      resultKinds: backend.result ? ["text", "json", "artifact"] : ["none"],
    },
    inputSnapshot: backend.input,
    progress: null, // derived from step.progress control events, not the snapshot (Stage 4)
    invocations: backend.invocations.map(adaptInvocation),
    pendingInputRequest: inputRequestItem ? adaptPendingInputRequest(inputRequestItem) : null,
    pendingApproval: approvalItem ? adaptPendingApproval(approvalItem) : null,
    result: adaptResult(backend.result),
    artifacts: backend.artifacts.map(adaptArtifact),
    error: turn.error_json
      ? {
          code: turn.error_json.code ?? "unknown",
          message: turn.error_json.message ?? "运行出错。",
          retryable: turn.error_json.retryable ?? true,
        }
      : null,
    lastEventSeq: backend.latest_sequence,
  };
}
