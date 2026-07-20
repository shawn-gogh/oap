import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const SOURCE = readFileSync(fileURLToPath(new URL("./InvocationTimeline.tsx", import.meta.url)), "utf8");

describe("InvocationTimeline", () => {
  it("nests via buildInvocationTree and contains no provider-name conditional", () => {
    expect(SOURCE).toContain("buildInvocationTree(invocations)");
    expect(SOURCE).not.toMatch(/\.providerName/);
    expect(SOURCE).not.toMatch(/provider\s*===\s*["']/);
  });
});
