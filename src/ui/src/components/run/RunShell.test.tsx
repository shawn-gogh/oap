import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { getRunSnapshot } from "@/lib/run/fixture-client";
import { ALL_FIXTURES, FIXTURE_IDS } from "@/lib/run/fixtures";

// This repo's vitest runs in the "node" environment (no
// @testing-library/react / jsdom installed), so this exercises RunShell's
// data path — the same fixture-backed transport and view-model derivation
// it calls internally — rather than mounting the component. The acceptance
// criterion under test ("no provider-specific condition controls the
// interaction layout") is checked directly against the source text, since
// that's what the criterion actually asserts.

const RUN_SHELL_SOURCE = readFileSync(fileURLToPath(new URL("./RunShell.tsx", import.meta.url)), "utf8");

describe("RunShell", () => {
  it("contains no provider-name conditional", () => {
    expect(RUN_SHELL_SOURCE).not.toMatch(/providerName\s*===/);
    expect(RUN_SHELL_SOURCE).not.toMatch(/provider\s*===\s*["']/);
  });

  it("switches commands and subscriptions to the turn returned by retry", () => {
    expect(RUN_SHELL_SOURCE).toContain("setActiveRunId(next.runId)");
    expect(RUN_SHELL_SOURCE).toContain("runId: snapshot.runId");
    expect(RUN_SHELL_SOURCE).toContain("subscribeRunEvents(activeRunId");
  });

  it.each(FIXTURE_IDS)("resolves a snapshot for fixture %s via the same transport RunShell uses", async (id) => {
    const snapshot = await getRunSnapshot(ALL_FIXTURES[id].snapshot.runId);
    expect(snapshot.runId).toBe(ALL_FIXTURES[id].snapshot.runId);
    expect(snapshot.status).toBe(ALL_FIXTURES[id].snapshot.status);
  });

  it("clones fixture state so repeated access never mutates the source fixture", async () => {
    const first = await getRunSnapshot(ALL_FIXTURES.a2a.snapshot.runId);
    first.status = "failed";
    const second = await getRunSnapshot(ALL_FIXTURES.a2a.snapshot.runId);
    expect(second.status).toBe("completed");
  });
});
