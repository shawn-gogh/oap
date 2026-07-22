// Candidate field names for the runtime-mapping dialog, read from a source's
// request/response JSON Schema.
//
// Deliberately not `@/lib/run/schema-form`'s `describeSchema`: that one is
// all-or-nothing (one `$ref` or a deeply nested object and it returns null for
// the whole schema), which is right for a form renderer — you cannot half-render
// an input form — but wrong here. This list only needs field *names* to offer as
// mapping targets, and a `$ref`-typed property is still a perfectly good name to
// map onto. So this stays lenient and degrades to "type unknown" rather than
// dropping the field.

export interface MappingFieldCandidate {
  name: string;
  /** Human-readable type, or a placeholder when the schema does not say. */
  typeLabel: string;
  /** `title` from the schema, when it carries a human label. */
  title?: string;
  required: boolean;
  enumValues?: string[];
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function typeLabelOf(schema: Record<string, unknown>): string {
  const type = schema.type;
  if (typeof type === "string") return type;
  // JSON Schema allows a type union (["string","null"]); show it as written.
  if (Array.isArray(type) && type.every((entry) => typeof entry === "string")) {
    return (type as string[]).join(" | ");
  }
  if ("$ref" in schema) return "$ref";
  if ("oneOf" in schema || "anyOf" in schema || "allOf" in schema) return "组合类型";
  return "未声明类型";
}

/**
 * Top-level properties of an object schema, in declaration order.
 *
 * Only depth 1: both mapping languages address top-level names — OpenAPI's
 * `output_field` is `payload.get(name)`, and an `input_field` is a single key
 * in the request body — so a nested property is not a valid mapping target and
 * offering it would produce a mapping that cannot read.
 */
export function mappingFieldCandidates(schema: unknown): MappingFieldCandidate[] {
  const record = asRecord(schema);
  const properties = record && asRecord(record.properties);
  if (!properties) return [];
  const required = new Set(
    Array.isArray(record?.required)
      ? (record.required as unknown[]).filter((entry): entry is string => typeof entry === "string")
      : [],
  );

  return Object.keys(properties).map((name) => {
    const property = asRecord(properties[name]) ?? {};
    const title = typeof property.title === "string" ? property.title.trim() : "";
    const enumValues = Array.isArray(property.enum)
      ? property.enum.filter((value): value is string => typeof value === "string")
      : undefined;
    return {
      name,
      typeLabel: typeLabelOf(property),
      ...(title ? { title } : {}),
      required: required.has(name),
      ...(enumValues && enumValues.length > 0 ? { enumValues } : {}),
    };
  });
}

/**
 * Where a mapping value came from. The distinction is the point of the
 * confirmation step, not a UI nicety: `spec` is what the source *claims*,
 * `probe` is what it actually returned. Signing a mapping assembled from
 * claims is a different act from signing one you watched work.
 */
export type MappingFieldOrigin = "spec" | "probe" | "manual";

export const MAPPING_ORIGIN_LABELS: Record<MappingFieldOrigin, string> = {
  spec: "来自规范",
  probe: "来自试跑",
  manual: "手动填写",
};

/**
 * The filled-in fields that were never checked against a real response.
 *
 * Only fields carrying a value are reported: an untouched field has no claim
 * attached to it yet, so calling it "unverified" would be noise.
 */
export function unobservedFields<K extends string>(
  origins: Partial<Record<K, MappingFieldOrigin>>,
  fields: readonly K[],
): K[] {
  return fields.filter((field) => {
    const origin = origins[field];
    return origin !== undefined && origin !== "probe";
  });
}

/**
 * Whether a chosen field is absent from the schema's declared properties —
 * the check that turns "saved fine, failed on the first real session" into a
 * warning shown while the operator is still looking at the dialog.
 *
 * Returns false when there is no schema to check against, so an unknown
 * contract never manufactures a false alarm.
 */
export function isFieldUndeclared(schema: unknown, field: string | undefined): boolean {
  const trimmed = field?.trim();
  if (!trimmed) return false;
  const candidates = mappingFieldCandidates(schema);
  if (candidates.length === 0) return false;
  return !candidates.some((candidate) => candidate.name === trimmed);
}
