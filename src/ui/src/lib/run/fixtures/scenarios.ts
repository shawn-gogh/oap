// Shared human-interaction / terminal-failure scenarios. Unlike the
// per-provider fixtures, these aren't meant to represent any specific
// provider — they exist to exercise the states RunShell must render:
// waiting on an approval, waiting on supplemental input, and a failed run.
// Each scenario gets its own runId/invocation ids (all three used to share
// "run_scenario", which meant the fixture-backed transport could only ever
// resolve one of them — caught by RunShell.test.tsx).

import type { ControlEventV1, RunSnapshotV1 } from "../types";

const BASE_TS = 1_800_100_000_000;

function rootInvocation(suffix: string) {
  return {
    id: `inv_scenario_${suffix}_root`,
    turnId: `run_scenario_${suffix}`,
    parentInvocationId: null,
    role: "agent" as const,
    label: "示例智能体",
    status: "running" as const,
    startedAt: BASE_TS + 100,
    endedAt: null,
    summary: null,
    raw: { note: "opaque provider evidence — never branched on" },
  };
}

function baseSnapshot(
  suffix: string,
): Omit<RunSnapshotV1, "status" | "pendingApproval" | "pendingInputRequest" | "error" | "result" | "lastEventSeq"> {
  return {
    version: "v1",
    runId: `run_scenario_${suffix}`,
    sessionId: `ses_scenario_${suffix}`,
    agentId: `agent_scenario_${suffix}`,
    agentName: "示例智能体",
    providerName: "a2a",
    trigger: "user",
    createdAt: BASE_TS,
    updatedAt: BASE_TS + 1000,
    startedAt: BASE_TS + 100,
    endedAt: null,
    interactionProfile: {
      version: "v1",
      supportsCancel: true,
      supportsRetry: true,
      supportsStreaming: true,
      inputSchema: null,
      resultKinds: ["text"],
    },
    inputSnapshot: { prompt: "请帮我预订下周三的会议室" },
    progress: { label: "等待处理", current: 1, total: 3 },
    invocations: [rootInvocation(suffix)],
    operations: [],
    artifacts: [],
  };
}

const waApproval = {
  id: "appr_scenario",
  kind: "runtime_permission",
  title: "允许调用「发送邮件」工具？",
  body: "智能体请求发送一封会议邀请邮件给 3 位收件人。",
  requestedAt: BASE_TS + 900,
  canDecide: true,
};

export const waitingApprovalFixture = {
  snapshot: {
    ...baseSnapshot("wa"),
    status: "waiting_approval",
    pendingApproval: waApproval,
    pendingInputRequest: null,
    error: null,
    result: null,
    lastEventSeq: 4,
  } satisfies RunSnapshotV1,
  events: [
    { seq: 1, ts: BASE_TS, type: "turn.status_changed", status: "running" },
    { seq: 2, ts: BASE_TS + 100, type: "invocation.started", invocation: rootInvocation("wa") },
    { seq: 3, ts: BASE_TS + 900, type: "approval.created", approval: waApproval },
    { seq: 4, ts: BASE_TS + 950, type: "turn.status_changed", status: "waiting_approval" },
  ] satisfies ControlEventV1[],
};

const wiRequest = {
  id: "req_scenario",
  requestedAt: BASE_TS + 900,
  prompt: "会议室容量不够，需要补充参会人数和优先楼层。",
  fields: [
    { id: "attendee_count", label: "参会人数", kind: "text" as const, required: true },
    {
      id: "floor",
      label: "优先楼层",
      kind: "choice" as const,
      required: false,
      choices: ["不限", "3 楼", "5 楼"],
    },
  ],
};

export const waitingInputFixture = {
  snapshot: {
    ...baseSnapshot("wi"),
    status: "waiting_input",
    pendingApproval: null,
    pendingInputRequest: wiRequest,
    error: null,
    result: null,
    lastEventSeq: 4,
  } satisfies RunSnapshotV1,
  events: [
    { seq: 1, ts: BASE_TS, type: "turn.status_changed", status: "running" },
    { seq: 2, ts: BASE_TS + 100, type: "invocation.started", invocation: rootInvocation("wi") },
    { seq: 3, ts: BASE_TS + 900, type: "input_request.created", request: wiRequest },
    { seq: 4, ts: BASE_TS + 950, type: "turn.status_changed", status: "waiting_input" },
  ] satisfies ControlEventV1[],
};

const failedToolInvocationRunning = {
  id: "inv_scenario_failed_tool",
  turnId: "run_scenario_failed",
  parentInvocationId: "inv_scenario_failed_root",
  role: "tool" as const,
  label: "预订会议室",
  status: "running" as const,
  startedAt: BASE_TS + 400,
  endedAt: null,
  summary: null,
  raw: { note: "opaque provider evidence — never branched on" },
};

const failedToolInvocationDone = {
  ...failedToolInvocationRunning,
  status: "failed" as const,
  endedAt: BASE_TS + 1200,
  summary: "会议室预订接口返回 409 冲突。",
};

const failedRootInvocationRunning = rootInvocation("failed");
const failedRootInvocationDone = {
  ...failedRootInvocationRunning,
  status: "failed" as const,
  endedAt: BASE_TS + 1300,
};

const failedError = {
  code: "upstream_conflict",
  message: "会议室预订接口返回 409 冲突：该时段已被占用。",
  retryable: true,
};

export const failedFixture = {
  snapshot: {
    ...baseSnapshot("failed"),
    status: "failed",
    endedAt: BASE_TS + 1300,
    invocations: [failedRootInvocationDone, failedToolInvocationDone],
    pendingApproval: null,
    pendingInputRequest: null,
    error: failedError,
    result: null,
    lastEventSeq: 7,
  } satisfies RunSnapshotV1,
  events: [
    { seq: 1, ts: BASE_TS, type: "turn.status_changed", status: "running" },
    { seq: 2, ts: BASE_TS + 100, type: "invocation.started", invocation: failedRootInvocationRunning },
    { seq: 3, ts: BASE_TS + 400, type: "invocation.started", invocation: failedToolInvocationRunning },
    { seq: 4, ts: BASE_TS + 1200, type: "invocation.updated", invocation: failedToolInvocationDone },
    { seq: 5, ts: BASE_TS + 1250, type: "turn.error", error: failedError },
    { seq: 6, ts: BASE_TS + 1300, type: "invocation.updated", invocation: failedRootInvocationDone },
    { seq: 7, ts: BASE_TS + 1300, type: "turn.status_changed", status: "failed" },
  ] satisfies ControlEventV1[],
};
