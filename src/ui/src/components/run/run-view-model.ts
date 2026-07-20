// Pure derivation from a RunSnapshotV1 into presentational fields —
// factored out of RunShell.tsx so it's testable without a DOM/rendering
// environment (this repo's vitest config runs in the "node" environment; no
// @testing-library/react is installed). RunShell.test.tsx exercises this
// module directly against every fixture.
//
// Nothing here reads `providerName` for anything but display — see the
// module doc comment in lib/run/types.ts.

import type { StatusDotTone } from "@/components/status-dot";
import type { RunSnapshotV1, RunStatus } from "@/lib/run/types";

export const RUN_STATUS_LABELS: Record<RunStatus, string> = {
  queued: "排队中",
  running: "运行中",
  waiting_input: "等待补充输入",
  waiting_approval: "等待审批",
  cancelling: "取消中",
  completed: "已完成",
  failed: "失败",
  rejected: "已拒绝",
  cancelled: "已取消",
  timed_out: "已超时",
};

export const RUN_STATUS_TONES: Record<RunStatus, StatusDotTone> = {
  queued: "idle",
  running: "success",
  waiting_input: "warning",
  waiting_approval: "warning",
  cancelling: "warning",
  completed: "success",
  failed: "error",
  rejected: "error",
  cancelled: "idle",
  timed_out: "error",
};

const TRIGGER_LABELS: Record<RunSnapshotV1["trigger"], string> = {
  user: "用户发起",
  schedule: "定时任务",
  webhook: "Webhook",
  resume: "续接会话",
  retry: "重试",
};

const TERMINAL_STATUSES = new Set<RunStatus>([
  "completed",
  "failed",
  "rejected",
  "cancelled",
  "timed_out",
]);

export interface RunView {
  runId: string;
  title: string;
  providerLabel: string | null;
  statusLabel: string;
  statusTone: StatusDotTone;
  triggerLabel: string;
  isTerminal: boolean;
  canCancel: boolean;
  canRetry: boolean;
  progress: RunSnapshotV1["progress"];
  invocations: RunSnapshotV1["invocations"];
  pendingInputRequest: RunSnapshotV1["pendingInputRequest"];
  pendingApproval: RunSnapshotV1["pendingApproval"];
  result: RunSnapshotV1["result"];
  artifacts: RunSnapshotV1["artifacts"];
  error: RunSnapshotV1["error"];
  inputSnapshot: unknown;
}

export function buildRunView(snapshot: RunSnapshotV1): RunView {
  const isTerminal = TERMINAL_STATUSES.has(snapshot.status);
  return {
    runId: snapshot.runId,
    title: snapshot.agentName,
    providerLabel: snapshot.providerName,
    statusLabel: RUN_STATUS_LABELS[snapshot.status],
    statusTone: RUN_STATUS_TONES[snapshot.status],
    triggerLabel: TRIGGER_LABELS[snapshot.trigger],
    isTerminal,
    canCancel: !isTerminal && snapshot.interactionProfile.supportsCancel,
    canRetry: isTerminal && snapshot.interactionProfile.supportsRetry,
    progress: snapshot.progress,
    invocations: snapshot.invocations,
    pendingInputRequest: snapshot.pendingInputRequest,
    pendingApproval: snapshot.pendingApproval,
    result: snapshot.result,
    artifacts: snapshot.artifacts,
    error: snapshot.error,
    inputSnapshot: snapshot.inputSnapshot,
  };
}
