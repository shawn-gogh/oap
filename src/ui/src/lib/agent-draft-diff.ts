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
      ? `原位置重写 ${afterLines} 行`
      : `${delta > 0 ? "+" : ""}${delta} 行（${beforeLines} → ${afterLines}）`;
  return { field, kind: before ? "edited" : "set", detail, before, after };
}

/** Compare two parsed drafts field by field and return the user-visible changes.
 *  Computed locally so the summary never depends on the model self-reporting. */
export function diffAgentDrafts(before: AgentDraft, after: AgentDraft): FieldChange[] {
  const beforeApplication = before.application;
  const afterApplication = after.application;
  const changes: Array<FieldChange | null> = [
    scalarChange("名称", before.name.trim(), after.name.trim()),
    scalarChange("描述", before.description.trim(), after.description.trim()),
    scalarChange("模型", before.model.trim(), after.model.trim()),
    scalarChange("运行时", before.runtime.trim(), after.runtime.trim()),
    scalarChange(
      "应用目标",
      beforeApplication?.objective.trim() ?? "",
      afterApplication?.objective.trim() ?? "",
    ),
    scalarChange(
      "交互方式",
      beforeApplication?.interaction_mode ?? "",
      afterApplication?.interaction_mode ?? "",
    ),
    listChange("使用者", beforeApplication?.audience ?? [], afterApplication?.audience ?? []),
    listChange(
      "应用输入",
      (beforeApplication?.inputs ?? []).map((input) => `${input.type}:${input.source}:${input.description}`),
      (afterApplication?.inputs ?? []).map((input) => `${input.type}:${input.source}:${input.description}`),
    ),
    listChange(
      "应用输出",
      (beforeApplication?.outputs ?? []).map((output) => `${output.type}:${output.description}`),
      (afterApplication?.outputs ?? []).map((output) => `${output.type}:${output.description}`),
    ),
    listChange("明确不做", beforeApplication?.non_goals ?? [], afterApplication?.non_goals ?? []),
    listChange(
      "完成条件",
      beforeApplication?.completion_criteria ?? [],
      afterApplication?.completion_criteria ?? [],
    ),
    scalarChange(
      "失败处理",
      beforeApplication?.failure_behavior.trim() ?? "",
      afterApplication?.failure_behavior.trim() ?? "",
    ),
    longTextChange("系统提示词", before.system, after.system),
    listChange(
      "工具",
      before.tools.map((tool) => tool.type).filter(Boolean),
      after.tools.map((tool) => tool.type).filter(Boolean),
    ),
    scalarChange("调度计划", before.cron, after.cron),
    scalarChange("时区", before.timezone, after.timezone),
    listChange("保险库密钥", before.vault_keys, after.vault_keys),
    listChange("技能", before.skill_ids, after.skill_ids),
    listChange("规则", before.rule_ids, after.rule_ids),
    listChange(
      "子智能体",
      before.sub_agents.map((agent) => agent.agent_id),
      after.sub_agents.map((agent) => agent.agent_id),
    ),
    listChange("MCP 服务器", before.mcp_server_ids, after.mcp_server_ids),
    scalarChange("最长运行时间", `${before.max_runtime_minutes} 分钟`, `${after.max_runtime_minutes} 分钟`),
  ];
  return changes.filter((change): change is FieldChange => change !== null);
}
