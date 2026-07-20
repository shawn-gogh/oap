import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// No jsdom in this repo's vitest setup (see RunShell.test.tsx) — checked
// against source text instead of mounting.

const SOURCE = readFileSync(fileURLToPath(new URL("./RunDrawer.tsx", import.meta.url)), "utf8");

describe("RunDrawer", () => {
  it("wires the real transport (createRealRunTransport), not the fixture one", () => {
    expect(SOURCE).toContain("createRealRunTransport(sessionId)");
    expect(SOURCE).not.toContain("fixtureRunTransport");
  });

  it("passes runId/open/onOpenChange straight through rather than deriving its own state", () => {
    expect(SOURCE).toContain("runId={turnId}");
    expect(SOURCE).toMatch(/Dialog open=\{open\} onOpenChange=\{onOpenChange\}/);
  });
});
