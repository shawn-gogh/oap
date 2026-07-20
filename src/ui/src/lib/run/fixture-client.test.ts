import { describe, expect, it } from "vitest";

import { createRun, getRunSnapshot } from "./fixture-client";
import { RUN_AGENT_TEMPLATES } from "./fixtures/templates";

describe("createRun", () => {
  it.each(RUN_AGENT_TEMPLATES)("creates a retrievable run for template $agentId", async (template) => {
    const input = { title: "示例任务" };
    const created = await createRun({ agentId: template.agentId, input });
    expect(created.agentId).toBe(template.agentId);
    expect(created.inputSnapshot).toEqual(input);
    expect(created.interactionProfile.inputSchema).toEqual(template.inputSchema);

    const fetched = await getRunSnapshot(created.runId);
    expect(fetched).toEqual(created);
  });

  it("preserves the input snapshot immutably across repeated fetches", async () => {
    const created = await createRun({
      agentId: "agent_template_supported",
      input: { title: "first" },
    });
    const first = await getRunSnapshot(created.runId);
    first.inputSnapshot = { title: "mutated" };
    const second = await getRunSnapshot(created.runId);
    expect(second.inputSnapshot).toEqual({ title: "first" });
  });

  it("rejects an unknown agent id", async () => {
    await expect(createRun({ agentId: "agent_does_not_exist", input: {} })).rejects.toThrow();
  });

  it("creates distinct run ids for repeated calls with the same template", async () => {
    const a = await createRun({ agentId: "agent_template_freeform", input: { text: "a" } });
    const b = await createRun({ agentId: "agent_template_freeform", input: { text: "b" } });
    expect(a.runId).not.toBe(b.runId);
  });
});
