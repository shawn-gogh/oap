import { describe, expect, it } from "vitest";

import { ALL_FIXTURES, FIXTURE_IDS } from "@/lib/run/fixtures";
import { buildRunView, RUN_STATUS_LABELS, RUN_STATUS_TONES } from "./run-view-model";

describe("buildRunView", () => {
  it.each(FIXTURE_IDS)("derives a consistent view shape for fixture %s", (id) => {
    const view = buildRunView(ALL_FIXTURES[id].snapshot);
    expect(view.runId).toBe(ALL_FIXTURES[id].snapshot.runId);
    expect(view.statusLabel).toBe(RUN_STATUS_LABELS[ALL_FIXTURES[id].snapshot.status]);
    expect(view.statusTone).toBe(RUN_STATUS_TONES[ALL_FIXTURES[id].snapshot.status]);
    // Every fixture must produce the same set of keys — the whole point of a
    // shared view model is that no provider gets special-cased fields.
    expect(Object.keys(view).sort()).toEqual(
      [
        "runId",
        "title",
        "providerLabel",
        "statusLabel",
        "statusTone",
        "triggerLabel",
        "isTerminal",
        "canCancel",
        "canRetry",
        "progress",
        "invocations",
        "pendingInputRequest",
        "pendingApproval",
        "result",
        "artifacts",
        "error",
        "inputSnapshot",
      ].sort(),
    );
  });

  it("marks completed runs terminal and retry-eligible, not cancellable", () => {
    const view = buildRunView(ALL_FIXTURES.a2a.snapshot);
    expect(view.isTerminal).toBe(true);
    expect(view.canCancel).toBe(false);
    expect(view.canRetry).toBe(true);
  });

  it("marks in-flight runs cancellable, not retry-eligible", () => {
    const view = buildRunView(ALL_FIXTURES.waiting_approval.snapshot);
    expect(view.isTerminal).toBe(false);
    expect(view.canCancel).toBe(true);
    expect(view.canRetry).toBe(false);
  });

  it("surfaces the pending approval for the waiting_approval scenario", () => {
    const view = buildRunView(ALL_FIXTURES.waiting_approval.snapshot);
    expect(view.pendingApproval?.id).toBe("appr_scenario");
    expect(view.pendingInputRequest).toBeNull();
  });

  it("surfaces the pending input request for the waiting_input scenario", () => {
    const view = buildRunView(ALL_FIXTURES.waiting_input.snapshot);
    expect(view.pendingInputRequest?.fields).toHaveLength(2);
    expect(view.pendingApproval).toBeNull();
  });

  it("surfaces the error for the failed scenario", () => {
    const view = buildRunView(ALL_FIXTURES.failed.snapshot);
    expect(view.error?.code).toBe("upstream_conflict");
    expect(view.isTerminal).toBe(true);
  });
});
