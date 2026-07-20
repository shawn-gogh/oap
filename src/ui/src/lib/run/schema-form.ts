// Pure logic for Stage 3 of docs/engineering/run-surface-branch-plan.mdx:
// "The initial renderer should support text, number, boolean, enum, simple
// arrays and objects, and file or Artifact references. Unsupported
// structures use a raw JSON editor. Show field-level validation."
//
// No DOM dependency by design (this repo's vitest runs in the "node"
// environment) — SchemaFieldsForm.tsx is a thin renderer over this module,
// same split as run-view-model.ts/apply-event.ts from Stage 2.

import type { JsonSchema } from "./types";

export type SchemaFieldKind = "string" | "number" | "boolean" | "enum" | "array" | "object" | "file";

export interface SchemaField {
  path: string[];
  key: string;
  label: string;
  kind: SchemaFieldKind;
  required: boolean;
  enumValues?: string[];
  itemKind?: "string" | "number" | "boolean";
  properties?: SchemaField[];
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function labelFor(schema: Record<string, unknown>, key: string): string {
  const title = schema.title;
  return typeof title === "string" && title.trim() ? title.trim() : key;
}

/** Describes one JSON Schema property, or returns `null` if it (or anything
 * nested inside it) falls outside the supported subset: oneOf/anyOf/allOf/
 * $ref, arrays of non-primitive items, object nesting deeper than one
 * level, or an unrecognized/missing `type`. */
function describeField(key: string, path: string[], propertySchema: unknown, required: boolean, depth: number): SchemaField | null {
  const schema = asRecord(propertySchema);
  if (!schema) return null;
  if ("oneOf" in schema || "anyOf" in schema || "allOf" in schema || "$ref" in schema) return null;

  const type = schema.type;
  const label = labelFor(schema, key);
  const fieldPath = [...path, key];

  if (type === "string") {
    if (Array.isArray(schema.enum) && schema.enum.every((value) => typeof value === "string")) {
      return { path: fieldPath, key, label, kind: "enum", required, enumValues: schema.enum as string[] };
    }
    if (typeof schema.contentMediaType === "string") {
      return { path: fieldPath, key, label, kind: "file", required };
    }
    return { path: fieldPath, key, label, kind: "string", required };
  }

  if (type === "number" || type === "integer") {
    return { path: fieldPath, key, label, kind: "number", required };
  }

  if (type === "boolean") {
    return { path: fieldPath, key, label, kind: "boolean", required };
  }

  if (type === "array") {
    const items = asRecord(schema.items);
    const itemType = items?.type;
    if (itemType !== "string" && itemType !== "number" && itemType !== "boolean") return null;
    return { path: fieldPath, key, label, kind: "array", required, itemKind: itemType };
  }

  if (type === "object") {
    // Only one level of object nesting is supported — an object field
    // whose own properties include another object would need depth 2+.
    if (depth >= 1) return null;
    const nested = describeObjectProperties(schema, fieldPath, depth + 1);
    if (!nested) return null;
    return { path: fieldPath, key, label, kind: "object", required, properties: nested };
  }

  return null;
}

function describeObjectProperties(schema: Record<string, unknown>, path: string[], depth: number): SchemaField[] | null {
  const properties = asRecord(schema.properties);
  if (!properties) return null;
  const requiredKeys = new Set(Array.isArray(schema.required) ? (schema.required as unknown[]) : []);
  const fields: SchemaField[] = [];
  for (const key of Object.keys(properties)) {
    const field = describeField(key, path, properties[key], requiredKeys.has(key), depth);
    if (!field) return null;
    fields.push(field);
  }
  return fields;
}

/** Top-level entry point. Returns `null` — meaning "fall back to the raw
 * JSON editor" — for `type !== "object"` top-level schemas too, since a Run
 * input is always submitted as a JSON object. */
export function describeSchema(schema: JsonSchema | null): SchemaField[] | null {
  if (!schema) return null;
  const record = asRecord(schema);
  if (!record || record.type !== "object") return null;
  return describeObjectProperties(record, [], 0);
}

function pathKey(path: string[]): string {
  return path.join(".");
}

function isEmpty(value: unknown): boolean {
  if (value === undefined || value === null) return true;
  if (typeof value === "string") return value.trim().length === 0;
  if (Array.isArray(value)) return value.length === 0;
  return false;
}

function validateField(field: SchemaField, rawValue: unknown, errors: Record<string, string>): void {
  const key = pathKey(field.path);
  if (field.required && isEmpty(rawValue)) {
    errors[key] = `${field.label}为必填项。`;
    return;
  }
  if (isEmpty(rawValue)) return;

  switch (field.kind) {
    case "number":
      if (typeof rawValue !== "number" || Number.isNaN(rawValue)) {
        errors[key] = `${field.label}必须是数字。`;
      }
      break;
    case "boolean":
      if (typeof rawValue !== "boolean") {
        errors[key] = `${field.label}必须是布尔值。`;
      }
      break;
    case "enum":
      if (typeof rawValue !== "string" || !field.enumValues?.includes(rawValue)) {
        errors[key] = `${field.label}不是可选值之一。`;
      }
      break;
    case "array":
      if (!Array.isArray(rawValue)) {
        errors[key] = `${field.label}必须是列表。`;
      } else {
        const badItem = rawValue.some((item) => typeof item !== field.itemKind);
        if (badItem) errors[key] = `${field.label}中的每一项都必须是${field.itemKind}。`;
      }
      break;
    case "string":
    case "file":
      if (typeof rawValue !== "string") {
        errors[key] = `${field.label}必须是文本。`;
      }
      break;
    case "object": {
      const nested = asRecord(rawValue);
      if (!nested) {
        errors[key] = `${field.label}必须是对象。`;
      } else if (field.properties) {
        for (const child of field.properties) {
          validateField(child, nested[child.key], errors);
        }
      }
      break;
    }
  }
}

/** Path-joined key ("contact.email") -> Chinese error message. Empty object
 * means the value is valid. */
export function validateValue(fields: SchemaField[], value: unknown): Record<string, string> {
  const errors: Record<string, string> = {};
  const record = asRecord(value) ?? {};
  for (const field of fields) {
    validateField(field, record[field.key], errors);
  }
  return errors;
}
