// Shared human-interaction / terminal-failure scenarios. Unlike the
// per-provider fixtures, these aren't meant to represent any specific
// provider — they exist to exercise the states RunShell must render:
// waiting on an approval, waiting on supplemental input, and a failed run.

import type { ControlEventV1, RunSnapshotV1 } from "../types";

const BASE_TS = 1_800_100_000_000;

const rootInvocationRunning = {
  id: "inv_scenario_root",
  turnId: "run_scenario",
  parentInvocationId: null,
  role: "agent" as const,
  label: "示例智能体",
  status: "running" as const,
  startedAt: BASE_TS + 100,
  endedAt: null,
  summary: null,
  raw: { note: "opaque provider evidence — never branched on" },
};

const baseSnapshot: Omit<
  RunSnapshotV1,
  "status" | "pendingApproval" | "pendingInputRequest" | "error" | "result" | "lastEventSeq"
> = {
  version: "v1",
  runId: "run_scenario",
  sessionId: "ses_scenario",
  agentId: "agent_scenario",
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
  invocations: [rootInvocationRunning],
  artifacts: [],
};

export const waitingApprovalFixture = {
  snapshot: {
    ...baseSnapshot,
    status: "waiting_approval",
    pendingApproval: {
      id: "appr_scenario",
      kind: "runtime_permission",
      title: "允许调用「发送邮件」工具？",
      body: "智能体请求发送一封会议邀请邮件给 3 位收件人。",
      requestedAt: BASE_TS + 900,
      canDecide: true,
    },
    pendingInputRequest: null,
    error: null,
    result: null,
    lastEventSeq: 4,
  } satisfies RunSnapshotV1,
  events: [
    { seq: 1, ts: BASE_TS, type: "turn.status_changed", status: "running" },
    { seq: 2, ts: BASE_TS + 100, type: "invocation.started", invocation: rootInvocationRunning },
    {
      seq: 3,
      ts: BASE_TS + 900,
      type: "approval.created",
      approval: {
        id: "appr_scenario",
        kind: "runtime_permission",
        title: "允许调用「发送邮件」工具？",
        body: "智能体请求发送一封会议邀请邮件给 3 位收件人。",
        requestedAt: BASE_TS + 900,
        canDecide: true,
      },
    },
    { seq: 4, ts: BASE_TS + 950, type: "turn.status_changed", status: "waiting_approval" },
  ] satisfies ControlEventV1[],
};

export const waitingInputFixture = {
  snapshot: {
    ...baseSnapshot,
    status: "waiting_input",
    pendingApproval: null,
    pendingInputRequest: {
      id: "req_scenario",
      requestedAt: BASE_TS + 900,
      prompt: "会议室容量不够，需要补充参会人数和优先楼层。",
      fields: [
        { id: "attendee_count", label: "参会人数", kind: "text", required: true },
        {
          id: "floor",
          label: "优先楼层",
          kind: "choice",
          required: false,
          choices: ["不限", "3 楼", "5 楼"],
        },
      ],
    },
    error: null,
    result: null,
    lastEventSeq: 4,
  } satisfies RunSnapshotV1,
  events: [
    { seq: 1, ts: BASE_TS, type: "turn.status_changed", status: "running" },
    { seq: 2, ts: BASE_TS + 100, type: "invocation.started", invocation: rootInvocationRunning },
    {
      seq: 3,
      ts: BASE_TS + 900,
      type: "input_request.created",
      request: {
        id: "req_scenario",
        requestedAt: BASE_TS + 900,
        prompt: "会议室容量不够，需要补充参会人数和优先楼层。",
        fields: [
          { id: "attendee_count", label: "参会人数", kind: "text", required: true },
          {
            id: "floor",
            label: "优先楼层",
            kind: "choice",
            required: false,
            choices: ["不限", "3 楼", "5 楼"],
          },
        ],
      },
    },
    { seq: 4, ts: BASE_TS + 950, type: "turn.status_changed", status: "waiting_input" },
  ] satisfies ControlEventV1[],
};

const failedToolInvocation = {
  id: "inv_scenario_tool",
  turnId: "run_scenario",
  parentInvocationId: "inv_scenario_root",
  role: "tool" as const,
  label: "预订会议室",
  status: "failed" as const,
  startedAt: BASE_TS + 400,
  endedAt: BASE_TS + 1200,
  summary: "会议室预订接口返回 409 冲突。",
  raw: { note: "opaque provider evidence — never branched on" },
};

export const failedFixture = {
  snapshot: {
    ...baseSnapshot,
    status: "failed",
    endedAt: BASE_TS + 1300,
    invocations: [
      { ...rootInvocationRunning, status: "failed", endedAt: BASE_TS + 1300 },
      failedToolInvocation,
    ],
    pendingApproval: null,
    pendingInputRequest: null,
    error: {
      code: "upstream_conflict",
      message: "会议室预订接口返回 409 冲突：该时段已被占用。",
      retryable: true,
    },
    result: null,
    lastEventSeq: 6,
  } satisfies RunSnapshotV1,
  events: [
    { seq: 1, ts: BASE_TS, type: "turn.status_changed", status: "running" },
    { seq: 2, ts: BASE_TS + 100, type: "invocation.started", invocation: rootInvocationRunning },
    {
      seq: 3,
      ts: BASE_TS + 400,
      type: "invocation.started",
      invocation: { ...failedToolInvocation, status: "running", endedAt: null, summary: null },
    },
    { seq: 4, ts: BASE_TS + 1200, type: "invocation.updated", invocation: failedToolInvocation },
    {
      seq: 5,
      ts: BASE_TS + 1250,
      type: "turn.error",
      error: {
        code: "upstream_conflict",
        message: "会议室预订接口返回 409 冲突：该时段已被占用。",
        retryable: true,
      },
    },
    { seq: 6, ts: BASE_TS + 1300, type: "turn.status_changed", status: "failed" },
  ] satisfies ControlEventV1[],
};
