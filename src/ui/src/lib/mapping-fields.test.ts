import { describe, expect, it } from "vitest";

import { isFieldUndeclared, mappingFieldCandidates, unobservedFields } from "./mapping-fields";

describe("mappingFieldCandidates", () => {
  it("lists top-level properties with types, labels, and required flags", () => {
    // The crewai-native kickoff request body.
    const candidates = mappingFieldCandidates({
      type: "object",
      required: ["topic"],
      properties: {
        topic: { type: "string", title: "研究主题" },
        llm_mode: { type: "string", enum: ["local", "gateway"] },
      },
    });

    expect(candidates).toEqual([
      { name: "topic", typeLabel: "string", title: "研究主题", required: true },
      {
        name: "llm_mode",
        typeLabel: "string",
        required: false,
        enumValues: ["local", "gateway"],
      },
    ]);
  });

  it("keeps fields whose type it cannot resolve, instead of dropping the schema", () => {
    // describeSchema (the form renderer) returns null for the whole schema
    // here. A mapping target only needs the name, so these must survive.
    const candidates = mappingFieldCandidates({
      type: "object",
      properties: {
        payload: { $ref: "#/components/schemas/Payload" },
        either: { oneOf: [{ type: "string" }, { type: "number" }] },
        untyped: {},
        nullable: { type: ["string", "null"] },
      },
    });

    expect(candidates.map((candidate) => [candidate.name, candidate.typeLabel])).toEqual([
      ["payload", "$ref"],
      ["either", "组合类型"],
      ["untyped", "未声明类型"],
      ["nullable", "string | null"],
    ]);
  });

  it("returns nothing for schemas without properties", () => {
    expect(mappingFieldCandidates({ type: "string" })).toEqual([]);
    expect(mappingFieldCandidates(null)).toEqual([]);
    expect(mappingFieldCandidates(undefined)).toEqual([]);
    expect(mappingFieldCandidates({ type: "object" })).toEqual([]);
  });

  it("does not descend into nested objects", () => {
    // Neither mapping language can address a nested field, so offering one
    // would produce a mapping that cannot read.
    const candidates = mappingFieldCandidates({
      type: "object",
      properties: { meta: { type: "object", properties: { nested: { type: "string" } } } },
    });

    expect(candidates.map((candidate) => candidate.name)).toEqual(["meta"]);
  });
});

describe("isFieldUndeclared", () => {
  const schema = {
    type: "object",
    properties: { id: {}, status: {}, output: {} },
  };

  it("flags a field the schema does not declare", () => {
    // The mistake that currently only surfaces as a failed session.
    expect(isFieldUndeclared(schema, "answer")).toBe(true);
  });

  it("accepts a declared field, ignoring surrounding whitespace", () => {
    expect(isFieldUndeclared(schema, "output")).toBe(false);
    expect(isFieldUndeclared(schema, "  output  ")).toBe(false);
  });

  it("stays silent when there is nothing to check against", () => {
    // An unknown or empty contract must not manufacture a false alarm — the
    // spec can legitimately be incomplete, and the probe is the real evidence.
    expect(isFieldUndeclared(null, "answer")).toBe(false);
    expect(isFieldUndeclared({ type: "object" }, "answer")).toBe(false);
    expect(isFieldUndeclared(schema, undefined)).toBe(false);
    expect(isFieldUndeclared(schema, "   ")).toBe(false);
  });
});

describe("unobservedFields", () => {
  const fields = ["input_field", "output_field"] as const;

  it("reports fields taken from the spec rather than a real response", () => {
    expect(unobservedFields({ input_field: "spec", output_field: "spec" }, fields)).toEqual([
      "input_field",
      "output_field",
    ]);
  });

  it("clears a field once a probe confirmed it", () => {
    expect(unobservedFields({ input_field: "spec", output_field: "probe" }, fields)).toEqual([
      "input_field",
    ]);
  });

  it("treats hand-typed values as unobserved too", () => {
    // Typing a field name asserts it exists; it is no more checked than a
    // value copied out of the spec.
    expect(unobservedFields({ output_field: "manual" }, fields)).toEqual(["output_field"]);
  });

  it("ignores fields that carry no value yet", () => {
    expect(unobservedFields({}, fields)).toEqual([]);
    expect(unobservedFields({ output_field: "probe" }, fields)).toEqual([]);
  });
});
