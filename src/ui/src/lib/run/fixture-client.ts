// Fixture-backed Run transport — same function signatures Stage 7 will
// later re-point at the real codex/run-control-plane APIs. Everything here
// is in-memory only; state is keyed by `runId` and lazily cloned from the
// fixtures on first access so repeated demo interaction never mutates the
// frozen fixture objects.

import type {
  ControlEventV1,
  RunApprovalDecisionCommand,
  RunCancelCommand,
  RunCreateCommand,
  RunResumeCommand,
  RunRetryCommand,
  RunSnapshotV1,
} from "./types";
import { ALL_FIXTURES } from "./fixtures";
import { buildCompletedRunFixture } from "./fixtures/shared";
import { findRunAgentTemplate } from "./fixtures/templates";
import type { RunTransport } from "./transport";

interface RunState {
  snapshot: RunSnapshotV1;
  events: ControlEventV1[];
}

const BY_RUN_ID = new Map<string, RunState>();

function seedState(runId: string): RunState {
  const fixture = Object.values(ALL_FIXTURES).find((entry) => entry.snapshot.runId === runId);
  if (!fixture) {
    throw new Error(`no fixture registered for run id "${runId}"`);
  }
  return {
    snapshot: structuredClone(fixture.snapshot),
    events: structuredClone(fixture.events),
  };
}

function stateFor(runId: string): RunState {
  let state = BY_RUN_ID.get(runId);
  if (!state) {
    state = seedState(runId);
    BY_RUN_ID.set(runId, state);
  }
  return state;
}

let createdRunCounter = 0;

/** Stage 3's "create a Run" entry point — looks up the matching
 * `RunAgentTemplate` by `cmd.agentId`, synthesizes a completed run via the
 * same `buildCompletedRunFixture` helper the 5 provider fixtures use, then
 * overrides `inputSnapshot` with what the caller actually submitted so the
 * "preserve the immutable input snapshot after submission" rule holds. */
export async function createRun(cmd: RunCreateCommand): Promise<RunSnapshotV1> {
  const template = findRunAgentTemplate(cmd.agentId);
  if (!template) {
    throw new Error(`no agent template registered for agent id "${cmd.agentId}"`);
  }
  createdRunCounter += 1;
  const fixtureId = `created_${createdRunCounter}`;
  const { snapshot, events } = buildCompletedRunFixture({
    fixtureId,
    providerName: template.providerName,
    agentName: template.agentName,
    toolLabel: template.toolLabel,
    resultText: template.resultText,
    artifact: template.artifact,
  });
  snapshot.agentId = template.agentId;
  snapshot.inputSnapshot = cmd.input;
  snapshot.interactionProfile = {
    ...snapshot.interactionProfile,
    inputSchema: template.inputSchema,
  };
  BY_RUN_ID.set(snapshot.runId, { snapshot, events });
  return structuredClone(snapshot);
}

export async function getRunSnapshot(runId: string): Promise<RunSnapshotV1> {
  return structuredClone(stateFor(runId).snapshot);
}

/** Replays fixture events after `fromSeq` with small delays, simulating an
 * SSE stream resuming from a known sequence (Stage 6). Returns an
 * unsubscribe function that cancels any events not yet delivered. */
export function subscribeRunEvents(
  runId: string,
  fromSeq: number,
  onEvent: (event: ControlEventV1) => void,
): () => void {
  const { events } = stateFor(runId);
  const pending = events.filter((event) => event.seq > fromSeq);
  const timers = pending.map((event, index) =>
    setTimeout(() => onEvent(event), (index + 1) * 250),
  );
  return () => {
    timers.forEach(clearTimeout);
  };
}

function appendEvent(state: RunState, event: ControlEventV1): void {
  state.events.push(event);
  state.snapshot.lastEventSeq = event.seq;
}

function nextSeq(state: RunState): number {
  return state.events.length > 0 ? state.events[state.events.length - 1].seq + 1 : 1;
}

export async function submitRunInput(cmd: RunResumeCommand): Promise<RunSnapshotV1> {
  const state = stateFor(cmd.runId);
  if (state.snapshot.pendingInputRequest?.id !== cmd.requestId) {
    throw new Error("input request no longer pending");
  }
  const now = Date.now();
  appendEvent(state, { seq: nextSeq(state), ts: now, type: "input_request.resolved", requestId: cmd.requestId });
  state.snapshot.pendingInputRequest = null;
  state.snapshot.status = "running";
  appendEvent(state, { seq: nextSeq(state), ts: now + 10, type: "turn.status_changed", status: "running" });
  const result = { kind: "text" as const, text: "已收到补充信息，运行完成。" };
  appendEvent(state, { seq: nextSeq(state), ts: now + 20, type: "turn.result", result });
  state.snapshot.result = result;
  state.snapshot.status = "completed";
  state.snapshot.endedAt = now + 20;
  appendEvent(state, { seq: nextSeq(state), ts: now + 30, type: "turn.status_changed", status: "completed" });
  return structuredClone(state.snapshot);
}

export async function decideRunApproval(cmd: RunApprovalDecisionCommand): Promise<RunSnapshotV1> {
  const state = stateFor(cmd.runId);
  if (state.snapshot.pendingApproval?.id !== cmd.approvalId) {
    throw new Error("approval no longer pending");
  }
  const now = Date.now();
  appendEvent(state, {
    seq: nextSeq(state),
    ts: now,
    type: "approval.resolved",
    approvalId: cmd.approvalId,
    decision: cmd.decision,
    feedback: cmd.feedback ?? null,
  });
  state.snapshot.pendingApproval = null;
  if (cmd.decision === "rejected") {
    const error = { code: "approval_rejected", message: "操作被拒绝。", retryable: true };
    appendEvent(state, { seq: nextSeq(state), ts: now + 10, type: "turn.error", error });
    state.snapshot.error = error;
    state.snapshot.status = "rejected";
    state.snapshot.endedAt = now + 10;
    appendEvent(state, { seq: nextSeq(state), ts: now + 20, type: "turn.status_changed", status: "rejected" });
  } else {
    state.snapshot.status = "running";
    appendEvent(state, { seq: nextSeq(state), ts: now + 10, type: "turn.status_changed", status: "running" });
    const result = { kind: "text" as const, text: "已获批准并完成执行。" };
    appendEvent(state, { seq: nextSeq(state), ts: now + 20, type: "turn.result", result });
    state.snapshot.result = result;
    state.snapshot.status = "completed";
    state.snapshot.endedAt = now + 20;
    appendEvent(state, { seq: nextSeq(state), ts: now + 30, type: "turn.status_changed", status: "completed" });
  }
  return structuredClone(state.snapshot);
}

export async function cancelRun(cmd: RunCancelCommand): Promise<RunSnapshotV1> {
  const state = stateFor(cmd.runId);
  const now = Date.now();
  state.snapshot.status = "cancelled";
  state.snapshot.endedAt = now;
  state.snapshot.pendingApproval = null;
  state.snapshot.pendingInputRequest = null;
  appendEvent(state, { seq: nextSeq(state), ts: now, type: "turn.status_changed", status: "cancelled" });
  return structuredClone(state.snapshot);
}

export async function retryRun(cmd: RunRetryCommand): Promise<RunSnapshotV1> {
  const state = stateFor(cmd.runId);
  const now = Date.now();
  state.snapshot.status = "running";
  state.snapshot.error = null;
  state.snapshot.result = null;
  state.snapshot.pendingApproval = null;
  state.snapshot.pendingInputRequest = null;
  state.snapshot.endedAt = null;
  state.snapshot.startedAt = now;
  appendEvent(state, { seq: nextSeq(state), ts: now, type: "turn.status_changed", status: "running" });
  return structuredClone(state.snapshot);
}

// RunShell/RunInputForm's default `transport` prop — bundles the functions
// above to satisfy RunTransport, so existing dev-page fixture demos and
// Stage 1-3's tests keep working unmodified now that a real transport
// (real-client.ts) also exists.
export const fixtureRunTransport: RunTransport = {
  getRunSnapshot,
  subscribeRunEvents,
  submitRunInput,
  decideRunApproval,
  cancelRun,
  retryRun,
  createRun,
};
