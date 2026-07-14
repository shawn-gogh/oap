"use client";

import { Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useSearchParams, useRouter } from "next/navigation";
import {
  Activity,
  AlertTriangle,
  Bot,
  CheckCircle2,
  ChevronDown,
  ListChecks,
  Clipboard,
  ClipboardCheck,
  Cpu,
  ExternalLink,
  FileText,
  FolderOpen,
  KeyRound,
  Loader2,
  Square,
  Wrench,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ModelSelect } from "@/components/model-select";
import { MessageBlock, isTodoTool, toolLabel, toolDescriptor } from "@/components/message-block";
import { TodoList, parseTodoItems, todoProgress } from "@/components/todo-list";
import { Composer } from "@/components/composer";
import { ThemeToggle } from "@/components/theme-toggle";
import { Sidebar } from "@/components/sidebar";
import { InspectorPanel } from "@/components/inspector-panel";
import { WorkspacePanel } from "@/components/workspace-panel";
import { JumpToBottomButton } from "@/components/jump-to-bottom-button";
import { SessionLoadingSkeleton } from "@/components/session-loading-skeleton";
import { useStickToBottom } from "@/lib/hooks/use-stick-to-bottom";
import { getMessages, getSession, createSession, deleteSession, renameSession, subscribeRuntimeEvents, listModels, abortSession, interruptSession, listAgents, listApprovals, acceptApproval, rejectApproval, sendMessageWithRuntimeModel, listRuntimeEvents, listRuntimeHarnesses, listWorkspaceFiles, apiErrorMessage } from "@/lib/api";
import type { PendingApproval, RuntimeAgentEvent } from "@/lib/api";
import { ApprovalDock } from "@/components/approval-dock";
import type { Agent, AgentRuntimeId, HarnessMessage, RuntimeHarness } from "@/lib/types";
import { resolveApiSpec } from "@/lib/types";
import { defaultModelForRuntime, runtimeSupportsModelDiscovery } from "@/lib/model-options";
import type { Frame } from "@/components/inspector-panel";
import {
  isRuntimeAssistantTextEvent,
  isRuntimeThinkingEvent,
  isRuntimeToolEvent,
  isRuntimeTurnStartEvent,
  makeQueuedPromptMessage,
  mergeRuntimeEventList,
  normalizedRuntimeEventType,
  runtimeErrorMessage,
  runtimeEventsToMessages,
  runtimeSessionStatusFromMetadata,
  runtimeStatusFromEvents,
} from "@/lib/runtime-events";
import type { QueuedPrompt } from "@/lib/runtime-events";
import {
  workspaceAgentTaskPrompt,
  workspaceConversationReference,
} from "@/lib/workspace-browser";

import SessionsPage from "../sessions/page";

const FALLBACK_MODELS = [
  "anthropic/claude-opus-4-8",
  "anthropic/claude-sonnet-4-6",
  "anthropic/claude-sonnet-5",
  "anthropic/claude-haiku-4-5",
];

const BUILTIN_AGENTS: Record<string, string> = {
  "claude-code": "Claude Code",
  cc: "Claude Code",
  "github-copilot": "GitHub Copilot",
  codex: "Codex",
};

function agentPrompt(agent: Agent | null): string {
  if (!agent) return "";
  return String(agent.prompt ?? agent.system ?? agent.system_prompt ?? "").trim();
}

function shortPrompt(prompt: string): string {
  const compact = prompt.replace(/\s+/g, " ").trim();
  return compact.length > 220 ? compact.slice(0, 220).trimEnd() + "…" : compact;
}

function runtimeLabel(runtime?: string): string {
  if (runtime === "claude_managed_agents") return "自托管开放 Harness";
  if (runtime === "cursor") return "Cursor";
  if (runtime === "gemini_antigravity") return "Gemini Antigravity";
  return BUILTIN_AGENTS[runtime ?? ""] ?? runtime ?? "Claude Code";
}

function providerSessionUrl(runtime?: string, providerSessionId?: string, providerUrl?: string): string | null {
  if (providerUrl) return providerUrl;
  if (runtime === "claude_managed_agents" && providerSessionId) {
    return `https://platform.claude.com/workspaces/default/sessions/${encodeURIComponent(providerSessionId)}`;
  }
  return null;
}


function ChatInner() {
  const sp = useSearchParams();
  const sid = sp.get("id");
  const autostartPrompt = sp.get("autostart") === "1" ? sp.get("prompt")?.trim() : "";
  const [messages, setMessages] = useState<HarnessMessage[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [models, setModels] = useState<string[]>(FALLBACK_MODELS);
  const [model, setModel] = useState(FALLBACK_MODELS[0]);
  const [sessionStatus, setSessionStatus] = useState<"idle" | "busy">("idle");
  const [approvals, setApprovals] = useState<PendingApproval[]>([]);
  const [approvalBusy, setApprovalBusy] = useState(false);
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [workspacePanelOpen, setWorkspacePanelOpen] = useState(false);
  const [workspaceBucket, setWorkspaceBucket] = useState<string | undefined>();
  const [composerDraft, setComposerDraft] = useState("");
  const [composerFocusVersion, setComposerFocusVersion] = useState(0);
  const [promptOpen, setPromptOpen] = useState(false);
  const [promptCopied, setPromptCopied] = useState(false);
  const eventBufferRef = useRef<Frame[]>([]);
  const [runtimeEvents, setRuntimeEvents] = useState<RuntimeAgentEvent[]>([]);
  const [runtimeEventsLoaded, setRuntimeEventsLoaded] = useState(false);
  const [queuedPrompts, setQueuedPrompts] = useState<QueuedPrompt[]>([]);
  const [interruptingQueuedPromptId, setInterruptingQueuedPromptId] = useState<string | null>(null);
  const [runtimeStreamVersion, setRuntimeStreamVersion] = useState(0);
  const [sessionHarness, setSessionHarness] = useState<string>("claude-code");
  const [sessionRuntime, setSessionRuntime] = useState<AgentRuntimeId | undefined>();
  const [harnesses, setHarnesses] = useState<RuntimeHarness[]>([]);
  const [sessionLoaded, setSessionLoaded] = useState(false);
  const [providerSessionId, setProviderSessionId] = useState<string | undefined>();
  const [providerUrl, setProviderUrl] = useState<string | undefined>();
  const [sessionTitle, setSessionTitle] = useState<string>("");
  const [savedAgents, setSavedAgents] = useState<Agent[]>([]);
  const [switchingAgent, setSwitchingAgent] = useState(false);
  const [editingTitle, setEditingTitle] = useState<string | null>(null);
  const [renamingTitle, setRenamingTitle] = useState(false);
  // null = follow the default (open until the conversation starts).
  const [infoOpenManual, setInfoOpenManual] = useState<boolean | null>(null);
  const { scrollRef, contentRef, onScroll, isPinned, jumpToBottom } = useStickToBottom(sessionStatus === "busy");
  const activeSessionRef = useRef<string | null>(null);
  const autostartedRef = useRef<string | null>(null);
  // Tombstones for approvals the user already decided: the DB write can lag
  // the next poll, and without this the dock flashes the stale approval back.
  const decidedApprovalsRef = useRef<Map<string, number>>(new Map());
  const approvalsRef = useRef<PendingApproval[]>([]);

  const applyApprovals = useCallback((items: PendingApproval[], sessionId: string) => {
    const now = Date.now();
    for (const [id, decidedAt] of decidedApprovalsRef.current) {
      if (now - decidedAt > 60_000) decidedApprovalsRef.current.delete(id);
    }
    const next = items.filter(
      (approval) =>
        approval.sessionId === sessionId && !decidedApprovalsRef.current.has(approval.id),
    );
    setApprovals((prev) => {
      const unchanged =
        prev.length === next.length && prev.every((approval, i) => approval.id === next[i].id);
      return unchanged ? prev : next;
    });
  }, []);

  useEffect(() => {
    approvalsRef.current = approvals;
  }, [approvals]);

  const refetch = useCallback(async () => {
    if (!sid) return;
    try {
      const sessionId = sid;
      const list = await getMessages(sid);
      if (activeSessionRef.current !== sessionId) return;
      setMessages(list);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [sid]);

  const router = useRouter();

  const activeAgent = useMemo(() => {
    const target = sessionHarness || sessionTitle;
    return (
      savedAgents.find((a) => a.id === target) ??
      savedAgents.find((a) => a.name === target) ??
      savedAgents.find((a) => sessionTitle && a.name === sessionTitle) ??
      null
    );
  }, [savedAgents, sessionHarness, sessionTitle]);

  const activePrompt = agentPrompt(activeAgent);
  const activeAgentName =
    activeAgent?.name || sessionTitle || BUILTIN_AGENTS[sessionHarness] || sessionHarness;
  const baseRuntime =
    sessionRuntime
      ? runtimeLabel(sessionRuntime)
      : String(activeAgent?.harness ?? activeAgent?.base_agent ?? sessionHarness ?? "claude-code");
  const providerLink = providerSessionUrl(sessionRuntime, providerSessionId, providerUrl);
  const skills = Array.isArray(activeAgent?.skills) ? activeAgent.skills : [];
  const vaultKeys = Array.isArray(activeAgent?.vault_keys) ? activeAgent.vault_keys : [];
  const runtimeMessages = useMemo(() => {
    if (!sid || !sessionRuntime) return null;
    // Keep this null (renders as "Loading…") until the initial events fetch
    // settles — it can take several seconds against a cold runtime provider,
    // and returning [] early made a session that hasn't loaded yet look like
    // one with no message history.
    if (!runtimeEventsLoaded) return null;
    return runtimeEventsToMessages(sid, runtimeEvents, sessionStatus);
  }, [runtimeEvents, runtimeEventsLoaded, sessionRuntime, sessionStatus, sid]);
  const displayMessages = useMemo(() => {
    const baseMessages = sessionRuntime ? runtimeMessages : messages;
    if (!sid || !sessionRuntime || queuedPrompts.length === 0) return baseMessages;
    return [
      ...(baseMessages ?? []),
      ...queuedPrompts.map((prompt) => makeQueuedPromptMessage(sid, prompt)),
    ];
  }, [messages, queuedPrompts, runtimeMessages, sessionRuntime, sid]);
  // Latest todo/task-list state across the whole conversation, pinned above
  // the transcript so it never drowns among tool calls and answers.
  const pinnedTodos = useMemo(() => {
    if (!displayMessages) return null;
    for (let i = displayMessages.length - 1; i >= 0; i--) {
      const parts = displayMessages[i].parts;
      for (let j = parts.length - 1; j >= 0; j--) {
        const part = parts[j];
        if (part.type !== "tool" || !isTodoTool(part.tool)) continue;
        const items = parseTodoItems(part.state?.input, part.state?.output);
        if (items && items.length > 0) return items;
      }
    }
    return null;
  }, [displayMessages]);
  // Live activity readout while the agent runs: which tool is executing now,
  // and how long the turn has been going.
  const [turnStartedAt, setTurnStartedAt] = useState<number | null>(null);
  const [nowTick, setNowTick] = useState(0);
  useEffect(() => {
    if (sessionStatus === "busy") {
      setTurnStartedAt((current) => current ?? Date.now());
      const timer = setInterval(() => setNowTick((t) => t + 1), 1000);
      return () => clearInterval(timer);
    }
    setTurnStartedAt(null);
  }, [sessionStatus]);
  const activeActivity = useMemo(() => {
    if (sessionStatus !== "busy" || !displayMessages) return null;
    for (let i = displayMessages.length - 1; i >= 0; i--) {
      const message = displayMessages[i];
      if (message.info.role !== "assistant") continue;
      for (let j = message.parts.length - 1; j >= 0; j--) {
        const part = message.parts[j];
        if (part.type !== "tool") continue;
        const partStatus = part.state?.status;
        if (partStatus === "running" || partStatus === "pending") {
          const desc = toolDescriptor(part.tool, part.state?.input);
          return { label: toolLabel(part.tool), desc };
        }
      }
      break;
    }
    return null;
    // nowTick keeps the elapsed label fresh even without new events.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [displayMessages, sessionStatus, nowTick]);
  const turnElapsedSeconds = turnStartedAt ? Math.max(0, Math.floor((Date.now() - turnStartedAt) / 1000)) : 0;

  // Workspace file paths for @-mentions in the composer.
  const [mentionFiles, setMentionFiles] = useState<string[]>([]);
  useEffect(() => {
    if (!sid || !workspaceBucket) {
      setMentionFiles([]);
      return;
    }
    let cancelled = false;
    listWorkspaceFiles(sid)
      .then((files) => {
        if (!cancelled) setMentionFiles(files.map((file) => file.path));
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [sid, workspaceBucket, sessionStatus]);

  // Last user prompt, used by the retry control after a failed turn.
  const lastUserPrompt = useMemo(() => {
    if (!displayMessages) return "";
    for (let i = displayMessages.length - 1; i >= 0; i--) {
      const message = displayMessages[i];
      if (message.info.role !== "user") continue;
      if ((message.info as Record<string, unknown>).status === "queued") continue;
      const text = message.parts
        .filter((part): part is Extract<typeof part, { type: "text" }> => part.type === "text")
        .map((part) => part.text)
        .join("\n")
        .trim();
      if (text) return text;
    }
    return "";
  }, [displayMessages]);
  const lastTurnFailed = useMemo(() => {
    if (sessionStatus === "busy" || !displayMessages) return false;
    const last = displayMessages.at(-1);
    if (!last || last.info.role !== "assistant") return false;
    return last.parts.some(
      (part) => part.type === "text" && typeof part.text === "string" && part.text.startsWith("Error:"),
    );
  }, [displayMessages, sessionStatus]);
  const sessionContentLoading = !displayMessages && !error;
  const hasStarted = Boolean(displayMessages && displayMessages.length > 0);
  const modelOptions = useMemo(() => {
    if (sessionRuntime) return models;
    return models.length > 0 ? models : FALLBACK_MODELS;
  }, [models, sessionRuntime]);

  const onCopyPrompt = useCallback(() => {
    if (!activePrompt) return;
    navigator.clipboard?.writeText(activePrompt).then(() => {
      setPromptCopied(true);
      window.setTimeout(() => setPromptCopied(false), 1400);
    }).catch(() => {});
  }, [activePrompt]);

  useEffect(() => {
    let cancelled = false;
    const runtimeDefaultModel = defaultModelForRuntime(sessionRuntime);
    if (sessionRuntime && !runtimeSupportsModelDiscovery(sessionRuntime)) {
      setModels(runtimeDefaultModel ? [runtimeDefaultModel] : []);
      setModel(runtimeDefaultModel);
      return () => {
        cancelled = true;
      };
    }
    const initialModels = sessionRuntime ? [] : FALLBACK_MODELS;
    setModels(initialModels);
    setModel((prev) => (initialModels.includes(prev) ? prev : initialModels[0] ?? ""));
    listModels(sessionRuntime).then((fetched) => {
      if (cancelled) return;
      const nextModels = sessionRuntime ? fetched : fetched.length > 0 ? fetched : FALLBACK_MODELS;
      setModels(nextModels);
      setModel((prev) => (nextModels.includes(prev) ? prev : nextModels[0] ?? ""));
    }).catch((err) => {
      if (cancelled) return;
      if (sessionRuntime) {
        setModels([]);
        setModel("");
        setError(err instanceof Error ? err.message : String(err));
      } else {
        setModels(FALLBACK_MODELS);
        setModel((prev) => (FALLBACK_MODELS.includes(prev) ? prev : FALLBACK_MODELS[0]));
      }
    });
    return () => {
      cancelled = true;
    };
  }, [sessionRuntime]);

  // When a runtime session has an agent with a configured model, prefer that model.
  useEffect(() => {
    if (!sessionRuntime) return;
    const agent = savedAgents.find((a) => a.id === sessionHarness);
    if (!agent?.model) return;
    setModel((prev) => (models.includes(agent.model!) ? agent.model! : prev));
  }, [savedAgents, sessionHarness, sessionRuntime, models]);

  // Fetch session metadata to get the locked agent
  useEffect(() => {
    if (!sid) return;
    activeSessionRef.current = sid;
    eventBufferRef.current = [];
    setMessages(null);
    setRuntimeEvents([]);
    setRuntimeEventsLoaded(false);
    setQueuedPrompts([]);
    setInterruptingQueuedPromptId(null);
    setError(null);
    setSessionRuntime(undefined);
    setSessionLoaded(false);
    setSessionStatus("idle");
    setSessionHarness("claude-code");
    setProviderSessionId(undefined);
    setProviderUrl(undefined);
    setSessionTitle("");
    setEditingTitle(null);
    setInfoOpenManual(null);
    decidedApprovalsRef.current.clear();
    setWorkspaceBucket(undefined);
    const resumed = sp.get("resumed") === "true";
    getSession(sid).then(s => {
      if (activeSessionRef.current !== sid) return;
      const a = s.agent_id ?? s.agent ?? s.harness;
      if (a) setSessionHarness(a);
      setSessionRuntime(s.runtime);
      const defaultStatus = s.runtime ? runtimeSessionStatusFromMetadata(s.status, s.provider_run_id) : s.status === "running" ? "busy" : "idle";
      setSessionStatus(resumed ? "busy" : defaultStatus);
      setProviderSessionId(s.provider_session_id);
      setProviderUrl(s.provider_url);
      setWorkspaceBucket(s.workspace_bucket);
      if (s.title) setSessionTitle(s.title);
    }).catch(() => {}).finally(() => {
      if (activeSessionRef.current === sid) setSessionLoaded(true);
    });
    // `sp` is read once for the transient ?resumed flag; depending on it
    // would re-reset the whole session when router.replace strips the flag.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sid]);

  useEffect(() => {
    if (sp.get("resumed") === "true" && sid) {
      router.replace(`/chat/?id=${encodeURIComponent(sid)}`, { scroll: false });
    }
  }, [sp, sid, router]);

  // Fetch saved agents for dropdown
  useEffect(() => {
    listAgents().then(setSavedAgents).catch(() => {});
    listRuntimeHarnesses().then(setHarnesses).catch(() => {});
  }, []);

  const onHarnessChange = useCallback(async (next: string) => {
    if (!sid || next === sessionHarness) return;
    setSwitchingAgent(true);
    setError(null);
    try {
      if (!hasStarted) await deleteSession(sid).catch(() => {});
      const options = next.startsWith("agent_") && sessionRuntime ? { runtime: sessionRuntime } : undefined;
      const s = await createSession(undefined, next, options);
      router.replace(`/chat/?id=${encodeURIComponent(s.id)}`);
    } catch (err) {
      setError(apiErrorMessage(err, "切换智能体失败"));
      setSwitchingAgent(false);
    }
  }, [hasStarted, sid, sessionHarness, sessionRuntime, router]);

  const mergeRuntimeEventsAndStatus = useCallback((events: RuntimeAgentEvent | RuntimeAgentEvent[]) => {
    setRuntimeEvents((prev) => {
      const next = mergeRuntimeEventList(prev, events);
      const eventStatus = runtimeStatusFromEvents(next);
      if (eventStatus) setSessionStatus(eventStatus);
      return next;
    });
  }, []);

  const appendRuntimeEvent = useCallback((ev: RuntimeAgentEvent) => {
    eventBufferRef.current = [
      ...eventBufferRef.current.slice(-499),
      { ts: Date.now(), ev: ev as Frame["ev"] },
    ];

    const type = normalizedRuntimeEventType(ev);

    // Gateway-local approval events pushed through the SSE stream: update the
    // dock immediately and skip the message-stream merge (approvals are not
    // transcript content).
    if (type === "approval.asked" || type === "approval.replied") {
      const raw = ev.approval as
        | { id?: string; kind?: PendingApproval["kind"]; title?: string; status?: string; args_json?: string | null; created_at?: number; session_id?: string | null }
        | undefined;
      if (!raw?.id) return;
      if (type === "approval.replied") {
        decidedApprovalsRef.current.set(raw.id, Date.now());
        setApprovals((prev) => prev.filter((approval) => approval.id !== raw.id));
        return;
      }
      if (decidedApprovalsRef.current.has(raw.id)) return;
      let parsedArgs: Record<string, unknown> = {};
      try {
        parsedArgs = raw.args_json ? (JSON.parse(raw.args_json) as Record<string, unknown>) : {};
      } catch {
        parsedArgs = {};
      }
      const next: PendingApproval = {
        id: raw.id,
        kind: raw.kind ?? "approval",
        tool: raw.title ?? "approval",
        arguments: parsedArgs,
        createdAt: raw.created_at ?? Date.now(),
        sessionId: raw.session_id ?? null,
      };
      setApprovals((prev) =>
        prev.some((approval) => approval.id === next.id) ? prev : [...prev, next],
      );
      return;
    }
    if (isRuntimeTurnStartEvent(type)) {
      setSessionStatus("busy");
    } else if (type === "session.status_idle") {
      setSessionStatus("idle");
    } else if (type === "session.status") {
      const status = ev.status;
      const statusType =
        typeof status === "string"
          ? status
          : status && typeof status === "object"
            ? (status as { type?: unknown }).type
            : undefined;
      if (statusType === "busy" || statusType === "running") setSessionStatus("busy");
      if (statusType === "idle") setSessionStatus("idle");
    } else if (type === "session.error") {
      setError(`Error: ${runtimeErrorMessage(ev)}`);
      setSessionStatus("idle");
    } else if (
      type === "user.message" ||
      isRuntimeAssistantTextEvent(type) ||
      isRuntimeThinkingEvent(type) ||
      isRuntimeToolEvent(type)
    ) {
      setSessionStatus((current) => (current === "busy" ? current : "busy"));
    }

    mergeRuntimeEventsAndStatus(ev);
  }, [mergeRuntimeEventsAndStatus]);

  const beginRuntimeTurn = useCallback((text?: string) => {
    if (!sessionRuntime || !sid) return;
    const trimmed = text?.trim();
    if (trimmed) {
      appendRuntimeEvent({
        id: `${sid}_local_user_${Date.now().toString(36)}`,
        type: "user.message",
        local: true,
        content: [{ type: "text", text: trimmed }],
      });
    }
    setSessionStatus("busy");
  }, [appendRuntimeEvent, sessionRuntime, sid]);

  const queueRuntimePrompt = useCallback((text: string) => {
    if (!sid) return;
    setQueuedPrompts((current) => [
      ...current,
      {
        id: `${sid}_queued_${Date.now().toString(36)}_${current.length}`,
        text,
      },
    ]);
  }, [sid]);

  const sendOrQueueRuntimePrompt = useCallback(async (text: string) => {
    if (!sid) return;
    if (!model.trim()) {
      setError("该会话没有可用的运行时模型。");
      return;
    }
    if (sessionStatus === "busy") {
      // Interrupt-and-steer (Codex-style): sending mid-run redirects the agent
      // immediately instead of silently queueing behind the current turn.
      try {
        await interruptSession(sid);
      } catch {
        // Interrupt failing (e.g. run just finished) shouldn't lose the
        // message — fall back to queueing it.
        queueRuntimePrompt(text);
        return;
      }
      if (activeSessionRef.current !== sid) return;
      beginRuntimeTurn(text);
      try {
        await sendMessageWithRuntimeModel({
          sessionId: sid,
          text,
          model,
          runtime: sessionRuntime,
          apiSpec: resolveApiSpec(sessionRuntime ?? "", harnesses),
        });
        if (activeSessionRef.current === sid) {
          setRuntimeStreamVersion((version) => version + 1);
        }
      } catch (err) {
        if (activeSessionRef.current !== sid) return;
        setError(err instanceof Error ? err.message : String(err));
        setSessionStatus("idle");
        throw err;
      }
      return;
    }
    try {
      await sendMessageWithRuntimeModel({
        sessionId: sid,
        text,
        model,
        runtime: sessionRuntime,
        apiSpec: resolveApiSpec(sessionRuntime ?? "", harnesses),
      });
    } catch (err) {
      if (activeSessionRef.current !== sid) return;
      setError(err instanceof Error ? err.message : String(err));
      setSessionStatus("idle");
      throw err;
    }
  }, [beginRuntimeTurn, model, queueRuntimePrompt, sessionRuntime, sessionStatus, sid, harnesses]);

  const retryLastPrompt = useCallback(() => {
    if (!lastUserPrompt || sessionStatus === "busy") return;
    setError(null);
    beginRuntimeTurn(lastUserPrompt);
    void sendOrQueueRuntimePrompt(lastUserPrompt).catch(() => {});
  }, [lastUserPrompt, sessionStatus, beginRuntimeTurn, sendOrQueueRuntimePrompt]);

  const insertWorkspacePaths = useCallback((paths: string[]) => {
    const reference = workspaceConversationReference(paths);
    setComposerDraft((current) =>
      current.trim() ? `${current.trimEnd()}\n\n${reference}` : reference,
    );
    setComposerFocusVersion((version) => version + 1);
  }, []);

  const processWorkspacePaths = useCallback(async (paths: string[]) => {
    if (!sid) return;
    const prompt = workspaceAgentTaskPrompt(paths);
    if (sessionRuntime) {
      if (sessionStatus !== "busy") beginRuntimeTurn(prompt);
      await sendOrQueueRuntimePrompt(prompt);
      return;
    }
    await sendMessageWithRuntimeModel({ sessionId: sid, text: prompt, model });
    await refetch();
  }, [beginRuntimeTurn, model, refetch, sendOrQueueRuntimePrompt, sessionRuntime, sessionStatus, sid]);

  const cancelQueuedPrompt = useCallback((id: string) => {
    setQueuedPrompts((current) => current.filter((prompt) => prompt.id !== id));
  }, []);

  const interruptAndSendQueuedPrompt = useCallback(async (id: string) => {
    if (!sid || !sessionRuntime || interruptingQueuedPromptId) return;
    if (!model.trim()) {
      setError("该会话没有可用的运行时模型。");
      return;
    }
    const prompt = queuedPrompts.find((item) => item.id === id);
    if (!prompt) return;

    setError(null);
    setInterruptingQueuedPromptId(id);
    try {
      if (sessionStatus === "busy") {
        await interruptSession(sid);
      }
      if (activeSessionRef.current !== sid) return;
      setQueuedPrompts((current) => current.filter((item) => item.id !== id));
      beginRuntimeTurn(prompt.text);
      await sendMessageWithRuntimeModel({
        sessionId: sid,
        text: prompt.text,
        model,
        runtime: sessionRuntime,
        apiSpec: resolveApiSpec(sessionRuntime ?? "", harnesses),
      });
      if (activeSessionRef.current === sid) {
        setRuntimeStreamVersion((version) => version + 1);
      }
    } catch (err) {
      if (activeSessionRef.current !== sid) return;
      setError(err instanceof Error ? err.message : String(err));
      setSessionStatus("idle");
    } finally {
      if (activeSessionRef.current === sid) {
        setInterruptingQueuedPromptId(null);
      }
    }
  }, [
    beginRuntimeTurn,
    harnesses,
    interruptingQueuedPromptId,
    model,
    queuedPrompts,
    sessionRuntime,
    sessionStatus,
    sid,
  ]);

  useEffect(() => {
    if (!sid || !sessionLoaded) return;
    let unsub: (() => void) | undefined;
    let cancelled = false;
    setApprovals([]);
    if (sessionRuntime) {
      listRuntimeEvents(sid)
        .then((events) => {
          if (activeSessionRef.current !== sid) return;
          eventBufferRef.current = events.slice(-500).map((ev) => ({ ts: Date.now(), ev: ev as Frame["ev"] }));
          mergeRuntimeEventsAndStatus(events);
          if (cancelled) return;
          unsub = subscribeRuntimeEvents({
            sessionId: sid,
            onEvent: (ev) => {
              if (activeSessionRef.current === sid) appendRuntimeEvent(ev);
            },
            onError: (err) => {
              if (activeSessionRef.current === sid) {
                setError(err instanceof Error ? err.message : String(err));
              }
            },
          });
        })
        .catch((err) => {
          if (activeSessionRef.current !== sid) return;
          setError(err instanceof Error ? err.message : String(err));
        })
        .finally(() => {
          if (activeSessionRef.current === sid) setRuntimeEventsLoaded(true);
        });
    } else {
      void refetch();
    }
    if (autostartPrompt && autostartedRef.current !== sid) {
      if (sessionRuntime && !model.trim()) return;
      autostartedRef.current = sid;
      beginRuntimeTurn(autostartPrompt);
      void sendMessageWithRuntimeModel({
        sessionId: sid,
        text: autostartPrompt,
        model,
        runtime: sessionRuntime,
        apiSpec: resolveApiSpec(sessionRuntime ?? "", harnesses),
      })
        .then(() => {
          if (activeSessionRef.current !== sid) return;
          if (!sessionRuntime) return refetch();
        })
        .then(() => router.replace(`/chat/?id=${encodeURIComponent(sid)}`))
        .catch((err) => {
          if (activeSessionRef.current !== sid) return;
          setError(err instanceof Error ? err.message : String(err));
          setSessionStatus("idle");
        });
    }
    listApprovals(sid)
      .then((items) => {
        if (activeSessionRef.current !== sid) return;
        applyApprovals(items, sid);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, [sid, sessionLoaded, refetch, appendRuntimeEvent, applyApprovals, mergeRuntimeEventsAndStatus, autostartPrompt, beginRuntimeTurn, model, router, sessionRuntime, runtimeStreamVersion, harnesses]);

  useEffect(() => {
    if (!sid || !sessionRuntime) return;
    let active = true;
    let timer: number | undefined;
    const replay = () => {
      // Re-fetching events replays (and re-persists) the session's full
      // history on the backend, which is only worth paying for while a turn
      // is actually producing new events — the live SSE subscription already
      // delivers those in real time otherwise. Approvals still need polling
      // regardless, since a background turn can raise one at any time.
      if (sessionStatus === "busy") {
        listRuntimeEvents(sid)
          .then((events) => {
            if (!active) return;
            if (activeSessionRef.current !== sid) return;
            mergeRuntimeEventsAndStatus(events);
          })
          .catch((err) => {
            if (active && activeSessionRef.current === sid) {
              setError(err instanceof Error ? err.message : String(err));
            }
          });
      }

      listApprovals(sid)
        .then((items) => {
          if (!active) return;
          if (activeSessionRef.current !== sid) return;
          applyApprovals(items, sid);
        })
        .catch(() => {});
    };
    const schedule = () => {
      // Poll fast only while something can actually change quickly: a busy
      // turn or a visible approval awaiting a decision. Otherwise back off,
      // so an idle chat tab isn't hammering the gateway every 2s.
      const delay =
        sessionStatus === "busy" || approvalsRef.current.length > 0 ? 2000 : 15000;
      timer = window.setTimeout(() => {
        replay();
        if (active) schedule();
      }, delay);
    };
    replay();
    schedule();
    return () => {
      active = false;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [applyApprovals, mergeRuntimeEventsAndStatus, sessionStatus, sid, sessionRuntime]);

  const onApprovalAccept = useCallback(async (id: string, args: Record<string, unknown>) => {
    setApprovalBusy(true);
    try {
      await acceptApproval(id, args);
      decidedApprovalsRef.current.set(id, Date.now());
      setApprovals((prev) => prev.filter((a) => a.id !== id));
      setSessionStatus("busy");
      setRuntimeStreamVersion((version) => version + 1);
    } catch (e) {
      setError(apiErrorMessage(e, "审批操作失败"));
    } finally {
      setApprovalBusy(false);
    }
  }, []);

  const onApprovalAcceptAlways = useCallback(async (id: string, args: Record<string, unknown>) => {
    setApprovalBusy(true);
    try {
      await acceptApproval(id, args, "session");
      decidedApprovalsRef.current.set(id, Date.now());
      setApprovals((prev) => prev.filter((approval) => approval.id !== id));
      setSessionStatus("busy");
      setRuntimeStreamVersion((version) => version + 1);
    } catch (error) {
      setError(apiErrorMessage(error, "审批操作失败"));
    } finally {
      setApprovalBusy(false);
    }
  }, []);

  const onApprovalReject = useCallback(async (id: string, feedback: string) => {
    setApprovalBusy(true);
    try {
      await rejectApproval(id, feedback);
      decidedApprovalsRef.current.set(id, Date.now());
      setApprovals((prev) => prev.filter((a) => a.id !== id));
      setSessionStatus("busy");
      setRuntimeStreamVersion((version) => version + 1);
    } catch (e) {
      setError(apiErrorMessage(e, "审批操作失败"));
    } finally {
      setApprovalBusy(false);
    }
  }, []);

  const commitRename = useCallback(async () => {
    if (!sid || editingTitle === null || renamingTitle) return;
    const next = editingTitle.trim();
    if (!next || next === sessionTitle) {
      setEditingTitle(null);
      return;
    }
    setRenamingTitle(true);
    try {
      await renameSession(sid, next);
      setSessionTitle(next);
      setEditingTitle(null);
    } catch (err) {
      setError(apiErrorMessage(err, "重命名会话失败"));
    } finally {
      setRenamingTitle(false);
    }
  }, [editingTitle, renamingTitle, sessionTitle, sid]);

  if (!sid) {
    return <SessionsPage />;
  }

  const shortSid = sid.length > 12 ? sid.slice(0, 12) + "…" : sid;
  const infoOpen = infoOpenManual ?? !hasStarted;

  return (
    <div className="flex h-screen bg-background text-foreground">
      <Sidebar activeId={sid} />

      <div className="flex-1 flex flex-col min-w-0">
        <header className="h-12 border-b border-border flex items-center justify-between px-4 shrink-0">
          <div className="flex min-w-0 items-center gap-2">
            {editingTitle !== null ? (
              <input
                autoFocus
                value={editingTitle}
                disabled={renamingTitle}
                onChange={(event) => setEditingTitle(event.target.value)}
                onBlur={() => void commitRename()}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    void commitRename();
                  } else if (event.key === "Escape") {
                    setEditingTitle(null);
                  }
                }}
                aria-label="会话标题"
                className="h-7 w-[240px] rounded-md border border-border bg-background px-2 text-sm font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
              />
            ) : (
              <button
                type="button"
                onClick={() => setEditingTitle(sessionTitle || "")}
                title="点击重命名会话"
                className="max-w-[280px] truncate rounded-md px-1 py-0.5 text-left text-sm font-medium hover:bg-muted"
              >
                {sessionTitle || "未命名会话"}
              </button>
            )}
            <span className="shrink truncate text-xs font-mono text-muted-foreground">{shortSid}</span>
            {sessionStatus === "busy" ? (
              <button
                onClick={() => sid && abortSession(sid).catch(() => {})}
                className="flex shrink-0 items-center gap-1 whitespace-nowrap text-[11px] text-amber-600 dark:text-amber-400 font-mono hover:text-red-600 dark:hover:text-red-400 transition-colors group"
                title="中止智能体"
                aria-label="Agent busy — click to abort"
              >
                <Loader2 className="w-3 h-3 shrink-0 animate-spin motion-reduce:animate-none group-hover:hidden" />
                <Square className="w-3 h-3 shrink-0 hidden group-hover:block fill-current" />
                <span className="group-hover:hidden">运行中</span>
                <span className="hidden group-hover:inline">中止</span>
              </button>
            ) : (
              <span className="flex shrink-0 items-center gap-1 whitespace-nowrap text-[11px] text-emerald-600 dark:text-emerald-400 font-mono">
                <span className="w-1.5 h-1.5 shrink-0 rounded-full bg-emerald-500 inline-block" />
                空闲
              </span>
            )}
          </div>
          <div className="flex items-center gap-3">
            <div className="flex items-center gap-1.5">
              <span className="text-[11px] text-muted-foreground">agent</span>
              <Select
                value={sessionHarness}
                onValueChange={(v) => v && onHarnessChange(v)}
                disabled={switchingAgent || sessionStatus === "busy"}
              >
                <SelectTrigger className="h-8 text-xs w-[190px]">
                  <SelectValue placeholder={activeAgentName} />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="claude-code" className="text-xs font-mono">claude code</SelectItem>
                  <SelectItem value="github-copilot" className="text-xs font-mono">github copilot</SelectItem>
                  {savedAgents.length > 0 && (
                    <>
                      <div className="px-2 py-1.5 text-[11px] text-muted-foreground uppercase tracking-wider border-t mt-1 pt-2">Saved agents</div>
                      {savedAgents.map(a => (
                        <SelectItem key={a.id} value={a.id} className="text-xs font-mono">{a.name}</SelectItem>
                      ))}
                    </>
                  )}
                  <div className="px-2 py-2 text-[11px] text-muted-foreground border-t mt-1">
                    切换智能体会打开一个新会话。
                  </div>
                </SelectContent>
              </Select>
              {switchingAgent && <Loader2 className="size-3.5 animate-spin motion-reduce:animate-none text-muted-foreground" />}
            </div>
            <div className="flex items-center gap-1.5">
              <span className="text-[11px] text-muted-foreground">model</span>
              <ModelSelect value={model} models={modelOptions} onValueChange={setModel} />
            </div>
            {providerLink && (
              <Button
                variant="outline"
                size="sm"
                className="h-8"
                render={
                  <a href={providerLink} target="_blank" rel="noreferrer">
                    <ExternalLink className="size-3.5" />
                    打开提供方会话
                  </a>
                }
              />
            )}
            {workspaceBucket && (
              <Button
                variant={workspacePanelOpen ? "default" : "outline"}
                size="sm"
                onClick={() => setWorkspacePanelOpen((v) => !v)}
                className="h-8"
              >
                <FolderOpen className="size-3.5" />
                工作区
              </Button>
            )}
            <Button
              variant={inspectorOpen ? "default" : "outline"}
              size="sm"
              onClick={() => setInspectorOpen((v) => !v)}
              className="h-8"
            >
              <Activity className="size-3.5" />
              检查器
            </Button>
            <ThemeToggle />
          </div>
        </header>

        <div
          ref={scrollRef}
          onScroll={onScroll}
          className="relative flex-1 overflow-y-auto"
        >
          {pinnedTodos && <PinnedTodoBar items={pinnedTodos} />}
          <div ref={contentRef} className="mx-auto flex w-full max-w-5xl flex-col gap-6 px-6 py-8">
            {sessionContentLoading && <SessionLoadingSkeleton />}
            {error && (
              <Card className="border-destructive p-4">
                <p className="text-sm text-destructive">{error}</p>
              </Card>
            )}
            <button
              type="button"
              onClick={() => setInfoOpenManual(!infoOpen)}
              aria-expanded={infoOpen}
              className="flex w-full items-center gap-2.5 rounded-lg border border-border/80 bg-card/80 px-3 py-2 text-left transition hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
            >
              <span className="flex size-7 shrink-0 items-center justify-center rounded-md border border-border bg-background">
                <Bot className="size-3.5" />
              </span>
              <span className="min-w-0 flex-1">
                <span className="flex min-w-0 flex-wrap items-center gap-2">
                  <span className="truncate text-sm font-semibold">{activeAgentName}</span>
                  <span className="shrink-0 rounded border border-border bg-muted/40 px-1.5 py-px font-mono text-[11px] text-muted-foreground">
                    {baseRuntime}
                  </span>
                  {activePrompt ? (
                    <span className="inline-flex h-5 shrink-0 items-center gap-1 rounded-md border border-emerald-500/25 bg-emerald-500/10 px-1.5 text-[11px] font-medium text-emerald-600 dark:text-emerald-400">
                      <CheckCircle2 className="size-3" />
                      prompt 已加载
                    </span>
                  ) : (
                    <span className="inline-flex h-5 shrink-0 items-center gap-1 rounded-md border border-amber-500/25 bg-amber-500/10 px-1.5 text-[11px] font-medium text-amber-600 dark:text-amber-400">
                      <AlertTriangle className="size-3" />
                      无保存的 prompt
                    </span>
                  )}
                </span>
              </span>
              <ChevronDown
                className={`size-4 shrink-0 text-muted-foreground transition-transform ${infoOpen ? "rotate-180" : ""}`}
              />
            </button>
            {infoOpen && (
            <Card className="gap-0 overflow-hidden rounded-lg border border-border/80 bg-card/80 py-0 ring-0">
              <div className="grid gap-0 md:grid-cols-[minmax(0,1fr)_minmax(280px,360px)]">
                <section className="min-w-0 border-b border-border/70 p-4 md:border-b-0 md:border-r">
                  <div className="flex items-start gap-3">
                    <div className="flex size-9 shrink-0 items-center justify-center rounded-md border border-border bg-background">
                      <Bot className="size-4 text-foreground" />
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <h2 className="truncate text-base font-semibold tracking-tight leading-5">{activeAgentName}</h2>
                        {activePrompt ? (
                          <span className="inline-flex h-5 items-center gap-1 rounded-md border border-emerald-500/25 bg-emerald-500/10 px-1.5 text-[11px] font-medium text-emerald-600 dark:text-emerald-400">
                            <CheckCircle2 className="size-3" />
                            prompt 已加载
                          </span>
                        ) : (
                          <span className="inline-flex h-5 items-center gap-1 rounded-md border border-amber-500/25 bg-amber-500/10 px-1.5 text-[11px] font-medium text-amber-600 dark:text-amber-400">
                            <AlertTriangle className="size-3" />
                            无保存的 prompt
                          </span>
                        )}
                      </div>
                      {activeAgent?.description ? (
                        <p className="mt-1 max-w-2xl text-xs leading-relaxed text-muted-foreground">
                          {String(activeAgent.description)}
                        </p>
                      ) : (
                        <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
                          会话指令与运行时上下文会展示在对话记录之前。
                        </p>
                      )}
                      <div className="mt-3 grid gap-1.5 text-[11px] sm:grid-cols-2">
                        <div className="flex min-w-0 items-center gap-1.5 rounded-md border border-border/70 bg-background px-2 py-1.5">
                          <Cpu className="size-3.5 shrink-0 text-muted-foreground" />
                          <span className="text-muted-foreground">runtime</span>
                          <span className="ml-auto truncate font-mono text-foreground">{baseRuntime}</span>
                        </div>
                        <div className="flex min-w-0 items-center gap-1.5 rounded-md border border-border/70 bg-background px-2 py-1.5">
                          <FileText className="size-3.5 shrink-0 text-muted-foreground" />
                          <span className="text-muted-foreground">session</span>
                          <span className="ml-auto truncate font-mono text-foreground">{shortSid}</span>
                        </div>
                        {providerLink && (
                          <a
                            href={providerLink}
                            target="_blank"
                            rel="noreferrer"
                            className="flex min-w-0 items-center gap-1.5 rounded-md border border-border/70 bg-background px-2 py-1.5 hover:bg-muted"
                          >
                            <ExternalLink className="size-3.5 shrink-0 text-muted-foreground" />
                            <span className="text-muted-foreground">provider</span>
                            <span className="ml-auto truncate font-mono text-foreground">
                              {providerSessionId ?? "open"}
                            </span>
                          </a>
                        )}
                      </div>
                      {(skills.length > 0 || vaultKeys.length > 0) && (
                        <div className="mt-3 flex flex-wrap gap-1.5">
                          {skills.map((skill) => (
                            <span
                              key={skill}
                              className="inline-flex h-5 items-center gap-1 rounded-md border border-sky-500/25 bg-sky-500/10 px-1.5 font-mono text-[11px] text-sky-600 dark:text-sky-400"
                            >
                              <Wrench className="size-3" />
                              {skill}
                            </span>
                          ))}
                          {vaultKeys.map((key) => (
                            <span
                              key={key}
                              className="inline-flex h-5 items-center gap-1 rounded-md border border-amber-500/25 bg-amber-500/10 px-1.5 font-mono text-[11px] text-amber-600 dark:text-amber-400"
                            >
                              <KeyRound className="size-3" />
                              {key}
                            </span>
                          ))}
                        </div>
                      )}
                    </div>
                  </div>
                </section>

                <section className="min-w-0 bg-background/35 p-4">
                  <div className="flex items-center gap-2">
                    <div className="min-w-0">
                      <h3 className="text-[13px] font-semibold tracking-tight">System prompt</h3>
                      <div className="text-[11px] text-muted-foreground">
                        {activePrompt ? "首轮运行前可在此查看。" : "未挂载可复用的智能体 prompt。"}
                      </div>
                    </div>
                    <div className="ml-auto flex shrink-0 items-center gap-1">
                      <Button
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        disabled={!activePrompt}
                        onClick={onCopyPrompt}
                        aria-label="复制 system prompt"
                        title="复制 system prompt"
                      >
                        {promptCopied ? <ClipboardCheck className="size-3.5" /> : <Clipboard className="size-3.5" />}
                      </Button>
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        className="h-7"
                        onClick={() => setPromptOpen((v) => !v)}
                        disabled={!activePrompt}
                        title={promptOpen ? "收起 system prompt" : "展开 system prompt"}
                      >
                        <span>{promptOpen ? "收起" : "展开"}</span>
                        <ChevronDown className={`size-3.5 transition-transform ${promptOpen ? "rotate-180" : ""}`} />
                      </Button>
                    </div>
                  </div>
                  <div className="mt-3">
                    {activePrompt ? (
                      promptOpen ? (
                        <pre className="max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-md border border-border bg-background p-3 font-mono text-xs leading-relaxed text-foreground">
                          {activePrompt}
                        </pre>
                      ) : (
                        <div className="rounded-md border border-border bg-background p-3">
                          <p className="line-clamp-4 font-mono text-xs leading-relaxed text-muted-foreground">
                            {shortPrompt(activePrompt)}
                          </p>
                        </div>
                      )
                    ) : (
                      <div className="rounded-md border border-amber-500/25 bg-amber-500/10 p-3 text-xs leading-relaxed text-amber-600 dark:text-amber-400">
                        {activeAgent
                          ? "该智能体尚未保存 system prompt，可到 Agents 页面补充后再运行。"
                          : "这是内置运行时会话，没有可查看的智能体 prompt。"}
                      </div>
                    )}
                  </div>
                  {promptCopied && (
                    <div className="mt-2 text-[11px] text-emerald-600 dark:text-emerald-400">
                      已复制 system prompt。
                    </div>
                  )}
                </section>
              </div>
            </Card>
            )}
            {displayMessages && displayMessages.length === 0 && (
              <div className="flex flex-col items-center gap-3 py-16 text-center">
                <Bot className="size-8 text-muted-foreground" />
                <p className="text-sm text-muted-foreground">还没有消息。</p>
                <p className="text-xs text-muted-foreground">在下方输入消息开始对话。</p>
              </div>
            )}
            {displayMessages?.map((m, i) => (
              <MessageBlock
                key={(m.info.id as string | undefined) ?? i}
                msg={m}
                onCancelQueued={cancelQueuedPrompt}
                onSendQueued={interruptAndSendQueuedPrompt}
                queuedActionBusy={interruptingQueuedPromptId === m.info.id}
              />
            ))}
            {sessionStatus === "idle" && sessionRuntime && (lastTurnFailed || error) && lastUserPrompt && (
              <div className="flex items-center gap-2">
                <Button variant="outline" size="sm" className="h-7" onClick={retryLastPrompt}>
                  重试上一条指令
                </Button>
                <span className="max-w-[50vw] truncate text-xs text-muted-foreground">{lastUserPrompt}</span>
              </div>
            )}
            {sessionStatus === "busy" && (
              <div className="flex items-center gap-2 text-[13px] text-muted-foreground">
                <Loader2 className="size-3.5 shrink-0 animate-spin motion-reduce:animate-none" />
                <span className="font-medium text-foreground">
                  {activeActivity ? `正在运行 ${activeActivity.label}` : "正在思考"}
                </span>
                {activeActivity?.desc && (
                  <span className="mono max-w-[40vw] truncate text-xs">{activeActivity.desc}</span>
                )}
                <span className="mono text-xs">· {formatElapsed(turnElapsedSeconds)}</span>
              </div>
            )}
          </div>
          {!isPinned && <JumpToBottomButton onClick={jumpToBottom} />}
        </div>

        <ApprovalDock
          approvals={approvals}
          onAccept={onApprovalAccept}
          onReject={onApprovalReject}
          onAcceptAlways={onApprovalAcceptAlways}
          busy={approvalBusy}
        />

        <Composer
          sessionId={sid}
          model={model}
          onSent={sessionRuntime ? undefined : refetch}
          onSend={sessionRuntime ? sendOrQueueRuntimePrompt : undefined}
          onSendStart={sessionRuntime ? (text) => {
            if (sessionStatus !== "busy") beginRuntimeTurn(text);
          } : undefined}
          onAbort={sessionRuntime ? () => abortSession(sid).catch(() => {}) : undefined}
          busy={Boolean(sessionRuntime && sessionStatus === "busy")}
          disabled={sessionContentLoading || Boolean(sessionRuntime && !model.trim())}
          disabledHint={sessionContentLoading ? "正在加载对话..." : undefined}
          mentionFiles={mentionFiles}
          draftValue={composerDraft}
          onDraftChange={setComposerDraft}
          focusVersion={composerFocusVersion}
        />
      </div>

      {workspacePanelOpen && workspaceBucket && (
        <WorkspacePanel
          sessionId={sid}
          onClose={() => setWorkspacePanelOpen(false)}
          onInsertPaths={insertWorkspacePaths}
          onProcessPaths={processWorkspacePaths}
        />
      )}

      <InspectorPanel
        open={inspectorOpen}
        onClose={() => setInspectorOpen(false)}
        sessionId={sid}
        initialFrames={eventBufferRef.current}
      />
    </div>
  );
}

function formatElapsed(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  return `${Math.floor(seconds / 60)}m${seconds % 60}s`;
}

function PinnedTodoBar({ items }: { items: import("@/components/todo-list").TodoItem[] }) {
  const [open, setOpen] = useState(true);
  const { done, total } = todoProgress(items);
  return (
    <div className="sticky top-0 z-10 border-b border-border bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/80">
      <div className="mx-auto w-full max-w-5xl px-6 py-2">
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          aria-expanded={open}
          className="flex w-full items-center gap-2 text-left text-xs text-muted-foreground hover:text-foreground"
        >
          <ListChecks className="size-3.5 shrink-0" />
          <span className="font-medium text-foreground">任务清单</span>
          <span className="mono">{done}/{total}</span>
          <span className="ml-auto">
            <ChevronDown className={`size-3.5 transition-transform ${open ? "rotate-180" : ""}`} />
          </span>
        </button>
        {open && (
          <div className="max-h-48 overflow-y-auto pb-1 pt-2">
            <TodoList items={items} />
          </div>
        )}
      </div>
    </div>
  );
}

export default function ChatPage() {
  return (
    <Suspense
      fallback={
        <div className="min-h-screen flex items-center justify-center text-muted-foreground text-sm">
          Loading…
        </div>
      }
    >
      <ChatInner />
    </Suspense>
  );
}
