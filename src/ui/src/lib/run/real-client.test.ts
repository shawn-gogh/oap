import { beforeEach, describe, expect, it, vi } from "vitest";

// Mocked at the @/lib/api boundary rather than global fetch — real-client.ts
// calls these exported functions, not fetch directly, so asserting on their
// call arguments is the accurate seam (and avoids re-deriving BASE/auth
// header internals that belong to api.ts, not this module).
vi.mock("@/lib/api", () => ({
  createRunTurn: vi.fn(),
  getRunTurn: vi.fn(),
  resumeRunTurn: vi.fn(),
  retryRunTurn: vi.fn(),
  cancelTurn: vi.fn(),
  getRunArtifact: vi.fn(),
  acceptApproval: vi.fn(),
  rejectApproval: vi.fn(),
  subscribeControlEvents: vi.fn(),
}));

import * as api from "@/lib/api";
import { createRealRunTransport } from "./real-client";
import type { BackendRunSnapshotV1 } from "./backend-types";

const BACKEND_SNAPSHOT: BackendRunSnapshotV1 = {
  schema_version: 1,
  turn: {
    id: "turn_1",
    session_id: "ses_1",
    request_id: "req_1",
    status: "completed",
  },
  interaction_profile: {
    schema_version: 1,
    primary_surface: "run",
    execution_mode: "async_stream",
    input_schema: {},
    output_schema: {},
    progress_mode: "none",
    continuation_modes: [],
    accepted_input_types: [],
    artifact_media_types: [],
    supports_retry: true,
    supports_checkpoint_resume: false,
    supports_child_invocations: false,
  },
  input: { topic: "x" },
  result: "done",
  invocations: [],
  operations: [],
  pending_requests: [],
  artifacts: [],
  latest_sequence: 3,
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.getRunTurn).mockResolvedValue(BACKEND_SNAPSHOT);
  vi.mocked(api.createRunTurn).mockResolvedValue(BACKEND_SNAPSHOT);
  vi.mocked(api.resumeRunTurn).mockResolvedValue(BACKEND_SNAPSHOT);
  vi.mocked(api.retryRunTurn).mockResolvedValue(BACKEND_SNAPSHOT);
  vi.mocked(api.cancelTurn).mockResolvedValue({
    turn: BACKEND_SNAPSHOT.turn,
    invocations: [],
  } as never);
});

describe("createRealRunTransport", () => {
  it("getRunSnapshot calls getRunTurn with the bound sessionId and adapts the response", async () => {
    const transport = createRealRunTransport("ses_1");
    const snapshot = await transport.getRunSnapshot("turn_1");
    expect(api.getRunTurn).toHaveBeenCalledWith("ses_1", "turn_1");
    expect(snapshot.runId).toBe("turn_1");
    expect(snapshot.result).toEqual({ kind: "text", text: "done" });
  });

  it("createRun requires sessionId and calls createRunTurn with the structured input", async () => {
    const transport = createRealRunTransport("ses_1");
    await transport.createRun({ agentId: "agent_x", input: { topic: "x" }, sessionId: "ses_1" });
    expect(api.createRunTurn).toHaveBeenCalledWith("ses_1", { input: { topic: "x" } });
  });

  it("createRun rejects when sessionId is missing", async () => {
    const transport = createRealRunTransport("ses_1");
    await expect(
      transport.createRun({ agentId: "agent_x", input: {} }),
    ).rejects.toThrow(/sessionId/);
  });

  it("submitRunInput calls resumeRunTurn with the known-gap generic input wrapper", async () => {
    const transport = createRealRunTransport("ses_1");
    await transport.submitRunInput({ runId: "turn_1", requestId: "req_x", values: { input: "answer" } });
    expect(api.resumeRunTurn).toHaveBeenCalledWith("ses_1", "turn_1", {
      request_id: "req_x",
      input: { input: "answer" },
    });
  });

  it("cancelRun calls cancelTurn then re-fetches the full snapshot (asymmetric response)", async () => {
    const transport = createRealRunTransport("ses_1");
    const snapshot = await transport.cancelRun({ runId: "turn_1" });
    expect(api.cancelTurn).toHaveBeenCalledWith("ses_1", "turn_1");
    expect(api.getRunTurn).toHaveBeenCalledWith("ses_1", "turn_1");
    expect(snapshot.runId).toBe("turn_1");
  });

  it("retryRun calls retryRunTurn and adapts the new turn's snapshot", async () => {
    const transport = createRealRunTransport("ses_1");
    const snapshot = await transport.retryRun({ runId: "turn_1" });
    expect(api.retryRunTurn).toHaveBeenCalledWith("ses_1", "turn_1");
    expect(snapshot.runId).toBe("turn_1");
  });

  it("decideRunApproval(accepted) calls acceptApproval then re-fetches the snapshot", async () => {
    const transport = createRealRunTransport("ses_1");
    await transport.decideRunApproval({ runId: "turn_1", approvalId: "appr_1", decision: "accepted" });
    expect(api.acceptApproval).toHaveBeenCalledWith("appr_1");
    expect(api.getRunTurn).toHaveBeenCalledWith("ses_1", "turn_1");
  });

  it("decideRunApproval(rejected) calls rejectApproval with feedback then re-fetches", async () => {
    const transport = createRealRunTransport("ses_1");
    await transport.decideRunApproval({
      runId: "turn_1",
      approvalId: "appr_1",
      decision: "rejected",
      feedback: "no thanks",
    });
    expect(api.rejectApproval).toHaveBeenCalledWith("appr_1", "no thanks");
  });

  it("subscribeRunEvents wires subscribeControlEvents, and patches a turn.* frame in place without refetching", async () => {
    let capturedOnFrame: ((lastEventId: string | null, data: unknown) => void) | undefined;
    const unsubscribe = vi.fn();
    vi.mocked(api.subscribeControlEvents).mockImplementation((opts) => {
      capturedOnFrame = opts.onFrame;
      return unsubscribe;
    });

    const transport = createRealRunTransport("ses_1");
    const onEvent = vi.fn();
    const stop = transport.subscribeRunEvents("turn_1", 5, onEvent);

    expect(api.subscribeControlEvents).toHaveBeenCalledWith(
      expect.objectContaining({ sessionId: "ses_1", afterSequence: 5 }),
    );
    expect(capturedOnFrame).toBeDefined();

    capturedOnFrame!("7", {
      schema_version: 1,
      type: "turn.completed",
      sequence: 7,
      session_id: "ses_1",
      turn_id: "turn_1",
      invocation_id: null,
      request_id: "req_1",
      occurred_at: 123,
      payload: { status: "completed", error: null },
    });

    expect(onEvent).toHaveBeenCalledWith({
      seq: 7,
      ts: 123,
      type: "turn.status_changed",
      status: "completed",
      error: null,
    });
    // The whole point of Stage 6's fine-grained handling: a patchable frame
    // never triggers the fallback refetch.
    expect(api.getRunTurn).not.toHaveBeenCalled();

    stop();
    expect(unsubscribe).toHaveBeenCalled();
  });

  it("subscribeRunEvents falls back to a full refetch for invocation.accepted (payload can't rebuild a full row)", async () => {
    let capturedOnFrame: ((lastEventId: string | null, data: unknown) => void) | undefined;
    vi.mocked(api.subscribeControlEvents).mockImplementation((opts) => {
      capturedOnFrame = opts.onFrame;
      return vi.fn();
    });

    const transport = createRealRunTransport("ses_1");
    const onEvent = vi.fn();
    transport.subscribeRunEvents("turn_1", 5, onEvent);

    capturedOnFrame!("8", {
      schema_version: 1,
      type: "invocation.accepted",
      sequence: 8,
      session_id: "ses_1",
      turn_id: "turn_1",
      invocation_id: "inv_new",
      request_id: null,
      occurred_at: 456,
      payload: { status: "queued", parent_invocation_id: null, role: "tool" },
    });

    await vi.waitFor(() => {
      expect(onEvent).toHaveBeenCalled();
    });

    expect(api.getRunTurn).toHaveBeenCalledWith("ses_1", "turn_1");
    expect(onEvent).toHaveBeenCalledWith(expect.objectContaining({ seq: 8, type: "snapshot.replaced" }));
  });
});
