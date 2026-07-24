"use client";

import { Suspense, useEffect, useMemo, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { ArrowUp, Bot, Loader2, MessageSquareText, MessagesSquare, Sparkles, Trash2 } from "lucide-react";
import { BrandIcon } from "@/components/brand-icons";
import { EmptyState } from "@/components/empty-state";
import { StatusDot } from "@/components/status-dot";
import { Sidebar } from "@/components/sidebar";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import {
  apiErrorMessage,
  createSession,
  listRuntimeHarnesses,
  listAgents,
  listModels,
  listSessions,
  setStoredMasterKey,
  ensureWebSession,
} from "@/lib/api";
import { defaultModelForRuntime, isFederatedBridgeRuntime, runtimeSupportsModelDiscovery, selectedRuntimeModel } from "@/lib/model-options";
import { runtimeBrandIconId } from "@/lib/runtime-branding";
import { importedSource } from "@/app/agents/agent-row-utils";
import { deletedAgentSnapshot } from "@/lib/types";
import type { Agent, AgentRuntimeId, OpencodeSession, RuntimeHarness } from "@/lib/types";

const TEMPORARY_SESSION_VALUE = "__temporary_session__";
const CLAUDE_RUNTIME: AgentRuntimeId = "claude_managed_agents";
const RECENT_SESSION_LIMIT = 6;

function runtimeLabel(runtime: RuntimeHarness | string): string {
  if (typeof runtime !== "string") return runtime.display_name;
  if (runtime === "claude_managed_agents") return "自托管开放 Harness";
  if (runtime === "cursor") return "Cursor";
  if (runtime === "gemini_antigravity") return "Gemini Antigravity";
  if (runtime === "claude-code" || runtime === "cc") return "Claude Code 智能体";
  return runtime;
}

function runtimeSubtitle(harness: RuntimeHarness): string {
  if (!harness.connected) return "需要配置密钥";
  if (harness.api_spec === "claude_managed_agents") return "Anthropic 协议与工具";
  if (harness.api_spec === "cursor") return "后台代码库智能体";
  if (harness.api_spec === "gemini_antigravity") return "Google 托管沙箱";
  return "托管运行时会话";
}

function connectedRuntimeHarnesses(harnesses: RuntimeHarness[]): RuntimeHarness[] {
  return harnesses.filter((item) => item.connected);
}

function defaultRuntimeAlias(harnesses: RuntimeHarness[]): AgentRuntimeId | "" {
  return harnesses.find((item) => item.alias === CLAUDE_RUNTIME)?.alias ?? harnesses[0]?.alias ?? "";
}

function selectableRuntimeAlias(
  runtime: AgentRuntimeId | "",
  harnesses: RuntimeHarness[],
): AgentRuntimeId | "" {
  if (runtime && (isFederatedBridgeRuntime(runtime) || harnesses.some((item) => item.alias === runtime))) {
    return runtime;
  }
  return defaultRuntimeAlias(harnesses);
}

function isAgentRuntimeId(value: unknown): value is AgentRuntimeId {
  return typeof value === "string" && value.length > 0;
}

function configuredRuntime(agent: Agent | null): AgentRuntimeId | "" {
  if (!agent) return "";
  const config = agent.config;
  if (config && typeof config === "object" && !Array.isArray(config)) {
    const runtime = (config as { runtime?: unknown }).runtime;
    if (isAgentRuntimeId(runtime)) return runtime;
  }
  return isAgentRuntimeId(agent.harness) ? agent.harness : "";
}

function promptTitle(prompt: string): string {
  const compact = prompt.replace(/\s+/g, " ").trim();
  if (!compact) return "新对话会话";
  return compact.length > 46 ? `${compact.slice(0, 46).trimEnd()}…` : compact;
}

function isDbBackedAgent(agent: Agent): boolean {
  return agent.id.startsWith("agent_");
}

function cursorEnvironment(repository: string, ref: string): Record<string, unknown> {
  return {
    repository: repository.trim(),
    ref: ref.trim() || "main",
    target_branch: "agent/{agent_id}/{session_id}",
    auto_create_pr: false,
  };
}

function sessionTimestamp(session: OpencodeSession): number {
  return session.time?.updated ?? session.time?.created ?? 0;
}

function relativeTimeLabel(timestampMs: number): string {
  if (!timestampMs) return "";
  const minutes = Math.round((Date.now() - timestampMs) / 60000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.round(hours / 24);
  if (days < 30) return `${days} 天前`;
  return new Date(timestampMs).toLocaleDateString();
}

function sessionIsBusy(session: OpencodeSession): boolean {
  const status = (session.status ?? "").toLowerCase();
  return status === "busy" || status === "running" || Boolean(session.provider_run_id);
}

type StartPhase = "session" | "redirect" | null;

function startButtonLabel(phase: StartPhase): string {
  if (phase === "session") return "创建中…";
  if (phase === "redirect") return "跳转中…";
  return "开启对话";
}

function SessionsStart() {
  const router = useRouter();
  const searchParams = useSearchParams();

  const token = searchParams.get("token");
  if (token && typeof window !== "undefined") {
    setStoredMasterKey(token);
  }

  const [selectedAgentId, setSelectedAgentId] = useState(searchParams.get("agent") ?? "");
  const [prompt, setPrompt] = useState("");
  const [runtime, setRuntime] = useState<AgentRuntimeId | "">("");
  const [harnesses, setHarnesses] = useState<RuntimeHarness[]>([]);
  const [savedAgents, setSavedAgents] = useState<Agent[]>([]);
  const [recentSessions, setRecentSessions] = useState<OpencodeSession[] | null>(null);
  const [repository, setRepository] = useState("");
  const [ref, setRef] = useState("main");
  const [startPhase, setStartPhase] = useState<StartPhase>(null);
  const [error, setError] = useState<string | null>(null);
  const starting = startPhase !== null;

  useEffect(() => {
    ensureWebSession();
    Promise.all([listRuntimeHarnesses(), listSessions(), listAgents()])
      .then(([nextHarnesses, nextSessions, nextAgents]) => {
        const nextRuntimeOptions = connectedRuntimeHarnesses(nextHarnesses);
        setHarnesses(nextHarnesses);
        setRuntime((current) => {
          return selectableRuntimeAlias(current, nextRuntimeOptions);
        });
        setRecentSessions(
          nextSessions
            .filter((session) => !session.title?.startsWith("agent-builder-"))
            .sort((a, b) => sessionTimestamp(b) - sessionTimestamp(a))
            .slice(0, RECENT_SESSION_LIMIT),
        );
        setSavedAgents(nextAgents);
      })
      .catch((err) => setError(apiErrorMessage(err, "加载运行时失败")));
  }, []);

  const runtimeOptions = useMemo(() => connectedRuntimeHarnesses(harnesses), [harnesses]);
  const selectedRuntime = useMemo(
    () => runtimeOptions.find((item) => item.alias === runtime),
    [runtime, runtimeOptions],
  );
  const selectedRuntimeSpec = selectedRuntime?.api_spec ?? null;
  const selectedAgent = useMemo(
    () => savedAgents.find((agent) => agent.id === selectedAgentId) ?? null,
    [savedAgents, selectedAgentId],
  );
  const selectedAgentRuntime = configuredRuntime(selectedAgent);
  const selectedAgentMissing = selectedAgentId !== "" && selectedAgent === null;
  const selectedAgentIsConfigured = Boolean(selectedAgent && !isDbBackedAgent(selectedAgent));
  const needsRuntime = !selectedAgentIsConfigured;
  const selectedAgentIsImported = Boolean(selectedAgent && importedSource(selectedAgent));
  const needsPrompt = !selectedAgent;
  const canStart =
    (!needsPrompt || prompt.trim().length > 0) &&
    !starting &&
    !selectedAgentMissing &&
    (!needsRuntime ||
      (runtime !== "" &&
        (isFederatedBridgeRuntime(runtime) || Boolean(selectedRuntime?.connected)) &&
        (selectedRuntimeSpec !== "cursor" || repository.trim().length > 0)));

  useEffect(() => {
    if (!selectedAgentRuntime) return;
    setRuntime(selectableRuntimeAlias(selectedAgentRuntime, runtimeOptions));
  }, [runtimeOptions, selectedAgentRuntime]);

  const startSession = async () => {
    const trimmed = prompt.trim();
    const runtimeId = runtime;
    if (selectedAgentMissing) {
      setError("选中的智能体已不存在，请重新选择。");
      return;
    }
    if (
      starting ||
      !canStart ||
      (needsRuntime && !runtimeId) ||
      (needsPrompt && !trimmed)
    ) {
      return;
    }
    setError(null);
    try {
      const title = selectedAgent ? `${selectedAgent.name} 会话` : promptTitle(trimmed);
      let shouldAutostartPrompt = Boolean(trimmed);
      let session: OpencodeSession;
      if (selectedAgent && !isDbBackedAgent(selectedAgent)) {
        setStartPhase("session");
        session = await createSession(title, selectedAgent.id);
      } else {
        const runtimeForSession = runtimeId as AgentRuntimeId;
        const model = runtimeSupportsModelDiscovery(runtimeForSession)
          ? selectedRuntimeModel(await listModels(runtimeForSession), "")
          : defaultModelForRuntime(runtimeForSession);
        if (!model) {
          throw new Error(`${runtimeLabel(selectedRuntime ?? runtimeForSession)} 没有配置可用模型。`);
        }
        const environment =
          selectedRuntimeSpec === "cursor" ? cursorEnvironment(repository, ref) : {};
        shouldAutostartPrompt = false;
        setStartPhase("session");
        session = await createSession(title, selectedAgent?.id, {
          runtime: runtimeForSession,
          model,
          prompt: trimmed || undefined,
          environment,
        });
      }
      setStartPhase("redirect");
      const params = new URLSearchParams({
        id: session.id,
      });
      if (trimmed && shouldAutostartPrompt) {
        params.set("prompt", trimmed);
        params.set("autostart", "1");
      }
      router.push(`/chat/?${params.toString()}`);
    } catch (err) {
      setError(apiErrorMessage(err, "创建会话失败"));
      setStartPhase(null);
    }
  };

  const runtimeStatusLabel = selectedAgentIsConfigured
    ? `${selectedAgent?.name} 就绪`
    : selectedAgentIsImported
      ? `${selectedRuntime?.display_name ?? runtimeLabel(runtime)}（固定运行时）`
      : selectedRuntime?.connected
        ? `${selectedRuntime.display_name} 就绪`
        : isFederatedBridgeRuntime(runtime)
          ? `${selectedAgent?.name ?? "远程智能体"} 就绪`
          : "密钥未配置";

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden selection:bg-blue-500/20">
      <Sidebar />
      <main id="main-content" className="relative flex min-w-0 flex-1 overflow-hidden bg-background text-foreground">
        <div
          aria-hidden
          className="absolute inset-0 opacity-40 pointer-events-none"
          style={{
            backgroundImage:
              "radial-gradient(circle at center, color-mix(in oklab, var(--primary) 18%, transparent) 1px, transparent 1.4px)",
            backgroundSize: "20px 20px",
          }}
        />
        <div
          aria-hidden
          className="absolute inset-x-0 bottom-0 h-[48%] opacity-30 pointer-events-none"
          style={{
            background:
              "radial-gradient(ellipse at 52% 15%, color-mix(in oklab, var(--primary) 26%, transparent), color-mix(in oklab, var(--primary) 8%, transparent) 42%, transparent 72%)",
          }}
        />

        <section className="relative z-10 flex min-h-full w-full flex-col items-center justify-center overflow-y-auto px-6 py-12">
          {/* Main Launch Box */}
          <div className="w-full max-w-2xl overflow-hidden rounded-2xl border border-border/80 bg-card p-2 shadow-lg backdrop-blur">
            <Textarea
              value={prompt}
              onChange={(event) => setPrompt(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
                  event.preventDefault();
                  void startSession();
                }
              }}
              placeholder={selectedAgent ? "输入第一条任务指令或提问..." : "描述你的任务，或向智能体发问..."}
              aria-label="会话消息"
              className="min-h-28 resize-none border-0 bg-transparent px-4 py-3 text-sm shadow-none outline-none placeholder:text-muted-foreground focus-visible:ring-0 text-foreground leading-relaxed"
            />
            <div className="flex flex-wrap items-center gap-2 border-t border-border/70 bg-muted/20 px-3 py-3 rounded-b-xl">
              <Select
                value={selectedAgentId || TEMPORARY_SESSION_VALUE}
                onValueChange={(value) => {
                  const next = value ?? "";
                  const nextAgentId = next === TEMPORARY_SESSION_VALUE ? "" : next;
                  setSelectedAgentId(nextAgentId);
                  const nextAgent = savedAgents.find((agent) => agent.id === nextAgentId) ?? null;
                  const nextRuntime = configuredRuntime(nextAgent);
                  if (nextRuntime) setRuntime(selectableRuntimeAlias(nextRuntime, runtimeOptions));
                }}
              >
                <SelectTrigger className="h-9 w-auto min-w-[200px] max-w-[280px] rounded-xl border border-border/70 bg-background px-3 text-left text-foreground shadow-2xs">
                  <span className="flex min-w-0 items-center gap-2">
                    <span className="shrink-0 text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
                      智能体
                    </span>
                    <span className="truncate text-xs font-semibold">
                      {selectedAgent?.name ?? "临时智能体"}
                    </span>
                  </span>
                </SelectTrigger>
                <SelectContent className="w-[360px]">
                  <SelectItem value={TEMPORARY_SESSION_VALUE} className="py-2.5">
                    <span className="flex min-w-0 items-center gap-3">
                      <span className="flex size-7 shrink-0 items-center justify-center rounded-lg border border-border bg-muted">
                        <MessagesSquare className="size-3.5" />
                      </span>
                      <span className="min-w-0">
                        <span className="block truncate text-xs font-semibold">临时对话会话</span>
                        <span className="block truncate text-[11px] text-muted-foreground">
                          不创建永久智能体预设，即用即走
                        </span>
                      </span>
                    </span>
                  </SelectItem>
                  {savedAgents.length > 0 && (
                    <>
                      <div className="px-2 py-1.5 text-[10px] uppercase font-bold tracking-wider text-muted-foreground border-t mt-1">
                        已保存的智能体
                      </div>
                      {savedAgents.map((agent) => {
                        const agentRuntime = configuredRuntime(agent);
                        return (
                          <SelectItem key={agent.id} value={agent.id} className="py-2.5">
                            <span className="flex min-w-0 items-center gap-3">
                              <span className="flex size-7 shrink-0 items-center justify-center rounded-lg border border-border bg-muted">
                                {agentRuntime ? (
                                  <BrandIcon id={runtimeBrandIconId(agentRuntime, null)} className="size-3.5" />
                                ) : (
                                  <Bot className="size-3.5" />
                                )}
                              </span>
                              <span className="min-w-0">
                                <span className="flex min-w-0 items-center gap-2">
                                  <span className="truncate text-xs font-semibold">{agent.name}</span>
                                  {agent.model && (
                                    <span className="shrink-0 rounded border border-border/60 bg-muted/40 px-1 py-px font-mono text-[10px] text-muted-foreground">
                                      {agent.model}
                                    </span>
                                  )}
                                </span>
                                <span className="block truncate text-[11px] text-muted-foreground">
                                  {agent.description || agentRuntime || "已保存智能体"}
                                </span>
                              </span>
                            </span>
                          </SelectItem>
                        );
                      })}
                    </>
                  )}
                </SelectContent>
              </Select>

              <Select
                value={runtime}
                onValueChange={(value) => setRuntime((value ?? "") as AgentRuntimeId | "")}
                disabled={selectedAgentIsImported}
              >
                <SelectTrigger
                  disabled={selectedAgentIsImported}
                  title={selectedAgentIsImported ? "导入的智能体使用固定运行时" : undefined}
                  className="h-9 w-auto min-w-[220px] max-w-[300px] rounded-xl border border-border/70 bg-background px-3 text-left text-foreground shadow-2xs">
                  <span className="flex min-w-0 items-center gap-2">
                    <span className="shrink-0 text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
                      运行时
                    </span>
                    <span className="flex size-5 shrink-0 items-center justify-center rounded-md bg-muted">
                      <BrandIcon
                        id={runtimeBrandIconId(
                          selectedRuntime?.alias ?? "",
                          selectedRuntimeSpec,
                        )}
                        className="size-3.5"
                      />
                    </span>
                    <span className="truncate text-xs font-semibold">
                      {selectedRuntime?.display_name ?? (harnesses.length > 0 ? "未连接" : "选择运行时")}
                    </span>
                  </span>
                </SelectTrigger>
                <SelectContent className="w-[320px]">
                  {runtimeOptions.length > 0 ? (
                    runtimeOptions.map((item) => (
                      <SelectItem key={item.alias} value={item.alias} className="py-2.5">
                        <span className="flex min-w-0 items-center gap-3">
                          <span className="flex size-7 shrink-0 items-center justify-center rounded-lg border border-border bg-muted">
                            <BrandIcon id={runtimeBrandIconId(item.alias, item.api_spec)} className="size-3.5" />
                          </span>
                          <span className="min-w-0">
                            <span className="block truncate text-xs font-semibold">{item.display_name}</span>
                            <span className="block truncate text-[11px] text-muted-foreground">
                              {runtimeSubtitle(item)}
                            </span>
                          </span>
                        </span>
                      </SelectItem>
                    ))
                  ) : (
                    <div className="px-3 py-3 text-xs text-muted-foreground">
                      暂无连接的运行时
                    </div>
                  )}
                  <div className="border-t border-border/60 px-3 py-2 text-[11px] text-muted-foreground">
                    前往 AI 网关 &gt; Agent 运行时 配置更多托管节点。
                  </div>
                </SelectContent>
              </Select>

              <span className="ml-auto hidden items-center gap-1.5 text-xs text-muted-foreground sm:flex font-mono">
                <StatusDot
                  tone={selectedAgentIsConfigured || selectedRuntime?.connected ? "success" : "warning"}
                  label={runtimeStatusLabel}
                  className="size-2"
                />
                {runtimeStatusLabel}
              </span>
              <Button
                type="button"
                size="sm"
                onClick={() => void startSession()}
                disabled={!canStart}
                className="rounded-xl h-9 px-4 bg-blue-600 hover:bg-blue-700 text-white font-medium text-xs shadow-2xs gap-1.5"
                aria-label="开始会话"
              >
                {starting ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : (
                  <ArrowUp className="size-3.5" />
                )}
                <span>{startButtonLabel(startPhase)}</span>
              </Button>
            </div>
            {selectedRuntimeSpec === "cursor" && (
              <div className="grid gap-3 border-t border-border/70 bg-muted/30 px-4 py-3 sm:grid-cols-[1fr_140px]">
                <div className="grid gap-1">
                  <Label htmlFor="cursor-repository" className="text-xs text-muted-foreground">
                    代码仓库地址 <span className="text-destructive">*</span>
                  </Label>
                  <Input
                    id="cursor-repository"
                    value={repository}
                    onChange={(event) => setRepository(event.target.value)}
                    placeholder="https://github.com/org/repo"
                    className="h-8 border-border bg-background text-xs font-mono"
                  />
                </div>
                <div className="grid gap-1">
                  <Label htmlFor="cursor-ref" className="text-xs text-muted-foreground">
                    分支
                  </Label>
                  <Input
                    id="cursor-ref"
                    value={ref}
                    onChange={(event) => setRef(event.target.value)}
                    placeholder="main"
                    className="h-8 border-border bg-background text-xs font-mono"
                  />
                </div>
              </div>
            )}
            {error && (
              <div className="border-t border-destructive/20 bg-destructive/10 px-4 py-3 text-xs font-mono text-destructive">
                {error}
              </div>
            )}
          </div>

          <div className="mt-8 w-full max-w-2xl space-y-3">
            <div className="flex items-center justify-between">
              <h2 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">最近建立的会话</h2>
            </div>
            {recentSessions === null ? (
              <div className="grid gap-2.5 sm:grid-cols-2">
                {[0, 1, 2, 3].map((item) => (
                  <div key={item} className="rounded-xl border border-border/70 bg-card p-3.5 shadow-2xs">
                    <div className="h-4 w-2/3 animate-pulse rounded bg-muted" />
                    <div className="mt-2 h-3 w-1/3 animate-pulse rounded bg-muted" />
                  </div>
                ))}
              </div>
            ) : recentSessions.length === 0 ? (
              <EmptyState
                icon={MessageSquareText}
                title="暂无最近会话"
                hint="在上方输入指令框并点击“开启对话”建立你的第一个智能体任务。"
                className="backdrop-blur rounded-2xl"
              />
            ) : (
              <div className="grid gap-2.5 sm:grid-cols-2">
                {recentSessions.map((session) => (
                  <RecentSessionCard
                    key={session.id}
                    session={session}
                    agents={savedAgents}
                    onOpen={() => router.push(`/chat/?id=${encodeURIComponent(session.id)}`)}
                  />
                ))}
              </div>
            )}
          </div>
        </section>
      </main>
    </div>
  );
}

function RecentSessionCard({
  session,
  agents,
  onOpen,
}: {
  session: OpencodeSession;
  agents: Agent[];
  onOpen: () => void;
}) {
  const agent = agents.find(
    (item) => item.id === session.agent_id || item.id === session.agent,
  );
  // `agents` only carries live agents, so a deleted agent's sessions would
  // otherwise render with no attribution at all.
  const deletedAgent = agent ? null : deletedAgentSnapshot(session);
  const busy = sessionIsBusy(session);
  const runtime = session.runtime ?? "";
  return (
    <button
      type="button"
      onClick={onOpen}
      className="rounded-xl border border-border/70 bg-card p-3.5 text-left shadow-2xs backdrop-blur transition-all hover:border-blue-500/40 hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
    >
      <div className="flex items-start gap-3">
        <span className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-lg border border-border bg-background shadow-2xs">
          {runtime ? (
            <BrandIcon id={runtimeBrandIconId(runtime, null)} className="size-4" />
          ) : (
            <MessageSquareText className="size-4 text-muted-foreground" />
          )}
        </span>
        <span className="min-w-0 flex-1">
          <span className="flex min-w-0 items-center gap-2">
            <span className="truncate text-xs font-semibold text-foreground">
              {session.title || "未命名会话"}
            </span>
            <StatusDot tone={busy ? "success" : "idle"} label={busy ? "运行中" : "空闲"} />
          </span>
          <span className="mt-1 flex min-w-0 items-center gap-2 text-[11px] text-muted-foreground">
            {agent && <span className="truncate font-medium">{agent.name}</span>}
            {deletedAgent && (
              <span className="flex min-w-0 items-center gap-1 text-amber-600 dark:text-amber-500">
                <Trash2 className="size-3 shrink-0" />
                <span className="truncate font-medium">
                  {deletedAgent.name || session.agent_id}
                </span>
                <span className="shrink-0">（已删除）</span>
              </span>
            )}
            <span className="shrink-0 font-mono">{relativeTimeLabel(sessionTimestamp(session))}</span>
          </span>
        </span>
      </div>
    </button>
  );
}

export default function SessionsPage() {
  return (
    <Suspense>
      <SessionsStart />
    </Suspense>
  );
}
