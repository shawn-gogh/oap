"use client";

import { ToolApprovalPanel } from "@/components/tool-approval-panel";
import type { PendingApproval } from "@/lib/api";

export interface ApprovalDockProps {
  approvals: PendingApproval[];
  onAccept: (id: string, args: Record<string, unknown>) => void;
  onReject: (id: string, feedback: string) => void;
  busy?: boolean;
}

/**
 * Docks the pending approval above the composer instead of inline in the
 * message stream, so it stays visible without scrolling. Shows one approval
 * at a time; a badge surfaces the rest of the queue.
 */
export function ApprovalDock({ approvals, onAccept, onReject, busy }: ApprovalDockProps) {
  if (approvals.length === 0) return null;
  const [current, ...rest] = approvals;

  return (
    <div className="border-t border-border bg-background/95 px-6 py-3 backdrop-blur">
      <div className="mx-auto flex max-h-[45vh] w-full max-w-5xl flex-col overflow-y-auto">
        {rest.length > 0 && (
          <div className="mb-2 flex items-center gap-1.5 text-[11px] font-medium text-amber-600 dark:text-amber-400">
            <span className="size-1.5 rounded-full bg-amber-500" />
            还有 {rest.length} 条待审批
          </div>
        )}
        <ToolApprovalPanel key={current.id} approval={current} onAccept={onAccept} onReject={onReject} busy={busy} />
      </div>
    </div>
  );
}
