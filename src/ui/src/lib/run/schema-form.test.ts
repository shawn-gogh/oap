import { describe, expect, it } from "vitest";

import { describeSchema, validateValue } from "./schema-form";
import type { JsonSchema } from "./types";

describe("describeSchema — supported subset", () => {
  it("returns null for a non-object top-level schema", () => {
    expect(describeSchema({ type: "string" })).toBeNull();
    expect(describeSchema(null)).toBeNull();
  });

  it("describes string/number/boolean/enum/array/file fields", () => {
    const schema: JsonSchema = {
      type: "object",
      required: ["title"],
      properties: {
        title: { type: "string" },
        count: { type: "integer" },
        urgent: { type: "boolean" },
        priority: { type: "string", enum: ["low", "high"] },
        tags: { type: "array", items: { type: "string" } },
        attachment: { type: "string", contentMediaType: "application/pdf" },
      },
    };
    const fields = describeSchema(schema);
    expect(fields).not.toBeNull();
    const kinds = Object.fromEntries(fields!.map((f) => [f.key, f.kind]));
    expect(kinds).toEqual({
      title: "string",
      count: "number",
      urgent: "boolean",
      priority: "enum",
      tags: "array",
      attachment: "file",
    });
    expect(fields!.find((f) => f.key === "title")!.required).toBe(true);
    expect(fields!.find((f) => f.key === "count")!.required).toBe(false);
    expect(fields!.find((f) => f.key === "priority")!.enumValues).toEqual(["low", "high"]);
    expect(fields!.find((f) => f.key === "tags")!.itemKind).toBe("string");
  });

  it("describes one level of nested object properties", () => {
    const schema: JsonSchema = {
      type: "object",
      properties: {
        contact: {
          type: "object",
          properties: { email: { type: "string" } },
        },
      },
    };
    const fields = describeSchema(schema);
    expect(fields).not.toBeNull();
    expect(fields![0].kind).toBe("object");
    expect(fields![0].properties?.[0].key).toBe("email");
  });

  it("uses the schema's title as the field label when present", () => {
    const schema: JsonSchema = {
      type: "object",
      properties: { q: { type: "string", title: "搜索关键词" } },
    };
    expect(describeSchema(schema)![0].label).toBe("搜索关键词");
  });
});

describe("describeSchema — unsupported subset falls back to null", () => {
  it("rejects oneOf/anyOf/allOf/$ref", () => {
    expect(describeSchema({ type: "object", properties: { x: { oneOf: [{ type: "string" }] } } })).toBeNull();
    expect(describeSchema({ type: "object", properties: { x: { anyOf: [{ type: "string" }] } } })).toBeNull();
    expect(describeSchema({ type: "object", properties: { x: { allOf: [{ type: "string" }] } } })).toBeNull();
    expect(describeSchema({ type: "object", properties: { x: { $ref: "#/defs/thing" } } })).toBeNull();
  });

  it("rejects arrays of non-primitive items", () => {
    const schema: JsonSchema = {
      type: "object",
      properties: { items: { type: "array", items: { type: "object" } } },
    };
    expect(describeSchema(schema)).toBeNull();
  });

  it("rejects object nesting deeper than one level", () => {
    const schema: JsonSchema = {
      type: "object",
      properties: {
        a: {
          type: "object",
          properties: {
            b: { type: "object", properties: { c: { type: "string" } } },
          },
        },
      },
    };
    expect(describeSchema(schema)).toBeNull();
  });

  it("rejects an unrecognized or missing type", () => {
    expect(describeSchema({ type: "object", properties: { x: {} } })).toBeNull();
    expect(describeSchema({ type: "object", properties: { x: { type: "null" } } })).toBeNull();
  });

  it("propagates a nested unsupported field up to the whole schema", () => {
    const schema: JsonSchema = {
      type: "object",
      properties: {
        fine: { type: "string" },
        broken: { oneOf: [{ type: "string" }] },
      },
    };
    expect(describeSchema(schema)).toBeNull();
  });
});

describe("validateValue", () => {
  const schema: JsonSchema = {
    type: "object",
    required: ["title"],
    properties: {
      title: { type: "string" },
      count: { type: "integer" },
      priority: { type: "string", enum: ["low", "high"] },
      tags: { type: "array", items: { type: "string" } },
      contact: { type: "object", properties: { email: { type: "string" } } },
    },
  };
  const fields = describeSchema(schema)!;

  it("passes on a fully valid value", () => {
    const errors = validateValue(fields, {
      title: "hello",
      count: 3,
      priority: "low",
      tags: ["a", "b"],
      contact: { email: "x@example.com" },
    });
    expect(errors).toEqual({});
  });

  it("flags a missing required field", () => {
    const errors = validateValue(fields, {});
    expect(errors.title).toBeTruthy();
  });

  it("flags a wrong-typed number field", () => {
    const errors = validateValue(fields, { title: "x", count: "not a number" });
    expect(errors.count).toBeTruthy();
  });

  it("flags an enum value outside the allowed set", () => {
    const errors = validateValue(fields, { title: "x", priority: "medium" });
    expect(errors.priority).toBeTruthy();
  });

  it("flags an array with a wrong-typed item", () => {
    const errors = validateValue(fields, { title: "x", tags: ["ok", 5] });
    expect(errors.tags).toBeTruthy();
  });

  it("validates nested object fields under a dotted path key", () => {
    const errors = validateValue(fields, { title: "x", contact: { email: 5 } });
    expect(errors["contact.email"]).toBeTruthy();
  });

  it("does not flag an absent optional field", () => {
    const errors = validateValue(fields, { title: "x" });
    expect(errors).toEqual({});
  });
});
