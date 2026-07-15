import type { HarnessMessagePart } from "@/lib/types";

type ToolPart = Extract<HarnessMessagePart, { type: "tool" }>;

// Read-only exploration tools whose consecutive calls are noisy to show one
// row at a time and are safe to batch into a single collapsed summary.
function isContextTool(tool: string): boolean {
  const n = tool.toLowerCase();
  return (
    n.includes("read") ||
    n.includes("grep") ||
    n.includes("glob") ||
    n.includes("list") ||
    n.includes("ls") ||
    n.includes("find") ||
    n.includes("search")
  );
}

function toolStatus(part: ToolPart): string {
  const status = part.state?.status;
  return typeof status === "string" ? status : "running";
}

// Tools whose output usually matters enough to show expanded by default,
// even before the user clicks anything.
function isHighSignalTool(tool: string): boolean {
  const n = tool.toLowerCase();
  return n === "bash" || n.includes("edit") || n.includes("write") || n.includes("patch");
}

export function defaultOpenForTool(tool: string, status: string): boolean {
  if (status === "error" || status === "timed_out" || status === "aborted") return true;
  if (isContextTool(tool)) return false;
  return isHighSignalTool(tool);
}

export type ToolGroup =
  | { kind: "single"; part: ToolPart }
  | { kind: "context-batch"; parts: ToolPart[] };

// Coalesces consecutive read-only "context" tool calls (read/grep/glob/list)
// into one group so a research-heavy turn doesn't render a wall of near-
// identical rows; every other tool call stays its own group.
export function groupToolParts(parts: HarnessMessagePart[]): ToolGroup[] {
  const toolParts = parts.filter((p): p is ToolPart => p.type === "tool");
  const groups: ToolGroup[] = [];
  let batch: ToolPart[] = [];

  const flush = () => {
    if (batch.length === 0) return;
    if (batch.length === 1) {
      groups.push({ kind: "single", part: batch[0] });
    } else {
      groups.push({ kind: "context-batch", parts: batch });
    }
    batch = [];
  };

  for (const part of toolParts) {
    if (isContextTool(part.tool) && toolStatus(part) !== "error") {
      batch.push(part);
      continue;
    }
    flush();
    groups.push({ kind: "single", part });
  }
  flush();

  return groups;
}

export { isContextTool, toolStatus };
