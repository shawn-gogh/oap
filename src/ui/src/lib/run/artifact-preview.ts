// Pure helpers for Stage 5's Artifact preview — dispatches strictly on
// `mediaType` (never on `providerName`/`agentName`, per the surface plan's
// "no provider branching" rule) so the same ArtifactPreview component
// renders every provider's Artifacts identically.

export type ArtifactPreviewKind =
  | "markdown"
  | "json"
  | "table"
  | "code"
  | "text"
  | "image"
  | "pdf"
  | "link-list"
  | "download";

export function resolveArtifactPreviewKind(mediaType: string): ArtifactPreviewKind {
  const type = mediaType.toLowerCase();
  if (type.startsWith("image/")) return "image";
  if (type === "application/pdf") return "pdf";
  if (type === "text/markdown") return "markdown";
  if (type === "application/json") return "json";
  if (type === "text/csv") return "table";
  if (type === "text/x-code") return "code";
  if (type === "text/uri-list") return "link-list";
  if (type === "text/plain") return "text";
  return "download";
}

/** Media types that can be safely previewed from fetched text content —
 * used to decide whether ArtifactPreview should lazy-fetch a real backend
 * artifact's `url` at all (image/pdf are embedded directly via <img>/
 * <embed> without a fetch; "download"/unrecognized types never fetch). */
export function isTextPreviewKind(kind: ArtifactPreviewKind): boolean {
  return kind === "markdown" || kind === "json" || kind === "table" || kind === "code" || kind === "text";
}

/** Minimal CSV parser: comma-separated, no quoted-field escaping — matches
 * the plain unquoted CSV this codebase's fixtures (dify.ts) and adapters
 * produce. Ragged rows (fewer/more cells than the header) are left as-is;
 * the table renderer pads/truncates visually rather than rejecting them. */
export function parseCsvTable(text: string): { headers: string[]; rows: string[][] } {
  const lines = text.split(/\r?\n/).filter((line) => line.length > 0);
  if (lines.length === 0) return { headers: [], rows: [] };
  const [headerLine, ...rest] = lines;
  return {
    headers: headerLine.split(","),
    rows: rest.map((line) => line.split(",")),
  };
}

/** `text/uri-list` per RFC 2483: one URI per line, `#`-prefixed lines are
 * comments and blank lines are ignored. */
export function parseUriList(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith("#"));
}

// Deliberately minimal Markdown rendering — this repo has no Markdown
// library installed and pulling one in for a single preview panel isn't
// worth the dependency; headings and bold cover the common case, anything
// else renders as plain text rather than raw asterisks/hashes being shown
// verbatim.

export interface MarkdownLine {
  /** 0 = plain paragraph, 1-3 = heading level (`#`/`##`/`###`). */
  level: 0 | 1 | 2 | 3;
  text: string;
}

export function parseMinimalMarkdownLines(markdown: string): MarkdownLine[] {
  return markdown.split(/\r?\n/).map((line) => {
    const match = /^(#{1,3})\s+(.*)$/.exec(line);
    if (match) return { level: match[1].length as 1 | 2 | 3, text: match[2] };
    return { level: 0 as const, text: line };
  });
}

export interface BoldSegment {
  bold: boolean;
  text: string;
}

/** Splits `**bold**` runs out of a line of text for inline rendering. */
export function splitBoldSegments(text: string): BoldSegment[] {
  return text
    .split(/\*\*(.+?)\*\*/g)
    .map((part, index) => ({ bold: index % 2 === 1, text: part }))
    .filter((segment) => segment.text.length > 0);
}
