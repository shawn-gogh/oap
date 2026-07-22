import { describe, expect, it } from "vitest";

import {
  escapePointerToken,
  flattenJsonPointers,
  JSON_POINTER_ROW_LIMIT,
} from "./json-pointer-tree";

/**
 * Minimal RFC 6901 resolver, written independently of the flattener so the
 * tests check pointers actually *resolve* rather than re-deriving them the
 * same way they were produced.
 */
function resolvePointer(document: unknown, pointer: string): unknown {
  if (pointer === "") return document;
  let current: unknown = document;
  for (const rawToken of pointer.slice(1).split("/")) {
    const token = rawToken.replaceAll("~1", "/").replaceAll("~0", "~");
    if (Array.isArray(current)) {
      current = current[Number(token)];
    } else if (current !== null && typeof current === "object") {
      current = (current as Record<string, unknown>)[token];
    } else {
      return undefined;
    }
  }
  return current;
}

describe("flattenJsonPointers", () => {
  it("emits pointers that resolve back to the value each row describes", () => {
    const document = {
      messages: [
        { type: "human", content: "hi" },
        { type: "ai", content: "hello" },
      ],
      usage: { tokens: 12, cached: false, detail: null },
    };

    const { rows } = flattenJsonPointers(document);

    expect(rows.length).toBeGreaterThan(0);
    for (const row of rows) {
      expect(resolvePointer(document, row.pointer)).not.toBeUndefined();
    }
    expect(rows.map((row) => row.pointer)).toContain("/messages/1/content");
  });

  it("keeps containers selectable so a whole array can be mapped", () => {
    // LangGraph's standard MessagesState answer is the array itself, not a
    // string leaf — the tree must let the operator pick it.
    const { rows } = flattenJsonPointers({ messages: [{ content: "hi" }] });

    const messages = rows.find((row) => row.pointer === "/messages");
    expect(messages).toBeDefined();
    expect(messages?.kind).toBe("array");
    expect(messages?.childCount).toBe(1);
  });

  it("addresses the whole document with the empty pointer", () => {
    const { rows } = flattenJsonPointers({ a: 1 });

    expect(rows[0].pointer).toBe("");
    expect(rows[0].depth).toBe(0);
  });

  it("escapes ~ and / in keys, in that order", () => {
    expect(escapePointerToken("a/b")).toBe("a~1b");
    expect(escapePointerToken("c~d")).toBe("c~0d");
    // A literal "~1" must not be mistaken for an already-escaped slash.
    expect(escapePointerToken("~1")).toBe("~01");

    const document = { "a/b": { "c~d": "found" } };
    const { rows } = flattenJsonPointers(document);
    const leaf = rows.find((row) => row.kind === "string");

    expect(leaf?.pointer).toBe("/a~1b/c~0d");
    expect(resolvePointer(document, leaf!.pointer)).toBe("found");
  });

  it("classifies every JSON type", () => {
    const { rows } = flattenJsonPointers({
      text: "s",
      count: 1,
      flag: true,
      nothing: null,
      list: [],
      map: {},
    });
    const kindAt = (pointer: string) => rows.find((row) => row.pointer === pointer)?.kind;

    expect(kindAt("/text")).toBe("string");
    expect(kindAt("/count")).toBe("number");
    expect(kindAt("/flag")).toBe("boolean");
    expect(kindAt("/nothing")).toBe("null");
    expect(kindAt("/list")).toBe("array");
    expect(kindAt("/map")).toBe("object");
  });

  it("stops at the row limit and reports truncation", () => {
    const wide = { items: Array.from({ length: JSON_POINTER_ROW_LIMIT + 50 }, (_, i) => i) };

    const { rows, truncated } = flattenJsonPointers(wide);

    expect(truncated).toBe(true);
    expect(rows.length).toBeLessThanOrEqual(JSON_POINTER_ROW_LIMIT);
  });

  it("does not report truncation for documents that fit", () => {
    const { truncated } = flattenJsonPointers({ a: 1, b: [1, 2] });
    expect(truncated).toBe(false);
  });

  it("exposes depth so OpenAPI can be restricted to top-level fields", () => {
    // invoke_openapi reads its answer with payload.get(output_field) — a
    // top-level key, not a pointer — so the dialog only lets depth-1 rows be
    // picked for openapi sources, and submits row.label rather than
    // row.pointer. This is the crewai-native response shape.
    const { rows } = flattenJsonPointers({
      id: "kickoff_abc",
      status: "completed",
      output: "Final Answer: …",
      meta: { nested: "not addressable by output_field" },
    });

    const topLevel = rows.filter((row) => row.depth === 1).map((row) => row.label);
    expect(topLevel).toEqual(["id", "status", "output", "meta"]);
    // The nested leaf still appears in the tree, but at a depth the dialog
    // renders as unselectable.
    expect(rows.find((row) => row.pointer === "/meta/nested")?.depth).toBe(2);
  });
});
