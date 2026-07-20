import { describe, expect, it } from "vitest";

import { buildInvocationTree, formatDuration, statusIconKind } from "./invocation-tree";
import type { RunInvocation } from "./types";

function invocation(overrides: Partial<RunInvocation> & Pick<RunInvocation, "id">): RunInvocation {
  return {
    turnId: "run_1",
    parentInvocationId: null,
    role: "agent",
    label: overrides.id,
    status: "completed",
    startedAt: null,
    endedAt: null,
    summary: null,
    raw: null,
    ...overrides,
  };
}

describe("buildInvocationTree", () => {
  it("nests a child under its parent", () => {
    const root = invocation({ id: "root" });
    const child = invocation({ id: "child", parentInvocationId: "root" });
    const tree = buildInvocationTree([root, child]);
    expect(tree).toHaveLength(1);
    expect(tree[0].invocation.id).toBe("root");
    expect(tree[0].children).toHaveLength(1);
    expect(tree[0].children[0].invocation.id).toBe("child");
  });

  it("treats every row as a root when parentInvocationId is always null (today's real-backend shape)", () => {
    const rows = [invocation({ id: "a" }), invocation({ id: "b" }), invocation({ id: "c" })];
    const tree = buildInvocationTree(rows);
    expect(tree.map((n) => n.invocation.id)).toEqual(["a", "b", "c"]);
    expect(tree.every((n) => n.children.length === 0)).toBe(true);
  });

  it("defaults an orphaned parent reference to root instead of dropping the row", () => {
    const orphan = invocation({ id: "orphan", parentInvocationId: "missing_parent" });
    const tree = buildInvocationTree([orphan]);
    expect(tree).toHaveLength(1);
    expect(tree[0].invocation.id).toBe("orphan");
  });

  it("nests multiple levels deep", () => {
    const grandparent = invocation({ id: "gp" });
    const parent = invocation({ id: "p", parentInvocationId: "gp" });
    const child = invocation({ id: "c", parentInvocationId: "p" });
    const tree = buildInvocationTree([grandparent, parent, child]);
    expect(tree[0].children[0].children[0].invocation.id).toBe("c");
  });

  it("preserves arrival order and supports multiple children under one parent", () => {
    const root = invocation({ id: "root" });
    const first = invocation({ id: "first", parentInvocationId: "root" });
    const second = invocation({ id: "second", parentInvocationId: "root" });
    const tree = buildInvocationTree([root, first, second]);
    expect(tree[0].children.map((n) => n.invocation.id)).toEqual(["first", "second"]);
  });
});

describe("formatDuration", () => {
  it("returns null when either timestamp is missing", () => {
    expect(formatDuration(null, 100)).toBeNull();
    expect(formatDuration(100, null)).toBeNull();
    expect(formatDuration(null, null)).toBeNull();
  });

  it("formats sub-second durations in milliseconds", () => {
    expect(formatDuration(1000, 1850)).toBe("850ms");
  });

  it("formats second-or-longer durations with one decimal", () => {
    expect(formatDuration(1000, 13300)).toBe("12.3s");
  });

  it("returns null for a negative duration (clock skew / bad data) rather than a nonsense string", () => {
    expect(formatDuration(2000, 1000)).toBeNull();
  });
});

describe("statusIconKind", () => {
  it("maps every RunStatus to an icon kind", () => {
    expect(statusIconKind("completed")).toBe("check");
    expect(statusIconKind("failed")).toBe("cross");
    expect(statusIconKind("rejected")).toBe("cross");
    expect(statusIconKind("cancelled")).toBe("cross");
    expect(statusIconKind("timed_out")).toBe("cross");
    expect(statusIconKind("waiting_input")).toBe("waiting");
    expect(statusIconKind("waiting_approval")).toBe("waiting");
    expect(statusIconKind("queued")).toBe("spinner");
    expect(statusIconKind("running")).toBe("spinner");
    expect(statusIconKind("cancelling")).toBe("spinner");
  });
});
