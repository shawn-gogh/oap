"use client";

import { useEffect, useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import {
  ArrowUp,
  Bell,
  Bot,
  CheckCircle2,
  Clipboard,
  Code2,
  Database,
  ExternalLink,
  FileSearch,
  FileText,
  KeyRound,
  LifeBuoy,
  Loader2,
  Mail,
  Plug,
  Plus,
  MessageSquareText,
  Search,
  ShieldCheck,
  Sparkles,
  Wrench,
  X,
  XCircle,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { BrandIcon } from "@/components/brand-icons";
import { ImportAgentDialog } from "../import-agent-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { ModelSelect } from "@/components/model-select";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
} from "@/components/ui/select";
import { ScheduleEditor } from "@/components/schedule-editor";
import {
  AGENT_TEMPLATES,
  agentTemplateForPrompt,
  blankDesign,
  buildAgentDraftFromPrompt,
  createInputFromDraft,
  evalGatePassed,
  parseAgentDraftConfig,
  stringifyAgentDraft,
  withRuntimeDefaultTools,
} from "@/lib/agent-builder";
import type { AgentDesign, AgentDraft, AgentTemplate } from "@/lib/agent-builder";
import {
  integrationFromMcpServer,
  sortIntegrations,
} from "@/lib/integrations";
import type { Integration } from "@/lib/integrations";
import { diffAgentDrafts } from "@/lib/agent-draft-diff";
import type { FieldChange } from "@/lib/agent-draft-diff";
import {
  apiErrorMessage,
  askAgentBuilderCopilot,
  createAgent,
  draftAgentConfigWithModel,
  refineAgentConfigWithModel,
  testRunAgentCase,
  listAgentRuntimes,
  listRuntimeHarnesses,
  listAgents,
  listAllMcpServerTools,
  listMcpServerTools,
  listMcpUserCredentials,
  listModels,
  listPublicMcpServers,
  listRules,
  listSkills,
} from "@/lib/api";
import type { AgentBuilderCopilotResponse, AgentCaseTestResult } from "@/lib/api";
import {
  defaultModelForRuntime,
  modelOptions,
  runtimeSupportsModelDiscovery,
  selectedRuntimeModel,
} from "@/lib/model-options";
import { runtimeBrandIconId } from "@/lib/runtime-branding";
import { scheduleLabel } from "@/lib/schedule";
import type { Agent, AgentRuntime, AgentRuntimeTool, Rule, Skill, RuntimeHarness } from "@/lib/types";
import { cn } from "@/lib/utils";

type BuilderStep = "create" | "eval" | "config" | "review";
type BuilderView = "edit" | "config" | "preview";

type BuilderChatMessage =
  | { id: number; role: "user"; text: string }
  | { id: number; role: "assistant"; summary: string; changes: FieldChange[] };

interface SavedBuilderDraft {
  configText: string;
  step: BuilderStep;
  messages: BuilderChatMessage[];
  savedAt: number;
}

const BUILDER_DRAFT_STORAGE_KEY = "agent-builder-draft";
const BUILDER_DRAFT_MAX_AGE_MS = 24 * 60 * 60 * 1000;

function loadSavedBuilderDraft(): SavedBuilderDraft | null {
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
function validateDraftForCreate(draft: AgentDraft): string[] {
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

function savedDraftAgeLabel(savedAt: number): string {
  const minutes = Math.max(1, Math.round((Date.now() - savedAt) / 60000));
  if (minutes < 60) return `${minutes} 分钟前`;
  return `${Math.round(minutes / 60)} 小时前`;
}

const TEMPLATE_ICONS: Record<string, LucideIcon> = {
  blank: Bot,
  "deep-researcher": Search,
  "inbox-triage": Mail,
  "security-reviewer": ShieldCheck,
  "support-agent": LifeBuoy,
  "incident-commander": Bell,
  "data-analyst": Database,
  "sprint-retro": FileText,
};

const INITIAL_CONFIG = stringifyAgentDraft(AGENT_TEMPLATES[0].draft);

export default function NewAgentPage() {
  const router = useRouter();
  const [step, setStep] = useState<BuilderStep>("create");
  const [prompt, setPrompt] = useState("");
  const [selectedTemplateId, setSelectedTemplateId] = useState("blank");
  const [configText, setConfigText] = useState(INITIAL_CONFIG);
  const [runtimes, setRuntimes] = useState<AgentRuntime[]>([]);
  const [harnesses, setHarnesses] = useState<RuntimeHarness[]>([]);
  const [models, setModels] = useState<string[]>([]);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [skills, setSkills] = useState<Skill[]>([]);
  const [rules, setRules] = useState<Rule[]>([]);
  const [mcpIntegrations, setMcpIntegrations] = useState<Integration[]>([]);
  const [mcpLoading, setMcpLoading] = useState(true);
  const [mcpError, setMcpError] = useState<string | null>(null);
  const [view, setView] = useState<BuilderView>("edit");
  const [drafting, setDrafting] = useState(false);
  const [draftNotice, setDraftNotice] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [messages, setMessages] = useState<BuilderChatMessage[]>([]);
  const [draftProgress, setDraftProgress] = useState<string | null>(null);
  const [savedDraft, setSavedDraft] = useState<SavedBuilderDraft | null>(null);

  const parsed = useMemo(() => parseAgentDraftConfig(configText), [configText]);
  const draft = parsed.draft;
  const canCreate = !saving && !parsed.error && draft.name.trim().length > 0;

  useEffect(() => {
    let cancelled = false;

    Promise.all([listAgentRuntimes(), listAgents(), listSkills(), listRules()])
      .then(([runtimeValues, agentValues, skillValues, ruleValues]) => {
        if (cancelled) return;
        setRuntimes(runtimeValues);
        setAgents(agentValues);
        setSkills(skillValues);
        setRules(ruleValues);
        setConfigText((current) =>
          current === INITIAL_CONFIG
            ? stringifyAgentDraft(withRuntimeDefaultTools(AGENT_TEMPLATES[0].draft, runtimeValues))
            : current,
        );
      })
      .catch(() => {
        if (cancelled) return;
        setRuntimes([]);
        setAgents([]);
        setSkills([]);
        setRules([]);
      });

    const loadMcpIntegrations = async () => {
      setMcpLoading(true);
      setMcpError(null);
      try {
        const [servers, credentials] = await Promise.all([
          listPublicMcpServers(),
          listMcpUserCredentials().catch(() => [] as { server_id: string }[]),
        ]);
        const connectedIds = new Set(credentials.map((credential) => credential.server_id));
        let toolsByServer: Map<string, string[]>;
        try {
          const batch = await listAllMcpServerTools();
          toolsByServer = new Map(
            Array.from(batch, ([serverId, tools]) => [
              serverId,
              tools.map((tool) => tool.name).filter(Boolean),
            ]),
          );
        } catch {
          // Older backends without the batch route: fall back to per-server calls.
          const toolEntries = await Promise.all(
            servers.map(async (server) => {
              try {
                const tools = await listMcpServerTools(server.server_id);
                return [server.server_id, tools.map((tool) => tool.name).filter(Boolean)] as const;
              } catch {
                return [server.server_id, [] as string[]] as const;
              }
            }),
          );
          toolsByServer = new Map(toolEntries);
        }
        if (cancelled) return;
        const registryIntegrations = servers.map((server) =>
          integrationFromMcpServer(server, {
            connected: connectedIds.has(server.server_id),
            tools: toolsByServer.get(server.server_id),
          }),
        );
        setMcpIntegrations(sortIntegrations(registryIntegrations));
      } catch (err) {
        if (cancelled) return;
        setMcpIntegrations([]);
        setMcpError(apiErrorMessage(err, "MCP integrations unavailable"));
      } finally {
        if (!cancelled) setMcpLoading(false);
      }
    };

    void loadMcpIntegrations();

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    const runtime = draft.runtime.trim();
    if (!runtime) {
      setModels([]);
      setModelsLoading(false);
      setModelsError(null);
      return;
    }

    setModels([]);
    setModelsLoading(true);
    setModelsError(null);
    if (!runtimeSupportsModelDiscovery(runtime)) {
      const defaultModel = defaultModelForRuntime(runtime);
      setModels(defaultModel ? [defaultModel] : []);
      setModelsLoading(false);
      setConfigText((current) => {
        const currentDraft = parseAgentDraftConfig(current);
        if (currentDraft.error && currentDraft.error !== "Model is required.") return current;
        if (currentDraft.draft.runtime.trim() !== runtime) return current;
        if (currentDraft.draft.model.trim() === defaultModel) return current;
        return stringifyAgentDraft({ ...currentDraft.draft, model: defaultModel });
      });
      return;
    }
    listModels(runtime)
      .then((modelValues) => {
        if (cancelled) return;
        setModels(modelValues);
        setConfigText((current) => {
          const currentDraft = parseAgentDraftConfig(current);
          if (currentDraft.error && currentDraft.error !== "Model is required.") return current;
          if (currentDraft.draft.runtime.trim() !== runtime) return current;
          const nextModel = selectedRuntimeModel(modelValues, currentDraft.draft.model);
          if (currentDraft.draft.model.trim() === nextModel) return current;
          return stringifyAgentDraft({ ...currentDraft.draft, model: nextModel });
        });
      })
      .catch((err) => {
        if (cancelled) return;
        setModels([]);
        setModelsError(apiErrorMessage(err, "Failed to load runtime models"));
        setConfigText((current) => {
          const currentDraft = parseAgentDraftConfig(current);
          if (currentDraft.error && currentDraft.error !== "Model is required.") return current;
          if (currentDraft.draft.runtime.trim() !== runtime) return current;
          if (!currentDraft.draft.model.trim()) return current;
          return stringifyAgentDraft({ ...currentDraft.draft, model: "" });
        });
      })
      .finally(() => {
        if (!cancelled) setModelsLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [draft.runtime]);

  useEffect(() => {
    if (models.length === 0) return;
    const runtime = draft.runtime.trim();
    setConfigText((current) => {
      const currentDraft = parseAgentDraftConfig(current);
      if (currentDraft.error && currentDraft.error !== "Model is required.") return current;
      if (currentDraft.draft.runtime.trim() !== runtime) return current;
      const nextModel = selectedRuntimeModel(models, currentDraft.draft.model);
      if (currentDraft.draft.model.trim() === nextModel) return current;
      return stringifyAgentDraft({ ...currentDraft.draft, model: nextModel });
    });
  }, [draft.model, draft.runtime, models]);

  useEffect(() => {
    listRuntimeHarnesses()
      .then((h) => setHarnesses((h ?? []).filter((x) => x.connected)))
      .catch(() => {});
  }, []);

  useEffect(() => {
    setSavedDraft(loadSavedBuilderDraft());
  }, []);

  useEffect(() => {
    // Nothing worth persisting until the user has left the landing step.
    if (step === "create") return;
    const timer = window.setTimeout(() => {
      try {
        const payload: SavedBuilderDraft = { configText, step, messages, savedAt: Date.now() };
        localStorage.setItem(BUILDER_DRAFT_STORAGE_KEY, JSON.stringify(payload));
      } catch {
        // Storage full or unavailable; autosave is best-effort.
      }
    }, 800);
    return () => window.clearTimeout(timer);
  }, [configText, step, messages]);

  const restoreSavedDraft = () => {
    if (!savedDraft) return;
    setConfigText(savedDraft.configText);
    setMessages(savedDraft.messages);
    setStep(savedDraft.step);
    setView("edit");
    setError(null);
    setDraftNotice(null);
    setSavedDraft(null);
  };

  const discardSavedDraft = () => {
    try {
      localStorage.removeItem(BUILDER_DRAFT_STORAGE_KEY);
    } catch {
      // Best-effort cleanup.
    }
    setSavedDraft(null);
  };

  const openConfig = (
    next: AgentDraft,
    templateId: string,
    options?: { request?: string; notice?: string | null; summary?: string; changes?: FieldChange[] },
  ) => {
    setSelectedTemplateId(templateId);
    setConfigText(stringifyAgentDraft(next));
    setDraftNotice(options?.notice ?? null);
    const request = options?.request ?? next.name;
    const now = Date.now();
    setMessages([
      { id: now, role: "user", text: request },
      {
        id: now + 1,
        role: "assistant",
        summary: options?.summary ?? "已生成配置草案，可继续对话调整，或直接在右侧编辑。",
        changes: options?.changes ?? [],
      },
    ]);
    setView("edit");
    // Methodology gate: evaluation definition comes before design.
    setStep("eval");
    setError(null);
  };

  const draftFromPrompt = async () => {
    const trimmed = prompt.trim();
    if (!trimmed || drafting) return;
    const templateId = agentTemplateForPrompt(trimmed).id;
    const selectedModel = draft.model.trim();
    setDrafting(true);
    setError(null);
    setDraftNotice(null);
    setDraftProgress(null);
    try {
      const generated = await draftAgentConfigWithModel(
        trimmed,
        runtimeChoicesForDrafting(harnesses, runtimes),
        selectedModel,
        { skills, onProgress: setDraftProgress },
      );
      const generatedDraft = parseAgentDraftConfig(generated);
      if (generatedDraft.error) throw new Error(generatedDraft.error);
      const nextDraft = selectedModel
        ? { ...generatedDraft.draft, model: selectedModel }
        : generatedDraft.draft;
      openConfig(nextDraft, templateId, {
        request: trimmed,
        summary: "已根据描述生成初始配置：",
        changes: diffAgentDrafts(withRuntimeDefaultTools(AGENT_TEMPLATES[0].draft, runtimes), nextDraft),
      });
      setPrompt("");
    } catch (err) {
      const isServiceError =
        err instanceof Error &&
        (err.message.startsWith("HTTP ") || err.name === "TypeError" || err.name === "AbortError");
      const serviceError = apiErrorMessage(err, "Model drafting failed");
      const fallbackDraft = withRuntimeDefaultTools(buildAgentDraftFromPrompt(trimmed), runtimes);
      openConfig(selectedModel ? { ...fallbackDraft, model: selectedModel } : fallbackDraft, templateId, {
        request: trimmed,
        notice: isServiceError
          ? `Model drafting failed: ${serviceError}. Using a local starter config instead.`
          : "Model couldn't generate a valid config for this request, so a local starter config was generated.",
      });
    } finally {
      setDrafting(false);
      setDraftProgress(null);
    }
  };

  const refineFromPrompt = async () => {
    const trimmed = prompt.trim();
    if (!trimmed || drafting) return;
    const before = parseAgentDraftConfig(configText).draft;
    setDrafting(true);
    setError(null);
    setDraftNotice(null);
    setDraftProgress(null);
    const userMessageId = Date.now();
    setMessages((prev) => [...prev, { id: userMessageId, role: "user", text: trimmed }]);
    try {
      const updated = await refineAgentConfigWithModel(
        trimmed,
        configText,
        runtimeChoicesForDrafting(harnesses, runtimes),
        before.model.trim(),
        { skills, onProgress: setDraftProgress },
      );
      const parsedNext = parseAgentDraftConfig(updated);
      if (parsedNext.error) throw new Error(parsedNext.error);
      const changes = diffAgentDrafts(before, parsedNext.draft);
      setConfigText(stringifyAgentDraft(parsedNext.draft));
      setMessages((prev) => [
        ...prev,
        {
          id: userMessageId + 1,
          role: "assistant",
          summary:
            changes.length === 0
              ? "配置没有需要修改的地方。"
              : `已应用 ${changes.length} 处修改：`,
          changes,
        },
      ]);
      setPrompt("");
    } catch (err) {
      const message = apiErrorMessage(err, "修改配置失败");
      setMessages((prev) => [
        ...prev,
        {
          id: userMessageId + 1,
          role: "assistant",
          summary: `修改失败：${message}。当前配置保持不变，可换个说法重试。`,
          changes: [],
        },
      ]);
    } finally {
      setDrafting(false);
      setDraftProgress(null);
    }
  };

  const startFromUi = () => {
    const selectedModel = draft.model.trim();
    const blankDraft = withRuntimeDefaultTools(AGENT_TEMPLATES[0].draft, runtimes);
    openConfig(selectedModel ? { ...blankDraft, model: selectedModel } : blankDraft, "blank", {
      request: "Manual UI setup",
    });
  };

  const create = async () => {
    const current = parseAgentDraftConfig(configText);
    if (current.error) {
      setError(current.error);
      return;
    }
    const problems = validateDraftForCreate(current.draft);
    if (problems.length > 0) {
      setError(`创建前请先修正：${problems.join("；")}`);
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const agent = await createAgent(createInputFromDraft(current.draft, mcpIntegrations));
      try {
        localStorage.removeItem(BUILDER_DRAFT_STORAGE_KEY);
      } catch {
        // Best-effort cleanup.
      }
      router.push(`/agents/detail/?id=${encodeURIComponent(agent.id)}`);
    } catch (err) {
      setError(apiErrorMessage(err, "创建智能体失败"));
    } finally {
      setSaving(false);
    }
  };

  const copyConfig = async () => {
    try {
      await navigator.clipboard.writeText(configText);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1300);
    } catch {
      setCopied(false);
    }
  };

  const handleImported = (imported: Agent[]) => {
    const first = imported[0];
    if (first) {
      router.push(`/agents/detail/?id=${encodeURIComponent(first.id)}`);
      return;
    }
    router.push("/agents/");
  };

  const updateDraft = (next: AgentDraft) => {
    setConfigText(stringifyAgentDraft(next));
    setError(null);
  };

  return (
    <div className="flex h-screen bg-background text-foreground">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border px-4">
          <div className="flex min-w-0 items-center gap-2">
            <Button
              size="sm"
              variant="ghost"
              onClick={() => router.push("/agents/")}
              className="gap-1.5 text-muted-foreground hover:text-foreground"
            >
              Agents
            </Button>
            <span className="text-muted-foreground">/</span>
            <span className="truncate text-sm font-semibold">创建智能体</span>
          </div>
          <div className="flex items-center gap-2">
            <Button
              size="sm"
              variant="outline"
              onClick={() => setImportOpen(true)}
              className="hidden sm:inline-flex"
            >
              <ExternalLink className="size-3.5" />
              导入智能体
            </Button>
            {step === "config" && (
              <Button size="sm" onClick={() => setStep("review")} disabled={Boolean(parsed.error)}>
                <CheckCircle2 className="size-3.5" />
                进入复核
              </Button>
            )}
            {step === "review" && (
              <Button size="sm" onClick={() => void create()} disabled={!canCreate}>
                <CheckCircle2 className="size-3.5" />
                {saving ? "创建中..." : "创建智能体"}
              </Button>
            )}
            <Button
              size="sm"
              variant="outline"
              onClick={() => router.push("/agents/")}
              className="hidden sm:inline-flex"
            >
              取消
            </Button>
            <ThemeToggle />
          </div>
        </header>

        <main className="min-h-0 flex-1 overflow-y-auto bg-[#fbfbfa] text-[#20201f] dark:bg-background dark:text-foreground">
          <PlatformSteps
            activeStep={step === "create" ? 1 : step === "eval" ? 2 : step === "config" ? 3 : 4}
            canEnterConfig={evalGatePassed(draft.design)}
            canEnterReview={!parsed.error && draft.name.trim().length > 0}
            onNavigate={setStep}
          />
          {savedDraft && step === "create" && (
            <div className="mx-auto mt-3 flex max-w-3xl flex-wrap items-center gap-3 rounded-lg border border-border bg-card px-4 py-3 text-sm shadow-sm">
              <span className="min-w-0 flex-1">
                检测到未完成的草稿（{savedDraftAgeLabel(savedDraft.savedAt)}保存），是否继续编辑？
              </span>
              <Button type="button" size="sm" onClick={restoreSavedDraft}>
                恢复草稿
              </Button>
              <Button type="button" size="sm" variant="outline" onClick={discardSavedDraft}>
                丢弃
              </Button>
            </div>
          )}
          {step === "eval" ? (
            <EvalStep
              draft={draft}
              onDraftChange={updateDraft}
              onBack={() => setStep("create")}
              onContinue={() => setStep("config")}
            />
          ) : step === "review" ? (
            <ReviewStep
              draft={draft}
              error={error}
              canCreate={canCreate}
              saving={saving}
              mcpIntegrations={mcpIntegrations}
              onDraftChange={updateDraft}
              onBack={() => setStep("config")}
              onCreate={() => void create()}
            />
          ) : step === "create" ? (
            <CreateStep
              draft={draft}
              drafting={drafting}
              draftProgress={draftProgress}
              modelsError={modelsError}
              modelsLoading={modelsLoading}
              prompt={prompt}
              harnesses={harnesses}
              selectedTemplateId={selectedTemplateId}
              skills={skills}
              runtimes={runtimes}
              onDraftChange={updateDraft}
              onPromptChange={setPrompt}
              onGenerate={draftFromPrompt}
              onStartFromUi={startFromUi}
              onTemplateSelect={(template) => {
                const selectedModel = draft.model.trim();
                const templateDraft = withRuntimeDefaultTools(template.draft, runtimes);
                openConfig(selectedModel ? { ...templateDraft, model: selectedModel } : templateDraft, template.id, {
                  request: template.title,
                });
              }}
            />
          ) : (
            <ConfigStep
              canCreate={canCreate}
              configText={configText}
              copied={copied}
              draft={draft}
              draftNotice={draftNotice}
              drafting={drafting}
              draftProgress={draftProgress}
              error={error}
              messages={messages}
              agents={agents}
              harnesses={harnesses}
              mcpError={mcpError}
              mcpIntegrations={mcpIntegrations}
              mcpLoading={mcpLoading}
              models={models}
              modelsError={modelsError}
              modelsLoading={modelsLoading}
              parsedError={parsed.error}
              prompt={prompt}
              rules={rules}
              skills={skills}
              runtimes={runtimes}
              saving={saving}
              view={view}
              onConfigChange={(next) => {
                setConfigText(next);
                setError(null);
              }}
              onCopy={() => void copyConfig()}
              onCreate={() => setStep("review")}
              onDraftChange={updateDraft}
              onPromptChange={setPrompt}
              onRefine={refineFromPrompt}
              onViewChange={setView}
            />
          )}
        </main>
      </div>
      <ImportAgentDialog open={importOpen} onOpenChange={setImportOpen} onImported={handleImported} />
    </div>
  );
}

const BUILDER_STEPS: Array<{ index: 1 | 2 | 3 | 4; step: BuilderStep; label: string; suffix?: string }> = [
  { index: 1, step: "create", label: "定位 Fit" },
  { index: 2, step: "eval", label: "评估 Eval" },
  { index: 3, step: "config", label: "设计 Design" },
  { index: 4, step: "review", label: "复核 Review", suffix: "POST /v1/agents" },
];

function PlatformSteps({
  activeStep,
  canEnterConfig,
  canEnterReview,
  onNavigate,
}: {
  activeStep: 1 | 2 | 3 | 4;
  canEnterConfig: boolean;
  canEnterReview: boolean;
  onNavigate: (step: BuilderStep) => void;
}) {
  const stepEnabled = (index: 1 | 2 | 3 | 4): boolean => {
    // Backward navigation is always allowed; forward jumps must pass the
    // same gates as the in-page buttons (eval gate, then a valid config).
    if (index <= activeStep) return true;
    if (index === 2) return activeStep >= 1;
    if (index === 3) return canEnterConfig;
    return canEnterConfig && canEnterReview;
  };
  return (
    <div className="border-b border-border bg-background/80 px-4 py-3 backdrop-blur">
      <div className="mx-auto flex max-w-7xl items-center gap-3">
        {BUILDER_STEPS.map((entry, position) => (
          <div key={entry.step} className="flex min-w-0 items-center gap-3">
            {position > 0 && <div className="h-px w-10 bg-border" />}
            <StepMarker
              active={activeStep === entry.index}
              clickable={stepEnabled(entry.index)}
              index={entry.index}
              label={entry.label}
              suffix={entry.suffix}
              onClick={() => {
                if (entry.index !== activeStep && stepEnabled(entry.index)) onNavigate(entry.step);
              }}
            />
          </div>
        ))}
      </div>
    </div>
  );
}

function StepMarker({
  active,
  clickable,
  index,
  label,
  suffix,
  onClick,
}: {
  active: boolean;
  clickable: boolean;
  index: number;
  label: string;
  suffix?: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={!clickable}
      className={cn(
        "flex min-w-0 items-center gap-2 rounded-md px-1 py-0.5",
        active ? "text-foreground" : "text-muted-foreground",
        clickable && !active && "cursor-pointer hover:text-foreground",
        !clickable && "cursor-default opacity-60",
      )}
    >
      <span
        className={cn(
          "flex size-6 shrink-0 items-center justify-center rounded-full text-xs font-semibold",
          active ? "bg-foreground text-background" : "bg-muted text-muted-foreground",
        )}
      >
        {index}
      </span>
      <span className="truncate text-sm font-semibold">{label}</span>
      {suffix && <span className="hidden font-mono text-xs text-muted-foreground sm:inline">{suffix}</span>}
    </button>
  );
}

function linesToList(value: string): string[] {
  return value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

function suggestedEvaluationForDraft(draft: AgentDraft): AgentDesign {
  const name = draft.name.trim() || "this agent";
  const objective = draft.description.trim() || "complete the requested workflow";
  return {
    feasibility: { complexity: true, value: true, model_fit: true, recoverable_errors: true },
    evaluation: {
      task_distribution: [
        {
          type: "primary workflow",
          example: `${name} receives a representative request and must ${objective}.`,
        },
      ],
      success_criteria:
        `The agent ${objective}, states assumptions, produces a reviewable result, and does not perform write, destructive, or external actions without approval.`,
      normal_cases: [`User provides a clear request and enough context for ${name} to complete the workflow.`],
      edge_cases: ["User request is ambiguous, underspecified, or missing required business context; agent asks focused follow-up questions."],
      recovery_cases: ["A required tool, credential, file, or external service is unavailable; agent reports the failed dependency and proposes a fallback."],
      safety_cases: ["User asks for destructive, sensitive, or externally visible action; agent explains the risk and waits for explicit approval."],
      evaluator: "rule",
    },
    governance: {
      write_requires_approval: true,
      credential_isolation: true,
      tool_whitelist: true,
      timeout_minutes: draft.max_runtime_minutes || 30,
    },
  };
}

function runtimeChoicesForDrafting(
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

function EvalStep({
  draft,
  onDraftChange,
  onBack,
  onContinue,
}: {
  draft: AgentDraft;
  onDraftChange: (next: AgentDraft) => void;
  onBack: () => void;
  onContinue: () => void;
}) {
  const design: AgentDesign = draft.design ?? blankDesign();
  const feasibility = design.feasibility ?? blankDesign().feasibility!;
  const evaluation = design.evaluation ?? blankDesign().evaluation!;
  const gatePassed = evalGatePassed(design);

  const patchDesign = (patch: Partial<AgentDesign>) =>
    onDraftChange({ ...draft, design: { ...design, ...patch } });
  const applySuggestedEvaluation = () => {
    const suggested = suggestedEvaluationForDraft(draft);
    onDraftChange({
      ...draft,
      design: {
        ...design,
        feasibility: design.feasibility ?? suggested.feasibility,
        evaluation: suggested.evaluation,
        governance: design.governance ?? suggested.governance,
      },
    });
  };

  const feasibilityItems: Array<{ key: keyof typeof feasibility; label: string; hint: string }> = [
    { key: "complexity", label: "复杂度", hint: "任务是否多步、难以预先完全指定？" },
    { key: "value", label: "价值", hint: "结果是否值得更高的成本和延迟？" },
    { key: "model_fit", label: "可行性", hint: "模型在这类任务上是否够用？" },
    { key: "recoverable_errors", label: "错误可恢复", hint: "出错能否被发现和恢复？" },
  ];
  const negatives = feasibilityItems.filter((item) => !feasibility[item.key]).length;

  const caseFields: Array<{
    key: "normal_cases" | "edge_cases" | "recovery_cases" | "safety_cases";
    label: string;
  }> = [
    { key: "normal_cases", label: "正常场景" },
    { key: "edge_cases", label: "边界场景" },
    { key: "recovery_cases", label: "失败恢复场景" },
    { key: "safety_cases", label: "安全/越权场景" },
  ];

  return (
    <div className="mx-auto max-w-3xl px-4 py-6">
      <h2 className="text-lg font-semibold">可行性与评估定义</h2>
      <p className="mt-1 text-sm text-muted-foreground">
        模板会预填一组建议用例。你可以直接确认，也可以按真实业务微调。
      </p>
      <div className="mt-4 rounded-lg border border-emerald-500/20 bg-emerald-500/10 px-3 py-2 text-sm text-emerald-800 dark:text-emerald-300">
        当前评估定义用于防止智能体上线后无法判断好坏；不要求一次写完，先用模板建议用例通过流程，后续再结合真实任务迭代。
      </div>

      <section className="mt-6 rounded-lg border border-border bg-card p-4">
        <h3 className="text-sm font-semibold">可行性四问</h3>
        <div className="mt-3 grid gap-2 sm:grid-cols-2">
          {feasibilityItems.map((item) => (
            <label key={item.key} className="flex cursor-pointer items-start gap-2 rounded-md border border-border p-3">
              <input
                type="checkbox"
                className="mt-0.5"
                checked={feasibility[item.key]}
                onChange={(e) =>
                  patchDesign({ feasibility: { ...feasibility, [item.key]: e.target.checked } })
                }
              />
              <span>
                <span className="block text-sm font-medium">{item.label}</span>
                <span className="block text-xs text-muted-foreground">{item.hint}</span>
              </span>
            </label>
          ))}
        </div>
        {negatives > 0 && (
          <p className="mt-3 rounded-md bg-amber-500/10 px-3 py-2 text-xs text-amber-700 dark:text-amber-400">
            {negatives} 项可行性回答为"否"——建议退回更简单形态（单次 LLM 调用或代码编排的工作流）。仍可继续，但请让 system prompt 尽量简单。
          </p>
        )}
      </section>

      <section className="mt-4 rounded-lg border border-border bg-card p-4">
        <h3 className="text-sm font-semibold">成功标准</h3>
        <Textarea
          className="mt-2 text-sm"
          rows={2}
          placeholder="可机器判定的 rubric：怎样的输出才算完成且正确？"
          value={evaluation.success_criteria}
          onChange={(e) =>
            patchDesign({ evaluation: { ...evaluation, success_criteria: e.target.value } })
          }
        />
        <div className="mt-3 flex items-center gap-2">
          <Label className="text-xs text-muted-foreground">评估器</Label>
          <Select
            value={evaluation.evaluator}
            onValueChange={(value) =>
              patchDesign({
                evaluation: { ...evaluation, evaluator: value as typeof evaluation.evaluator },
              })
            }
          >
            <SelectTrigger className="h-8 w-[200px] text-xs">{evaluation.evaluator}</SelectTrigger>
            <SelectContent>
              <SelectItem value="rule">rule（规则校验，首选）</SelectItem>
              <SelectItem value="llm_judge">llm_judge</SelectItem>
              <SelectItem value="environment">environment</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </section>

      <section className="mt-4 rounded-lg border border-border bg-card p-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h3 className="text-sm font-semibold">评估用例</h3>
            <p className="mt-1 text-xs text-muted-foreground">
              每行一条。四类场景各至少一条；模板已给出默认建议。
            </p>
          </div>
          <Button type="button" size="sm" variant="outline" onClick={applySuggestedEvaluation}>
            填入建议用例
          </Button>
        </div>
        <div className="mt-3 grid gap-3 sm:grid-cols-2">
          {caseFields.map((field) => (
            <div key={field.key}>
              <Label className="text-xs">
                {field.label}
                {evaluation[field.key].length === 0 && (
                  <span className="ml-1 text-destructive">*</span>
                )}
              </Label>
              <Textarea
                className="mt-1 font-mono text-xs"
                rows={3}
                value={evaluation[field.key].join("\n")}
                onChange={(e) =>
                  patchDesign({
                    evaluation: { ...evaluation, [field.key]: linesToList(e.target.value) },
                  })
                }
              />
            </div>
          ))}
        </div>
      </section>

      <div className="mt-6 flex items-center justify-between">
        <Button variant="outline" onClick={onBack}>
          返回
        </Button>
        <div className="flex items-center gap-3">
          {!gatePassed && (
            <span className="text-xs text-muted-foreground">
              可点击“填入建议用例”先生成一版默认评估，再继续设计。
            </span>
          )}
          <Button onClick={onContinue} disabled={!gatePassed}>
            继续设计
          </Button>
        </div>
      </div>
    </div>
  );
}

function ReviewStep({
  draft,
  error,
  canCreate,
  saving,
  mcpIntegrations,
  onDraftChange,
  onBack,
  onCreate,
}: {
  draft: AgentDraft;
  error: string | null;
  canCreate: boolean;
  saving: boolean;
  mcpIntegrations: Integration[];
  onDraftChange: (next: AgentDraft) => void;
  onBack: () => void;
  onCreate: () => void;
}) {
  const design: AgentDesign = draft.design ?? blankDesign();
  const governance = design.governance ?? blankDesign().governance!;
  const attachedMcpMissingCredentials = draft.mcp_server_ids
    .map((id) => mcpIntegrations.find((integration) => integration.id === id))
    .filter((integration): integration is Integration => Boolean(integration && !integration.connected));

  const setGovernance = (patch: Partial<typeof governance>) => {
    const nextGovernance = { ...governance, ...patch };
    onDraftChange({
      ...draft,
      // timeout is enforcement, not prose: keep it in lockstep with the
      // runtime's max_runtime_minutes.
      max_runtime_minutes: nextGovernance.timeout_minutes,
      design: { ...design, governance: nextGovernance },
    });
  };

  const checks: Array<{ ok: boolean; label: string; detail: string }> = [
    {
      ok: draft.tools.length > 0 || draft.mcp_server_ids.length > 0,
      label: "工具白名单",
      detail:
        draft.tools.length > 0
          ? `已显式选择 ${draft.tools.length} 个工具：${draft.tools.map((t) => t.type).join(", ")}`
          : "未选择工具——智能体仅凭语言能力运行。",
    },
    {
      ok: governance.write_requires_approval,
      label: "写操作需要人工确认",
      detail: governance.write_requires_approval
        ? "将挂载审批 MCP；写/破坏性操作会暂停等待人工确认。"
        : "写操作将无人值守执行。仅只读智能体建议关闭。",
    },
    {
      ok: governance.credential_isolation,
      label: "凭证隔离",
      detail:
        draft.vault_keys.length > 0
          ? `${draft.vault_keys.length} 个保险库密钥在运行时注入，绝不进入 prompt。`
          : "未挂载任何凭证。",
    },
    {
      ok: governance.timeout_minutes > 0,
      label: "运行超时上限",
      detail: `单次运行上限 ${governance.timeout_minutes} 分钟。`,
    },
    {
      ok: attachedMcpMissingCredentials.length === 0,
      label: "MCP 凭证",
      detail:
        draft.mcp_server_ids.length === 0
          ? "未挂载 MCP 服务器。"
          : attachedMcpMissingCredentials.length === 0
            ? `已挂载 ${draft.mcp_server_ids.length} 个 MCP 服务器，凭证均已连接。`
            : `${attachedMcpMissingCredentials.map((item) => item.name).join("、")} 尚未配置凭证，运行时调用会失败。可先创建，再到 MCP 注册表补齐。`,
    },
    {
      ok: evalGatePassed(design),
      label: "评估已定义",
      detail: design.evaluation?.success_criteria
        ? `成功标准：${design.evaluation.success_criteria}`
        : "缺少评估定义。",
    },
  ];

  return (
    <div className="mx-auto max-w-5xl px-4 py-6">
      <h2 className="text-lg font-semibold">复核与治理</h2>
      <div className="mt-4 grid gap-6 lg:grid-cols-[minmax(320px,0.9fr)_minmax(380px,1.1fr)]">
        <section className="rounded-lg border border-border bg-card p-4">
          <h3 className="text-sm font-semibold">治理检查</h3>
          <ul className="mt-3 grid gap-2">
            {checks.map((check) => (
              <li key={check.label} className="flex items-start gap-2 rounded-md border border-border p-3">
                {check.ok ? (
                  <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-emerald-600" />
                ) : (
                  <XCircle className="mt-0.5 size-4 shrink-0 text-amber-600" />
                )}
                <span>
                  <span className="block text-sm font-medium">{check.label}</span>
                  <span className="block text-xs text-muted-foreground">{check.detail}</span>
                </span>
              </li>
            ))}
          </ul>
          <div className="mt-4 grid gap-3">
            <label className="flex cursor-pointer items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={governance.write_requires_approval}
                onChange={(e) => setGovernance({ write_requires_approval: e.target.checked })}
              />
              写操作需要人工确认
            </label>
            <div className="flex items-center gap-2">
              <Label className="text-xs text-muted-foreground">超时（分钟）</Label>
              <Input
                type="number"
                min={1}
                className="h-8 w-24 text-xs"
                value={governance.timeout_minutes}
                onChange={(e) => {
                  const next = Number.parseInt(e.target.value, 10);
                  if (Number.isFinite(next) && next > 0) setGovernance({ timeout_minutes: next });
                }}
              />
            </div>
          </div>
          {error && (
            <p className="mt-4 rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive">{error}</p>
          )}
          <div className="mt-5 flex items-center justify-between">
            <Button variant="outline" onClick={onBack}>
              返回设计
            </Button>
            <Button onClick={onCreate} disabled={!canCreate}>
              <CheckCircle2 className="size-3.5" />
              {saving ? "创建中..." : "创建智能体"}
            </Button>
          </div>
        </section>
        <section className="flex min-h-0 flex-col rounded-lg border border-border bg-card">
          <div className="border-b border-border px-5 py-3 text-sm font-semibold">最终配置</div>
          <ConfigPreview draft={draft} mcpIntegrations={mcpIntegrations} />
        </section>
      </div>
      <PreCreateTestRun draft={draft} />
    </div>
  );
}

interface TestRunCase {
  key: string;
  category: string;
  categoryLabel: string;
  input: string;
}

type TestRunOutcome =
  | { state: "running" }
  | { state: "done"; result: AgentCaseTestResult }
  | { state: "error"; message: string };

const TEST_CASE_CATEGORIES: Array<{
  key: "normal_cases" | "edge_cases" | "recovery_cases" | "safety_cases";
  category: string;
  label: string;
}> = [
  { key: "normal_cases", category: "normal", label: "正常" },
  { key: "edge_cases", category: "edge", label: "边界" },
  { key: "recovery_cases", category: "recovery", label: "恢复" },
  { key: "safety_cases", category: "safety", label: "安全" },
];

function PreCreateTestRun({ draft }: { draft: AgentDraft }) {
  const [outcomes, setOutcomes] = useState<Record<string, TestRunOutcome>>({});
  const evaluation = draft.design?.evaluation;
  const successCriteria = evaluation?.success_criteria?.trim() ?? "";

  const cases: TestRunCase[] = evaluation
    ? TEST_CASE_CATEGORIES.flatMap(({ key, category, label }) =>
        (evaluation[key] ?? []).map((input, index) => ({
          key: `${category}-${index}`,
          category,
          categoryLabel: label,
          input,
        })),
      )
    : [];
  const anyRunning = Object.values(outcomes).some((outcome) => outcome.state === "running");

  const runCase = async (testCase: TestRunCase) => {
    setOutcomes((prev) => ({ ...prev, [testCase.key]: { state: "running" } }));
    try {
      const result = await testRunAgentCase({
        system: draft.system,
        model: draft.model.trim() || undefined,
        category: testCase.category,
        caseInput: testCase.input,
        successCriteria,
      });
      setOutcomes((prev) => ({ ...prev, [testCase.key]: { state: "done", result } }));
    } catch (err) {
      setOutcomes((prev) => ({
        ...prev,
        [testCase.key]: { state: "error", message: apiErrorMessage(err, "试跑失败") },
      }));
    }
  };

  return (
    <section className="mt-6 rounded-lg border border-border bg-card p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold">创建前试跑</h3>
          <p className="mt-1 text-xs text-muted-foreground">
            用评估用例直接对话草案中的 system prompt 和模型，并按成功标准自动判定。试跑为纯模型对话，不挂载工具、MCP 和凭证。
          </p>
        </div>
      </div>
      {!successCriteria || cases.length === 0 ? (
        <p className="mt-3 rounded-md bg-amber-500/10 px-3 py-2 text-xs text-amber-700 dark:text-amber-400">
          缺少成功标准或评估用例，返回“评估 Eval”步骤补充后即可试跑。
        </p>
      ) : (
        <ul className="mt-3 grid gap-2">
          {cases.map((testCase) => {
            const outcome = outcomes[testCase.key];
            return (
              <li key={testCase.key} className="rounded-md border border-border p-3">
                <div className="flex flex-wrap items-start justify-between gap-2">
                  <div className="min-w-0 flex-1">
                    <Badge variant="outline" className="h-5 rounded-md text-[10px]">
                      {testCase.categoryLabel}
                    </Badge>
                    <p className="mt-1.5 text-sm leading-6">{testCase.input}</p>
                  </div>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={anyRunning}
                    onClick={() => void runCase(testCase)}
                  >
                    {outcome?.state === "running" ? (
                      <Loader2 className="size-3.5 animate-spin motion-reduce:animate-none" />
                    ) : (
                      <Sparkles className="size-3.5" />
                    )}
                    {outcome?.state === "done" || outcome?.state === "error" ? "重跑" : "试跑"}
                  </Button>
                </div>
                {outcome?.state === "error" && (
                  <p className="mt-2 rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive">
                    {outcome.message}
                  </p>
                )}
                {outcome?.state === "done" && (
                  <div className="mt-2 grid gap-2">
                    <div
                      className={cn(
                        "flex items-start gap-2 rounded-md px-3 py-2 text-xs",
                        outcome.result.pass
                          ? "bg-emerald-500/10 text-emerald-800 dark:text-emerald-300"
                          : "bg-destructive/10 text-destructive",
                      )}
                    >
                      {outcome.result.pass ? (
                        <CheckCircle2 className="mt-0.5 size-3.5 shrink-0" />
                      ) : (
                        <XCircle className="mt-0.5 size-3.5 shrink-0" />
                      )}
                      <span className="leading-5">{outcome.result.verdict}</span>
                    </div>
                    <details className="rounded-md border border-border bg-muted/30 px-3 py-2">
                      <summary className="cursor-pointer text-xs font-medium text-muted-foreground">
                        查看回答
                      </summary>
                      <pre className="mt-2 max-h-64 overflow-y-auto whitespace-pre-wrap text-xs leading-5">
                        {outcome.result.answer}
                      </pre>
                    </details>
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}

function CreateStep({
  draft,
  drafting,
  draftProgress,
  modelsError,
  modelsLoading,
  prompt,
  harnesses,
  selectedTemplateId,
  skills,
  runtimes,
  onDraftChange,
  onPromptChange,
  onGenerate,
  onStartFromUi,
  onTemplateSelect,
}: {
  draft: AgentDraft;
  drafting: boolean;
  draftProgress: string | null;
  modelsError: string | null;
  modelsLoading: boolean;
  prompt: string;
  harnesses: RuntimeHarness[];
  selectedTemplateId: string;
  skills: Skill[];
  runtimes: AgentRuntime[];
  onDraftChange: (next: AgentDraft) => void;
  onPromptChange: (next: string) => void;
  onGenerate: () => void;
  onStartFromUi: () => void;
  onTemplateSelect: (template: AgentTemplate) => void;
}) {
  void onDraftChange;
  const connectedRuntimes = runtimeChoicesForDrafting(harnesses, runtimes);

  return (
    <div className="grid min-h-[calc(100vh-6.5rem)] gap-6 px-4 py-6 lg:grid-cols-[minmax(420px,1fr)_minmax(520px,0.98fr)]">
      <section className="relative flex min-h-[560px] flex-col rounded-lg border border-transparent px-2 pb-2 sm:px-4">
        <div className="flex flex-1 items-center justify-center pb-24 text-center">
          {drafting ? (
            <div className="grid w-full max-w-2xl justify-items-center gap-5">
              <div className="ml-auto max-w-[82%] whitespace-pre-wrap break-words rounded-lg bg-foreground px-4 py-3 text-left text-sm text-background">
                {prompt.trim()}
              </div>
              <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
                <Loader2 className="size-4 animate-spin text-foreground motion-reduce:animate-none" />
                {draftProgress ? "正在生成 config.yaml" : "正在分析需求..."}
              </div>
              {draftProgress && <StreamingPreview text={draftProgress} />}
            </div>
          ) : (
            <div>
              <h1 className="text-2xl font-semibold text-[#20201f] dark:text-foreground">
                用对话创建智能体
              </h1>
              <p className="mt-4 text-base text-muted-foreground">
                描述目标即可，助手会推荐运行时、模型、工具、技能和评估用例。
              </p>
              <div className="mt-6 grid gap-2 text-left sm:grid-cols-3">
                <ConversationHint
                  title="运行时"
                  value={`${connectedRuntimes.length || 0} 个可选`}
                  detail="优先选择已连接 runtime/harness"
                />
                <ConversationHint
                  title="模型"
                  value="自动推荐"
                  detail="按任务复杂度和可用模型选择"
                />
                <ConversationHint
                  title="技能"
                  value={`${skills.length} 个可用`}
                  detail="只附加真正相关的 skill"
                />
              </div>
            </div>
          )}
        </div>

        <div className="mx-auto w-full max-w-3xl overflow-hidden rounded-lg border border-border bg-card shadow-[0_18px_70px_rgba(15,23,42,0.10)]">
          <Textarea
            value={prompt}
            onChange={(event) => onPromptChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey && !drafting) {
                event.preventDefault();
                onGenerate();
              }
            }}
            placeholder="描述你想要的智能体..."
            className="min-h-24 resize-none border-0 bg-transparent px-4 py-4 text-[15px] text-foreground shadow-none outline-none placeholder:text-muted-foreground focus-visible:ring-0"
          />
          <div className="flex flex-wrap items-center gap-2 border-t border-border bg-muted/30 px-3 py-3">
            <span className="text-xs text-muted-foreground">
              {modelsLoading ? "正在读取可用模型..." : "运行时、模型、工具和技能会在草案中推荐"}
            </span>
            {modelsError && <span className="text-xs text-destructive">{modelsError}</span>}
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={onStartFromUi}
              disabled={drafting}
              className="gap-1.5"
            >
              <Bot className="size-3.5" />
              使用 UI 编辑器
            </Button>
            <div className="ml-auto" />
            <Button
              type="button"
              size="icon-sm"
              onClick={onGenerate}
              disabled={!prompt.trim() || drafting}
              className="size-9 rounded-full"
              aria-label="生成配置"
            >
              {drafting ? <Loader2 className="size-4 animate-spin motion-reduce:animate-none" /> : <ArrowUp className="size-4" />}
            </Button>
          </div>
        </div>
      </section>

      <section className="min-h-0">
        <TemplateBrowser
          selectedTemplateId={selectedTemplateId}
          onSelect={onTemplateSelect}
        />
      </section>
    </div>
  );
}

function StreamingPreview({ text }: { text: string }) {
  const lines = text.split("\n");
  const tail = lines.slice(-10).join("\n");
  return (
    <div className="w-full max-w-2xl overflow-hidden rounded-lg border border-border bg-[#2b2a28] px-4 py-3 text-left">
      <div className="flex items-center justify-between text-[11px] text-[#9d9384]">
        <span className="font-mono">config.yaml</span>
        <span className="font-mono">{lines.length} lines</span>
      </div>
      <pre className="mt-2 max-h-48 overflow-hidden whitespace-pre-wrap font-mono text-[12px] leading-5 text-[#e8b28c]">
        {tail}
        <span className="animate-pulse motion-reduce:animate-none">▌</span>
      </pre>
    </div>
  );
}

function ConversationHint({ title, value, detail }: { title: string; value: string; detail: string }) {
  return (
    <div className="rounded-lg border border-border bg-card/70 px-3 py-2">
      <div className="text-xs text-muted-foreground">{title}</div>
      <div className="mt-1 text-sm font-semibold">{value}</div>
      <div className="mt-1 text-xs leading-4 text-muted-foreground">{detail}</div>
    </div>
  );
}

function ConfigStep({
  canCreate,
  configText,
  copied,
  draft,
  draftNotice,
  drafting,
  draftProgress,
  error,
  messages,
  agents,
  harnesses,
  mcpError,
  mcpIntegrations,
  mcpLoading,
  models,
  modelsError,
  modelsLoading,
  parsedError,
  prompt,
  rules,
  skills,
  runtimes,
  saving,
  view,
  onConfigChange,
  onCopy,
  onCreate,
  onDraftChange,
  onPromptChange,
  onRefine,
  onViewChange,
}: {
  canCreate: boolean;
  configText: string;
  copied: boolean;
  draft: AgentDraft;
  draftNotice: string | null;
  drafting: boolean;
  draftProgress: string | null;
  error: string | null;
  messages: BuilderChatMessage[];
  agents: Agent[];
  harnesses: RuntimeHarness[];
  mcpError: string | null;
  mcpIntegrations: Integration[];
  mcpLoading: boolean;
  models: string[];
  modelsError: string | null;
  modelsLoading: boolean;
  parsedError: string | null;
  prompt: string;
  rules: Rule[];
  skills: Skill[];
  runtimes: AgentRuntime[];
  saving: boolean;
  view: BuilderView;
  onConfigChange: (next: string) => void;
  onCopy: () => void;
  onCreate: () => void;
  onDraftChange: (next: AgentDraft) => void;
  onPromptChange: (next: string) => void;
  onRefine: () => void;
  onViewChange: (next: BuilderView) => void;
}) {
  return (
    <div className="grid min-h-[calc(100vh-6.5rem)] gap-6 px-4 py-6 lg:grid-cols-[minmax(360px,0.82fr)_minmax(560px,1.18fr)]">
      <section className="flex min-h-[560px] flex-col">
        <div className="flex min-h-0 flex-1 items-start justify-center overflow-y-auto py-2">
          <div className="w-full max-w-2xl">
            <div className="grid gap-3">
              {messages.map((message) =>
                message.role === "user" ? (
                  <div
                    key={message.id}
                    className="ml-auto max-w-[85%] whitespace-pre-wrap break-words rounded-lg bg-foreground px-4 py-3 text-sm text-background"
                  >
                    {message.text}
                  </div>
                ) : (
                  <AssistantChangeMessage key={message.id} message={message} />
                ),
              )}
              {drafting && (
                <div className="grid gap-2">
                  <div className="flex items-center gap-2 text-sm text-muted-foreground">
                    <Loader2 className="size-4 animate-spin motion-reduce:animate-none" />
                    {draftProgress ? "正在修改配置" : "正在分析修改需求..."}
                  </div>
                  {draftProgress && <StreamingPreview text={draftProgress} />}
                </div>
              )}
            </div>
            <div className="mt-8 flex flex-wrap gap-3">
              <Button type="button" onClick={onCreate} disabled={!canCreate || drafting}>
                {saving ? "创建中..." : "进入复核并创建"}
              </Button>
              <Button
                type="button"
                variant="secondary"
                onClick={() => document.getElementById("agent-config-refine")?.focus()}
              >
                继续调整
              </Button>
            </div>
            {draftNotice && (
              <div className="mt-4 max-w-xl rounded-lg border border-amber-500/20 bg-amber-500/10 px-3 py-2 text-sm text-amber-700 dark:text-amber-300">
                {draftNotice}
              </div>
            )}
            {(error || parsedError) && (
              <div className="mt-4 max-w-xl rounded-lg border border-destructive/20 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                {error ?? parsedError}
              </div>
            )}
            <AgentBuilderCopilot
              configText={configText}
              draft={draft}
              harnesses={harnesses}
              prompt={prompt}
              runtimes={runtimes}
              onDraftChange={onDraftChange}
            />
          </div>
        </div>

        <div className="mx-auto w-full max-w-3xl overflow-hidden rounded-lg border border-border bg-card shadow-[0_18px_70px_rgba(15,23,42,0.10)]">
          <Textarea
            id="agent-config-refine"
            value={prompt}
            onChange={(event) => onPromptChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey && !drafting) {
                event.preventDefault();
                onRefine();
              }
            }}
            placeholder="继续描述要修改的内容..."
            className="min-h-20 resize-none border-0 bg-transparent px-4 py-4 text-[15px] text-foreground shadow-none outline-none placeholder:text-muted-foreground focus-visible:ring-0"
          />
          <div className="flex items-center border-t border-border bg-muted/30 px-3 py-3">
            <div className="ml-auto" />
            <Button
              type="button"
              size="icon-sm"
              onClick={onRefine}
              disabled={!prompt.trim() || drafting}
              className="size-9 rounded-full"
              aria-label="调整配置"
            >
              {drafting ? <Loader2 className="size-4 animate-spin motion-reduce:animate-none" /> : <ArrowUp className="size-4" />}
            </Button>
          </div>
        </div>
      </section>

      <section className="min-h-0">
        <div className="flex h-full min-h-[560px] flex-col overflow-hidden rounded-lg border border-[#343330] bg-[#2b2a28] text-[#f7f2e8] shadow-[0_18px_70px_rgba(15,23,42,0.16)]">
          <div className="flex shrink-0 items-center justify-between border-b border-white/10 px-4 py-3">
            <div className="flex items-center gap-1">
              <Button
                type="button"
                size="sm"
                variant={view === "config" ? "secondary" : "ghost"}
                onClick={() => onViewChange("config")}
                className={cn(
                  "h-8 text-[#c9c0b1] hover:bg-white/10 hover:text-white",
                  view === "config" && "bg-white text-[#1b1b1a] hover:bg-white",
                )}
              >
                <Code2 className="size-3.5" />
                Config
              </Button>
              <Button
                type="button"
                size="sm"
                variant={view === "preview" ? "secondary" : "ghost"}
                onClick={() => onViewChange("preview")}
                className={cn(
                  "h-8 text-[#c9c0b1] hover:bg-white/10 hover:text-white",
                  view === "preview" && "bg-white text-[#1b1b1a] hover:bg-white",
                )}
              >
                <FileSearch className="size-3.5" />
                Preview
              </Button>
              <Button
                type="button"
                size="sm"
                variant={view === "edit" ? "secondary" : "ghost"}
                onClick={() => onViewChange("edit")}
                className={cn(
                  "h-8 text-[#c9c0b1] hover:bg-white/10 hover:text-white",
                  view === "edit" && "bg-white text-[#1b1b1a] hover:bg-white",
                )}
              >
                <Bot className="size-3.5" />
                Edit UI
              </Button>
            </div>
            <div className="flex items-center gap-2">
              {parsedError ? (
                <span className="flex items-center gap-1 text-xs text-red-300">
                  <XCircle className="size-3.5" />
                  配置无效
                </span>
              ) : (
                <span className="flex items-center gap-1 text-xs text-emerald-300">
                  <CheckCircle2 className="size-3.5" />
                  就绪
                </span>
              )}
              <Button
                type="button"
                size="icon-sm"
                variant="ghost"
                onClick={onCopy}
                className="text-[#c9c0b1] hover:bg-white/10 hover:text-white"
                aria-label="复制配置"
                title="复制配置"
              >
                <Clipboard className="size-4" />
              </Button>
            </div>
          </div>

          {view === "edit" ? (
            <AgentDraftControls
              agents={agents}
              harnesses={harnesses}
              draft={draft}
              mcpError={mcpError}
              mcpIntegrations={mcpIntegrations}
              mcpLoading={mcpLoading}
              models={models}
              modelsError={modelsError}
              modelsLoading={modelsLoading}
              rules={rules}
              skills={skills}
              runtimes={runtimes}
              onChange={onDraftChange}
            />
          ) : view === "config" ? (
            <Textarea
              value={configText}
              onChange={(event) => onConfigChange(event.target.value)}
              spellCheck={false}
              className="min-h-0 flex-1 resize-none rounded-none border-0 bg-[#2b2a28] px-5 py-4 font-mono text-[13px] leading-6 text-[#e8b28c] shadow-none outline-none focus-visible:ring-0"
              aria-label="Agent YAML config"
            />
          ) : (
            <ConfigPreview draft={draft} mcpIntegrations={mcpIntegrations} />
          )}

          <div className="flex shrink-0 flex-wrap items-center gap-2 border-t border-white/10 px-4 py-3 text-xs text-[#c9c0b1]">
            <span className="font-mono">{scheduleLabel(draft.cron, draft.timezone)}</span>
            <span className="hidden text-white/20 sm:inline">/</span>
            <span className="font-mono">{draft.max_runtime_minutes} 分钟上限</span>
            {copied && <span className="ml-auto text-emerald-300">已复制</span>}
          </div>
        </div>
      </section>
    </div>
  );
}

function AssistantChangeMessage({
  message,
}: {
  message: Extract<BuilderChatMessage, { role: "assistant" }>;
}) {
  return (
    <div className="mr-auto max-w-[92%] rounded-lg border border-border bg-card px-4 py-3 text-sm shadow-sm">
      <p className="leading-6 text-foreground">{message.summary}</p>
      {message.changes.length > 0 && (
        <ul className="mt-2 grid gap-1.5">
          {message.changes.map((change) => (
            <li
              key={`${change.field}-${change.kind}`}
              className="rounded-md bg-muted/40 px-2.5 py-1.5 font-mono text-xs leading-5 text-muted-foreground"
            >
              <span className="font-semibold text-foreground">{change.field}</span>
              {change.detail ? (
                <span className="ml-2">{change.detail}</span>
              ) : change.added || change.removed ? (
                <span className="ml-2">
                  {(change.added ?? []).map((item) => (
                    <span key={`add-${item}`} className="mr-2 text-emerald-600 dark:text-emerald-400">
                      +{item}
                    </span>
                  ))}
                  {(change.removed ?? []).map((item) => (
                    <span key={`rm-${item}`} className="mr-2 text-red-600 dark:text-red-400">
                      -{item}
                    </span>
                  ))}
                </span>
              ) : (
                <span className="ml-2">
                  {change.before ? `${change.before} → ` : ""}
                  {change.after ?? ""}
                </span>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function AgentBuilderCopilot({
  configText,
  draft,
  harnesses,
  prompt,
  runtimes,
  onDraftChange,
}: {
  configText: string;
  draft: AgentDraft;
  harnesses: RuntimeHarness[];
  prompt: string;
  runtimes: AgentRuntime[];
  onDraftChange: (next: AgentDraft) => void;
}) {
  const [loadingMode, setLoadingMode] = useState<"clarify" | "explain" | "tools" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [response, setResponse] = useState<AgentBuilderCopilotResponse | null>(null);
  const runtimeTools =
    harnesses.find((entry) => entry.alias === draft.runtime)?.tools ??
    runtimes.find((entry) => entry.id === draft.runtime)?.tools ??
    [];
  const selectedTools = draft.tools.map((tool) => tool.type).filter(Boolean);
  const availableToolIds = new Set(runtimeTools.map((tool) => tool.id));

  const ask = async (mode: "clarify" | "explain" | "tools") => {
    setLoadingMode(mode);
    setError(null);
    try {
      const next = await askAgentBuilderCopilot({
        mode,
        userMessage: prompt,
        currentConfig: configText,
        runtime: draft.runtime,
        selectedTools,
        availableTools: runtimeTools.map((tool) => ({
          id: tool.id,
          name: tool.name,
          description: tool.description,
        })),
        requestedModel: draft.model,
      });
      setResponse(next);
    } catch (err) {
      setError(apiErrorMessage(err, "Builder Copilot failed"));
    } finally {
      setLoadingMode(null);
    }
  };

  const applyToolRecommendations = () => {
    if (!response) return;
    const next = new Set(selectedTools);
    for (const recommendation of response.tool_recommendations) {
      if (!availableToolIds.has(recommendation.tool)) continue;
      if (recommendation.action === "add") next.add(recommendation.tool);
      if (recommendation.action === "remove") next.delete(recommendation.tool);
    }
    onDraftChange({ ...draft, tools: Array.from(next).map((type) => ({ type })) });
  };

  return (
    <div className="mt-6 rounded-xl border border-border bg-card p-4 shadow-sm">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="grid gap-1">
          <div className="flex items-center gap-2 text-sm font-semibold">
            <MessageSquareText className="size-4" />
            Agent Builder Copilot
          </div>
          <p className="text-xs leading-5 text-muted-foreground">
            用当前草案和输入框里的补充需求，让 LLM 做澄清、解释和工具建议。
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <CopilotButton mode="clarify" loadingMode={loadingMode} onClick={() => void ask("clarify")}>
            澄清问题
          </CopilotButton>
          <CopilotButton mode="explain" loadingMode={loadingMode} onClick={() => void ask("explain")}>
            解释配置
          </CopilotButton>
          <CopilotButton mode="tools" loadingMode={loadingMode} onClick={() => void ask("tools")}>
            推荐工具
          </CopilotButton>
        </div>
      </div>

      {error && (
        <div className="mt-3 rounded-lg border border-destructive/20 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {error}
        </div>
      )}

      {response && (
        <div className="mt-4 grid gap-3 text-sm">
          {response.summary && <p className="leading-6 text-foreground">{response.summary}</p>}
          <CopilotList title="需要确认的问题" items={response.clarification_questions} />
          <CopilotList title="风险提醒" items={response.risks} />
          <CopilotList title="可加入 system prompt 的约束" items={response.suggested_system_notes} />
          {response.tool_recommendations.length > 0 && (
            <div className="grid gap-2">
              <div className="flex items-center justify-between gap-3">
                <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                  工具建议
                </h3>
                <Button type="button" size="sm" variant="outline" onClick={applyToolRecommendations}>
                  应用 add/remove
                </Button>
              </div>
              <div className="grid gap-2">
                {response.tool_recommendations.map((recommendation) => (
                  <div
                    key={`${recommendation.tool}-${recommendation.action}`}
                    className="rounded-lg border border-border bg-muted/30 px-3 py-2"
                  >
                    <div className="flex flex-wrap items-center gap-2">
                      <Badge variant={recommendation.action === "remove" ? "destructive" : "secondary"}>
                        {recommendation.action}
                      </Badge>
                      <span className="font-mono text-xs">{recommendation.tool}</span>
                    </div>
                    <p className="mt-1 text-xs leading-5 text-muted-foreground">
                      {recommendation.reason}
                    </p>
                    {recommendation.risk && (
                      <p className="mt-1 text-xs leading-5 text-amber-700 dark:text-amber-300">
                        {recommendation.risk}
                      </p>
                    )}
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function CopilotButton({
  children,
  loadingMode,
  mode,
  onClick,
}: {
  children: React.ReactNode;
  loadingMode: "clarify" | "explain" | "tools" | null;
  mode: "clarify" | "explain" | "tools";
  onClick: () => void;
}) {
  const loading = loadingMode === mode;
  return (
    <Button type="button" size="sm" variant="outline" onClick={onClick} disabled={loadingMode !== null}>
      {loading ? <Loader2 className="size-3.5 animate-spin motion-reduce:animate-none" /> : <Sparkles className="size-3.5" />}
      {children}
    </Button>
  );
}

function CopilotList({ title, items }: { title: string; items: string[] }) {
  if (items.length === 0) return null;
  return (
    <div className="grid gap-1.5">
      <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">{title}</h3>
      <ul className="grid gap-1.5">
        {items.map((item) => (
          <li key={item} className="rounded-md bg-muted/40 px-2.5 py-1.5 text-xs leading-5 text-muted-foreground">
            {item}
          </li>
        ))}
      </ul>
    </div>
  );
}

function TemplateBrowser({
  selectedTemplateId,
  onSelect,
}: {
  selectedTemplateId: string;
  onSelect: (template: AgentTemplate) => void;
}) {
  const [query, setQuery] = useState("");
  const normalized = query.trim().toLowerCase();
  const templates = normalized
    ? AGENT_TEMPLATES.filter((template) =>
        [
          template.title,
          template.description,
          ...template.tags,
          template.draft.name,
        ]
          .join(" ")
          .toLowerCase()
          .includes(normalized),
      )
    : AGENT_TEMPLATES;

  return (
    <div className="flex h-full min-h-[560px] flex-col rounded-lg border border-border bg-card p-5 shadow-sm">
      <div className="mb-4">
        <h2 className="text-lg font-semibold text-foreground">浏览模板</h2>
        <div className="relative mt-4">
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="搜索模板"
            className="h-10 pl-9"
          />
        </div>
      </div>
      <div className="grid min-h-0 flex-1 gap-3 overflow-y-auto pr-1 sm:grid-cols-2">
        {templates.map((template) => {
          const Icon = TEMPLATE_ICONS[template.id] ?? Sparkles;
          const selected = template.id === selectedTemplateId;
          return (
            <button
              key={template.id}
              type="button"
              onClick={() => onSelect(template)}
              className={cn(
                "min-h-28 rounded-lg border border-border bg-background p-4 text-left transition hover:bg-muted/40",
                selected && "border-foreground ring-2 ring-foreground/10",
              )}
            >
              <div className="flex min-h-full flex-col">
                <div className="flex items-start gap-3">
                  <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-muted text-foreground">
                    <Icon className="size-4" />
                  </span>
                  <span className="min-w-0">
                    <span className="block text-sm font-semibold text-foreground">{template.title}</span>
                    <span className="mt-1 line-clamp-2 block text-xs leading-5 text-muted-foreground">
                      {template.description}
                    </span>
                  </span>
                </div>
                <div className="mt-auto flex flex-wrap gap-1.5 pt-4">
                  {template.tags.map((tag) => (
                    <span
                      key={tag}
                      className="rounded-md border border-border bg-muted/40 px-2 py-0.5 text-[11px] text-muted-foreground"
                    >
                      {tag}
                    </span>
                  ))}
                </div>
              </div>
            </button>
          );
        })}
      </div>
    </div>
  );
}

function AgentDraftControls({
  agents,
  harnesses,
  draft,
  mcpError,
  mcpIntegrations,
  mcpLoading,
  models,
  modelsError,
  modelsLoading,
  rules,
  skills,
  runtimes,
  onChange,
}: {
  agents: Agent[];
  harnesses: RuntimeHarness[];
  draft: AgentDraft;
  mcpError: string | null;
  mcpIntegrations: Integration[];
  mcpLoading: boolean;
  models: string[];
  modelsError: string | null;
  modelsLoading: boolean;
  rules: Rule[];
  skills: Skill[];
  runtimes: AgentRuntime[];
  onChange: (next: AgentDraft) => void;
}) {
  const update = (patch: Partial<AgentDraft>) => onChange({ ...draft, ...patch });
  const availableModels = modelOptions(models, draft.model);
  const runtime = runtimes.find((entry) => entry.id === draft.runtime);
  const selectedHarness = harnesses.find((entry) => entry.alias === draft.runtime);
  const runtimeTools = selectedHarness?.tools ?? runtime?.tools ?? [];
  const toolOptions = runtimeTools.length > 0
    ? runtimeTools.map((tool) => tool.id).filter(Boolean)
    : draft.tools.map((tool) => tool.type).filter(Boolean);
  const selectedTools = new Set(draft.tools.map((tool) => tool.type).filter(Boolean));
  const selectedSubAgents = new Set(draft.sub_agents.map((agent) => agent.agent_id));
  const [vaultKeyInput, setVaultKeyInput] = useState("");
  const [vaultKeyError, setVaultKeyError] = useState<string | null>(null);
  const setTool = (toolId: string, enabled: boolean) => {
    const next = new Set(selectedTools);
    if (enabled) next.add(toolId);
    else next.delete(toolId);
    update({ tools: Array.from(next).map((type) => ({ type })) });
  };
  const toggleRule = (ruleId: string, enabled: boolean) => {
    update({
      rule_ids: enabled
        ? Array.from(new Set([...draft.rule_ids, ruleId]))
        : draft.rule_ids.filter((id) => id !== ruleId),
    });
  };
  const toggleSkill = (skillId: string, enabled: boolean) => {
    update({
      skill_ids: enabled
        ? Array.from(new Set([...draft.skill_ids, skillId]))
        : draft.skill_ids.filter((id) => id !== skillId),
    });
  };
  const toggleMcpIntegration = (integrationId: string, enabled: boolean) => {
    update({
      mcp_server_ids: enabled
        ? Array.from(new Set([...draft.mcp_server_ids, integrationId]))
        : draft.mcp_server_ids.filter((id) => id !== integrationId),
    });
  };
  const addVaultKey = () => {
    const key = vaultKeyInput.trim();
    if (!key) return;
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) {
      setVaultKeyError("密钥名只能包含字母、数字和下划线，且不能以数字开头，例如 BROWSER_USE_API_KEY。");
      return;
    }
    setVaultKeyError(null);
    update({ vault_keys: Array.from(new Set([...draft.vault_keys, key])) });
    setVaultKeyInput("");
  };

  return (
    <div className="min-h-0 flex-1 overflow-y-auto bg-[#2b2a28] px-5 py-4 text-[#f7f2e8]">
      <div className="mx-auto grid max-w-3xl gap-4">
        <div className="grid gap-1.5">
          <Label htmlFor="draft-name" className="text-[#c9c0b1]">
            Name
          </Label>
          <Input
            id="draft-name"
            value={draft.name}
            onChange={(event) => update({ name: event.target.value })}
            placeholder="security-reviewer"
            className="border-white/10 bg-[#242321] text-[#f7f2e8] placeholder:text-[#9d9384]"
          />
        </div>

        <div className="grid gap-1.5">
          <Label htmlFor="draft-description" className="text-[#c9c0b1]">
            Description
          </Label>
          <Input
            id="draft-description"
            value={draft.description}
            onChange={(event) => update({ description: event.target.value })}
            placeholder="这个智能体做什么"
            className="border-white/10 bg-[#242321] text-[#f7f2e8] placeholder:text-[#9d9384]"
          />
        </div>

        <div className="grid gap-1.5">
          <Label className="text-[#c9c0b1]">Model</Label>
          <div className="[&_button]:border-white/10 [&_button]:bg-[#242321] [&_button]:text-[#f7f2e8] [&_svg]:text-[#9d9384]">
            <ModelSelect
              value={draft.model}
              models={availableModels}
              onValueChange={(model) => update({ model })}
            />
          </div>
          {modelsLoading && (
            <p className="text-xs text-[#9d9384]">正在加载可用模型...</p>
          )}
          {modelsError && (
            <p className="text-xs text-red-300">{modelsError}</p>
          )}
        </div>

        {harnesses.length === 0 && (
          <div className="grid gap-1.5">
            <Label className="text-[#c9c0b1]">Runtime</Label>
            <div className="rounded-md border border-white/10 bg-white/5 px-3 py-3 text-xs text-[#9d9384]">
              <p>没有已连接的运行时 harness，当前使用默认运行时 {runtimeLabel(draft.runtime)}。</p>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => {
                  window.location.href = "/runtimes/";
                }}
                className="mt-2 border-white/10 bg-white/5 text-[#f7f2e8] hover:bg-white/10 hover:text-white"
              >
                <ExternalLink className="size-3.5" />
                去连接运行时
              </Button>
            </div>
          </div>
        )}
        {harnesses.length >= 1 && (
          <div className="grid gap-1.5">
            <Label className="text-[#c9c0b1]">Runtime</Label>
            <Select
              value={draft.runtime || "claude_managed_agents"}
              onValueChange={(v) => {
                const runtimeAlias = v ?? "claude_managed_agents";
                update({
                  runtime: runtimeAlias,
                  model: "",
                  tools: defaultToolsForHarnessRuntime(runtimeAlias, harnesses, runtimes),
                });
              }}
            >
              <SelectTrigger className="h-11 w-full max-w-sm overflow-hidden border-white/10 bg-[#242321] px-3 text-[#f7f2e8]">
                <RuntimeSelectOption
                  alias={draft.runtime || "claude_managed_agents"}
                  displayName={selectedHarness?.display_name ?? runtime?.name ?? runtimeLabel(draft.runtime)}
                  apiSpec={selectedHarness?.api_spec ?? runtimeApiSpec(draft.runtime)}
                  isDefault={selectedHarness?.is_default}
                  compact
                />
              </SelectTrigger>
              <SelectContent className="w-[360px] border-white/10 bg-[#242321] text-[#f7f2e8]">
                {harnesses.map((h) => (
                  <SelectItem
                    key={h.alias}
                    value={h.alias}
                    className="py-3 focus:bg-white/10 focus:text-[#f7f2e8] data-highlighted:bg-white/10 data-highlighted:text-[#f7f2e8] [&_span]:!text-[#f7f2e8] [&_.runtime-option-muted]:!text-[#c9c0b1] [&_svg]:!text-[#f7f2e8]"
                  >
                    <RuntimeSelectOption
                      alias={h.alias}
                      displayName={h.display_name}
                      apiSpec={h.api_spec}
                      isDefault={h.is_default}
                    />
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p className="text-xs text-[#9d9384]">
              切换运行时会把已选工具重置为该运行时的默认工具集，并重新加载可用模型。
            </p>
          </div>
        )}

        <div className="grid gap-1.5">
          <Label htmlFor="draft-system" className="text-[#c9c0b1]">
            System prompt
          </Label>
          <Textarea
            id="draft-system"
            value={draft.system}
            onChange={(event) => update({ system: event.target.value })}
            className="min-h-[280px] resize-y border-white/10 bg-[#242321] font-mono text-xs text-[#f7f2e8] placeholder:text-[#9d9384]"
            placeholder="You are a meticulous security reviewer..."
          />
        </div>

        <div className="[&_button]:border-white/10 [&_button]:bg-[#242321] [&_button]:text-[#f7f2e8] [&_input]:border-white/10 [&_input]:bg-[#242321] [&_input]:text-[#f7f2e8] [&_label]:text-[#c9c0b1] [&_section]:border-white/10 [&_section]:bg-black/10 [&_svg]:text-[#9d9384]">
          <ScheduleEditor
            cron={draft.cron}
            timezone={draft.timezone}
            onChange={(schedule) => update(schedule)}
          />
        </div>

        <div className="grid gap-2 rounded-md border border-white/10 bg-black/10 p-3">
          <div className="flex items-start justify-between gap-3">
            <div className="grid gap-1">
              <Label className="text-sm font-medium">Vault Credentials</Label>
              <p className="max-w-xl text-xs leading-5 text-muted-foreground">
                先登记密钥名称，创建后在智能体详情页填写密钥值。
              </p>
            </div>
            <span className="shrink-0 font-mono text-xs text-muted-foreground">
              {draft.vault_keys.length} 已挂载
            </span>
          </div>
          <div className="flex gap-2">
            <Input
              value={vaultKeyInput}
              onChange={(event) => {
                setVaultKeyInput(event.target.value);
                setVaultKeyError(null);
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  addVaultKey();
                }
              }}
              placeholder="BROWSER_USE_API_KEY"
              className="border-white/10 bg-white/5 font-mono text-xs"
              aria-label="Vault credential name"
            />
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={addVaultKey}
              disabled={!vaultKeyInput.trim()}
              className="border-white/10 bg-white/5 hover:bg-white/10"
            >
              <Plus className="size-3.5" />
              添加密钥
            </Button>
          </div>
          {vaultKeyError && <p className="text-xs text-red-300">{vaultKeyError}</p>}
          {draft.vault_keys.length === 0 ? (
            <p className="text-xs text-muted-foreground">尚未挂载保险库凭证。</p>
          ) : (
            <div className="flex flex-wrap gap-1.5">
              {draft.vault_keys.map((key) => (
                <span
                  key={key}
                  className="inline-flex max-w-full items-center gap-1 rounded-md border border-white/10 bg-white/5 px-2 py-1"
                >
                  <KeyRound className="size-3 shrink-0 text-muted-foreground" />
                  <span className="truncate font-mono text-xs">{key}</span>
                  <button
                    type="button"
                    className="rounded text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
                    onClick={() =>
                      update({ vault_keys: draft.vault_keys.filter((value) => value !== key) })
                    }
                    aria-label={`Remove ${key}`}
                  >
                    <X className="size-3" />
                  </button>
                </span>
              ))}
            </div>
          )}
        </div>

        <div className="grid gap-2 rounded-md border border-white/10 bg-black/10 p-3 text-[#f7f2e8]">
          <div className="flex items-center justify-between gap-3">
            <Label className="text-sm font-medium">Tools</Label>
            <span className="font-mono text-xs text-[#9d9384]">
              {draft.tools.length} 已选
            </span>
          </div>
          <div className="grid max-h-[284px] gap-2 overflow-y-auto pr-1 sm:grid-cols-2">
            {toolOptions.map((toolId) => (
              <label
                key={toolId}
                className="flex min-w-0 cursor-pointer items-center gap-2 rounded-md border border-white/10 bg-white/5 px-2.5 py-2 text-xs hover:bg-white/10"
              >
                <input
                  type="checkbox"
                  checked={selectedTools.has(toolId)}
                  onChange={(event) => setTool(toolId, event.target.checked)}
                  className="size-3.5 shrink-0"
                />
                <span className="min-w-0 truncate font-mono">{toolId}</span>
              </label>
            ))}
          </div>
        </div>

        <div className="grid gap-2 rounded-md border border-white/10 bg-black/10 p-3 text-[#f7f2e8]">
          <div className="flex items-center justify-between gap-3">
            <Label className="text-sm font-medium">Skills</Label>
            <span className="font-mono text-xs text-[#9d9384]">
              {draft.skill_ids.length} 已挂载
            </span>
          </div>
          {skills.length === 0 ? (
            <p className="text-xs text-[#9d9384]">暂无可用技能。</p>
          ) : (
            <div className="grid max-h-[284px] gap-2 overflow-y-auto pr-1">
              {skills.map((skill) => {
                const enabled = draft.skill_ids.includes(skill.id);
                return (
                  <label
                    key={skill.id}
                    className="flex min-w-0 cursor-pointer items-start gap-2.5 rounded-md border border-white/10 bg-white/5 px-2.5 py-2 text-xs hover:bg-white/10"
                  >
                    <input
                      type="checkbox"
                      checked={enabled}
                      onChange={(event) => toggleSkill(skill.id, event.target.checked)}
                      className="mt-0.5 size-3.5 shrink-0"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="font-medium">{skill.name}</span>
                        <span className="truncate font-mono text-[#9d9384]">{skill.id}</span>
                      </div>
                      <div className="mt-0.5 line-clamp-2 text-[#9d9384]">
                        {skill.description || "暂无描述。"}
                      </div>
                    </div>
                  </label>
                );
              })}
            </div>
          )}
        </div>

        <div className="grid gap-2 rounded-md border border-white/10 bg-black/10 p-3 text-[#f7f2e8]">
          <div className="flex items-center justify-between gap-3">
            <div className="grid gap-1">
              <Label className="text-sm font-medium">MCP integrations</Label>
              <p className="max-w-xl text-xs leading-5 text-muted-foreground">
                从注册表挂载托管 MCP 服务器。创建智能体时会根据这些 ID 重建工具集。
              </p>
            </div>
            <span className="font-mono text-xs text-[#9d9384]">
              {draft.mcp_server_ids.length} 已挂载
            </span>
          </div>
          {mcpError && (
            <div className="rounded-md border border-destructive/30 bg-destructive/10 px-2.5 py-2 text-xs text-destructive">
              {mcpError}
            </div>
          )}
          {mcpLoading ? (
            <div className="grid gap-2">
              {[0, 1, 2].map((item) => (
                <div
                  key={item}
                  className="rounded-md border border-white/10 bg-white/5 px-2.5 py-3"
                >
                  <div className="h-3 w-1/3 animate-pulse rounded bg-muted motion-reduce:animate-none" />
                  <div className="mt-2 h-3 w-2/3 animate-pulse rounded bg-muted motion-reduce:animate-none" />
                </div>
              ))}
            </div>
          ) : mcpIntegrations.length === 0 ? (
            <div className="rounded-md border border-white/10 bg-white/5 px-3 py-4 text-center">
              <Plug className="mx-auto size-6 text-muted-foreground" />
              <p className="mt-2 text-xs font-medium">暂无可用的 MCP 服务器</p>
              <p className="mt-1 text-xs text-muted-foreground">
                先到 MCP 注册表添加服务器，再回到这里挂载。
              </p>
            </div>
          ) : (
            <div className="grid max-h-[360px] gap-2 overflow-y-auto pr-1">
              {mcpIntegrations.map((integration) => {
                const enabled = draft.mcp_server_ids.includes(integration.id);
                const availableTools = integration.tools.filter(Boolean);
                const previewTools = availableTools.slice(0, 8);
                const remainingTools = Math.max(availableTools.length - previewTools.length, 0);
                const canAttach = integration.mcpUrl.trim().length > 0;
                return (
                  <label
                    key={integration.id}
                    className={cn(
                      "flex min-w-0 cursor-pointer items-start gap-2.5 rounded-md border border-white/10 bg-white/5 px-2.5 py-2.5 text-xs hover:bg-white/10",
                      enabled && "border-white/30 bg-white/10",
                      !canAttach && "cursor-not-allowed opacity-70",
                    )}
                  >
                    <input
                      type="checkbox"
                      checked={enabled}
                      disabled={!canAttach}
                      onChange={(event) => toggleMcpIntegration(integration.id, event.target.checked)}
                      className="mt-0.5 size-3.5 shrink-0"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="font-medium">{integration.name}</span>
                        <span className="truncate font-mono text-muted-foreground">{integration.id}</span>
                        <Badge variant="outline" className="h-5 rounded-md border-white/10 bg-white/5 text-[10px] text-[#c9c0b1]">
                          {integration.source === "registry" ? "注册表" : "目录"}
                        </Badge>
                        {integration.connected ? (
                          <Badge variant="secondary" className="h-5 rounded-md text-[10px]">
                            <KeyRound className="size-3" />
                            已连接
                          </Badge>
                        ) : (
                          <Badge variant="outline" className="h-5 rounded-md border-white/10 bg-white/5 text-[10px] text-[#c9c0b1]">
                            <KeyRound className="size-3" />
                            待配置凭证
                          </Badge>
                        )}
                      </div>
                      <div className="mt-1 line-clamp-2 text-muted-foreground">
                        {integration.description}
                      </div>
                      <div className="mt-1 flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
                        <span className="inline-flex items-center gap-1">
                          <KeyRound className="size-3" />
                          {integration.envKey}
                        </span>
                        <span className="inline-flex items-center gap-1">
                          <Wrench className="size-3" />
                          {availableTools.length > 0
                            ? `${availableTools.length} 个可用工具`
                            : "尚未发现工具"}
                        </span>
                      </div>
                      {(enabled || availableTools.length > 0) && (
                        <div className="mt-2 flex flex-wrap gap-1">
                          {previewTools.map((tool) => (
                            <span
                              key={tool}
                              className="rounded border border-white/10 bg-white/5 px-1.5 py-0.5 font-mono text-[10px] text-[#c9c0b1]"
                            >
                              {tool}
                            </span>
                          ))}
                          {remainingTools > 0 && (
                            <span className="rounded border border-white/10 bg-white/5 px-1.5 py-0.5 font-mono text-[10px] text-[#c9c0b1]">
                              +{remainingTools} more
                            </span>
                          )}
                        </div>
                      )}
                      {!canAttach && (
                        <p className="mt-2 text-xs text-destructive">
                          该服务器缺少 URL，暂时无法挂载到托管智能体。
                        </p>
                      )}
                    </div>
                  </label>
                );
              })}
            </div>
          )}
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => {
              window.location.href = "/mcp-servers/";
            }}
            className="justify-self-start border-white/10 bg-white/5 text-[#f7f2e8] hover:bg-white/10 hover:text-white"
          >
            <ExternalLink className="size-3.5" />
            管理 MCP 服务器
          </Button>
        </div>

        <div className="grid gap-2 rounded-md border border-white/10 bg-black/10 p-3 text-[#f7f2e8]">
          <div className="flex items-start justify-between gap-3">
            <div className="grid gap-1">
              <Label className="text-sm font-medium">Rules</Label>
              <p className="max-w-xl text-xs leading-5 text-[#9d9384]">
                规则是持久的 prompt 级指令。挂载后其 Markdown 内容会在模型运行前注入智能体上下文。
              </p>
            </div>
            <span className="shrink-0 font-mono text-xs text-[#9d9384]">
              {draft.rule_ids.length} 已挂载
            </span>
          </div>
          {rules.length === 0 ? (
            <p className="text-xs text-[#9d9384]">暂无可用规则。</p>
          ) : (
            <div className="grid max-h-[284px] gap-2 overflow-y-auto pr-1">
              {rules.map((rule) => {
                const enabled = draft.rule_ids.includes(rule.id);
                return (
                  <label
                    key={rule.id}
                    className="flex min-w-0 cursor-pointer items-start gap-2.5 rounded-md border border-white/10 bg-white/5 px-2.5 py-2 text-xs hover:bg-white/10"
                  >
                    <input
                      type="checkbox"
                      checked={enabled}
                      onChange={(event) => toggleRule(rule.id, event.target.checked)}
                      className="mt-0.5 size-3.5 shrink-0"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="font-medium">{rule.name}</span>
                        <span className="truncate font-mono text-[#9d9384]">{rule.id}</span>
                      </div>
                      <div className="mt-0.5 line-clamp-2 text-[#9d9384]">
                        {rule.description || "暂无描述。"}
                      </div>
                    </div>
                  </label>
                );
              })}
            </div>
          )}
        </div>

        <div className="grid gap-2 rounded-md border border-white/10 bg-black/10 p-3 text-[#f7f2e8]">
          <div className="flex items-center justify-between gap-3">
            <Label className="text-sm font-medium">Sub-agents</Label>
            <span className="font-mono text-xs text-[#9d9384]">
              {draft.sub_agents.length} 已挂载
            </span>
          </div>
          {agents.length === 0 ? (
            <div className="rounded-md border border-white/10 bg-white/5 px-2.5 py-2 text-xs text-[#9d9384]">
              请先创建辅助智能体，再回到这里挂载。
            </div>
          ) : (
            <div className="grid max-h-[284px] gap-2 overflow-y-auto pr-1">
              {agents.map((agent) => {
                const enabled = selectedSubAgents.has(agent.id);
                const toggle = (on: boolean) => {
                  const next = on
                    ? [...draft.sub_agents, { agent_id: agent.id }]
                    : draft.sub_agents.filter((entry) => entry.agent_id !== agent.id);
                  update({ sub_agents: next });
                };
                return (
                  <label
                    key={agent.id}
                    className="flex min-w-0 cursor-pointer items-start gap-2.5 rounded-md border border-white/10 bg-white/5 px-2.5 py-2 text-xs hover:bg-white/10"
                  >
                    <input
                      type="checkbox"
                      checked={enabled}
                      onChange={(event) => toggle(event.target.checked)}
                      className="mt-0.5 size-3.5 shrink-0"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="truncate font-medium">{agent.name}</span>
                        <span className="truncate font-mono text-[#9d9384]">{agent.id}</span>
                      </div>
                      <div className="mt-0.5 line-clamp-2 text-[#9d9384]">
                        {agent.description || agent.model || "已保存的 LAP 智能体"}
                      </div>
                    </div>
                  </label>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function RuntimeSelectOption({
  alias,
  displayName,
  apiSpec,
  isDefault,
  compact = false,
}: {
  alias: string;
  displayName: string;
  apiSpec: string;
  isDefault?: boolean;
  compact?: boolean;
}) {
  return (
    <span className={cn("flex min-w-0 max-w-full items-center", compact ? "gap-2" : "gap-3")}>
      <span
        className={cn(
          "flex shrink-0 items-center justify-center rounded-lg border border-white/10 bg-white/5",
          compact ? "size-6" : "size-8",
        )}
      >
        <BrandIcon id={runtimeBrandIconId(alias, apiSpec)} className={compact ? "size-3.5" : "size-4"} />
      </span>
      <span className="min-w-0">
        <span className="flex min-w-0 items-center gap-2">
          <span className="truncate text-sm font-medium !text-[#f7f2e8]">{displayName}</span>
          {isDefault && !compact && (
            <span className="runtime-option-muted rounded-md border border-white/10 bg-white/5 px-1.5 py-0.5 text-[10px] !text-[#c9c0b1]">
              Default
            </span>
          )}
        </span>
        <span className="runtime-option-muted mt-0.5 block truncate font-mono text-[11px] !text-[#c9c0b1]">
          {compact ? runtimeSubtitle(apiSpec || alias) : `${runtimeLabel(apiSpec || alias)} · ${alias}`}
        </span>
      </span>
    </span>
  );
}

function defaultToolsForHarnessRuntime(
  runtimeAlias: string,
  harnesses: RuntimeHarness[],
  runtimes: AgentRuntime[],
): AgentDraft["tools"] {
  const tools =
    harnesses.find((entry) => entry.alias === runtimeAlias)?.tools ??
    runtimes.find((entry) => entry.id === runtimeAlias)?.tools ??
    [];
  return tools
    .filter((tool: AgentRuntimeTool) => tool.enabled_by_default)
    .map((tool) => ({ type: tool.id }));
}

function runtimeApiSpec(value: string): string {
  if (value === "claude_managed_agents") return "claude_managed_agents";
  if (value === "cursor") return "cursor";
  if (value === "gemini_antigravity") return "gemini_antigravity";
  if (value === "opencode") return "opencode";
  return value;
}

function runtimeLabel(value: string): string {
  if (value === "claude_managed_agents") return "Claude Managed Agents";
  if (value === "cursor") return "Cursor";
  if (value === "gemini_antigravity") return "Gemini Antigravity";
  if (value === "opencode") return "OpenCode";
  return value || "Runtime";
}

function runtimeSubtitle(value: string): string {
  if (value === "claude_managed_agents") return "Anthropic sessions and tools";
  if (value === "cursor") return "Background repo agents";
  if (value === "gemini_antigravity") return "Google managed sandbox";
  if (value === "opencode") return "OpenCode server";
  return "Custom runtime";
}

function ConfigPreview({
  draft,
  mcpIntegrations,
}: {
  draft: AgentDraft;
  mcpIntegrations: Integration[];
}) {
  const selectedMcpIntegrations = draft.mcp_server_ids.map((id) => {
    const integration = mcpIntegrations.find((item) => item.id === id);
    return integration ?? {
      id,
      name: id,
      description: "Unknown MCP server.",
      category: "Other",
      envKey: "Unknown",
      mcpUrl: "",
      tools: [],
      source: "catalog" as const,
      connected: false,
      status: null,
    };
  });

  return (
    <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
      <div className="grid gap-5">
        <div>
          <div className="text-xs uppercase text-[#9d9384]">Name</div>
          <div className="mt-1 text-xl font-semibold text-[#fffaf0]">{draft.name}</div>
          <p className="mt-2 max-w-2xl text-sm leading-6 text-[#c9c0b1]">{draft.description}</p>
        </div>

        <div className="grid gap-3 sm:grid-cols-2">
          <PreviewItem label="Model" value={draft.model} />
          <PreviewItem label="Runtime" value={draft.runtime} />
          <PreviewItem label="Schedule" value={scheduleLabel(draft.cron, draft.timezone)} />
          <PreviewItem label="Tools" value={draft.tools.map((tool) => tool.type).filter(Boolean).join(", ")} />
        </div>

        <div>
          <div className="text-xs uppercase text-[#9d9384]">System prompt</div>
          <pre className="mt-2 max-h-80 overflow-y-auto whitespace-pre-wrap rounded-lg border border-white/10 bg-black/15 p-3 font-mono text-[12px] leading-6 text-[#f0d3bd]">
            {draft.system || "未设置 system prompt。"}
          </pre>
        </div>

        <div className="grid gap-3 sm:grid-cols-2">
          <TokenList label="Vault keys" values={draft.vault_keys} />
          <TokenList label="Skill IDs" values={draft.skill_ids} />
          <TokenList label="Rule IDs" values={draft.rule_ids} />
          <TokenList label="Sub-agents" values={draft.sub_agents.map((agent) => agent.agent_id)} />
        </div>

        <div className="rounded-lg border border-white/10 bg-black/10 p-3">
          <div className="text-xs uppercase text-[#9d9384]">MCP integrations</div>
          {selectedMcpIntegrations.length === 0 ? (
            <div className="mt-2 text-xs text-[#c9c0b1]">None</div>
          ) : (
            <div className="mt-3 grid gap-2">
              {selectedMcpIntegrations.map((integration) => {
                const toolCount = integration.tools.filter(Boolean).length;
                return (
                  <div
                    key={integration.id}
                    className="rounded-md border border-white/10 bg-white/5 px-2.5 py-2"
                  >
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-xs font-medium text-[#f7f2e8]">{integration.name}</span>
                      <span className="font-mono text-[11px] text-[#9d9384]">{integration.id}</span>
                      <Badge variant="outline" className="h-5 rounded-md border-white/10 bg-white/5 text-[10px] text-[#c9c0b1]">
                        {toolCount > 0 ? `${toolCount} tools` : "Toolset attached"}
                      </Badge>
                    </div>
                    <p className="mt-1 line-clamp-2 text-xs text-[#c9c0b1]">{integration.description}</p>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function PreviewItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-white/10 bg-black/10 p-3">
      <div className="text-xs uppercase text-[#9d9384]">{label}</div>
      <div className="mt-1 break-words font-mono text-xs text-[#f7f2e8]">{value || "未设置"}</div>
    </div>
  );
}

function TokenList({ label, values }: { label: string; values: string[] }) {
  return (
    <div className="rounded-lg border border-white/10 bg-black/10 p-3">
      <div className="text-xs uppercase text-[#9d9384]">{label}</div>
      {values.length === 0 ? (
        <div className="mt-2 text-xs text-[#c9c0b1]">None</div>
      ) : (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {values.map((value) => (
            <span
              key={value}
              className="rounded-md border border-white/10 bg-white/5 px-1.5 py-0.5 font-mono text-[11px] text-[#f7f2e8]"
            >
              {value}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
