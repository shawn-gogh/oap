// Pure reducer: folds one ControlEventV1 onto a RunSnapshotV1. Used by
// RunShell to apply events arriving from `subscribeRunEvents`. Events at or
// below the snapshot's `lastEventSeq` are ignored (dedup-by-sequence, the
// rule Stage 6 requires for live reconnect — this slice doesn't do real
// reconnect yet, but the merge behavior is the same either way).

import type { ControlEventV1, RunSnapshotV1 } from "./types";

export function applyRunEvent(snapshot: RunSnapshotV1, event: ControlEventV1): RunSnapshotV1 {
  if (event.seq <= snapshot.lastEventSeq) {
    return snapshot;
  }
  const next: RunSnapshotV1 = { ...snapshot, lastEventSeq: event.seq, updatedAt: event.ts };
  switch (event.type) {
    case "turn.status_changed":
      next.status = event.status;
      return next;
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
