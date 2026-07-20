"use client";

import { Dialog, DialogContent } from "@/components/ui/dialog";
import { createRealRunTransport } from "@/lib/run/real-client";
import { RunShell } from "./RunShell";

// Stage 7 of docs/engineering/run-surface-branch-plan.mdx: a reusable "open
// this Run" surface for entry points that already know a sessionId+turnId
// (Chat's active turn, an Inbox item's resolved active turn) — a Dialog
// wide/tall enough to comfortably host RunShell rather than a full page
// navigation, matching the plan's "Chat keeps a Run drawer" note.

export function RunDrawer({
  sessionId,
  turnId,
  open,
  onOpenChange,
}: {
  sessionId: string;
  turnId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-2xl">
        <RunShell runId={turnId} transport={createRealRunTransport(sessionId)} />
      </DialogContent>
    </Dialog>
  );
}
