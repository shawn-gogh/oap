import { describe, expect, it } from "vitest";

import { applyRunEvent } from "./apply-event";
import { ALL_FIXTURES, FIXTURE_IDS } from "./fixtures";
import type { RunSnapshotV1 } from "./types";

function emptyRunFromSnapshot(snapshot: RunSnapshotV1): RunSnapshotV1 {
  return {
    ...snapshot,
    status: "queued",
    progress: null,
    invocations: [],
    operations: [],
    pendingInputRequest: null,
    pendingApproval: null,
    result: null,
    artifacts: [],
    error: null,
    lastEventSeq: 0,
  };
}

describe("applyRunEvent", () => {
  it.each(FIXTURE_IDS)("replaying fixture %s's events reconstructs its final snapshot", (id) => {
    const fixture = ALL_FIXTURES[id];
    const replayed = fixture.events.reduce(applyRunEvent, emptyRunFromSnapshot(fixture.snapshot));
    expect(replayed.status).toBe(fixture.snapshot.status);
    expect(replayed.lastEventSeq).toBe(fixture.snapshot.lastEventSeq);
    expect(replayed.invocations).toEqual(fixture.snapshot.invocations);
    expect(replayed.pendingApproval).toEqual(fixture.snapshot.pendingApproval);
    expect(replayed.pendingInputRequest).toEqual(fixture.snapshot.pendingInputRequest);
    expect(replayed.result).toEqual(fixture.snapshot.result);
    expect(replayed.artifacts).toEqual(fixture.snapshot.artifacts);
    expect(replayed.error).toEqual(fixture.snapshot.error);
  });

  it("ignores events at or below the current lastEventSeq (dedup)", () => {
    const fixture = ALL_FIXTURES.a2a;
    const alreadyCaughtUp = fixture.snapshot;
    const stale = fixture.events[0];
    const result = applyRunEvent(alreadyCaughtUp, stale);
    expect(result).toBe(alreadyCaughtUp);
  });

  it("replaces the snapshot wholesale on a real-transport snapshot.replaced event", () => {
    const stale = emptyRunFromSnapshot(ALL_FIXTURES.a2a.snapshot);
    const replacement: RunSnapshotV1 = { ...ALL_FIXTURES.langgraph.snapshot };
    const result = applyRunEvent(stale, {
      seq: 99,
      ts: 123,
      type: "snapshot.replaced",
      snapshot: replacement,
    });
    expect(result.runId).toBe(replacement.runId);
    expect(result.status).toBe(replacement.status);
    expect(result.lastEventSeq).toBe(99);
  });

  it("turn.status_changed also patches error when the optional field is present (real-transport-only)", () => {
    const base = ALL_FIXTURES.a2a.snapshot;
    const result = applyRunEvent(base, {
      seq: base.lastEventSeq + 1,
      ts: 123,
      type: "turn.status_changed",
      status: "failed",
      error: { code: "boom", message: "it broke", retryable: true },
    });
    expect(result.status).toBe("failed");
    expect(result.error).toEqual({ code: "boom", message: "it broke", retryable: true });
  });

  it("turn.status_changed leaves error untouched when the field is omitted (fixture-transport shape)", () => {
    const base = { ...ALL_FIXTURES.a2a.snapshot, error: { code: "prior", message: "prior error", retryable: false } };
    const result = applyRunEvent(base, {
      seq: base.lastEventSeq + 1,
      ts: 123,
      type: "turn.status_changed",
      status: "running",
    });
    expect(result.error).toEqual({ code: "prior", message: "prior error", retryable: false });
  });

  it("invocation.status_changed patches an existing invocation's status by id, setting endedAt on first terminal transition", () => {
    const base = ALL_FIXTURES.a2a.snapshot;
    const targetId = base.invocations[0].id;
    const running = { ...base, invocations: base.invocations.map((inv) => (inv.id === targetId ? { ...inv, status: "running" as const, endedAt: null } : inv)) };
    const result = applyRunEvent(running, {
      seq: base.lastEventSeq + 1,
      ts: 5000,
      type: "invocation.status_changed",
      invocationId: targetId,
      status: "completed",
    });
    const patched = result.invocations.find((inv) => inv.id === targetId);
    expect(patched?.status).toBe("completed");
    expect(patched?.endedAt).toBe(5000);
  });

  it("invocation.status_changed sets startedAt on first transition to running", () => {
    const base = ALL_FIXTURES.a2a.snapshot;
    const targetId = base.invocations[0].id;
    const queued = { ...base, invocations: base.invocations.map((inv) => (inv.id === targetId ? { ...inv, status: "queued" as const, startedAt: null } : inv)) };
    const result = applyRunEvent(queued, {
      seq: base.lastEventSeq + 1,
      ts: 42,
      type: "invocation.status_changed",
      invocationId: targetId,
      status: "running",
    });
    expect(result.invocations.find((inv) => inv.id === targetId)?.startedAt).toBe(42);
  });

  it("turn.status_changed cascades onto non-terminal invocations (mirrors the backend's own transition() SQL cascade)", () => {
    const base = ALL_FIXTURES.a2a.snapshot;
    const runningInvocations = base.invocations.map((inv) => ({ ...inv, status: "running" as const, endedAt: null }));
    const running = { ...base, invocations: runningInvocations };
    const result = applyRunEvent(running, {
      seq: base.lastEventSeq + 1,
      ts: 7000,
      type: "turn.status_changed",
      status: "completed",
    });
    expect(result.invocations.every((inv) => inv.status === "completed")).toBe(true);
    expect(result.invocations.every((inv) => inv.endedAt === 7000)).toBe(true);
  });

  it("turn.status_changed's cascade leaves an already-terminal invocation untouched", () => {
    const base = ALL_FIXTURES.a2a.snapshot;
    const [first, ...rest] = base.invocations;
    const mixed = {
      ...base,
      invocations: [{ ...first, status: "failed" as const, endedAt: 100 }, ...rest.map((inv) => ({ ...inv, status: "running" as const, endedAt: null }))],
    };
    const result = applyRunEvent(mixed, {
      seq: base.lastEventSeq + 1,
      ts: 7000,
      type: "turn.status_changed",
      status: "completed",
    });
    expect(result.invocations[0]).toEqual(mixed.invocations[0]); // untouched: already terminal
    expect(result.invocations.slice(1).every((inv) => inv.status === "completed" && inv.endedAt === 7000)).toBe(true);
  });

  it("invocation.status_changed is a no-op when the invocation id isn't found", () => {
    const base = ALL_FIXTURES.a2a.snapshot;
    const result = applyRunEvent(base, {
      seq: base.lastEventSeq + 1,
      ts: 42,
      type: "invocation.status_changed",
      invocationId: "inv_does_not_exist",
      status: "completed",
    });
    expect(result.invocations).toEqual(base.invocations);
  });
});
