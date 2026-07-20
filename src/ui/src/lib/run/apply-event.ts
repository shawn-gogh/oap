// Pure reducer: folds one ControlEventV1 onto a RunSnapshotV1. Used by
// RunShell to apply events arriving from `subscribeRunEvents`. Events at or
// below the snapshot's `lastEventSeq` are ignored (dedup-by-sequence, the
// rule Stage 6 requires for live reconnect — this slice doesn't do real
// reconnect yet, but the merge behavior is the same either way).

import type { ControlEventV1, RunSnapshotV1, RunStatus } from "./types";

const TERMINAL_INVOCATION_STATUSES = new Set<RunStatus>([
  "completed",
  "failed",
  "rejected",
  "cancelled",
  "timed_out",
]);

export function applyRunEvent(snapshot: RunSnapshotV1, event: ControlEventV1): RunSnapshotV1 {
  if (event.seq <= snapshot.lastEventSeq) {
    return snapshot;
  }
  const next: RunSnapshotV1 = { ...snapshot, lastEventSeq: event.seq, updatedAt: event.ts };
  switch (event.type) {
    case "turn.status_changed": {
      next.status = event.status;
      if ("error" in event) next.error = event.error ?? null;
      // Real-transport-only cascade, restricted to terminal turn statuses:
      // the backend's own turn transition (session_control::repository::
      // transition()) updates every non-terminal invocation under the turn
      // to the turn's new status in the SAME SQL statement — WITHOUT
      // emitting any invocation.* event for that cascade. A runtime with a
      // single "primary" invocation and no independently-managed sub-agents
      // (confirmed live: local-opencode via claude_managed_agents) therefore
      // only ever produces turn.* events — without mirroring this cascade
      // here, its invocation row would stay stuck on its last-seen status
      // (e.g. "running") forever once the turn reaches a terminal state.
      //
      // The backend's SQL actually cascades on *every* transition, not just
      // terminal ones, but this is deliberately narrower: the fixture
      // scenarios (waitingApprovalFixture/waitingInputFixture) intentionally
      // keep an invocation "running" while its turn is merely
      // waiting_input/waiting_approval — a defensible display choice (the
      // invocation genuinely is still active, just paused on a human) that
      // a blanket cascade would overwrite. Terminal statuses carry no such
      // ambiguity: an invocation cannot still be "running" once its turn is
      // done. Already-terminal invocations (a tool call that finished
      // independently before the turn did) are left untouched either way,
      // matching the backend's own `WHERE status NOT IN (...)`.
      if (TERMINAL_INVOCATION_STATUSES.has(event.status)) {
        next.invocations = next.invocations.map((inv) =>
          TERMINAL_INVOCATION_STATUSES.has(inv.status)
            ? inv
            : { ...inv, status: event.status, endedAt: inv.endedAt ?? event.ts },
        );
      }
      return next;
    }
    case "turn.progress":
      next.progress = event.progress;
      return next;
    case "invocation.started":
    case "invocation.updated": {
      const existingIndex = next.invocations.findIndex((inv) => inv.id === event.invocation.id);
      next.invocations =
        existingIndex >= 0
          ? next.invocations.map((inv, index) => (index === existingIndex ? event.invocation : inv))
          : [...next.invocations, event.invocation];
      return next;
    }
    case "invocation.status_changed": {
      const existingIndex = next.invocations.findIndex((inv) => inv.id === event.invocationId);
      if (existingIndex < 0) return next;
      const invocation = next.invocations[existingIndex];
      const isTerminal = TERMINAL_INVOCATION_STATUSES.has(event.status);
      next.invocations = next.invocations.map((inv, index) =>
        index === existingIndex
          ? {
              ...invocation,
              status: event.status,
              startedAt: event.status === "running" ? (invocation.startedAt ?? event.ts) : invocation.startedAt,
              endedAt: isTerminal ? (invocation.endedAt ?? event.ts) : invocation.endedAt,
            }
          : inv,
      );
      return next;
    }
    case "message.appended": {
      const existingIndex = next.invocations.findIndex((inv) => inv.id === event.invocationId);
      if (existingIndex < 0) return next;
      const invocation = next.invocations[existingIndex];
      next.invocations = next.invocations.map((inv, index) =>
        index === existingIndex ? { ...invocation, summary: event.text } : inv,
      );
      return next;
    }
    case "input_request.created":
      next.pendingInputRequest = event.request;
      return next;
    case "input_request.resolved":
      if (next.pendingInputRequest?.id === event.requestId) next.pendingInputRequest = null;
      return next;
    case "approval.created":
      next.pendingApproval = event.approval;
      return next;
    case "approval.resolved":
      if (next.pendingApproval?.id === event.approvalId) next.pendingApproval = null;
      return next;
    case "artifact.added":
      next.artifacts = [...next.artifacts, event.artifact];
      return next;
    case "turn.result":
      next.result = event.result;
      return next;
    case "turn.error":
      next.error = event.error;
      return next;
    case "snapshot.replaced":
      // Real-transport-only: the event *is* the new authoritative snapshot
      // (see the type's doc comment in types.ts) — replace wholesale rather
      // than patching individual fields.
      return { ...event.snapshot, lastEventSeq: event.seq };
    default:
      return next;
  }
}
