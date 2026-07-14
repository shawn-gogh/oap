"use client";

import { useEffect } from "react";
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

/**
 * Docks the pending approval above the composer instead of inline in the
 * message stream, so it stays visible without scrolling. Shows one approval
 * at a time; a badge surfaces the rest of the queue. Y approves / N rejects
 * the current approval when focus is outside an input.
 */
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
      } else if (event.key === "n" || event.key === "N") {
        event.preventDefault();
        onReject(current.id, "");
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [current, busy, onAccept, onReject]);

  if (approvals.length === 0) return null;
  const rest = approvals.slice(1);

  return (
    <div className="border-t border-border bg-background/95 px-6 py-3 backdrop-blur">
      <div className="mx-auto flex max-h-[45vh] w-full max-w-5xl flex-col overflow-y-auto">
        {rest.length > 0 && (
          <div className="mb-2 flex items-center gap-1.5 text-[11px] font-medium text-amber-600 dark:text-amber-400">
            <span className="size-1.5 rounded-full bg-amber-500" />
            还有 {rest.length} 条待审批
          </div>
        )}
        <ToolApprovalPanel
          key={current.id}
          approval={current}
          onAccept={onAccept}
          onReject={onReject}
          onAcceptAlways={onAcceptAlways}
          busy={busy}
        />
      </div>
    </div>
  );
}
