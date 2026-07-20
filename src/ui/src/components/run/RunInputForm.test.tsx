import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { createRun, getRunSnapshot } from "@/lib/run/fixture-client";
import { RUN_AGENT_TEMPLATES } from "@/lib/run/fixtures/templates";

// Same source-scan approach as RunShell.test.tsx (no @testing-library/react
// in this repo — vitest runs in the "node" environment). The behavioral
// coverage lives in schema-form.test.ts and fixture-client.test.ts;
// this file asserts RunInputForm delegates to them rather than
// reimplementing validation, and that its data path (createRun) round-trips
// correctly for every template it's meant to drive.

const SOURCE = readFileSync(fileURLToPath(new URL("./RunInputForm.tsx", import.meta.url)), "utf8");

describe("RunInputForm", () => {
  it("delegates validation and schema parsing to lib/run/schema-form rather than reimplementing it", () => {
    expect(SOURCE).toMatch(/from "@\/lib\/run\/schema-form"/);
    expect(SOURCE).toMatch(/validateValue\(/);
    expect(SOURCE).toMatch(/describeSchema\(/);
  });

  it("contains no agent-id or provider-name conditional", () => {
    expect(SOURCE).not.toMatch(/agentId\s*===/);
    expect(SOURCE).not.toMatch(/providerName\s*===/);
  });

  it.each(RUN_AGENT_TEMPLATES)(
    "submitting for template $agentId produces a run retrievable afterward",
    async (template) => {
      const created = await createRun({ agentId: template.agentId, input: { title: "来自表单的提交" } });
      const fetched = await getRunSnapshot(created.runId);
      expect(fetched.agentId).toBe(template.agentId);
      expect(fetched.inputSnapshot).toEqual({ title: "来自表单的提交" });
    },
  );
});
