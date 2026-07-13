"use client";

import { useState } from "react";
import {
  ArrowUp,
  Bot,
  CheckCircle2,
  Clipboard,
  Code2,
  FileSearch,
  Loader2,
  MessageSquareText,
  Sparkles,
  XCircle,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { apiErrorMessage, askAgentBuilderCopilot } from "@/lib/api";
import type { AgentBuilderCopilotResponse } from "@/lib/api";
import type { AgentDraft } from "@/lib/agent-builder";
import type { Integration } from "@/lib/integrations";
import type { Agent, AgentRuntime, Rule, RuntimeHarness, Skill } from "@/lib/types";
import { scheduleLabel } from "@/lib/schedule";
import { cn } from "@/lib/utils";
import type { BuilderChatMessage, BuilderView } from "./builder-shared";
import { AgentDraftControls } from "./draft-controls";
import { ConfigPreview } from "./config-preview";
import { StreamingPreview } from "./streaming-preview";

export function ConfigStep({
  canCreate,
  configText,
  copied,
  draft,
  draftNotice,
  modelSuggestion,
  onModelSuggestion,
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
  modelSuggestion: { suggested: string; current: string } | null;
  onModelSuggestion: (accept: boolean) => void;
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
                {saving ? "处理中..." : "进入评估与验证"}
              </Button>
              <Button
                type="button"
                variant="secondary"
                onClick={() => document.getElementById("agent-config-refine")?.focus()}
              >
                继续调整
              </Button>
            </div>
            {modelSuggestion && (
              <div className="mt-4 flex max-w-xl flex-wrap items-center gap-2 rounded-lg border border-sky-500/20 bg-sky-500/10 px-3 py-2 text-sm text-sky-800 dark:text-sky-300">
                <span className="min-w-0">
                  AI 建议将模型从 <span className="font-mono text-xs">{modelSuggestion.current}</span> 改为{" "}
                  <span className="font-mono text-xs">{modelSuggestion.suggested}</span>
                  （当前保留了你的选择）
                </span>
                <span className="ml-auto flex shrink-0 gap-2">
                  <button
                    type="button"
                    className="rounded border border-sky-500/40 px-2 py-0.5 text-xs hover:bg-sky-500/20"
                    onClick={() => onModelSuggestion(true)}
                  >
                    使用建议
                  </button>
                  <button
                    type="button"
                    className="rounded px-2 py-0.5 text-xs text-muted-foreground hover:bg-muted"
                    onClick={() => onModelSuggestion(false)}
                  >
                    保留当前
                  </button>
                </span>
              </div>
            )}
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
              {drafting ? (
                <Loader2 className="size-4 animate-spin motion-reduce:animate-none" />
              ) : (
                <ArrowUp className="size-4" />
              )}
            </Button>
          </div>
        </div>
      </section>

      <section className="min-h-0">
        <div className="flex h-full min-h-[560px] flex-col overflow-hidden rounded-lg border border-editor-border bg-editor-surface text-editor-foreground shadow-[0_18px_70px_rgba(15,23,42,0.16)]">
          <div className="flex shrink-0 items-center justify-between border-b border-white/10 px-4 py-3">
            <div className="flex items-center gap-1">
              <Button
                type="button"
                size="sm"
                variant={view === "config" ? "secondary" : "ghost"}
                onClick={() => onViewChange("config")}
                className={cn(
                  "h-8 text-editor-muted hover:bg-white/10 hover:text-white",
                  view === "config" && "bg-white text-editor-inverse hover:bg-white",
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
                  "h-8 text-editor-muted hover:bg-white/10 hover:text-white",
                  view === "preview" && "bg-white text-editor-inverse hover:bg-white",
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
                  "h-8 text-editor-muted hover:bg-white/10 hover:text-white",
                  view === "edit" && "bg-white text-editor-inverse hover:bg-white",
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
                className="text-editor-muted hover:bg-white/10 hover:text-white"
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
              className="min-h-0 flex-1 resize-none rounded-none border-0 bg-editor-surface px-5 py-4 font-mono text-[13px] leading-6 text-editor-accent shadow-none outline-none focus-visible:ring-0"
              aria-label="Agent YAML config"
            />
          ) : (
            <ConfigPreview draft={draft} mcpIntegrations={mcpIntegrations} />
          )}

          <div className="flex shrink-0 flex-wrap items-center gap-2 border-t border-white/10 px-4 py-3 text-xs text-editor-muted">
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

function AssistantChangeMessage({ message }: { message: Extract<BuilderChatMessage, { role: "assistant" }> }) {
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
    onDraftChange({
      ...draft,
      tools: Array.from(next).map((type) => ({ type })),
    });
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
                <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">工具建议</h3>
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
                    <p className="mt-1 text-xs leading-5 text-muted-foreground">{recommendation.reason}</p>
                    {recommendation.risk && (
                      <p className="mt-1 text-xs leading-5 text-amber-700 dark:text-amber-300">{recommendation.risk}</p>
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
      {loading ? (
        <Loader2 className="size-3.5 animate-spin motion-reduce:animate-none" />
      ) : (
        <Sparkles className="size-3.5" />
      )}
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
