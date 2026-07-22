// Frozen frontend-side version of the shared Run contract described in
// docs/engineering/run-control-plane-branch-plan.mdx (InteractionProfileV1 /
// RunSnapshotV1 / ControlEventV1). The real contract ships from
// codex/run-control-plane; until that branch has a shared-contract commit,
// this is the frontend team's own copy, built against fixtures (see
// src/ui/src/lib/run/fixtures/) per docs/engineering/run-surface-branch-plan.mdx
// Stage 1. Keep this additive-only once anything downstream depends on it —
// the plan requires joint contract-test updates for breaking changes.
//
// A "Run" is presented to the user as one execution; its canonical persisted
// resource is the existing Session Turn. `RunStatus` therefore reuses
// `SessionTurnStatus` rather than redefining an equivalent enum, and
// `RunInvocation` mirrors the existing `SessionInvocation` shape (see
// src/ui/src/lib/api.ts).
//
// Product rule: `providerName` is metadata only. Nothing in this module (or
// in components consuming it) may branch UI behavior on provider identity —
// see rule 2/3 in the surface plan.

import type { SessionTurnStatus } from "@/lib/api";

export type RunStatus = SessionTurnStatus;

/** Minimal JSON Schema alias — Stage 3 owns the real structured-input
 * renderer; this module only needs a type to hang the "unsupported schema
 * falls back to a JSON editor" rule on. */
export type JsonSchema = Record<string, unknown>;

export type RunResultKind = "text" | "json" | "artifact" | "none";

export interface RunResult {
  kind: RunResultKind;
  text?: string;
  json?: unknown;
  artifactIds?: string[];
}

export interface InteractionProfileV1 {
  version: "v1";
  supportsCancel: boolean;
  supportsRetry: boolean;
  supportsStreaming: boolean;
  /** `null` means the agent only accepts free-text input. */
  inputSchema: JsonSchema | null;
  /** Advertises what kinds of result to expect — display hint, not a
   * dispatch key. A final text answer is only one possible result. */
  resultKinds: RunResultKind[];
}

export interface RunInvocation {
  id: string;
  turnId: string;
  parentInvocationId: string | null;
  role: "agent" | "tool" | "delegate" | "workflow";
  label: string;
  status: RunStatus;
  startedAt: number | null;
  endedAt: number | null;
  summary: string | null;
  /** Opaque provider evidence — never branched on, display-only (Stage 4
   * raw-event inspector). */
  raw: unknown;
}

export interface RunInputRequestField {
  id: string;
  label: string;
  kind: "text" | "choice" | "file" | "auth";
  required: boolean;
  choices?: string[];
  /** `auth`-kind fields only — a link to complete authorization elsewhere.
   * Never a place to collect credentials in-app. Optional because the real
   * backend's auth-request shape isn't confirmed yet (see PendingInputCard). */
  authUrl?: string;
}

export interface RunInputRequest {
  id: string;
  requestedAt: number;
  prompt: string;
  fields: RunInputRequestField[];
}

export interface RunApproval {
  id: string;
  kind: string;
  title: string;
  body: string | null;
  requestedAt: number;
  canDecide: boolean;
}

export interface RunArtifact {
  id: string;
  name: string;
  /** e.g. "text/markdown", "application/json", "text/csv", "text/plain",
   * "text/x-code", "image/png", "application/pdf",
   * "application/octet-stream", "text/uri-list". */
  mediaType: string;
  sizeBytes: number | null;
  url: string | null;
  /** Present for small inline content (markdown/json/text) so the shell
   * doesn't need a round trip to preview it. */
  inline: unknown;
}

export interface RunError {
  code: string;
  message: string;
  retryable: boolean;
}

export interface RunProgress {
  label: string;
  current: number;
  total: number | null;
}

export interface RunOperation {
  id: string;
  invocationId: string;
  type: string;
  status: string;
  request: unknown;
  result: unknown | null;
  error: unknown | null;
}

export type RunTrigger = "user" | "schedule" | "webhook" | "resume" | "retry";

export interface RunSnapshotV1 {
  version: "v1";
  /** Equal to the backing Session Turn id. */
  runId: string;
  sessionId: string;
  agentId: string | null;
  agentName: string;
  /** Metadata only — see module doc comment. */
  providerName: string | null;
  status: RunStatus;
  trigger: RunTrigger;
  createdAt: number;
  updatedAt: number;
  startedAt: number | null;
  endedAt: number | null;
  interactionProfile: InteractionProfileV1;
  /** Immutable structured input as submitted — preserved even after later
   * events update the run. */
  inputSnapshot: unknown;
  progress: RunProgress | null;
  invocations: RunInvocation[];
  operations: RunOperation[];
  pendingInputRequest: RunInputRequest | null;
  pendingApproval: RunApproval | null;
  result: RunResult | null;
  artifacts: RunArtifact[];
  error: RunError | null;
  /** Highest event sequence folded into this snapshot — reconnect resumes
   * `subscribeRunEvents` from here (Stage 6). */
  lastEventSeq: number;
}

export type ControlEventV1 =
  // `error` is optional/additive: the real backend's turn-transition events
  // (adapt-backend.ts's `adaptControlEvent`) always carry status+error
  // together in one frame, and folding both into one frontend event (rather
  // than emitting two events that would share one `seq`) keeps apply-event's
  // per-event dedup-by-sequence check correct — see Stage 6 design notes.
  | { seq: number; ts: number; type: "turn.status_changed"; status: RunStatus; error?: RunError | null }
  | { seq: number; ts: number; type: "turn.progress"; progress: RunProgress | null }
  | { seq: number; ts: number; type: "invocation.started"; invocation: RunInvocation }
  | { seq: number; ts: number; type: "invocation.updated"; invocation: RunInvocation }
  // Real-transport-only: an invocation status transition whose backend event
  // payload (`{status, error}`) can't rebuild a full RunInvocation (no
  // label/adapter id/timestamps) — patches an already-known invocation by id
  // instead of replacing it. See adapt-backend.ts's adaptControlEvent.
  | { seq: number; ts: number; type: "invocation.status_changed"; invocationId: string; status: RunStatus }
  | { seq: number; ts: number; type: "message.appended"; invocationId: string; text: string }
  | { seq: number; ts: number; type: "input_request.created"; request: RunInputRequest }
  | { seq: number; ts: number; type: "input_request.resolved"; requestId: string }
  | { seq: number; ts: number; type: "approval.created"; approval: RunApproval }
  | {
      seq: number;
      ts: number;
      type: "approval.resolved";
      approvalId: string;
      decision: "accepted" | "rejected";
      feedback: string | null;
    }
  | { seq: number; ts: number; type: "artifact.added"; artifact: RunArtifact }
  | { seq: number; ts: number; type: "turn.result"; result: RunResult }
  | { seq: number; ts: number; type: "turn.error"; error: RunError }
  // Real-transport-only (see real-client.ts's adaptControlEvent usage): the
  // backend's control event taxonomy is now a confirmed closed set (see
  // adapt-backend.ts's adaptControlEvent), but `invocation.accepted` (a
  // brand-new invocation appearing) and any future/unrecognized event_type
  // still can't be safely patched in place — those fall back to a full
  // snapshot refetch delivered as this variant. The fixture transport never
  // emits this.
  | { seq: number; ts: number; type: "snapshot.replaced"; snapshot: RunSnapshotV1 };

// Commands — signatures the real API will satisfy once codex/run-control-plane
// ships (Stage 7 swaps the fixture-backed transport for these).
export interface RunCreateCommand {
  agentId: string;
  input: unknown;
  // Required by the real transport (a Turn is created inside an existing
  // Session — there is no agent-scoped create-a-run endpoint); ignored by
  // the fixture transport, which still keys off `agentId`.
  sessionId?: string;
}

export interface RunResumeCommand {
  runId: string;
  requestId: string;
  values: Record<string, string>;
}

export interface RunApprovalDecisionCommand {
  runId: string;
  approvalId: string;
  decision: "accepted" | "rejected";
  feedback?: string;
}

export interface RunCancelCommand {
  runId: string;
}

export interface RunRetryCommand {
  runId: string;
  requestId: string;
}
