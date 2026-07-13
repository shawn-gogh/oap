"use client";

import { toast } from "sonner";
import { useEffect, useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import { CheckCircle2, ExternalLink } from "lucide-react";
import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { ImportAgentDialog } from "../import-agent-dialog";
import { Button } from "@/components/ui/button";
import {
  AGENT_TEMPLATES,
  applicationGatePassed,
  agentTemplateForPrompt,
  buildAgentDraftFromPrompt,
  createInputFromDraft,
  evalGatePassed,
  parseAgentDraftConfig,
  stringifyAgentDraft,
  withRuntimeDefaultTools,
} from "@/lib/agent-builder";
import type { AgentDraft } from "@/lib/agent-builder";
import { diffAgentDrafts } from "@/lib/agent-draft-diff";
import type { FieldChange } from "@/lib/agent-draft-diff";
import { integrationFromMcpServer, sortIntegrations } from "@/lib/integrations";
import type { Integration } from "@/lib/integrations";
import {
  apiErrorMessage,
  createAgent,
  draftAgentConfigWithModel,
  refineAgentConfigWithModel,
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
import { defaultModelForRuntime, runtimeSupportsModelDiscovery, selectedRuntimeModel } from "@/lib/model-options";
import type { Agent, AgentRuntime, Rule, Skill, RuntimeHarness } from "@/lib/types";
import {
  BUILDER_DRAFT_STORAGE_KEY,
  loadSavedBuilderDraft,
  runtimeChoicesForDrafting,
  savedDraftAgeLabel,
  validateDraftForCreate,
} from "./builder-shared";
import type { BuilderChatMessage, BuilderStep, BuilderView, SavedBuilderDraft } from "./builder-shared";
import { PlatformSteps } from "./steps-bar";
import { CreateStep } from "./create-step";
import { ConfigStep } from "./config-step";
import { EvalStep } from "./eval-step";
import { ReviewStep } from "./review-step";

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
  const [modelSuggestion, setModelSuggestion] = useState<{
    suggested: string;
    current: string;
  } | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [messages, setMessages] = useState<BuilderChatMessage[]>([]);
  const [draftProgress, setDraftProgress] = useState<string | null>(null);
  const [savedDraft, setSavedDraft] = useState<SavedBuilderDraft | null>(null);

  const parsed = useMemo(() => parseAgentDraftConfig(configText), [configText]);
  const draft = parsed.draft;
  const canCreate =
    !saving && !parsed.error && draft.name.trim().length > 0 && applicationGatePassed(draft.application);

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
            Array.from(batch, ([serverId, tools]) => [serverId, tools.map((tool) => tool.name).filter(Boolean)]),
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
        return stringifyAgentDraft({
          ...currentDraft.draft,
          model: defaultModel,
        });
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
          return stringifyAgentDraft({
            ...currentDraft.draft,
            model: nextModel,
          });
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
        const payload: SavedBuilderDraft = {
          configText,
          step,
          messages,
          savedAt: Date.now(),
        };
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
    options?: {
      request?: string;
      notice?: string | null;
      summary?: string;
      changes?: FieldChange[];
    },
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
    setStep("config");
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
      // Keep the user's explicit model choice, but don't silently discard a
      // differing recommendation: surface it as an inline suggestion the
      // user can accept or dismiss (only when it's actually selectable).
      const recommendedModel = generatedDraft.draft.model?.trim() ?? "";
      const nextDraft = selectedModel ? { ...generatedDraft.draft, model: selectedModel } : generatedDraft.draft;
      setModelSuggestion(
        selectedModel && recommendedModel && recommendedModel !== selectedModel && models.includes(recommendedModel)
          ? { suggested: recommendedModel, current: selectedModel }
          : null,
      );
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
          summary: changes.length === 0 ? "配置没有需要修改的地方。" : `已应用 ${changes.length} 处修改：`,
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
      toast.success("已创建为草稿：通过预检并激活后才能运行或被调度。");
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
            <Button size="sm" variant="outline" onClick={() => setImportOpen(true)} className="hidden sm:inline-flex">
              <ExternalLink className="size-3.5" />
              导入智能体
            </Button>
            {step === "config" && (
              <Button
                size="sm"
                onClick={() => setStep("eval")}
                disabled={Boolean(parsed.error) || !applicationGatePassed(draft.application)}
              >
                <CheckCircle2 className="size-3.5" />
                进入验证
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

        <main className="min-h-0 flex-1 overflow-y-auto bg-background text-foreground">
          <PlatformSteps
            activeStep={step === "create" ? 1 : step === "config" ? 2 : step === "eval" ? 3 : 4}
            canEnterEvaluation={applicationGatePassed(draft.application) && !parsed.error}
            canEnterReview={evalGatePassed(draft.design) && !parsed.error && draft.name.trim().length > 0}
            onNavigate={setStep}
          />
          {savedDraft && step === "create" && (
            <div className="mx-auto mt-3 flex max-w-3xl flex-wrap items-center gap-3 rounded-lg border border-border bg-card px-4 py-3 text-sm shadow-sm">
              <span className="min-w-0 flex-1">
                检测到未完成的草稿（{savedDraftAgeLabel(savedDraft.savedAt)}
                保存），是否继续编辑？
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
              onBack={() => setStep("config")}
              onContinue={() => setStep("review")}
            />
          ) : step === "review" ? (
            <ReviewStep
              draft={draft}
              approvalEnforcement={
                harnesses.find((entry) => entry.alias === draft.runtime)?.approval_enforcement ??
                runtimes.find((entry) => entry.id === draft.runtime)?.approval_enforcement ??
                "advisory"
              }
              error={error}
              canCreate={canCreate}
              saving={saving}
              mcpIntegrations={mcpIntegrations}
              onDraftChange={updateDraft}
              onBack={() => setStep("eval")}
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
              modelSuggestion={modelSuggestion}
              onModelSuggestion={(accept) => {
                if (accept && modelSuggestion) {
                  updateDraft({ ...draft, model: modelSuggestion.suggested });
                }
                setModelSuggestion(null);
              }}
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
              onCreate={() => setStep("eval")}
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
