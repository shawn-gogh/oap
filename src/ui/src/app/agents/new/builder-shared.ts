import type { AgentDraft } from "@/lib/agent-builder";
import type { FieldChange } from "@/lib/agent-draft-diff";
import type { AgentRuntime, RuntimeHarness } from "@/lib/types";

export type BuilderStep = "create" | "eval" | "config" | "review";
export type BuilderView = "edit" | "config" | "preview";

export type BuilderChatMessage =
  | { id: number; role: "user"; text: string }
  | { id: number; role: "assistant"; summary: string; changes: FieldChange[] };

export interface SavedBuilderDraft {
  configText: string;
  step: BuilderStep;
  messages: BuilderChatMessage[];
  savedAt: number;
}

export const BUILDER_DRAFT_STORAGE_KEY = "agent-builder-draft";
const BUILDER_DRAFT_MAX_AGE_MS = 24 * 60 * 60 * 1000;

export function loadSavedBuilderDraft(): SavedBuilderDraft | null {
  try {
    const raw = localStorage.getItem(BUILDER_DRAFT_STORAGE_KEY);
    if (!raw) return null;
    const saved = JSON.parse(raw) as Partial<SavedBuilderDraft>;
    if (typeof saved.configText !== "string" || typeof saved.savedAt !== "number") return null;
    if (Date.now() - saved.savedAt > BUILDER_DRAFT_MAX_AGE_MS) {
      localStorage.removeItem(BUILDER_DRAFT_STORAGE_KEY);
      return null;
    }
    return {
      configText: saved.configText,
      step: saved.step === "eval" || saved.step === "config" || saved.step === "review" ? saved.step : "config",
      messages: Array.isArray(saved.messages) ? saved.messages : [],
      savedAt: saved.savedAt,
    };
  } catch {
    return null;
  }
}

/** Client-side checks before POST, mapped to the field the user must fix —
 *  the backend only returns opaque config errors. */
export function validateDraftForCreate(draft: AgentDraft): string[] {
  const problems: string[] = [];
  if (!draft.name.trim()) problems.push("名称（Name）不能为空");
  if (!draft.model.trim()) problems.push("未选择模型（Model）");
  if (!draft.runtime.trim()) problems.push("未选择运行时（Runtime）");
  if (!draft.system.trim()) problems.push("System prompt 为空");
  const badVaultKeys = draft.vault_keys.filter((key) => !/^[A-Za-z_][A-Za-z0-9_]*$/.test(key));
  if (badVaultKeys.length > 0) {
    problems.push(`保险库密钥名不合法：${badVaultKeys.join(", ")}（只能包含字母、数字、下划线且不能以数字开头）`);
  }
  return problems;
}

export function savedDraftAgeLabel(savedAt: number): string {
  const minutes = Math.max(1, Math.round((Date.now() - savedAt) / 60000));
  if (minutes < 60) return `${minutes} 分钟前`;
  return `${Math.round(minutes / 60)} 小时前`;
}

export function runtimeChoicesForDrafting(
  harnesses: RuntimeHarness[],
  runtimes: AgentRuntime[],
): Array<Pick<AgentRuntime, "id" | "name" | "tools" | "connected">> {
  const connectedHarnesses = harnesses.filter((harness) => harness.connected);
  if (connectedHarnesses.length > 0) {
    return connectedHarnesses.map((harness) => ({
      id: harness.alias,
      name: harness.display_name,
      tools: harness.tools,
      connected: harness.connected,
    }));
  }
  return runtimes.map((runtime) => ({
    id: runtime.id,
    name: runtime.name,
    tools: runtime.tools,
    connected: runtime.connected,
  }));
}
