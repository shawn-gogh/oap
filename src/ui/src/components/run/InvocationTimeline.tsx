"use client";

import { useState } from "react";
import { CheckCircle2, ChevronRight, CircleDashed, Loader2, XCircle } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import { buildInvocationTree, formatDuration, statusIconKind, type InvocationNode } from "@/lib/run/invocation-tree";
import type { RunInvocation } from "@/lib/run/types";

// Stage 4 of docs/engineering/run-surface-branch-plan.mdx: ordered steps,
// current/failed stages, parent/child Invocations, expandable details, and
// a raw-event inspector. Replaces RunShell's previous flat <ol> block.
// `buildInvocationTree` degrades to today's flat list when every row's
// `parentInvocationId` is null (the real backend's current shape), so this
// is a pure addition for fixtures/future backends with real hierarchy, not
// a regression for what's live today.

const STATUS_ICON = {
  check: <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-emerald-600 dark:text-emerald-400" />,
  cross: <XCircle className="mt-0.5 size-4 shrink-0 text-destructive" />,
  spinner: <Loader2 className="mt-0.5 size-4 shrink-0 animate-spin text-muted-foreground" />,
  waiting: <CircleDashed className="mt-0.5 size-4 shrink-0 text-amber-600 dark:text-amber-400" />,
};

export function InvocationTimeline({ invocations }: { invocations: RunInvocation[] }) {
  const tree = buildInvocationTree(invocations);
  return (
    <ol className="grid gap-2">
      {tree.map((node) => (
        <InvocationRow key={node.invocation.id} node={node} depth={0} />
      ))}
    </ol>
  );
}

function InvocationRow({ node, depth }: { node: InvocationNode; depth: number }) {
  const [showRaw, setShowRaw] = useState(false);
  const { invocation, children } = node;
  const isFailed = invocation.status === "failed";
  const duration = formatDuration(invocation.startedAt, invocation.endedAt);

  return (
    <li style={depth > 0 ? { marginLeft: depth * 20 } : undefined}>
      <div
        className={cn(
          "flex items-start gap-2 rounded-md border px-3 py-2 text-sm",
          isFailed ? "border-destructive/40 bg-destructive/5" : "border-border",
        )}
      >
        {STATUS_ICON[statusIconKind(invocation.status)]}
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-medium">{invocation.label}</span>
            <Badge variant="outline" className="text-[10px]">
              {invocation.role === "agent" ? "智能体" : "工具"}
            </Badge>
            {duration && <span className="text-xs text-muted-foreground">{duration}</span>}
            <button
              type="button"
              onClick={() => setShowRaw((current) => !current)}
              className="ml-auto flex shrink-0 items-center gap-0.5 text-xs text-muted-foreground hover:text-foreground"
            >
              <ChevronRight className={cn("size-3 transition-transform", showRaw && "rotate-90")} />
              详情
            </button>
          </div>
          {invocation.summary && (
            <p className="mt-0.5 text-xs text-muted-foreground">{invocation.summary}</p>
          )}
          {showRaw && (
            <pre className="mt-2 overflow-x-auto rounded-md bg-muted/40 p-2 text-[11px]">
              {JSON.stringify(invocation.raw, null, 2)}
            </pre>
          )}
        </div>
      </div>
      {children.length > 0 && (
        <ol className="mt-2 grid gap-2">
          {children.map((child) => (
            <InvocationRow key={child.invocation.id} node={child} depth={depth + 1} />
          ))}
        </ol>
      )}
    </li>
  );
}
