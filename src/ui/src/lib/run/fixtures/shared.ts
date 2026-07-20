// Shared builder for the five provider "completed run" fixtures
// (a2a.ts / openapi.ts / langgraph.ts / dify.ts / crewai.ts). Every field
// except providerName/agentName/toolLabel/artifact varies identically —
// that sameness is the point: it's what lets one RunShell render all five
// without a provider branch.

import type { ControlEventV1, RunArtifact, RunSnapshotV1 } from "../types";

const BASE_TS = 1_800_000_000_000;

export interface CompletedRunFixtureInput {
  fixtureId: string;
  providerName: string;
  agentName: string;
  toolLabel: string;
  resultText: string;
  artifact: Pick<RunArtifact, "name" | "mediaType" | "inline">;
}

export interface RunFixture {
  snapshot: RunSnapshotV1;
  events: ControlEventV1[];
}

export function buildCompletedRunFixture(input: CompletedRunFixtureInput): RunFixture {
  const runId = `run_${input.fixtureId}`;
  const rootInvocationId = `inv_${input.fixtureId}_root`;
  const toolInvocationId = `inv_${input.fixtureId}_tool`;
  const artifactId = `artifact_${input.fixtureId}`;

  const rootInvocationRunning = {
    id: rootInvocationId,
    turnId: runId,
    parentInvocationId: null,
    role: "agent" as const,
    label: input.agentName,
    status: "running" as const,
    startedAt: BASE_TS + 100,
    endedAt: null,
    summary: null,
    raw: { note: "opaque provider evidence — never branched on" },
  };

  const toolInvocationRunning = {
    id: toolInvocationId,
    turnId: runId,
    parentInvocationId: rootInvocationId,
    role: "tool" as const,
    label: input.toolLabel,
    status: "running" as const,
    startedAt: BASE_TS + 400,
    endedAt: null,
    summary: null,
    raw: { note: "opaque provider evidence — never branched on" },
  };

  const toolInvocationDone = {
    ...toolInvocationRunning,
    status: "completed" as const,
    endedAt: BASE_TS + 1600,
    summary: `${input.toolLabel} 执行完成`,
  };

  const rootInvocationDone = {
    ...rootInvocationRunning,
    status: "completed" as const,
    endedAt: BASE_TS + 2000,
    summary: input.resultText,
  };

  const artifact: RunArtifact = {
    id: artifactId,
    name: input.artifact.name,
    mediaType: input.artifact.mediaType,
    sizeBytes: 1024,
    url: null,
    inline: input.artifact.inline,
  };

  const events: ControlEventV1[] = [
    { seq: 1, ts: BASE_TS, type: "turn.status_changed", status: "running" },
    { seq: 2, ts: BASE_TS + 100, type: "invocation.started", invocation: rootInvocationRunning },
    {
      seq: 3,
      ts: BASE_TS + 300,
      type: "turn.progress",
      progress: { label: "规划中", current: 1, total: 4 },
    },
    { seq: 4, ts: BASE_TS + 400, type: "invocation.started", invocation: toolInvocationRunning },
    {
      seq: 5,
      ts: BASE_TS + 800,
      type: "message.appended",
      invocationId: toolInvocationId,
      text: `正在调用 ${input.toolLabel}…`,
    },
    {
      seq: 6,
      ts: BASE_TS + 1200,
      type: "turn.progress",
      progress: { label: "执行中", current: 3, total: 4 },
    },
    { seq: 7, ts: BASE_TS + 1600, type: "invocation.updated", invocation: toolInvocationDone },
    { seq: 8, ts: BASE_TS + 1800, type: "artifact.added", artifact },
    {
      seq: 9,
      ts: BASE_TS + 2000,
      type: "turn.progress",
      progress: { label: "已完成", current: 4, total: 4 },
    },
    { seq: 10, ts: BASE_TS + 2000, type: "invocation.updated", invocation: rootInvocationDone },
    {
      seq: 11,
      ts: BASE_TS + 2100,
      type: "turn.result",
      result: { kind: "text", text: input.resultText, artifactIds: [artifactId] },
    },
    { seq: 12, ts: BASE_TS + 2200, type: "turn.status_changed", status: "completed" },
  ];

  const snapshot: RunSnapshotV1 = {
    version: "v1",
    runId,
    sessionId: `ses_${input.fixtureId}`,
    agentId: `agent_${input.fixtureId}`,
    agentName: input.agentName,
    providerName: input.providerName,
    status: "completed",
    trigger: "user",
    createdAt: BASE_TS,
    updatedAt: BASE_TS + 2200,
    startedAt: BASE_TS + 100,
    endedAt: BASE_TS + 2200,
    interactionProfile: {
      version: "v1",
      supportsCancel: true,
      supportsRetry: true,
      supportsStreaming: true,
      inputSchema: null,
      resultKinds: ["text", "artifact"],
    },
    inputSnapshot: { prompt: `请帮我处理一下 ${input.agentName} 的任务` },
    progress: { label: "已完成", current: 4, total: 4 },
    invocations: [rootInvocationDone, toolInvocationDone],
    pendingInputRequest: null,
    pendingApproval: null,
    result: { kind: "text", text: input.resultText, artifactIds: [artifactId] },
    artifacts: [artifact],
    error: null,
    lastEventSeq: 12,
  };

  return { snapshot, events };
}
