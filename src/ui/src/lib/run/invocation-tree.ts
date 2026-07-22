// Pure helpers for Stage 4 (execution presentation) — factored out of
// InvocationTimeline.tsx so the nesting/formatting logic is testable without
// a DOM (this repo's vitest runs in the "node" environment).

import type { RunInvocation, RunStatus } from "./types";

export interface InvocationNode {
  invocation: RunInvocation;
  children: InvocationNode[];
}

/** Nests invocations by `parentInvocationId`. Anything whose parent isn't
 * present in the list is treated as a root. Order is preserved as given. */
export function buildInvocationTree(invocations: RunInvocation[]): InvocationNode[] {
  const byId = new Map(invocations.map((invocation) => [invocation.id, invocation]));
  const childrenOf = new Map<string, RunInvocation[]>();
  const roots: RunInvocation[] = [];

  for (const invocation of invocations) {
    const parentId = invocation.parentInvocationId;
    if (parentId && byId.has(parentId)) {
      const siblings = childrenOf.get(parentId) ?? [];
      siblings.push(invocation);
      childrenOf.set(parentId, siblings);
    } else {
      roots.push(invocation);
    }
  }

  const toNode = (invocation: RunInvocation): InvocationNode => ({
    invocation,
    children: (childrenOf.get(invocation.id) ?? []).map(toNode),
  });

  return roots.map(toNode);
}

/** e.g. "12.3s" / "850ms" / null when either timestamp is missing (still
 * running, or the backend never set one). Timestamps are epoch millis. */
export function formatDuration(startedAt: number | null, endedAt: number | null): string | null {
  if (startedAt == null || endedAt == null) return null;
  const ms = endedAt - startedAt;
  if (ms < 0) return null;
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

export type StatusIconKind = "check" | "cross" | "spinner" | "waiting";

/** Maps the full RunStatus space to one of four icon treatments — kept as
 * pure, tested logic rather than inline JSX branching, since the previous
 * inline version only ever handled completed/failed/else-spinner and
 * silently mis-rendered cancelled/rejected/timed_out/waiting_* rows. */
export function statusIconKind(status: RunStatus): StatusIconKind {
  switch (status) {
    case "completed":
      return "check";
    case "failed":
    case "rejected":
    case "cancelled":
    case "timed_out":
      return "cross";
    case "waiting_input":
    case "waiting_approval":
      return "waiting";
    case "queued":
    case "running":
    case "cancelling":
      return "spinner";
  }
}
