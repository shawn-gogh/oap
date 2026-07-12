"use client";

import { Suspense, useEffect, useMemo, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { ArrowUp, Bot, Loader2, MessageSquareText, Plus } from "lucide-react";
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
  createAgent,
  createSession,
  listRuntimeHarnesses,
  listAgents,
  listModels,
  listSessions,
  setStoredMasterKey,
} from "@/lib/api";
import { defaultModelForRuntime, runtimeSupportsModelDiscovery, selectedRuntimeModel } from "@/lib/model-options";
import { runtimeBrandIconId } from "@/lib/runtime-branding";
import type { Agent, AgentRuntimeId, OpencodeSession, RuntimeHarness } from "@/lib/types";
import { cn } from "@/lib/utils";

const NEW_AGENT_VALUE = "__new_agent__";
const CLAUDE_RUNTIME: AgentRuntimeId = "claude_managed_agents";
const RECENT_SESSION_LIMIT = 6;

function runtimeLabel(runtime: RuntimeHarness | string): string {
  if (typeof runtime !== "string") return runtime.display_name;
  if (runtime === "claude_managed_agents") return "Claude Agents";
  if (runtime === "cursor") return "Cursor";
  if (runtime === "gemini_antigravity") return "Gemini Antigravity";
  if (runtime === "claude-code" || runtime === "cc") return "Claude Code";
  return runtime;
}

function runtimeSubtitle(harness: RuntimeHarness): string {
  if (!harness.connected) return "缺少密钥";
  if (harness.api_spec === "claude_managed_agents") return "Anthropic 会话与工具";
  if (harness.api_spec === "cursor") return "后台仓库智能体";
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
  if (runtime && harnesses.some((item) => item.alias === runtime)) return runtime;
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
  if (!compact) return "New agent session";
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

type StartPhase = "agent" | "session" | "redirect" | null;

function startButtonLabel(phase: StartPhase): string {
  if (phase === "agent") return "创建智能体...";
  if (phase === "session") return "创建会话...";
  if (phase === "redirect") return "跳转中...";
  return "开始";
}

function SessionsStart() {
  const router = useRouter();
  const searchParams = useSearchParams();

  // Consume ?token= from litellm plugin mode before any API call fires.
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
  const needsPrompt = !selectedAgent;
  const canStart =
    (!needsPrompt || prompt.trim().length > 0) &&
    !starting &&
    !selectedAgentMissing &&
    (!needsRuntime ||
      (runtime !== "" &&
        Boolean(selectedRuntime?.connected) &&
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
      const title = selectedAgent ? `${selectedAgent.name} session` : promptTitle(trimmed);
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
        let agent = selectedAgent;
        if (!agent) {
          setStartPhase("agent");
          agent = await createAgent({
            name: title,
            description: `Started from ${runtimeLabel(selectedRuntime ?? runtimeForSession)} landing prompt.`,
            model,
            runtime: runtimeForSession,
            harness: "claude-code",
            system: "You are a helpful managed agent. Use available tools when they help complete the user's request.",
            tools: [{ type: "agent_toolset_20260401" }],
            mcp_servers: [],
            skills: [],
          });
        }
        const environment =
          selectedRuntimeSpec === "cursor" ? cursorEnvironment(repository, ref) : {};
        shouldAutostartPrompt = false;
        setStartPhase("session");
        session = await createSession(title, agent.id, {
          runtime: runtimeForSession,
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
    : selectedRuntime?.connected
      ? `${selectedRuntime.display_name} 就绪`
      : "运行时密钥缺失";

  return (
    <div className="flex h-screen bg-background text-foreground">
      <Sidebar />
      <main id="main-content" className="relative flex min-w-0 flex-1 overflow-hidden bg-background text-foreground">
        <div
          aria-hidden
          className="absolute inset-0 opacity-80"
          style={{
            backgroundImage:
              "radial-gradient(circle at center, rgba(59, 130, 246, 0.18) 1px, transparent 1.4px)",
            backgroundSize: "10px 10px",
          }}
        />
        <div
          aria-hidden
          className="absolute inset-x-0 bottom-0 h-[48%] opacity-70"
          style={{
            background:
              "radial-gradient(ellipse at 52% 15%, rgba(59,130,246,0.26), rgba(59,130,246,0.08) 42%, transparent 72%)",
          }}
        />

        <section className="relative z-10 flex min-h-full w-full flex-col items-center justify-center overflow-y-auto px-6 py-12">
          <div className="w-full max-w-2xl overflow-hidden rounded-xl border border-border bg-card shadow-lg backdrop-blur">
            <Textarea
              value={prompt}
              onChange={(event) => setPrompt(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.shiftKey) {
                  event.preventDefault();
                  void startSession();
                }
              }}
              placeholder={selectedAgent ? "可选的第一条消息" : "描述任务，或直接提问"}
              aria-label="Session prompt"
              className="min-h-24 resize-none border-0 bg-transparent px-4 py-4 text-[15px] shadow-none outline-none placeholder:text-muted-foreground focus-visible:ring-0 dark:text-foreground"
            />
            <div className="flex flex-wrap items-center gap-2 border-t border-border bg-muted/30 px-3 py-3">
              <Select
                value={selectedAgentId || NEW_AGENT_VALUE}
                onValueChange={(value) => {
                  const next = value ?? "";
                  const nextAgentId = next === NEW_AGENT_VALUE ? "" : next;
                  setSelectedAgentId(nextAgentId);
                  const nextAgent = savedAgents.find((agent) => agent.id === nextAgentId) ?? null;
                  const nextRuntime = configuredRuntime(nextAgent);
                  if (nextRuntime) setRuntime(selectableRuntimeAlias(nextRuntime, runtimeOptions));
                }}
              >
                <SelectTrigger className="h-10 w-auto min-w-[230px] max-w-[320px] rounded-full border border-border bg-background px-3 text-left text-foreground shadow-sm transition-colors hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring/50">
                  <span className="flex min-w-0 items-center gap-2">
                    <span className="shrink-0 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
                      Agent
                    </span>
                    <span className="truncate text-sm font-medium">
                      {selectedAgent?.name ?? "新建托管智能体"}
                    </span>
                  </span>
                </SelectTrigger>
                <SelectContent className="w-[380px]">
                  <SelectItem value={NEW_AGENT_VALUE} className="py-3">
                    <span className="flex min-w-0 items-center gap-3">
                      <span className="flex size-8 shrink-0 items-center justify-center rounded-lg border border-border bg-background">
                        <Plus className="size-4" />
                      </span>
                      <span className="min-w-0">
                        <span className="block truncate text-sm font-medium">新建托管智能体</span>
                        <span className="block truncate text-xs text-muted-foreground">
                          根据这条提示词即时创建
                        </span>
                      </span>
                    </span>
                  </SelectItem>
                  {savedAgents.length > 0 && (
                    <>
                      <div className="px-2 py-1.5 text-[11px] uppercase tracking-wider text-muted-foreground">
                        已保存的智能体
                      </div>
                      {savedAgents.map((agent) => {
                        const agentRuntime = configuredRuntime(agent);
                        return (
                          <SelectItem key={agent.id} value={agent.id} className="py-3">
                            <span className="flex min-w-0 items-center gap-3">
                              <span className="flex size-8 shrink-0 items-center justify-center rounded-lg border border-border bg-background">
                                {agentRuntime ? (
                                  <BrandIcon id={runtimeBrandIconId(agentRuntime, null)} className="size-4" />
                                ) : (
                                  <Bot className="size-4" />
                                )}
                              </span>
                              <span className="min-w-0">
                                <span className="flex min-w-0 items-center gap-2">
                                  <span className="truncate text-sm font-medium">{agent.name}</span>
                                  {agent.model && (
                                    <span className="shrink-0 rounded border border-border bg-muted/40 px-1 py-px font-mono text-[11px] text-muted-foreground">
                                      {agent.model}
                                    </span>
                                  )}
                                </span>
                                <span className="block truncate text-xs text-muted-foreground">
                                  {agent.description || agentRuntime || "已保存的智能体"}
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

              <Select value={runtime} onValueChange={(value) => setRuntime((value ?? "") as AgentRuntimeId | "")}>
                <SelectTrigger className="h-10 w-auto min-w-[260px] max-w-[340px] rounded-full border border-border bg-background px-3 text-left text-foreground shadow-sm transition-colors hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring/50">
                  <span className="flex min-w-0 items-center gap-2">
                    <span className="shrink-0 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
                      Runtime
                    </span>
                    <span className="flex size-6 shrink-0 items-center justify-center rounded-md bg-muted">
                      <BrandIcon
                        id={runtimeBrandIconId(
                          selectedRuntime?.alias ?? "",
                          selectedRuntimeSpec,
                        )}
                        className="size-4"
                      />
                    </span>
                    <span className="truncate text-sm font-medium">
                      {selectedRuntime?.display_name ?? (harnesses.length > 0 ? "无已配置运行时" : "选择运行时")}
                    </span>
                  </span>
                </SelectTrigger>
                <SelectContent className="w-[340px]">
                  {runtimeOptions.length > 0 ? (
                    runtimeOptions.map((item) => (
                      <SelectItem key={item.alias} value={item.alias} className="py-3">
                        <span className="flex min-w-0 items-center gap-3">
                          <span className="flex size-8 shrink-0 items-center justify-center rounded-lg border border-border bg-background">
                            <BrandIcon id={runtimeBrandIconId(item.alias, item.api_spec)} className="size-4" />
                          </span>
                          <span className="min-w-0">
                            <span className="block truncate text-sm font-medium">{item.display_name}</span>
                            <span className="block truncate text-xs text-muted-foreground">
                              {runtimeSubtitle(item)}
                            </span>
                          </span>
                        </span>
                      </SelectItem>
                    ))
                  ) : (
                    <div className="px-3 py-3 text-sm text-muted-foreground">
                      暂无已配置的运行时
                    </div>
                  )}
                  <div className="border-t border-border px-3 py-2 text-xs text-muted-foreground">
                    前往 AI Gateway &gt; Agent Runtimes 配置更多运行时。
                  </div>
                </SelectContent>
              </Select>

              <span className="ml-auto hidden items-center gap-1.5 text-xs text-muted-foreground sm:flex">
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
                className="rounded-full"
                aria-label="Start session"
              >
                {starting ? (
                  <Loader2 className="size-4 animate-spin motion-reduce:animate-none" />
                ) : (
                  <ArrowUp className="size-4" />
                )}
                <span>{startButtonLabel(startPhase)}</span>
              </Button>
            </div>
            {selectedRuntimeSpec === "cursor" && (
              <div className="grid gap-3 border-t border-border bg-muted/40 px-4 py-3 sm:grid-cols-[1fr_140px]">
                <div className="grid gap-1">
                  <Label htmlFor="cursor-repository" className="text-xs text-muted-foreground">
                    仓库地址 <span className="text-destructive">*</span>
                  </Label>
                  <Input
                    id="cursor-repository"
                    value={repository}
                    onChange={(event) => setRepository(event.target.value)}
                    placeholder="https://github.com/org/repo"
                    className="h-8 border-border bg-background text-sm"
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
                    className="h-8 border-border bg-background text-sm"
                  />
                </div>
              </div>
            )}
            {error && (
              <div className="border-t border-destructive/20 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                {error}
              </div>
            )}
          </div>

          <div className="mt-6 w-full max-w-2xl">
            <div className="mb-2 flex items-center justify-between">
              <h2 className="text-sm font-semibold text-muted-foreground">最近会话</h2>
            </div>
            {recentSessions === null ? (
              <div className="grid gap-2 sm:grid-cols-2">
                {[0, 1, 2, 3].map((item) => (
                  <div key={item} className="rounded-lg border border-border bg-card/90 px-4 py-3 shadow-sm backdrop-blur">
                    <div className="h-4 w-2/3 animate-pulse rounded bg-muted motion-reduce:animate-none" />
                    <div className="mt-2 h-3 w-1/3 animate-pulse rounded bg-muted motion-reduce:animate-none" />
                  </div>
                ))}
              </div>
            ) : recentSessions.length === 0 ? (
              <EmptyState
                icon={MessageSquareText}
                title="还没有会话。"
                hint="在上方输入任务描述即可开始第一个会话。"
                className="backdrop-blur"
              />
            ) : (
              <div className="grid gap-2 sm:grid-cols-2">
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
  const busy = sessionIsBusy(session);
  const runtime = session.runtime ?? "";
  return (
    <button
      type="button"
      onClick={onOpen}
      className="rounded-lg border border-border bg-card/90 px-4 py-3 text-left shadow-sm backdrop-blur transition hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
    >
      <div className="flex items-start gap-2.5">
        <span className="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-md border border-border bg-background">
          {runtime ? (
            <BrandIcon id={runtimeBrandIconId(runtime, null)} className="size-3.5" />
          ) : (
            <MessageSquareText className="size-3.5 text-muted-foreground" />
          )}
        </span>
        <span className="min-w-0 flex-1">
          <span className="flex min-w-0 items-center gap-2">
            <span className="truncate text-sm font-medium">
              {session.title || "未命名会话"}
            </span>
            <StatusDot tone={busy ? "success" : "idle"} label={busy ? "运行中" : "空闲"} />
          </span>
          <span className="mt-0.5 flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
            {agent && <span className="truncate">{agent.name}</span>}
            <span className="shrink-0">{relativeTimeLabel(sessionTimestamp(session))}</span>
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
