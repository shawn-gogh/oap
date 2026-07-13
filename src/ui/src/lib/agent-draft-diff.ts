import type { AgentDraft } from "@/lib/agent-builder";

export type FieldChangeKind = "set" | "added" | "removed" | "edited";

export interface FieldChange {
  /** Human-readable field name, e.g. "model", "tools", "system prompt". */
  field: string;
  kind: FieldChangeKind;
  /** Compact before → after rendering for scalar fields. */
  before?: string;
  after?: string;
  /** Items added/removed for list fields. */
  added?: string[];
  removed?: string[];
  /** Extra detail line, e.g. "+3 lines" for long text fields. */
  detail?: string;
}

function scalarChange(field: string, before: string, after: string): FieldChange | null {
  if (before === after) return null;
  if (!before) return { field, kind: "set", after };
  return { field, kind: "edited", before, after };
}

function listChange(field: string, before: string[], after: string[]): FieldChange | null {
  const beforeSet = new Set(before);
  const afterSet = new Set(after);
  const added = after.filter((item) => !beforeSet.has(item));
  const removed = before.filter((item) => !afterSet.has(item));
  if (added.length === 0 && removed.length === 0) return null;
  return {
    field,
    kind:
      added.length > 0 && removed.length === 0
        ? "added"
        : removed.length > 0 && added.length === 0
          ? "removed"
          : "edited",
    added,
    removed,
  };
}

function longTextChange(field: string, before: string, after: string): FieldChange | null {
  if (before === after) return null;
  const beforeLines = before ? before.split("\n").length : 0;
  const afterLines = after ? after.split("\n").length : 0;
  const delta = afterLines - beforeLines;
  const detail =
    delta === 0
      ? `${afterLines} lines rewritten in place`
      : `${delta > 0 ? "+" : ""}${delta} lines (${beforeLines} → ${afterLines})`;
  return { field, kind: before ? "edited" : "set", detail, before, after };
}

/** Compare two parsed drafts field by field and return the user-visible changes.
 *  Computed locally so the summary never depends on the model self-reporting. */
export function diffAgentDrafts(before: AgentDraft, after: AgentDraft): FieldChange[] {
  const beforeApplication = before.application;
  const afterApplication = after.application;
  const changes: Array<FieldChange | null> = [
    scalarChange("name", before.name.trim(), after.name.trim()),
    scalarChange("description", before.description.trim(), after.description.trim()),
    scalarChange("model", before.model.trim(), after.model.trim()),
    scalarChange("runtime", before.runtime.trim(), after.runtime.trim()),
    scalarChange(
      "application objective",
      beforeApplication?.objective.trim() ?? "",
      afterApplication?.objective.trim() ?? "",
    ),
    scalarChange(
      "interaction mode",
      beforeApplication?.interaction_mode ?? "",
      afterApplication?.interaction_mode ?? "",
    ),
    listChange("audience", beforeApplication?.audience ?? [], afterApplication?.audience ?? []),
    listChange(
      "application inputs",
      (beforeApplication?.inputs ?? []).map((input) => `${input.type}:${input.source}:${input.description}`),
      (afterApplication?.inputs ?? []).map((input) => `${input.type}:${input.source}:${input.description}`),
    ),
    listChange(
      "application outputs",
      (beforeApplication?.outputs ?? []).map((output) => `${output.type}:${output.description}`),
      (afterApplication?.outputs ?? []).map((output) => `${output.type}:${output.description}`),
    ),
    listChange("non-goals", beforeApplication?.non_goals ?? [], afterApplication?.non_goals ?? []),
    listChange(
      "completion criteria",
      beforeApplication?.completion_criteria ?? [],
      afterApplication?.completion_criteria ?? [],
    ),
    scalarChange(
      "failure behavior",
      beforeApplication?.failure_behavior.trim() ?? "",
      afterApplication?.failure_behavior.trim() ?? "",
    ),
    longTextChange("system prompt", before.system, after.system),
    listChange(
      "tools",
      before.tools.map((tool) => tool.type).filter(Boolean),
      after.tools.map((tool) => tool.type).filter(Boolean),
    ),
    scalarChange("schedule", before.cron, after.cron),
    scalarChange("timezone", before.timezone, after.timezone),
    listChange("vault keys", before.vault_keys, after.vault_keys),
    listChange("skills", before.skill_ids, after.skill_ids),
    listChange("rules", before.rule_ids, after.rule_ids),
    listChange(
      "sub-agents",
      before.sub_agents.map((agent) => agent.agent_id),
      after.sub_agents.map((agent) => agent.agent_id),
    ),
    listChange("MCP servers", before.mcp_server_ids, after.mcp_server_ids),
    scalarChange("max runtime", `${before.max_runtime_minutes} min`, `${after.max_runtime_minutes} min`),
  ];
  return changes.filter((change): change is FieldChange => change !== null);
}
