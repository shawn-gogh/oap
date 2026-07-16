"use client";

import { useEffect } from "react";
import { CircleAlert, Layers3 } from "lucide-react";
import { ToolApprovalPanel } from "@/components/tool-approval-panel";
import type { PendingApproval } from "@/lib/api";

export interface ApprovalDockProps {
  approvals: PendingApproval[];
  onAccept: (id: string, args: Record<string, unknown>) => void;
  onReject: (id: string, feedback: string) => void;
  onAcceptAlways?: (id: string, args: Record<string, unknown>) => void;
  busy?: boolean;
}

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target.isContentEditable;
}

export function ApprovalDock({ approvals, onAccept, onReject, onAcceptAlways, busy }: ApprovalDockProps) {
  const current = approvals[0];

  useEffect(() => {
    if (!current) return;
    const handler = (event: KeyboardEvent) => {
      if (busy || event.metaKey || event.ctrlKey || event.altKey) return;
      if (isTypingTarget(event.target)) return;
      if (event.key === "y" || event.key === "Y") {
        event.preventDefault();
        onAccept(current.id, current.arguments ?? {});
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [current, busy, onAccept, onReject]);

  if (approvals.length === 0) return null;

  return (
    <div
      className="flex h-[min(52dvh,36rem)] min-h-72 shrink-0 flex-col border-t border-border bg-background/95 px-4 py-3 backdrop-blur sm:px-6"
      aria-live="polite"
    >
      <div className="mx-auto flex min-h-0 w-full max-w-5xl flex-1 flex-col">
        <div className="mb-2 shrink-0 flex items-center gap-2 px-1 text-xs text-muted-foreground">
          <CircleAlert className="size-3.5 text-amber-600 dark:text-amber-400" />
          <span className="font-medium text-foreground">智能体已暂停，等待审批</span>
          {approvals.length > 1 && (
            <span className="ml-auto inline-flex items-center gap-1 rounded-full border border-border bg-card px-2 py-0.5 font-medium">
              <Layers3 className="size-3" />
              1 / {approvals.length}
            </span>
          )}
        </div>
        <ToolApprovalPanel
          key={current.id}
          approval={current}
          onAccept={onAccept}
          onReject={onReject}
          onAcceptAlways={onAcceptAlways}
          busy={busy}
          canDecide={current.canDecide}
        />
      </div>
    </div>
  );
}
