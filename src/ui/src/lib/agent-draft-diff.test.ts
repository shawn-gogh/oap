import { describe, expect, it } from "vitest";

import { blankAgentDraft } from "./agent-builder";
import { diffAgentDrafts } from "./agent-draft-diff";

function base() {
  return {
    ...blankAgentDraft(),
    name: "a",
    model: "m1",
    tools: [{ type: "read" }, { type: "bash" }],
  };
}

describe("diffAgentDrafts", () => {
  it("returns no changes for identical drafts", () => {
    expect(diffAgentDrafts(base(), base())).toEqual([]);
  });

  it("reports scalar edits as before → after", () => {
    const changes = diffAgentDrafts(base(), { ...base(), model: "m2" });
    expect(changes).toEqual([
      { field: "model", kind: "edited", before: "m1", after: "m2" },
    ]);
  });

  it("reports setting a previously empty scalar as set", () => {
    const before = { ...base(), description: "" };
    const changes = diffAgentDrafts(before, { ...before, description: "does things" });
    expect(changes).toEqual([{ field: "description", kind: "set", after: "does things" }]);
  });

  it("splits tool list changes into added and removed", () => {
    const after = { ...base(), tools: [{ type: "read" }, { type: "web_search" }] };
    const changes = diffAgentDrafts(base(), after);
    expect(changes).toHaveLength(1);
    expect(changes[0].field).toBe("tools");
    expect(changes[0].added).toEqual(["web_search"]);
    expect(changes[0].removed).toEqual(["bash"]);
    expect(changes[0].kind).toBe("edited");
  });

  it("marks pure additions as added", () => {
    const after = { ...base(), skill_ids: ["s1"] };
    const changes = diffAgentDrafts(base(), after);
    expect(changes).toEqual([
      { field: "skills", kind: "added", added: ["s1"], removed: [] },
    ]);
  });

  it("summarizes system prompt edits with a line delta instead of full text", () => {
    const before = { ...base(), system: "line1\nline2" };
    const after = { ...before, system: "line1\nline2\nline3\nline4\nline5" };
    const [change] = diffAgentDrafts(before, after);
    expect(change.field).toBe("system prompt");
    expect(change.detail).toBe("+3 lines (2 → 5)");
  });

  it("describes an in-place rewrite of the system prompt", () => {
    const before = { ...base(), system: "old text" };
    const after = { ...before, system: "new text" };
    const [change] = diffAgentDrafts(before, after);
    expect(change.detail).toBe("1 lines rewritten in place");
  });

  it("diffs sub-agents by agent_id", () => {
    const after = { ...base(), sub_agents: [{ agent_id: "agent_1" }] };
    const changes = diffAgentDrafts(base(), after);
    expect(changes).toEqual([
      { field: "sub-agents", kind: "added", added: ["agent_1"], removed: [] },
    ]);
  });

  it("reports max runtime changes with units", () => {
    const after = { ...base(), max_runtime_minutes: 60 };
    const changes = diffAgentDrafts(base(), after);
    expect(changes).toEqual([
      { field: "max runtime", kind: "edited", before: "30 min", after: "60 min" },
    ]);
  });
});
