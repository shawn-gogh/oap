// Flattens an arbitrary JSON document into rows addressed by RFC 6901 JSON
// Pointers, so the mapping confirmation dialog can let an operator *click* the
// field that holds the answer instead of hand-writing `output_path` — the
// step that silently fails when the pointer is malformed (missing leading `/`)
// or simply addresses a field the response does not have.
//
// Escaping must match the backend's `escape_pointer_token`
// (src/http/managed_agents/source_management.rs): a pointer produced here is
// submitted verbatim as `output_path` and later read by `serde_json::pointer`.

export type JsonPointerKind = "string" | "number" | "boolean" | "null" | "array" | "object";

export interface JsonPointerRow {
  /** RFC 6901 pointer. `""` addresses the whole document. */
  pointer: string;
  /** Object key or array index; empty at the root. */
  label: string;
  depth: number;
  kind: JsonPointerKind;
  /** Compact one-line rendering of the value. */
  preview: string;
  /** Element/property count, for containers only. */
  childCount?: number;
}

/**
 * Upper bound on emitted rows. A graph that echoes a large document would
 * otherwise render tens of thousands of DOM nodes into a dialog.
 */
export const JSON_POINTER_ROW_LIMIT = 500;

const PREVIEW_MAX = 80;

/**
 * RFC 6901 token escaping. `~` must be escaped before `/`, otherwise the `~1`
 * produced for a slash would itself be re-escaped into `~01`.
 */
export function escapePointerToken(token: string): string {
  return token.replaceAll("~", "~0").replaceAll("/", "~1");
}

function kindOf(value: unknown): JsonPointerKind {
  if (value === null) return "null";
  if (Array.isArray(value)) return "array";
  switch (typeof value) {
    case "string":
      return "string";
    case "number":
      return "number";
    case "boolean":
      return "boolean";
    default:
      return "object";
  }
}

function truncate(text: string): string {
  return text.length > PREVIEW_MAX ? `${text.slice(0, PREVIEW_MAX - 1)}…` : text;
}

function previewOf(value: unknown, kind: JsonPointerKind): string {
  switch (kind) {
    case "string":
      return truncate(JSON.stringify(value as string) ?? "");
    case "array":
      return `[ ${(value as unknown[]).length} 项 ]`;
    case "object":
      return `{ ${Object.keys(value as object).length} 个字段 }`;
    case "null":
      return "null";
    default:
      return truncate(String(value));
  }
}

/**
 * Depth-first flattening. Containers are emitted too, and are selectable:
 * addressing a whole array is a legitimate mapping (LangGraph's standard
 * `MessagesState` answer lives at `/messages`, not at a string leaf).
 */
export function flattenJsonPointers(
  value: unknown,
  limit: number = JSON_POINTER_ROW_LIMIT,
): { rows: JsonPointerRow[]; truncated: boolean } {
  const rows: JsonPointerRow[] = [];
  let truncated = false;

  const walk = (current: unknown, pointer: string, label: string, depth: number): void => {
    if (rows.length >= limit) {
      truncated = true;
      return;
    }
    const kind = kindOf(current);
    const row: JsonPointerRow = {
      pointer,
      label,
      depth,
      kind,
      preview: previewOf(current, kind),
    };
    if (kind === "array") row.childCount = (current as unknown[]).length;
    if (kind === "object") row.childCount = Object.keys(current as object).length;
    rows.push(row);

    if (kind === "array") {
      (current as unknown[]).forEach((item, index) => {
        walk(item, `${pointer}/${index}`, String(index), depth + 1);
      });
      return;
    }
    if (kind === "object") {
      for (const [key, item] of Object.entries(current as Record<string, unknown>)) {
        walk(item, `${pointer}/${escapePointerToken(key)}`, key, depth + 1);
      }
    }
  };

  walk(value, "", "", 0);
  return { rows, truncated };
}
