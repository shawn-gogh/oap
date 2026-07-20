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
});
