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
  X,
  Sparkles,
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
import { MessageBlock, isTodoTool } from "@/components/message-block";
import { TodoList, parseTodoItems, todoProgress } from "@/components/todo-list";
import { Composer } from "@/components/composer";
import { ThemeToggle } from "@/components/theme-toggle";
import { Sidebar } from "@/components/sidebar";
import { InspectorPanel } from "@/components/inspector-panel";
import { WorkspacePanel } from "@/components/workspace-panel";
import { JumpToBottomButton } from "@/components/jump-to-bottom-button";
import { SessionLoadingSkeleton } from "@/components/session-loading-skeleton";
import { useStickToBottom } from "@/lib/hooks/use-stick-to-bottom";
import {
  getMessages,
  getSession,
  createSession,
  deleteSession,
  renameSession,
  subscribeRuntimeEvents,
  listModels,
  abortSession,
  listAgents,
  listApprovals,
  acceptApproval,
  rejectApproval,
  sendMessageWithRuntimeModel,
  listRuntimeEvents,
  listRuntimeHarnesses,
  listWorkspaceFiles,
  apiErrorMessage,
  ensureWebSession,
  getActiveTurn,
  cancelTurn,
} from "@/lib/api";
import { setSessionApprovalMode } from "@/lib/api";
import type { ApprovalMode, PendingApproval, RuntimeAgentEvent, SessionTurnSnapshot } from "@/lib/api";
import { ApprovalDock } from "@/components/approval-dock";
import { RunDrawer } from "@/components/run/RunDrawer";
import { ExposedAppsMenu } from "@/components/exposed-apps-menu";
import { toast } from "sonner";
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
  "claude-code": "Claude Code 智能体",
  cc: "Claude Code 智能体",
  "github-copilot": "GitHub Copilot 助手",
  codex: "Codex 智能体",
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
  return BUILTIN_AGENTS[runtime ?? ""] ?? runtime ?? "Claude Code 智能体";
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
  const [activeTurn, setActiveTurn] = useState<SessionTurnSnapshot | null | undefined>(undefined);
  const [runDrawerOpen, setRunDrawerOpen] = useState(false);
  const [approvals, setApprovals] = useState<PendingApproval[]>([]);
  const [approvalsLoaded, setApprovalsLoaded] = useState(false);
  const [approvalBusy, setApprovalBusy] = useState(false);
  const [contextPanel, setContextPanel] = useState<"workspace" | "inspector" | null>(null);
  const [workspaceBucket, setWorkspaceBucket] = useState<string | undefined>();
  const [approvalMode, setApprovalMode] = useState<ApprovalMode>("ask");
  const [composerDraft, setComposerDraft] = useState("");
  const [composerFocusVersion, setComposerFocusVersion] = useState(0);
  const [promptOpen, setPromptOpen] = useState(false);
  const [promptCopied, setPromptCopied] = useState(false);
  const eventBufferRef = useRef<Frame[]>([]);
  const lastRuntimeEventAtRef = useRef(0);
  const [runtimeEvents, setRuntimeEvents] = useState<RuntimeAgentEvent[]>([]);
  const [runtimeEventsLoaded, setRuntimeEventsLoaded] = useState(false);
  const [queuedPrompts, setQueuedPrompts] = useState<QueuedPrompt[]>([]);
  const [interruptingQueuedPromptId, setInterruptingQueuedPromptId] = useState<string | null>(null);
  const workspacePanelOpen = contextPanel === "workspace";
  const inspectorOpen = contextPanel === "inspector";
  const dispatchingQueuedPromptRef = useRef(false);
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
  const [infoOpenManual, setInfoOpenManual] = useState<boolean | null>(null);
  const { scrollRef, contentRef, onScroll, isPinned, jumpToBottom } = useStickToBottom(sessionStatus === "busy");
  const activeSessionRef = useRef<string | null>(null);
  const terminalSessionSnapshotRef = useRef(false);
  const canonicalTurnObservedRef = useRef(false);
  const initiallyScrolledSessionRef = useRef<string | null>(null);
  const autostartedRef = useRef<string | null>(null);
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

  useEffect(() => {
    if (activeTurn) canonicalTurnObservedRef.current = true;
  }, [activeTurn]);

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

  const pinnedTodos = useMemo(() => {
    if (!displayMessages || sessionStatus !== "busy") return null;
    for (let i = displayMessages.length - 1; i >= 0; i--) {
      const parts = displayMessages[i].parts;
      for (let j = parts.length - 1; j >= 0; j--) {
        const part = parts[j];
        if (part.type !== "tool" || !isTodoTool(part.tool)) continue;
        const items = parseTodoItems(part.state?.input, part.state?.output);
        if (items && items.length > 0) {
          const { done, total } = todoProgress(items);
          return done < total ? items : null;
        }
      }
    }
    return null;
  }, [displayMessages, sessionStatus]);

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
      (part) =>
        (part.type === "text" && typeof part.text === "string" && part.text.startsWith("Error:")) ||
        (part.type === "tool" && ["error", "timed_out", "aborted"].includes(part.state.status)),
    );
  }, [displayMessages, sessionStatus]);

  const sessionContentLoading = !displayMessages && !error;
  const sessionStatusLoading = !sessionLoaded || sessionContentLoading || !approvalsLoaded;
  const waitingForApproval = approvals.length > 0;
  const waitingForAuthorizedApprover = approvals.some((approval) => !approval.canDecide);
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

  useEffect(() => {
    if (!sessionRuntime) return;
    const agent = savedAgents.find((a) => a.id === sessionHarness);
    if (!agent?.model) return;
    setModel((prev) => (models.includes(agent.model!) ? agent.model! : prev));
  }, [savedAgents, sessionHarness, sessionRuntime, models]);

  useEffect(() => {
    if (!sid) return;
    initiallyScrolledSessionRef.current = null;
    jumpToBottom();
    activeSessionRef.current = sid;
    eventBufferRef.current = [];
    lastRuntimeEventAtRef.current = 0;
    setMessages(null);
    setRuntimeEvents([]);
    setRuntimeEventsLoaded(false);
    setQueuedPrompts([]);
    setInterruptingQueuedPromptId(null);
    setApprovals([]);
    setApprovalsLoaded(false);
    setError(null);
    setSessionRuntime(undefined);
    setSessionLoaded(false);
    setSessionStatus("idle");
    setActiveTurn(undefined);
    terminalSessionSnapshotRef.current = false;
    canonicalTurnObservedRef.current = false;
    setSessionHarness("claude-code");
    setProviderSessionId(undefined);
    setProviderUrl(undefined);
    setSessionTitle("");
    setEditingTitle(null);
    setInfoOpenManual(null);
    decidedApprovalsRef.current.clear();
    setWorkspaceBucket(undefined);
    setApprovalMode("ask");
    const resumed = sp.get("resumed") === "true";
    getSession(sid).then(s => {
      if (activeSessionRef.current !== sid) return;
      const a = s.agent_id ?? s.agent ?? s.harness;
      if (a) setSessionHarness(a);
      setSessionRuntime(s.runtime);
      const defaultStatus = s.runtime ? runtimeSessionStatusFromMetadata(s.status, s.provider_run_id) : s.status === "running" ? "busy" : "idle";
      terminalSessionSnapshotRef.current = defaultStatus === "idle";
      setSessionStatus(resumed ? "busy" : defaultStatus);
      setProviderSessionId(s.provider_session_id);
      setProviderUrl(s.provider_url);
      setWorkspaceBucket(s.workspace_bucket);
      const mode = (s.environment as Record<string, unknown> | undefined)?.approval_mode;
      if (mode === "auto" || mode === "full") setApprovalMode(mode);
      if (s.title) setSessionTitle(s.title);
    }).catch(() => {}).finally(() => {
      if (activeSessionRef.current === sid) setSessionLoaded(true);
    });
  }, [jumpToBottom, sid]);

  useEffect(() => {
    if (!sid || !displayMessages || initiallyScrolledSessionRef.current === sid) return;
    initiallyScrolledSessionRef.current = sid;
    const frame = window.requestAnimationFrame(jumpToBottom);
    return () => window.cancelAnimationFrame(frame);
  }, [displayMessages, jumpToBottom, sid]);

  useEffect(() => {
    if (sp.get("resumed") === "true" && sid) {
      router.replace(`/chat/?id=${encodeURIComponent(sid)}`, { scroll: false });
    }
  }, [sp, sid, router]);

  useEffect(() => {
    ensureWebSession();
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
      if (eventStatus && !(eventStatus === "busy" && terminalSessionSnapshotRef.current)) {
        terminalSessionSnapshotRef.current = eventStatus === "idle";
        setSessionStatus(eventStatus);
      }
      return next;
    });
  }, []);

  const appendRuntimeEvent = useCallback((ev: RuntimeAgentEvent) => {
    lastRuntimeEventAtRef.current = Date.now();
    eventBufferRef.current = [
      ...eventBufferRef.current.slice(-499),
      { ts: Date.now(), ev: ev as Frame["ev"] },
    ];

    const type = normalizedRuntimeEventType(ev);

    if (type === "approval.asked" || type === "approval.replied") {
      const raw = ev.approval as
        | { id?: string; kind?: PendingApproval["kind"]; title?: string; status?: string; args_json?: string | null; created_at?: number; session_id?: string | null; can_decide?: boolean }
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
        canDecide:
          raw.can_decide ??
          (raw.kind !== "data_egress" && raw.kind !== "unlisted_data_egress"),
      };
      setApprovals((prev) =>
        prev.some((approval) => approval.id === next.id) ? prev : [...prev, next],
      );
      return;
    }
    if (isRuntimeTurnStartEvent(type)) {
      terminalSessionSnapshotRef.current = false;
      setSessionStatus("busy");
    } else if (type === "session.status") {
      const status = ev.status;
      const statusType =
        typeof status === "string"
          ? status
          : status && typeof status === "object"
            ? (status as { type?: unknown }).type
            : undefined;
      if (statusType === "busy" || statusType === "running") {
        terminalSessionSnapshotRef.current = false;
        setSessionStatus("busy");
      }
    } else if (type === "session.error") {
      setError(`错误: ${runtimeErrorMessage(ev)}`);
      terminalSessionSnapshotRef.current = true;
      setSessionStatus("idle");
    } else if (isRuntimeToolEvent(type) && ev.status === "rejected") {
      terminalSessionSnapshotRef.current = true;
      setSessionStatus("idle");
    } else if (
      type === "user.message" ||
      isRuntimeAssistantTextEvent(type) ||
      isRuntimeThinkingEvent(type) ||
      isRuntimeToolEvent(type)
    ) {
      terminalSessionSnapshotRef.current = false;
      setSessionStatus((current) => (current === "busy" ? current : "busy"));
    }

    mergeRuntimeEventsAndStatus(ev);
  }, [mergeRuntimeEventsAndStatus]);

  const beginRuntimeTurn = useCallback((text?: string) => {
    if (!sessionRuntime || !sid) return;
    terminalSessionSnapshotRef.current = false;
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
      const pendingApprovals = approvalsRef.current;
      if (pendingApprovals.length > 0) {
        const approvalsWithoutPermission = pendingApprovals.filter((approval) => !approval.canDecide);
        if (approvalsWithoutPermission.length > 0) {
          queueRuntimePrompt(text);
          setError("当前审批需要有权限的审批人处理，消息已排队。");
          return;
        }

        setApprovalBusy(true);
        try {
          const rejectedIds = new Set<string>();
          let deliveryFailed = false;
          for (const approval of pendingApprovals) {
            const result = await rejectApproval(
              approval.id,
              "用户发送了新消息，已取消当前审批和对应操作。",
            );
            rejectedIds.add(approval.id);
            decidedApprovalsRef.current.set(approval.id, Date.now());
            deliveryFailed ||= result.delivery_status === "delivery_failed";
          }
          setApprovals((current) => current.filter((approval) => !rejectedIds.has(approval.id)));
          if (deliveryFailed) {
            setError("已取消当前审批，但审批结果尚未送达运行时。正在切换到新消息。");
          }
        } catch (err) {
          const message = apiErrorMessage(err, "取消当前审批失败，新消息未发送");
          setError(message);
          throw new Error(message);
        } finally {
          setApprovalBusy(false);
        }
      }
      await abortSession(sid);
      if (activeSessionRef.current !== sid) return;
      beginRuntimeTurn(text);
    }
    try {
      const turn = await sendMessageWithRuntimeModel({
        sessionId: sid,
        text,
        model,
        runtime: sessionRuntime,
        apiSpec: resolveApiSpec(sessionRuntime ?? "", harnesses),
      });
      setActiveTurn(turn);
    } catch (err) {
      if (activeSessionRef.current !== sid) return;
      setError(err instanceof Error ? err.message : String(err));
      setSessionStatus("idle");
      throw err;
    }
  }, [beginRuntimeTurn, model, queueRuntimePrompt, sessionRuntime, sessionStatus, sid, harnesses]);

  useEffect(() => {
    if (
      !sid ||
      !sessionRuntime ||
      sessionStatus !== "idle" ||
      queuedPrompts.length === 0 ||
      dispatchingQueuedPromptRef.current
    ) return;
    const next = queuedPrompts[0];
    dispatchingQueuedPromptRef.current = true;
    setQueuedPrompts((current) => current.filter((prompt) => prompt.id !== next.id));
    beginRuntimeTurn(next.text);
    sendMessageWithRuntimeModel({
      sessionId: sid,
      text: next.text,
      model,
      runtime: sessionRuntime,
      apiSpec: resolveApiSpec(sessionRuntime, harnesses),
    })
      .then((turn) => {
        if (activeSessionRef.current === sid) {
          setActiveTurn(turn);
          setRuntimeStreamVersion((version) => version + 1);
        }
      })
      .catch((err) => {
        if (activeSessionRef.current !== sid) return;
        setQueuedPrompts((current) => [next, ...current]);
        setError(err instanceof Error ? err.message : String(err));
        setSessionStatus("idle");
      })
      .finally(() => {
        dispatchingQueuedPromptRef.current = false;
      });
  }, [beginRuntimeTurn, harnesses, model, queuedPrompts, sessionRuntime, sessionStatus, sid]);

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
    const turn = await sendMessageWithRuntimeModel({ sessionId: sid, text: prompt, model });
    setActiveTurn(turn);
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
        await abortSession(sid);
      }
      if (activeSessionRef.current !== sid) return;
      setQueuedPrompts((current) => current.filter((item) => item.id !== id));
      beginRuntimeTurn(prompt.text);
      const turn = await sendMessageWithRuntimeModel({
        sessionId: sid,
        text: prompt.text,
        model,
        runtime: sessionRuntime,
        apiSpec: resolveApiSpec(sessionRuntime ?? "", harnesses),
      });
      setActiveTurn(turn);
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

  const stopRuntimeTurn = useCallback(async () => {
    if (!sid || !sessionRuntime) return;
    setError(null);
    terminalSessionSnapshotRef.current = true;
    setSessionStatus("idle");
    setQueuedPrompts([]);
    try {
      if (activeTurn?.turn.id) {
        const turn = await cancelTurn(sid, activeTurn.turn.id);
        setActiveTurn(turn);
      } else {
        await abortSession(sid);
        setActiveTurn(null);
      }
      if (activeSessionRef.current !== sid) return;
      setRuntimeStreamVersion((version) => version + 1);
    } catch (err) {
      if (activeSessionRef.current !== sid) return;
      terminalSessionSnapshotRef.current = false;
      setSessionStatus("busy");
      setError(apiErrorMessage(err, "中断会话失败"));
    }
  }, [activeTurn, sessionRuntime, sid]);

  useEffect(() => {
    if (!sid || !sessionLoaded) return;
    let unsub: (() => void) | undefined;
    let cancelled = false;
    if (sessionRuntime) {
      listRuntimeEvents(sid, { snapshot: true })
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
                toast.error(apiErrorMessage(err, "事件流连接中断，正在重试"));
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
        .then((turn) => {
          if (activeSessionRef.current !== sid) return;
          canonicalTurnObservedRef.current = true;
          setActiveTurn(turn);
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
      .catch(() => {})
      .finally(() => {
        if (activeSessionRef.current === sid) setApprovalsLoaded(true);
      });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, [sid, sessionLoaded, refetch, appendRuntimeEvent, applyApprovals, mergeRuntimeEventsAndStatus, autostartPrompt, beginRuntimeTurn, model, router, sessionRuntime, runtimeStreamVersion, harnesses]);

  useEffect(() => {
    if (!sid || !sessionLoaded) return;
    let mounted = true;
    let timer: number | undefined;
    const refresh = () => {
      getActiveTurn(sid)
        .then((turn) => {
          if (!mounted || activeSessionRef.current !== sid) return;
          const turnStatus = turn?.turn?.status;
          const isBusyTurn =
            turnStatus === "running" ||
            turnStatus === "queued" ||
            turnStatus === "cancelling";
          if (turn && isBusyTurn) {
            canonicalTurnObservedRef.current = true;
            setActiveTurn(turn);
            terminalSessionSnapshotRef.current = false;
            setSessionStatus("busy");
          } else {
            if (turn) setActiveTurn(turn);
            if (canonicalTurnObservedRef.current || !isBusyTurn) {
              terminalSessionSnapshotRef.current = true;
              setSessionStatus("idle");
            }
          }
        })
        .catch(() => {})
        .finally(() => {
          if (!mounted) return;
          timer = window.setTimeout(refresh, sessionStatus === "busy" ? 1500 : 8000);
        });
    };
    refresh();
    return () => {
      mounted = false;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [sessionLoaded, sessionStatus, sid]);

  useEffect(() => {
    if (!sid || !sessionRuntime) return;
    let active = true;
    let timer: number | undefined;
    const replay = () => {
      if (
        sessionStatus === "busy" &&
        Date.now() - lastRuntimeEventAtRef.current >= 8_000
      ) {
        listRuntimeEvents(sid)
          .then((events) => {
            if (!active) return;
            if (activeSessionRef.current !== sid) return;
            lastRuntimeEventAtRef.current = Date.now();
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
      const result = await acceptApproval(id, args);
      decidedApprovalsRef.current.set(id, Date.now());
      setApprovals((prev) => prev.filter((a) => a.id !== id));
      setSessionStatus("busy");
      setRuntimeStreamVersion((version) => version + 1);
      if (result.delivery_status === "delivery_failed") {
        setError("审批决定已记录，但尚未送达运行时。请前往收件箱重试交付。");
      }
    } catch (e) {
      setError(apiErrorMessage(e, "审批操作失败"));
    } finally {
      setApprovalBusy(false);
    }
  }, []);

  const onApprovalAcceptAlways = useCallback(async (id: string, args: Record<string, unknown>) => {
    setApprovalBusy(true);
    try {
      const result = await acceptApproval(id, args, "session");
      decidedApprovalsRef.current.set(id, Date.now());
      setApprovals((prev) => prev.filter((approval) => approval.id !== id));
      setSessionStatus("busy");
      setRuntimeStreamVersion((version) => version + 1);
      if (result.delivery_status === "delivery_failed") {
        setError("审批决定已记录，但尚未送达运行时。请前往收件箱重试交付。");
      }
    } catch (error) {
      setError(apiErrorMessage(error, "审批操作失败"));
    } finally {
      setApprovalBusy(false);
    }
  }, []);

  const onApprovalReject = useCallback(async (id: string, feedback: string) => {
    setApprovalBusy(true);
    try {
      const result = await rejectApproval(id, feedback);
      decidedApprovalsRef.current.set(id, Date.now());
      setApprovals((prev) => prev.filter((a) => a.id !== id));
      terminalSessionSnapshotRef.current = true;
      setSessionStatus("idle");
      setRuntimeStreamVersion((version) => version + 1);
      if (result.delivery_status === "delivery_failed") {
        setError("拒绝决定已记录，但尚未送达运行时。请前往收件箱重试交付。");
      }
    } catch (e) {
      setError(apiErrorMessage(e, "审批操作失败"));
    } finally {
      setApprovalBusy(false);
    }
  }, []);

  const onApprovalModeChange = useCallback(
    (mode: ApprovalMode) => {
      if (!sid) return;
      const previous = approvalMode;
      setApprovalMode(mode);
      setSessionApprovalMode(sid, mode).catch((err) => {
        setApprovalMode(previous);
        toast.error(apiErrorMessage(err, "切换审批模式失败"));
      });
    },
    [approvalMode, sid],
  );

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
    <div className="flex h-screen bg-background text-foreground overflow-hidden selection:bg-blue-500/20">
      <Sidebar activeId={sid} />

      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        {/* Anti-slop Pure Chinese Header */}
        <header className="h-12 border-b border-border/80 bg-background/80 backdrop-blur flex items-center justify-between px-4 shrink-0">
          <div className="flex min-w-0 items-center gap-2.5">
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
                className="h-7 w-[240px] rounded-lg border border-border bg-background px-2.5 text-xs font-semibold outline-none focus-visible:ring-2 focus-visible:ring-blue-500/40"
              />
            ) : (
              <button
                type="button"
                onClick={() => setEditingTitle(sessionTitle || "")}
                title="点击重命名会话"
                className="max-w-[280px] truncate rounded-lg px-2 py-1 text-left text-xs font-bold hover:bg-muted transition-colors text-foreground"
              >
                {sessionTitle || "新智能体对话会话"}
              </button>
            )}
            <button
              type="button"
              title="点击复制完整会话 ID"
              onClick={() => {
                navigator.clipboard?.writeText(sid).then(
                  () => toast.success("会话 ID 已复制到剪贴板"),
                  () => {},
                );
              }}
              className="shrink truncate rounded-md border border-border/60 bg-muted/40 px-2 py-0.5 text-[11px] font-mono text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
            >
              {shortSid}
            </button>

            {/* Status Pills */}
            {sessionStatusLoading ? (
              <span className="flex shrink-0 items-center gap-1.5 whitespace-nowrap text-[11px] text-muted-foreground font-mono">
                <Loader2 className="size-3 animate-spin" />
                正在初始化...
              </span>
            ) : waitingForApproval ? (
              <span className="flex shrink-0 items-center gap-1.5 whitespace-nowrap text-[11px] text-amber-600 dark:text-amber-400 font-medium bg-amber-500/10 px-2 py-0.5 rounded-md border border-amber-500/20">
                <span className="size-1.5 shrink-0 rounded-full bg-amber-500 animate-ping" />
                {waitingForAuthorizedApprover ? "等待授权审批人处理" : "等待确认"}
              </span>
            ) : sessionStatus === "busy" ? (
              <button
                onClick={() => void stopRuntimeTurn()}
                className="flex shrink-0 items-center gap-1.5 whitespace-nowrap text-[11px] text-amber-600 dark:text-amber-400 font-mono hover:text-destructive bg-amber-500/10 hover:bg-destructive/10 px-2 py-0.5 rounded-md border border-amber-500/20 hover:border-destructive/30 transition-all group"
                title="点击立刻中止智能体当前任务"
                aria-label="智能体运行中，点击中止"
              >
                <Loader2 className="size-3 shrink-0 animate-spin group-hover:hidden" />
                <Square className="size-3 shrink-0 hidden group-hover:block fill-current" />
                <span className="group-hover:hidden">思考执行中...</span>
                <span className="hidden group-hover:inline">立即中止</span>
              </button>
            ) : (
              <span className="flex shrink-0 items-center gap-1.5 whitespace-nowrap text-[11px] text-emerald-600 dark:text-emerald-400 font-medium bg-emerald-500/10 px-2 py-0.5 rounded-md border border-emerald-500/20">
                <span className="size-1.5 shrink-0 rounded-full bg-emerald-500" />
                就绪空闲
              </span>
            )}

            {activeTurn && (
              <button
                type="button"
                onClick={() => setRunDrawerOpen(true)}
                title="查看这个 Turn 的 Run 详情（执行时间线、结果、Artifact）"
                className="shrink-0 whitespace-nowrap rounded-md border border-border/60 bg-muted/40 px-2 py-0.5 text-[11px] text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
              >
                查看 Run
              </button>
            )}
          </div>

          <div className="flex items-center gap-2.5">
            <div className="flex items-center gap-1.5">
              <span className="text-xs text-muted-foreground font-medium">智能体</span>
              <Select
                value={activeAgent?.id ?? ""}
                onValueChange={(v) => v && onHarnessChange(v)}
                disabled={switchingAgent || sessionStatus === "busy"}
              >
                <SelectTrigger className="h-8 text-xs w-[180px] bg-card">
                  <SelectValue placeholder={activeAgent ? activeAgentName : "默认智能体"} />
                </SelectTrigger>
                <SelectContent>
                  {savedAgents.length > 0 && (
                    <>
                      <div className="mt-1 border-t px-2 py-1.5 pt-2 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">可调度的智能体</div>
                      {savedAgents.map(a => (
                        <SelectItem key={a.id} value={a.id} className="text-xs">{a.name}</SelectItem>
                      ))}
                    </>
                  )}
                  <div className="px-2 py-2 text-[11px] text-muted-foreground border-t mt-1">
                    切换智能体会开启新会话。
                  </div>
                </SelectContent>
              </Select>
              {switchingAgent && <Loader2 className="size-3.5 animate-spin text-muted-foreground" />}
            </div>

            <div className="flex items-center gap-1.5">
              <span className="text-xs text-muted-foreground font-medium">接入模型</span>
              <ModelSelect value={model} models={modelOptions} onValueChange={setModel} />
            </div>

            {providerLink && (
              <Button
                variant="outline"
                size="sm"
                className="h-8 text-xs gap-1"
                render={
                  <a href={providerLink} target="_blank" rel="noreferrer">
                    <ExternalLink className="size-3.5" />
                    提供方会话
                  </a>
                }
              />
            )}
            <ExposedAppsMenu
              sessionId={sid}
              agentId={sessionHarness.startsWith("agent_") ? sessionHarness : undefined}
            />
            {workspaceBucket && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setContextPanel((panel) => panel === "workspace" ? null : "workspace")}
                className={`h-8 gap-1.5 text-xs ${workspacePanelOpen ? "bg-muted text-foreground" : "text-muted-foreground"}`}
                aria-pressed={workspacePanelOpen}
              >
                <FolderOpen className="size-3.5" />
                工作区文件
              </Button>
            )}
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setContextPanel((panel) => panel === "inspector" ? null : "inspector")}
              className={`h-8 gap-1.5 text-xs ${inspectorOpen ? "bg-muted text-foreground" : "text-muted-foreground"}`}
              aria-pressed={inspectorOpen}
            >
              <Activity className="size-3.5" />
              轨迹检查器
            </Button>
            <ThemeToggle />
          </div>
        </header>

        <div
          ref={scrollRef}
          onScroll={onScroll}
          className="relative min-h-0 flex-1 overflow-y-auto"
        >
          {pinnedTodos && <PinnedTodoBar items={pinnedTodos} />}
          <div ref={contentRef} className="mx-auto flex w-full max-w-5xl flex-col gap-6 px-6 py-8">
            {sessionContentLoading && <SessionLoadingSkeleton />}
            {error && (
              <div className="flex flex-row items-start justify-between gap-3 rounded-xl border border-destructive/40 bg-destructive/10 p-4 text-xs text-destructive">
                <p className="min-w-0 font-mono leading-relaxed">{error}</p>
                <button
                  type="button"
                  onClick={() => setError(null)}
                  aria-label="关闭错误提示"
                  className="shrink-0 rounded p-0.5 text-destructive/70 hover:bg-destructive/10 hover:text-destructive"
                >
                  <X className="size-4" />
                </button>
              </div>
            )}

            {/* Agent Info Drawer Pill Header */}
            <button
              type="button"
              onClick={() => setInfoOpenManual(!infoOpen)}
              aria-expanded={infoOpen}
              className="flex w-full items-center gap-3 rounded-xl border border-border/70 bg-card p-3 text-left transition-all hover:border-blue-500/40 hover:shadow-2xs"
            >
              <div className="flex size-9 shrink-0 items-center justify-center rounded-xl bg-blue-500/10 text-blue-600 dark:text-blue-400 border border-blue-500/20">
                <Bot className="size-4" />
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex min-w-0 flex-wrap items-center gap-2">
                  <span className="truncate text-sm font-bold text-foreground">{activeAgentName}</span>
                  <span className="shrink-0 rounded-md border border-border/60 bg-muted/40 px-2 py-0.5 font-mono text-[10px] text-muted-foreground">
                    {baseRuntime}
                  </span>
                  {activePrompt ? (
                    <span className="inline-flex items-center gap-1 rounded-md border border-emerald-500/25 bg-emerald-500/10 px-2 py-0.5 text-[10px] font-medium text-emerald-600 dark:text-emerald-400">
                      <CheckCircle2 className="size-3" />
                      提示词已加载
                    </span>
                  ) : (
                    <span className="inline-flex items-center gap-1 rounded-md border border-amber-500/25 bg-amber-500/10 px-2 py-0.5 text-[10px] font-medium text-amber-600 dark:text-amber-400">
                      <AlertTriangle className="size-3" />
                      未挂载自定义提示词
                    </span>
                  )}
                </div>
              </div>
              <ChevronDown
                className={`size-4 shrink-0 text-muted-foreground transition-transform ${infoOpen ? "rotate-180" : ""}`}
              />
            </button>

            {infoOpen && (
              <div className="overflow-hidden rounded-2xl border border-border/70 bg-card p-0 shadow-2xs">
                <div className="grid gap-0 md:grid-cols-[minmax(0,1fr)_minmax(280px,360px)]">
                  <section className="min-w-0 border-b border-border/70 p-5 md:border-b-0 md:border-r">
                    <div className="flex items-start gap-3">
                      <div className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-blue-500/10 text-blue-600 dark:text-blue-400 border border-blue-500/20">
                        <Bot className="size-5" />
                      </div>
                      <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-center gap-2">
                          <h2 className="truncate text-base font-bold tracking-tight text-foreground">{activeAgentName}</h2>
                        </div>
                        {activeAgent?.description ? (
                          <p className="mt-1.5 max-w-2xl text-xs leading-relaxed text-muted-foreground">
                            {String(activeAgent.description)}
                          </p>
                        ) : (
                          <p className="mt-1.5 text-xs leading-relaxed text-muted-foreground">
                            智能体会话上下文与参数会在对话流展开前自动装载。
                          </p>
                        )}

                        <div className="mt-3.5 grid gap-2 text-xs sm:grid-cols-2">
                          <div className="flex min-w-0 items-center gap-1.5 rounded-lg border border-border/70 bg-background p-2">
                            <Cpu className="size-3.5 shrink-0 text-muted-foreground" />
                            <span className="text-muted-foreground text-[11px]">运行时</span>
                            <span className="ml-auto truncate font-mono font-medium text-foreground text-[11px]">{baseRuntime}</span>
                          </div>
                          <div className="flex min-w-0 items-center gap-1.5 rounded-lg border border-border/70 bg-background p-2">
                            <FileText className="size-3.5 shrink-0 text-muted-foreground" />
                            <span className="text-muted-foreground text-[11px]">会话编号</span>
                            <span className="ml-auto truncate font-mono font-medium text-foreground text-[11px]">{shortSid}</span>
                          </div>
                        </div>

                        {(skills.length > 0 || vaultKeys.length > 0) && (
                          <div className="mt-3 flex flex-wrap gap-1.5">
                            {skills.map((skill) => (
                              <span
                                key={skill}
                                className="inline-flex items-center gap-1 rounded-md border border-teal-500/25 bg-teal-500/10 px-2 py-0.5 text-[10px] font-medium text-teal-600 dark:text-teal-400"
                              >
                                <Wrench className="size-3" />
                                {skill}
                              </span>
                            ))}
                            {vaultKeys.map((key) => (
                              <span
                                key={key}
                                className="inline-flex items-center gap-1 rounded-md border border-emerald-500/25 bg-emerald-500/10 px-2 py-0.5 text-[10px] font-medium text-emerald-600 dark:text-emerald-400"
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

                  <section className="min-w-0 bg-muted/20 p-5">
                    <div className="flex items-center gap-2">
                      <div className="min-w-0">
                        <h3 className="text-xs font-semibold tracking-tight text-foreground uppercase">系统提示词 (System Prompt)</h3>
                        <div className="text-[11px] text-muted-foreground mt-0.5">
                          {activePrompt ? "智能体预设指令规范。" : "未挂载提示词规则。"}
                        </div>
                      </div>
                      <div className="ml-auto flex shrink-0 items-center gap-1">
                        <Button
                          type="button"
                          variant="outline"
                          size="icon-sm"
                          disabled={!activePrompt}
                          onClick={onCopyPrompt}
                          aria-label="复制系统提示词"
                          title="复制系统提示词"
                        >
                          {promptCopied ? <ClipboardCheck className="size-3.5 text-emerald-500" /> : <Clipboard className="size-3.5" />}
                        </Button>
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          className="h-7 text-xs"
                          onClick={() => setPromptOpen((v) => !v)}
                          disabled={!activePrompt}
                          title={promptOpen ? "收起提示词" : "展开提示词"}
                        >
                          <span>{promptOpen ? "收起" : "展开"}</span>
                          <ChevronDown className={`size-3.5 transition-transform ${promptOpen ? "rotate-180" : ""}`} />
                        </Button>
                      </div>
                    </div>
                    <div className="mt-3">
                      {activePrompt ? (
                        promptOpen ? (
                          <pre className="max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-xl border border-border/70 bg-background p-3.5 font-mono text-xs leading-relaxed text-foreground">
                            {activePrompt}
                          </pre>
                        ) : (
                          <div className="rounded-xl border border-border/70 bg-background p-3">
                            <p className="line-clamp-3 font-mono text-xs leading-relaxed text-muted-foreground">
                              {shortPrompt(activePrompt)}
                            </p>
                          </div>
                        )
                      ) : (
                        <div className="rounded-xl border border-amber-500/25 bg-amber-500/10 p-3 text-xs leading-relaxed text-amber-600 dark:text-amber-400">
                          {activeAgent
                            ? "该智能体尚未配置 System Prompt 提示词。"
                            : "内置对话会话，无特定提示词预设。"}
                        </div>
                      )}
                    </div>
                  </section>
                </div>
              </div>
            )}

            {displayMessages && displayMessages.length === 0 && (
              <div className="flex flex-col items-center gap-3 py-16 text-center">
                <div className="flex size-14 items-center justify-center rounded-2xl bg-blue-500/10 text-blue-500">
                  <Bot className="size-7" />
                </div>
                <h3 className="text-sm font-semibold text-foreground">准备就绪，开启智能体对话</h3>
                <p className="text-xs text-muted-foreground max-w-sm leading-relaxed">
                  在下方输入框中发送你的第一条指令。智能体将自动检索扩展工具与上下文并实时响应。
                </p>
              </div>
            )}

            {displayMessages?.map((m, i) => (
              <MessageBlock
                key={(m.info.id as string | undefined) ?? i}
                msg={m}
                onCancelQueued={cancelQueuedPrompt}
                onSendQueued={interruptAndSendQueuedPrompt}
                queuedActionBusy={interruptingQueuedPromptId === m.info.id}
                hideTodoTools
                showProgressIndicator={
                  sessionStatus === "busy" && i === displayMessages.length - 1 && !waitingForApproval
                }
              />
            ))}

            {sessionStatus === "idle" && sessionRuntime && (lastTurnFailed || error) && lastUserPrompt && (
              <div className="flex items-center gap-2 pt-2">
                <Button variant="outline" size="sm" className="h-7 text-xs bg-card" onClick={retryLastPrompt}>
                  重试上一条指令
                </Button>
                <span className="max-w-[50vw] truncate text-xs font-mono text-muted-foreground">{lastUserPrompt}</span>
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
          onAbort={sessionRuntime ? () => void stopRuntimeTurn() : undefined}
          busy={Boolean(sessionRuntime && sessionStatus === "busy")}
          disabled={sessionContentLoading || Boolean(sessionRuntime && !model.trim())}
          disabledHint={sessionContentLoading ? "正在加载对话数据..." : undefined}
          mentionFiles={mentionFiles}
          queuedCount={queuedPrompts.length}
          draftValue={composerDraft}
          onDraftChange={setComposerDraft}
          focusVersion={composerFocusVersion}
          approvalMode={approvalMode}
          onApprovalModeChange={onApprovalModeChange}
        />
      </div>

      {workspacePanelOpen && workspaceBucket && (
        <WorkspacePanel
          sessionId={sid}
          onClose={() => setContextPanel(null)}
          onInsertPaths={insertWorkspacePaths}
          onProcessPaths={processWorkspacePaths}
        />
      )}

      <InspectorPanel
        open={inspectorOpen}
        onClose={() => setContextPanel(null)}
        sessionId={sid}
        initialFrames={eventBufferRef.current}
      />

      {activeTurn && (
        <RunDrawer
          sessionId={sid}
          turnId={activeTurn.turn.id}
          open={runDrawerOpen}
          onOpenChange={setRunDrawerOpen}
        />
      )}
    </div>
  );
}

function PinnedTodoBar({ items }: { items: import("@/components/todo-list").TodoItem[] }) {
  const [open, setOpen] = useState(true);
  const { done, total } = todoProgress(items);
  return (
    <div className="sticky top-0 z-10 border-b border-border/80 bg-background/95 backdrop-blur">
      <div className="mx-auto w-full max-w-5xl px-6 py-2">
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          aria-expanded={open}
          className="flex w-full items-center gap-2 text-left text-xs text-muted-foreground hover:text-foreground transition-colors"
        >
          <ListChecks className="size-3.5 shrink-0 text-blue-500" />
          <span className="font-semibold text-foreground">任务实时进度清单</span>
          <span className="font-mono text-blue-600 dark:text-blue-400 font-bold">{done}/{total}</span>
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
        <div className="mx-auto min-h-screen w-full max-w-5xl px-6 py-8">
          <SessionLoadingSkeleton />
        </div>
      }
    >
      <ChatInner />
    </Suspense>
  );
}
