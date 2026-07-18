"use client";

import { toast } from "sonner";
import { useConfirm } from "@/components/confirm-dialog";
import { Suspense, useEffect, useMemo, useRef, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import {
  ArrowLeft,
  Brain,
  Check,
  Clock,
  Download,
  FileText,
  KeyRound,
  Pencil,
  Pin,
  PinOff,
  Play,
  Plus,
  RefreshCw,
  Search,
  Trash2,
  Upload,
  Users,
  X,
} from "lucide-react";
import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { DraftPreflightPanel, ManagedGovernancePanel } from "./governance-panel";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { VaultCredentialsEditor } from "@/components/vault-credentials-editor";
import {
  AgentApplicationOverview,
  AgentInteractiveDashboard,
  applicationContractFromAgent,
  type AgentDashboardSection,
} from "./application-dashboard";
import {
  DEFAULT_VAULT_USER,
  agentFileDownloadUrl,
  createAgentGrant,
  createAgentGrantsBatch,
  createAgentGroupGrant,
  createAgentGroupGrantsBatch,
  createAgentTask,
  createSession,
  createImprovementProposal,
  deleteAgentGrant,
  deleteAgentGroupGrant,
  listAgentGrants,
  listAgentGroupGrants,
  listAgentTasks,
  listTaskAcceptance,
  listTaskArtifacts,
  listTaskAttempts,
  listGrantableGroups,
  listGrantableUsers,
  type AgentGrant,
  type AgentGroupGrant,
  type ManagedGroup,
  type ManagedUser,
  listEvalRuns,
  startEvalRun,
  type EvalRun,
  deleteAgent,
  deleteAgentFile,
  deleteMemory,
  apiErrorMessage,
  cancelAgentTask,
  getAgent,
  getAgentGovernance,
  preflightAgent,
  type AgentGovernanceResponse,
  type AgentPreflightReport,
  listAgentFiles,
  uploadAgentFile,
  listMemory,
  listRoutines,
  updateRoutine,
  triggerRoutine,
  listSessions,
  listVaultKeysForUser,
  storeMemory,
  updateAgent,
  updateTaskAcceptance,
  resumeAgentTask,
  retryAgentTask,
} from "@/lib/api";
import { scheduleLabel } from "@/lib/schedule";
import type { AgentApplicationInput } from "@/lib/agent-builder";
import type {
  Agent,
  AgentTask,
  Memory,
  OpencodeSession,
  Routine,
  TaskAcceptanceCheck,
  TaskAttempts,
  TaskArtifact,
  VaultKeyEntry,
  WorkspaceFile,
} from "@/lib/types";

function cnEval(allPass: boolean, status: string): string {
  const base = "rounded-full px-2 py-0.5 text-[11px] font-medium";
  if (status === "running") return `${base} bg-muted text-muted-foreground`;
  if (status === "failed") return `${base} bg-destructive/10 text-destructive`;
  return allPass
    ? `${base} bg-emerald-500/10 text-emerald-600`
    : `${base} bg-amber-500/10 text-amber-600`;
}

function timeAgo(ms: number): string {
  const diff = Date.now() - ms;
  const mins = Math.floor(diff / 60000);
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  return `${Math.floor(hrs / 24)}d ago`;
}

function isAlwaysOn(memory: Memory): boolean {
  return memory.always_on === true || memory.always_on === 1;
}

function formatMemoryDate(ms: number): string {
  try {
    return new Intl.DateTimeFormat(undefined, {
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    }).format(new Date(ms));
  } catch {
    return timeAgo(ms);
  }
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  for (const unit of units) {
    if (value < 1024 || unit === units[units.length - 1]) {
      return `${value.toFixed(value >= 10 ? 0 : 1)} ${unit}`;
    }
    value /= 1024;
  }
  return `${bytes} B`;
}

function formatGrantExpiry(expiresAt?: number | null): string {
  if (!expiresAt) return "长期有效";
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(expiresAt));
}

function grantExpiryMillis(value: string): number | undefined {
  if (!value) return undefined;
  const timestamp = Date.parse(value);
  return Number.isFinite(timestamp) ? timestamp : undefined;
}

function runtimeFromAgent(agent: Agent): string {
  const config = agent.config;
  if (config && typeof config === "object" && !Array.isArray(config)) {
    const runtime = (config as { runtime?: unknown }).runtime;
    if (typeof runtime === "string" && runtime.trim()) return runtime;
  }
  if (typeof agent.harness === "string" && agent.harness.trim()) return agent.harness;
  return "claude_managed_agents";
}

function vaultUserFromAgent(agent: Agent): string {
  return agent.owner_id?.trim() || DEFAULT_VAULT_USER;
}

function vaultKeysFromAgent(agent: Agent | null): string[] {
  return Array.isArray(agent?.vault_keys)
    ? agent.vault_keys.filter((key): key is string => typeof key === "string")
    : [];
}

function configEntryLabels(agent: Agent, key: "tools" | "mcp_servers"): string[] {
  const values = agent.config?.[key];
  if (!Array.isArray(values)) return [];
  return [...new Set(values.flatMap((value) => {
    if (typeof value === "string" && value.trim()) return [value];
    if (!value || typeof value !== "object" || Array.isArray(value)) return [];
    const record = value as Record<string, unknown>;
    const label = record.name ?? record.id ?? record.type ?? record.mcp_server_name;
    return typeof label === "string" && label.trim() ? [label] : [];
  }))];
}

function fileNameFromPath(filePath: string): string {
  return filePath.split("/").filter(Boolean).at(-1) || "agent-file";
}

function artifactText(artifact: TaskArtifact): string | null {
  const text =
    artifact.content_json && typeof artifact.content_json === "object" && !Array.isArray(artifact.content_json)
      ? (artifact.content_json as Record<string, unknown>).text
      : null;
  return typeof text === "string" && text.trim() ? text : null;
}

function taskInputsFromAgent(agent: Agent): AgentApplicationInput[] {
  const declared = applicationContractFromAgent(agent)?.inputs ?? [];
  const seen = new Set<string>();
  const inputs = declared.filter((input) => {
    const key = input.type.trim();
    if (!key || seen.has(key)) return false;
    seen.add(key);
    return true;
  });
  return inputs.length > 0
    ? inputs
    : [{ type: "request", source: "user", description: "Describe the task to complete." }];
}

function executionTaskInputs(agent: Agent): AgentApplicationInput[] {
  const inputs = taskInputsFromAgent(agent);
  if (runtimeFromAgent(agent) !== "cursor" || inputs.some((input) => input.type === "repository")) {
    return inputs;
  }
  return [
    ...inputs,
    {
      type: "repository",
      source: "Cursor runtime",
      description: "Repository URL or owner/name required by the Cursor runtime.",
    },
  ];
}

function taskPrompt(agent: Agent, input: Record<string, string>): string {
  const objective = applicationContractFromAgent(agent)?.objective ?? agent.description ?? agent.name;
  return `Task objective: ${objective}\n\nSupplied inputs:\n${JSON.stringify(input, null, 2)}`;
}

type MemoryFilter = "all" | "always" | "standard";
type TaskStatusFilter = "all" | "active" | "failed";

const MAX_VISIBLE_SESSIONS = 30;

const SESSION_STATUS_BADGES: Record<string, { label: string; className: string }> = {
  idle: { label: "空闲", className: "bg-muted text-muted-foreground" },
  starting: { label: "启动中", className: "bg-sky-500/10 text-sky-700 dark:text-sky-400" },
  running: { label: "运行中", className: "bg-sky-500/10 text-sky-700 dark:text-sky-400" },
  busy: { label: "运行中", className: "bg-sky-500/10 text-sky-700 dark:text-sky-400" },
  completed: { label: "已完成", className: "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400" },
  cancelled: { label: "已取消", className: "bg-muted text-muted-foreground" },
  timed_out: { label: "超时", className: "bg-destructive/10 text-destructive" },
  failed: { label: "失败", className: "bg-destructive/10 text-destructive" },
  error: { label: "错误", className: "bg-destructive/10 text-destructive" },
};

const TERMINAL_TASK_STATUSES = ["succeeded", "failed", "cancelled"];

const DASHBOARD_SECTIONS: Array<{ id: AgentDashboardSection; label: string }> = [
  { id: "overview", label: "总览" },
  { id: "setup", label: "配置与资源" },
  { id: "runs", label: "运行" },
  { id: "quality", label: "质量" },
  { id: "governance", label: "治理" },
];

function AgentDetail() {
  const router = useRouter();
  const confirmAction = useConfirm();
  const searchParams = useSearchParams();
  const id = decodeURIComponent(searchParams.get("id") ?? "");

  const [agent, setAgent] = useState<Agent | null>(null);
  const [activeSection, setActiveSection] = useState<AgentDashboardSection>("overview");
  const [sessions, setSessions] = useState<OpencodeSession[]>([]);
  const [tasks, setTasks] = useState<AgentTask[]>([]);
  const [taskFilter, setTaskFilter] = useState<TaskStatusFilter>("all");
  const visibleTasks = useMemo(() => {
    if (taskFilter === "active") {
      return tasks.filter((task) => !TERMINAL_TASK_STATUSES.includes(task.status));
    }
    if (taskFilter === "failed") {
      return tasks.filter((task) => task.status === "failed");
    }
    return tasks;
  }, [tasks, taskFilter]);
  const [taskLauncherOpen, setTaskLauncherOpen] = useState(false);
  const [taskInputValues, setTaskInputValues] = useState<Record<string, string>>({});
  const [taskStarting, setTaskStarting] = useState(false);
  const [taskStartError, setTaskStartError] = useState<string | null>(null);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [taskArtifacts, setTaskArtifacts] = useState<TaskArtifact[]>([]);
  const [dashboardArtifacts, setDashboardArtifacts] = useState<TaskArtifact[]>([]);
  const [dashboardLoading, setDashboardLoading] = useState(false);
  const [dashboardRefreshKey, setDashboardRefreshKey] = useState(0);
  const [taskChecks, setTaskChecks] = useState<TaskAcceptanceCheck[]>([]);
  const [taskAttempts, setTaskAttempts] = useState<TaskAttempts>({
    sessions: [],
    runs: [],
    artifacts: [],
    acceptance_checks: [],
    max_attempts: 3,
  });
  const [taskDetailLoading, setTaskDetailLoading] = useState(false);
  const [acceptanceBusy, setAcceptanceBusy] = useState(false);
  const [acceptanceEvidence, setAcceptanceEvidence] = useState("");
  const [taskDetailError, setTaskDetailError] = useState<string | null>(null);
  const [resumeInputValues, setResumeInputValues] = useState<Record<string, string>>({});
  const [taskResuming, setTaskResuming] = useState(false);
  const [taskRetrying, setTaskRetrying] = useState(false);
  const [taskCancelling, setTaskCancelling] = useState<string | null>(null);
  const [routines, setRoutines] = useState<Routine[]>([]);
  const [preflightReport, setPreflightReport] = useState<AgentPreflightReport | null>(null);
  const [governance, setGovernance] = useState<AgentGovernanceResponse | null>(null);
  const [files, setFiles] = useState<WorkspaceFile[]>([]);
  const [uploadingFiles, setUploadingFiles] = useState(false);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const [evalRuns, setEvalRuns] = useState<EvalRun[]>([]);
  const [evalStarting, setEvalStarting] = useState(false);
  const [evalError, setEvalError] = useState<string | null>(null);
  const [proposing, setProposing] = useState(false);
  const [proposalNotice, setProposalNotice] = useState<string | null>(null);
  const [grants, setGrants] = useState<AgentGrant[]>([]);
  const [groupGrants, setGroupGrants] = useState<AgentGroupGrant[]>([]);
  const [grantableUsers, setGrantableUsers] = useState<ManagedUser[]>([]);
  const [grantableGroups, setGrantableGroups] = useState<ManagedGroup[]>([]);
  const [grantUserQuery, setGrantUserQuery] = useState("");
  const [grantGroupQuery, setGrantGroupQuery] = useState("");
  const [grantPermission, setGrantPermission] = useState("use");
  const [grantExpiry, setGrantExpiry] = useState("");
  const [selectedGrantUsers, setSelectedGrantUsers] = useState<Set<string>>(new Set());
  const [selectedGrantGroups, setSelectedGrantGroups] = useState<Set<string>>(new Set());
  const [grantBusy, setGrantBusy] = useState(false);
  const [grantError, setGrantError] = useState<string | null>(null);
  const [grantsVisible, setGrantsVisible] = useState(true);
  const [filesLoading, setFilesLoading] = useState(false);
  const [fileQuery, setFileQuery] = useState("");
  const [downloadingPath, setDownloadingPath] = useState<string | null>(null);
  const [memories, setMemories] = useState<Memory[]>([]);
  const [storedKeyEntries, setStoredKeyEntries] = useState<VaultKeyEntry[]>([]);
  const [vaultUserId, setVaultUserId] = useState(DEFAULT_VAULT_USER);
  const [memoryLoading, setMemoryLoading] = useState(false);
  const [memoryQuery, setMemoryQuery] = useState("");
  const [memoryFilter, setMemoryFilter] = useState<MemoryFilter>("all");
  const [selectedKeys, setSelectedKeys] = useState<Set<string>>(new Set());
  const [newMemory, setNewMemory] = useState({ key: "", value: "", alwaysOn: false });
  const [editingKey, setEditingKey] = useState<string | null>(null);
  const [editDraft, setEditDraft] = useState({ key: "", value: "", alwaysOn: false });
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const loadMemories = async (agentId = id) => {
    if (!agentId) return;
    setMemoryLoading(true);
    try {
      const rows = await listMemory(agentId);
      setMemories(rows);
      setSelectedKeys((prev) => new Set([...prev].filter((key) => rows.some((m) => m.key === key))));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setMemoryLoading(false);
    }
  };

  const loadFiles = async (agentId = id) => {
    if (!agentId) return;
    setFilesLoading(true);
    try {
      setFiles(await listAgentFiles(agentId));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setFilesLoading(false);
    }
  };

  const openTaskDetails = async (task: AgentTask) => {
    const taskId = task.id;
    if (selectedTaskId === taskId) {
      setSelectedTaskId(null);
      return;
    }
    setSelectedTaskId(taskId);
    setTaskDetailLoading(true);
    setTaskDetailError(null);
    setTaskAttempts({ sessions: [], runs: [], artifacts: [], acceptance_checks: [], max_attempts: 3 });
    setAcceptanceEvidence("");
    setResumeInputValues(
      Object.fromEntries(
        agent
          ? executionTaskInputs(agent).map((input) => {
              const value = task.input_json[input.type];
              return [input.type, typeof value === "string" ? value : ""];
            })
          : [],
      ),
    );
    try {
      const [artifacts, checks, attempts] = await Promise.all([
        listTaskArtifacts(id, taskId),
        listTaskAcceptance(id, taskId),
        listTaskAttempts(id, taskId),
      ]);
      setTaskArtifacts(artifacts);
      setTaskChecks(checks);
      setTaskAttempts(attempts);
    } catch (e) {
      setTaskDetailError(e instanceof Error ? e.message : String(e));
    } finally {
      setTaskDetailLoading(false);
    }
  };

  const recordTaskAcceptance = async (
    task: AgentTask,
    check: TaskAcceptanceCheck | null,
    verdict: "passed" | "failed",
  ) => {
    const evidence = acceptanceEvidence.trim();
    if (!evidence) {
      setTaskDetailError("请填写验收证据，再记录通过或失败。 ");
      return;
    }
    setAcceptanceBusy(true);
    setTaskDetailError(null);
    try {
      const result = await updateTaskAcceptance(id, task.id, {
        criterion_index: check?.criterion_index ?? 0,
        criterion: check ? undefined : "人工确认交付结果满足任务要求",
        verdict,
        evidence,
      });
      setTasks((current) => current.map((item) => (item.id === task.id ? result.task : item)));
      setTaskChecks(result.checks);
      setAcceptanceEvidence("");
    } catch (e) {
      setTaskDetailError(e instanceof Error ? e.message : String(e));
    } finally {
      setAcceptanceBusy(false);
    }
  };

  useEffect(() => {
    if (!id) return;
    (async () => {
      try {
        const ag = await getAgent(id);
        const owner = vaultUserFromAgent(ag);
        const [allSessions, memoryRows, fileRows, keyRows, routineRows, taskRows, report, governanceState] = await Promise.all([
          listSessions(id).catch(() => []),
          listMemory(id).catch(() => []),
          listAgentFiles(id).catch(() => []),
          listVaultKeysForUser(owner).catch(() => []),
          listRoutines(id).catch(() => []),
          listAgentTasks(id).catch(() => []),
          preflightAgent(id).catch(() => null),
          getAgentGovernance(id).catch(() => null),
        ]);
        setVaultUserId(owner);
        setAgent(ag);
        setSessions(allSessions);
        setMemories(memoryRows);
        setFiles(fileRows);
        setStoredKeyEntries(keyRows);
        setRoutines(routineRows);
        setTasks(taskRows);
        setPreflightReport(report);
        setGovernance(governanceState);
        listEvalRuns(id).then(setEvalRuns).catch(() => {});
        listAgentGrants(id)
          .then(setGrants)
          // 非 owner 看不到授权列表（404）——直接隐藏卡片。
          .catch(() => setGrantsVisible(false));
        listAgentGroupGrants(id).then(setGroupGrants).catch(() => {});
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setLoading(false);
      }
    })();
  }, [id]);

  // Live refresh: while any task or session is still in a non-terminal state,
  // re-fetch both every few seconds so the 运行 panel reflects reality without
  // a manual page reload. Stops by itself once everything is terminal.
  const hasActiveWork =
    tasks.some((task) => !["succeeded", "failed", "cancelled"].includes(task.status)) ||
    sessions.some(
      (s) => s.status && !["idle", "completed", "cancelled", "failed", "error", "timed_out"].includes(s.status),
    );
  useEffect(() => {
    if (!id || !hasActiveWork) return;
    const timer = window.setInterval(() => {
      listAgentTasks(id).then(setTasks).catch(() => {});
      listSessions(id).then(setSessions).catch(() => {});
    }, 8000);
    return () => window.clearInterval(timer);
  }, [id, hasActiveWork]);

  useEffect(() => {
    const query = grantUserQuery.trim();
    if (query.length < 2) {
      setGrantableUsers([]);
      return;
    }
    const timer = window.setTimeout(() => {
      listGrantableUsers(id, query).then(setGrantableUsers).catch(() => setGrantableUsers([]));
    }, 200);
    return () => window.clearTimeout(timer);
  }, [grantUserQuery, id]);

  useEffect(() => {
    if (activeSection !== "dashboard") return;
    const latestTask = [...tasks].sort((a, b) => b.created_at - a.created_at)[0];
    if (!latestTask) {
      setDashboardArtifacts([]);
      setDashboardLoading(false);
      return;
    }
    let cancelled = false;
    setDashboardLoading(true);
    listTaskArtifacts(id, latestTask.id)
      .then((artifacts) => {
        if (!cancelled) setDashboardArtifacts(artifacts);
      })
      .catch(() => {
        if (!cancelled) setDashboardArtifacts([]);
      })
      .finally(() => {
        if (!cancelled) setDashboardLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [activeSection, dashboardRefreshKey, id, tasks]);

  useEffect(() => {
    const query = grantGroupQuery.trim();
    if (query.length < 2) {
      setGrantableGroups([]);
      return;
    }
    const timer = window.setTimeout(() => {
      listGrantableGroups(id, query).then(setGrantableGroups).catch(() => setGrantableGroups([]));
    }, 200);
    return () => window.clearTimeout(timer);
  }, [grantGroupQuery, id]);

  const visibleFiles = useMemo(() => {
    const q = fileQuery.trim().toLowerCase();
    const rows = q
      ? files.filter((file) => file.path.toLowerCase().includes(q))
      : files;
    return [...rows].sort((a, b) => a.path.localeCompare(b.path));
  }, [files, fileQuery]);

  const visibleMemories = useMemo(() => {
    const q = memoryQuery.trim().toLowerCase();
    return memories
      .filter((memory) => {
        if (memoryFilter === "always" && !isAlwaysOn(memory)) return false;
        if (memoryFilter === "standard" && isAlwaysOn(memory)) return false;
        if (!q) return true;
        return memory.key.toLowerCase().includes(q) || memory.value.toLowerCase().includes(q);
      })
      .sort((a, b) => {
        const pinDiff = Number(isAlwaysOn(b)) - Number(isAlwaysOn(a));
        return pinDiff || b.updated_at - a.updated_at;
      });
  }, [memories, memoryFilter, memoryQuery]);

  const alwaysOnCount = memories.filter(isAlwaysOn).length;
  const selectedMemories = memories.filter((memory) => selectedKeys.has(memory.key));

  const handleDelete = async () => {
    if (!agent) return;
    const ok = await confirmAction({
      title: `删除智能体「${agent.name}」？`,
      description: "其配置、评估历史和工作区文件将一并删除，且无法恢复。",
    });
    if (!ok) return;
    try {
      await deleteAgent(id);
      toast.success(`已删除智能体「${agent.name}」`);
      router.push("/agents/");
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    }
  };

  const runDisabledReason = !agent
    ? null
    : agent.status === "draft"
      ? "草稿智能体需先激活才能运行"
      : agent.status === "paused"
        ? "智能体已暂停（紧急停止或健康检查失败），恢复后才能运行"
        : agent.status === "archived_pending_delete"
          ? "智能体已退役，不能再运行"
          : null;

  const [routineBusy, setRoutineBusy] = useState<string | null>(null);

  const toggleRoutine = async (routine: Routine) => {
    setRoutineBusy(routine.id);
    try {
      const next = await updateRoutine(routine.id, {
        status: routine.status === "active" ? "paused" : "active",
      });
      setRoutines((current) => current.map((item) => (item.id === next.id ? next : item)));
      toast.success(next.status === "active" ? "Routine 已启用" : "Routine 已暂停");
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setRoutineBusy(null);
    }
  };

  const runRoutineNow = async (routine: Routine) => {
    setRoutineBusy(routine.id);
    try {
      await triggerRoutine(routine.id);
      toast.success(`已触发「${routine.name}」，任务列表稍后更新`);
      listAgentTasks(id).then(setTasks).catch(() => {});
      listRoutines(id).then(setRoutines).catch(() => {});
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setRoutineBusy(null);
    }
  };

  const openSessionStart = () => {
    if (!agent || runDisabledReason) return;
    const initial = Object.fromEntries(
      executionTaskInputs(agent).map((input) => [input.type, ""]),
    );
    setTaskInputValues(initial);
    setTaskStartError(null);
    setTaskLauncherOpen(true);
    setActiveSection("runs");
  };

  const startStructuredTask = async () => {
    if (!agent || taskStarting) return;
    const inputs = executionTaskInputs(agent);
    const normalized = Object.fromEntries(
      inputs.map((input) => [input.type, taskInputValues[input.type]?.trim() ?? ""]),
    );
    const missing = inputs.filter((input) => !normalized[input.type]);
    if (missing.length > 0) {
      setTaskStartError(`请填写：${missing.map((input) => input.type).join("、")}`);
      return;
    }
    setTaskStarting(true);
    setTaskStartError(null);
    try {
      const request = normalized.request || Object.values(normalized)[0] || `${agent.name} task`;
      const title = request.length > 60 ? `${request.slice(0, 60)}…` : request;
      const task = await createAgentTask(agent.id, {
        title,
        source: "manual",
        input: normalized,
      });
      const runtime = runtimeFromAgent(agent);
      const environment = runtime === "cursor"
        ? {
            repository: normalized.repository ?? "",
            ref: normalized.ref || "main",
            target_branch: "agent/{agent_id}/{session_id}",
            auto_create_pr: false,
          }
        : normalized;
      const session = await createSession(`${agent.name}: ${title}`, agent.id, {
        runtime,
        prompt: taskPrompt(agent, normalized),
        environment,
        taskId: task.id,
      });
      setTasks((current) => [task, ...current.filter((item) => item.id !== task.id)]);
      setSessions((current) => [session, ...current.filter((item) => item.id !== session.id)]);
      setTaskLauncherOpen(false);
      router.push(`/chat/?id=${encodeURIComponent(session.id)}`);
    } catch (e) {
      setTaskStartError(e instanceof Error ? e.message : String(e));
    } finally {
      setTaskStarting(false);
    }
  };

  const resumeWaitingTask = async (task: AgentTask) => {
    if (!agent || taskResuming) return;
    // Fields left blank fall back to the task's original input — the user
    // only has to fill in what the remote actually asked for, not retype
    // every blueprint field.
    const normalized = Object.fromEntries(
      executionTaskInputs(agent).map((input) => {
        const edited = resumeInputValues[input.type]?.trim() ?? "";
        const original = task.input_json[input.type];
        return [input.type, edited || (typeof original === "string" ? original : "")];
      }),
    );
    if (Object.values(normalized).every((value) => !value)) {
      setTaskDetailError("请至少填写一项补充输入。");
      return;
    }
    setTaskResuming(true);
    setTaskDetailError(null);
    try {
      const result = await resumeAgentTask(id, task.id, normalized);
      setTasks((current) => current.map((item) => (item.id === task.id ? result.task : item)));
      router.push(`/chat/?id=${encodeURIComponent(result.session_id)}`);
    } catch (e) {
      setTaskDetailError(e instanceof Error ? e.message : String(e));
    } finally {
      setTaskResuming(false);
    }
  };

  const retryFailedTask = async (task: AgentTask) => {
    if (taskRetrying) return;
    setTaskRetrying(true);
    setTaskDetailError(null);
    try {
      const result = await retryAgentTask(id, task.id);
      setTasks((current) => current.map((item) => (item.id === task.id ? result.task : item)));
      setTaskAttempts((current) => ({
        ...current,
        sessions: [result.session, ...current.sessions.filter((item) => item.id !== result.session.id)],
      }));
      router.push(`/chat/?id=${encodeURIComponent(result.session.id)}`);
    } catch (e) {
      setTaskDetailError(e instanceof Error ? e.message : String(e));
    } finally {
      setTaskRetrying(false);
    }
  };

  const cancelActiveTask = async (task: AgentTask) => {
    if (taskCancelling) return;
    const confirmed = await confirmAction({
      title: `取消任务「${task.title}」？`,
      description: "当前 Attempt 将进入终态，后续迟到的输出不会再改变任务结果。",
      confirmLabel: "取消任务",
      destructive: true,
    });
    if (!confirmed) return;
    setTaskCancelling(task.id);
    setTaskDetailError(null);
    try {
      const result = await cancelAgentTask(id, task.id);
      setTasks((current) => current.map((item) => (item.id === task.id ? result.task : item)));
      if (result.session_id) {
        setSessions((current) => current.map((session) => (
          session.id === result.session_id ? { ...session, status: "cancelled" } : session
        )));
        setTaskAttempts((current) => ({
          ...current,
          sessions: current.sessions.map((attempt) => (
            attempt.id === result.session_id ? { ...attempt, status: "cancelled" } : attempt
          )),
        }));
      }
      toast.success(
        result.interruption === "provider_interrupted"
          ? "任务已取消，并已向 Runtime 发送中断"
          : result.interruption === "sandbox_terminated"
            ? "任务已取消，执行沙箱已终止"
          : result.interruption === "cooperative"
            ? "任务已取消；底层执行可能仍在收尾"
            : "任务已取消",
      );
    } catch (e) {
      setTaskDetailError(e instanceof Error ? e.message : String(e));
    } finally {
      setTaskCancelling(null);
    }
  };

  const handleDownloadFile = async (file: WorkspaceFile) => {
    setDownloadingPath(file.path);
    try {
      const url = await agentFileDownloadUrl(id, file.path);
      // Anchor click instead of window.open: the presign round-trip consumes
      // the user-gesture window and window.open would be popup-blocked.
      const a = document.createElement("a");
      a.href = url;
      a.download = fileNameFromPath(file.path);
      a.target = "_blank";
      a.rel = "noopener noreferrer";
      document.body.appendChild(a);
      a.click();
      a.remove();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setDownloadingPath(null);
    }
  };

  const handleUploadFiles = async (fileList: FileList | null) => {
    if (!fileList || fileList.length === 0 || !id) return;
    setUploadingFiles(true);
    try {
      for (const file of Array.from(fileList)) {
        await uploadAgentFile(id, file, file.name);
      }
      await loadFiles();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setUploadingFiles(false);
      if (fileInputRef.current) fileInputRef.current.value = "";
    }
  };

  const handleDeleteFile = async (file: WorkspaceFile) => {
    const ok = await confirmAction({
      title: `删除文件 "${file.path}"？`,
      description: "该文件将从智能体工作区中永久删除。",
    });
    if (!ok) return;
    setDownloadingPath(file.path);
    try {
      await deleteAgentFile(id, file.path);
      await loadFiles();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setDownloadingPath(null);
    }
  };

  const runEval = async () => {
    if (!id || evalStarting) return;
    setEvalStarting(true);
    setEvalError(null);
    try {
      await startEvalRun(id);
      setEvalRuns(await listEvalRuns(id));
      // Refresh a few times while the background run completes.
      for (const delay of [4000, 8000, 15000, 30000]) {
        window.setTimeout(() => {
          listEvalRuns(id).then(setEvalRuns).catch(() => {});
        }, delay);
      }
    } catch (e) {
      setEvalError(e instanceof Error ? e.message : String(e));
    } finally {
      setEvalStarting(false);
    }
  };

  const autoEvolve = Boolean(
    agent?.config &&
      typeof agent.config === "object" &&
      (agent.config as { design?: { auto_evolve?: boolean } }).design?.auto_evolve,
  );

  const toggleAutoEvolve = async (enabled: boolean) => {
    if (!agent) return;
    const config = (agent.config && typeof agent.config === "object" ? agent.config : {}) as Record<
      string,
      unknown
    >;
    const design = (config.design && typeof config.design === "object" ? config.design : {}) as Record<
      string,
      unknown
    >;
    try {
      const updated = await updateAgent(id, {
        config: { ...config, design: { ...design, auto_evolve: enabled } },
      });
      setAgent(updated);
    } catch (e) {
      setEvalError(e instanceof Error ? e.message : String(e));
    }
  };

  const addGrant = async () => {
    const users = [...selectedGrantUsers];
    if (users.length === 0 || grantBusy) return;
    setGrantBusy(true);
    setGrantError(null);
    try {
      const expiry = grantExpiryMillis(grantExpiry);
      if (users.length === 1) {
        await createAgentGrant(id, users[0], grantPermission, expiry);
      } else {
        await createAgentGrantsBatch(id, users, grantPermission, expiry);
      }
      setGrants(await listAgentGrants(id));
      setSelectedGrantUsers(new Set());
      setGrantUserQuery("");
    } catch (e) {
      setGrantError(e instanceof Error ? e.message : String(e));
    } finally {
      setGrantBusy(false);
    }
  };

  const removeGrant = async (granteeUserId: string) => {
    setGrantBusy(true);
    setGrantError(null);
    try {
      await deleteAgentGrant(id, granteeUserId);
      setGrants(await listAgentGrants(id));
    } catch (e) {
      setGrantError(e instanceof Error ? e.message : String(e));
    } finally {
      setGrantBusy(false);
    }
  };

  const addGroupGrant = async () => {
    const groups = [...selectedGrantGroups];
    if (groups.length === 0 || grantBusy) return;
    setGrantBusy(true);
    setGrantError(null);
    try {
      const expiry = grantExpiryMillis(grantExpiry);
      if (groups.length === 1) {
        await createAgentGroupGrant(id, groups[0], grantPermission, expiry);
      } else {
        await createAgentGroupGrantsBatch(id, groups, grantPermission, expiry);
      }
      setGroupGrants(await listAgentGroupGrants(id));
      setSelectedGrantGroups(new Set());
      setGrantGroupQuery("");
    } catch (e) {
      setGrantError(e instanceof Error ? e.message : String(e));
    } finally {
      setGrantBusy(false);
    }
  };

  const removeGroupGrant = async (groupId: string) => {
    setGrantBusy(true);
    setGrantError(null);
    try {
      await deleteAgentGroupGrant(id, groupId);
      setGroupGrants(await listAgentGroupGrants(id));
    } catch (e) {
      setGrantError(e instanceof Error ? e.message : String(e));
    } finally {
      setGrantBusy(false);
    }
  };

  const proposeImprovement = async () => {
    if (!id || proposing) return;
    setProposing(true);
    setEvalError(null);
    setProposalNotice(null);
    try {
      await createImprovementProposal(id);
      setProposalNotice("改进提案已生成，进入收件箱等待审批。批准后会自动应用并回归评估。");
    } catch (e) {
      setEvalError(e instanceof Error ? e.message : String(e));
    } finally {
      setProposing(false);
    }
  };

  const updateVaultKeys = async (vaultKeys: string[]) => {
    if (!agent) return;
    const updated = await updateAgent(id, { vault_keys: vaultKeys });
    setAgent(updated);
  };

  const toggleSelected = (key: string) => {
    setSelectedKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const beginEditMemory = (memory: Memory) => {
    setEditingKey(memory.key);
    setEditDraft({ key: memory.key, value: memory.value, alwaysOn: isAlwaysOn(memory) });
  };

  const saveMemoryDraft = async () => {
    if (!editingKey) return;
    const key = editDraft.key.trim();
    const value = editDraft.value.trim();
    if (!key || !value) return;
    try {
      const updated = await storeMemory(id, key, editDraft.value, editDraft.alwaysOn);
      if (key !== editingKey) await deleteMemory(id, editingKey);
      setMemories((prev) => {
        const withoutOld = prev.filter((m) => m.key !== editingKey && m.key !== key);
        return [updated, ...withoutOld];
      });
      setSelectedKeys((prev) => {
        const next = new Set(prev);
        if (next.delete(editingKey)) next.add(key);
        return next;
      });
      setEditingKey(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const addMemory = async () => {
    const key = newMemory.key.trim();
    const value = newMemory.value.trim();
    if (!key || !value) return;
    try {
      const row = await storeMemory(id, key, newMemory.value, newMemory.alwaysOn);
      setMemories((prev) => [row, ...prev.filter((m) => m.key !== row.key)]);
      setNewMemory({ key: "", value: "", alwaysOn: false });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const setMemoryAlwaysOn = async (memory: Memory, alwaysOn: boolean) => {
    try {
      const row = await storeMemory(id, memory.key, memory.value, alwaysOn);
      setMemories((prev) => prev.map((m) => (m.key === row.key ? row : m)));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const deleteMemoryRow = async (key: string) => {
    setMemories((prev) => prev.filter((m) => m.key !== key));
    setSelectedKeys((prev) => {
      const next = new Set(prev);
      next.delete(key);
      return next;
    });
    try {
      await deleteMemory(id, key);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      await loadMemories();
    }
  };

  const bulkSetAlwaysOn = async (alwaysOn: boolean) => {
    if (selectedMemories.length === 0) return;
    try {
      const updated = await Promise.all(
        selectedMemories.map((memory) => storeMemory(id, memory.key, memory.value, alwaysOn)),
      );
      setMemories((prev) =>
        prev.map((memory) => updated.find((row) => row.key === memory.key) ?? memory),
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const bulkDelete = async () => {
    const keys = [...selectedKeys];
    if (keys.length === 0) return;
    const ok = await confirmAction({
      title: `删除选中的 ${keys.length} 条记忆？`,
    });
    if (!ok) return;
    setMemories((prev) => prev.filter((memory) => !selectedKeys.has(memory.key)));
    setSelectedKeys(new Set());
    try {
      await Promise.all(keys.map((key) => deleteMemory(id, key)));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      await loadMemories();
    }
  };

  return (
    <div className="flex h-screen bg-background text-foreground">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border px-4">
          <div className="flex items-center gap-2">
            <Button
              size="sm"
              variant="ghost"
              onClick={() => router.push("/agents/")}
              className="gap-1.5 text-muted-foreground hover:text-foreground"
            >
              <ArrowLeft className="size-3.5" />
              Agents
            </Button>
            {agent && (
              <>
                <span className="text-muted-foreground">/</span>
                <span className="max-w-[240px] truncate text-sm font-semibold">{agent.name}</span>
              </>
            )}
          </div>
          <div className="flex items-center gap-2">
            {agent && (
              <>
                <Button
                  size="sm"
                  variant="default"
                  onClick={openSessionStart}
                  disabled={runDisabledReason != null}
                  title={runDisabledReason ?? undefined}
                >
                  <Play className="size-3.5" />
                  Run
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => router.push(`/agents/edit/?id=${encodeURIComponent(id)}`)}
                >
                  <Pencil className="size-3.5" />
                  Edit
                </Button>
                <Button size="sm" variant="outline" onClick={handleDelete} aria-label="删除">
                  <Trash2 className="size-3.5" />
                </Button>
              </>
            )}
            <ThemeToggle />
          </div>
        </header>

        <main className="flex-1 overflow-y-auto">
          <div className="mx-auto flex max-w-6xl flex-col gap-6 px-4 py-6">
            {error && (
              <Card className="border-destructive p-3">
                <p className="text-sm text-destructive">{error}</p>
              </Card>
            )}
            {loading && <div className="text-sm text-muted-foreground">加载中...</div>}

            {agent && (
              <>
                <div className="flex flex-col gap-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <h1 className="text-xl font-semibold">{agent.name}</h1>
                    {agent.model && (
                      <span className="rounded bg-muted px-2 py-0.5 font-mono text-xs text-muted-foreground">
                        {String(agent.model)}
                      </span>
                    )}
                  </div>
                  {agent.description && (
                    <p className="text-sm text-muted-foreground">{agent.description}</p>
                  )}
                  {agent.created_at && (
                    <p className="mt-1 flex items-center gap-1 text-xs text-muted-foreground/60">
                      <Clock className="size-3" />
                      Created {timeAgo(Number(agent.created_at) * 1000)}
                    </p>
                  )}
                </div>

                <nav className="flex gap-1 overflow-x-auto border-b border-border" aria-label="智能体应用视图">
                  {[
                    ...DASHBOARD_SECTIONS.slice(0, 1),
                    ...(applicationContractFromAgent(agent)?.dashboard &&
                    applicationContractFromAgent(agent)?.outputs.some(
                      (output) => output.type === "interactive_dashboard",
                    )
                      ? [{ id: "dashboard" as const, label: "大屏应用" }]
                      : []),
                    ...DASHBOARD_SECTIONS.slice(1),
                  ].map((section) => (
                    <button
                      key={section.id}
                      type="button"
                      onClick={() => setActiveSection(section.id)}
                      className={`shrink-0 border-b-2 px-3 py-2 text-sm transition-colors ${
                        activeSection === section.id
                          ? "border-foreground font-medium text-foreground"
                          : "border-transparent text-muted-foreground hover:text-foreground"
                      }`}
                    >
                      {section.label}
                    </button>
                  ))}
                </nav>

                {activeSection === "overview" && (
                  <AgentApplicationOverview
                    agent={agent}
                    runtime={runtimeFromAgent(agent)}
                    sessions={sessions}
                    tasks={tasks}
                    routines={routines}
                    evalRuns={evalRuns}
                    filesCount={files.length}
                    memoryCount={memories.length}
                    alwaysOnCount={alwaysOnCount}
                    credentialCount={vaultKeysFromAgent(agent).length}
                    grantCount={grants.length + groupGrants.length}
                    preflightReport={preflightReport}
                    onSelectSection={setActiveSection}
                  />
                )}

                {activeSection === "overview" && agent.status === "draft" && !governance && (
                  <DraftPreflightPanel
                    agentId={agent.id}
                    initialReport={preflightReport}
                    onReport={setPreflightReport}
                    onActivated={() => setAgent({ ...agent, status: "active" })}
                  />
                )}

                {activeSection === "dashboard" && applicationContractFromAgent(agent)?.dashboard && (
                  <AgentInteractiveDashboard
                    definition={applicationContractFromAgent(agent)!.dashboard!}
                    artifacts={dashboardArtifacts}
                    loading={dashboardLoading}
                    onRefresh={() => setDashboardRefreshKey((value) => value + 1)}
                  />
                )}

                {activeSection === "setup" && (
                <>
                <section>
                  <h2 className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                    配置
                  </h2>
                  <Card className="p-4">
                    <dl className="grid gap-x-6 gap-y-3 text-sm sm:grid-cols-[140px_1fr]">
                      <dt className="font-medium text-muted-foreground">ID</dt>
                      <dd className="break-all font-mono text-xs text-muted-foreground">{agent.id}</dd>

                      {agent.model && (
                        <>
                          <dt className="font-medium text-muted-foreground">模型</dt>
                          <dd className="font-mono text-xs">{String(agent.model)}</dd>
                        </>
                      )}

                      {agent.owner_id && (
                        <>
                          <dt className="font-medium text-muted-foreground">属主</dt>
                          <dd className="font-mono text-xs">{String(agent.owner_id)}</dd>
                        </>
                      )}

                      <dt className="font-medium text-muted-foreground">默认运行时</dt>
                      <dd className="font-mono text-xs">{runtimeFromAgent(agent)}</dd>

                      <dt className="font-medium text-muted-foreground">运行计划</dt>
                      <dd className="flex flex-col gap-1">
                        <span className="font-mono text-xs">
                          {scheduleLabel(agent.cron, agent.timezone)}
                        </span>
                        {agent.cron && (
                          <span className="font-mono text-[11px] text-muted-foreground">
                            {String(agent.cron)}
                          </span>
                        )}
                      </dd>

                      <dt className="font-medium text-muted-foreground">工具</dt>
                      <dd className="flex flex-wrap gap-1.5">
                        {configEntryLabels(agent, "tools").length > 0 ? (
                          configEntryLabels(agent, "tools").map((tool) => (
                            <Badge key={tool} variant="secondary" className="font-mono text-[11px]">
                              {tool}
                            </Badge>
                          ))
                        ) : (
                          <span className="text-xs text-muted-foreground">未显式配置</span>
                        )}
                      </dd>

                      <dt className="font-medium text-muted-foreground">MCP 集成</dt>
                      <dd className="flex flex-wrap gap-1.5">
                        {configEntryLabels(agent, "mcp_servers").length > 0 ? (
                          configEntryLabels(agent, "mcp_servers").map((server) => (
                            <Badge key={server} variant="outline" className="font-mono text-[11px]">
                              {server}
                            </Badge>
                          ))
                        ) : (
                          <span className="text-xs text-muted-foreground">无</span>
                        )}
                      </dd>

                      {agent.prompt && (
                        <>
                          <dt className="pt-1 font-medium text-muted-foreground">System prompt</dt>
                          <dd>
                            <pre className="max-h-48 overflow-y-auto whitespace-pre-wrap break-words rounded-md bg-muted/40 px-3 py-2 font-mono text-[11px] leading-relaxed text-foreground">
                              {String(agent.prompt)}
                            </pre>
                          </dd>
                        </>
                      )}
                      {!agent.prompt && agent.system && (
                        <>
                          <dt className="pt-1 font-medium text-muted-foreground">System prompt</dt>
                          <dd>
                            <pre className="max-h-48 overflow-y-auto whitespace-pre-wrap break-words rounded-md bg-muted/40 px-3 py-2 font-mono text-[11px] leading-relaxed text-foreground">
                              {String(agent.system)}
                            </pre>
                          </dd>
                        </>
                      )}
                    </dl>
                  </Card>
                </section>

                <VaultCredentialsEditor
                  vaultKeys={vaultKeysFromAgent(agent)}
                  storedKeyEntries={storedKeyEntries}
                  vaultUserId={vaultUserId}
                  onVaultKeysChange={updateVaultKeys}
                  onStoredKeyEntriesChange={(updater) => setStoredKeyEntries(updater)}
                />
                {agent.status === "draft" && !governance && (
                  <DraftPreflightPanel
                    agentId={agent.id}
                    initialReport={preflightReport}
                    onReport={setPreflightReport}
                    onActivated={() => setAgent({ ...agent, status: "active" })}
                  />
                )}
                </>
                )}

                {activeSection === "governance" && governance && (
                  <ManagedGovernancePanel
                    response={governance}
                    agentStatus={agent.status ?? "draft"}
                    grantsCount={grants.length}
                    onChange={setGovernance}
                    onAgentChange={setAgent}
                    onReport={setPreflightReport}
                  />
                )}

                {activeSection === "governance" && grantsVisible && (
                <section>
                  <div className="mb-2">
                    <h2 className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                      <KeyRound className="size-3.5" />
                      使用授权
                    </h2>
                    <p className="mt-1 text-xs text-muted-foreground">
                      把该智能体授权给其他用户：use 可见并可开会话；edit 还可修改配置与工作区。
                    </p>
                  </div>
                  {grantError && (
                    <p className="mb-2 rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive">{grantError}</p>
                  )}
                  <Card className="overflow-hidden">
                    <div className="flex flex-wrap items-center gap-2 border-b border-border px-3 py-2.5">
                      <Input
                        value={grantUserQuery}
                        onChange={(e) => {
                          setGrantUserQuery(e.target.value);
                        }}
                        placeholder="搜索姓名、邮箱或用户 ID"
                        className="h-8 max-w-[200px] text-xs"
                      />
                      <select
                        value={grantPermission}
                        onChange={(e) => setGrantPermission(e.target.value)}
                        className="h-8 rounded-md border border-input bg-transparent px-2 text-xs"
                      >
                        <option value="use">use（使用）</option>
                        <option value="edit">edit（可修改）</option>
                      </select>
                      <Input
                        value={grantExpiry}
                        onChange={(e) => setGrantExpiry(e.target.value)}
                        type="datetime-local"
                        aria-label="授权到期时间"
                        className="h-8 w-auto text-xs"
                      />
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        className="h-8"
                        onClick={() => void addGrant()}
                        disabled={grantBusy || selectedGrantUsers.size === 0}
                      >
                        <Plus className="size-3.5" />
                        授权所选（{selectedGrantUsers.size}）
                      </Button>
                    </div>
                    {grantableUsers.length > 0 && (
                      <div className="grid gap-1 border-b border-border px-3 py-2 sm:grid-cols-2">
                        {grantableUsers.map((user) => (
                          <label key={user.id} className="flex min-w-0 items-center gap-2 rounded px-1 py-1 text-xs hover:bg-muted/60">
                            <input
                              type="checkbox"
                              checked={selectedGrantUsers.has(user.id)}
                              disabled={user.status !== "active"}
                              onChange={() => setSelectedGrantUsers((current) => {
                                const next = new Set(current);
                                if (next.has(user.id)) next.delete(user.id); else next.add(user.id);
                                return next;
                              })}
                            />
                            <span className="truncate">{user.display_name}（{user.id}）{user.status !== "active" ? " · 已停用" : ""}</span>
                          </label>
                        ))}
                      </div>
                    )}
                    {grants.length === 0 ? (
                      <div className="p-4 text-center text-xs text-muted-foreground">
                        尚未授权给任何用户，仅 owner 与 admin 可见。
                      </div>
                    ) : (
                      <div className="divide-y divide-border">
                        {grants.map((grant) => (
                          <div key={grant.id} className="flex items-center justify-between px-3 py-2">
                            <div className="min-w-0">
                              <span className="text-xs font-medium">{grant.user?.display_name ?? grant.grantee_user_id}</span>
                              <span className="ml-1 font-mono text-[11px] text-muted-foreground">{grant.grantee_user_id}</span>
                              <span className="ml-2 rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground">
                                {grant.permission}
                              </span>
                              <span className="ml-2 text-[11px] text-muted-foreground">直接授予 · {formatGrantExpiry(grant.expires_at)}</span>
                            </div>
                            <Button
                              type="button"
                              size="sm"
                              variant="ghost"
                              className="h-7 w-7 p-0 text-destructive"
                              onClick={() => void removeGrant(grant.grantee_user_id)}
                              disabled={grantBusy}
                              aria-label={`撤销 ${grant.grantee_user_id}`}
                            >
                              <Trash2 className="size-3.5" />
                            </Button>
                          </div>
                        ))}
                      </div>
                    )}
                  </Card>
                </section>
                )}

                {activeSection === "governance" && !grantsVisible && (
                  <Card className="p-5 text-sm text-muted-foreground">
                    当前账号无权查看或修改该智能体的授权策略。
                  </Card>
                )}

                {activeSection === "governance" && grantsVisible && (
                <section>
                  <div className="mb-2">
                    <h2 className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                      <Users className="size-3.5" />
                      用户组授权
                    </h2>
                    <p className="mt-1 text-xs text-muted-foreground">
                      对组内全部启用用户授予 use 或 edit；停用组或移出成员后权限立即失效。
                    </p>
                  </div>
                  <Card className="overflow-hidden">
                    <div className="flex flex-wrap items-center gap-2 border-b border-border px-3 py-2.5">
                      <Input
                        value={grantGroupQuery}
                        onChange={(e) => {
                          setGrantGroupQuery(e.target.value);
                        }}
                        placeholder="搜索用户组"
                        className="h-8 max-w-[180px] text-xs"
                      />
                      <select
                        value={grantPermission}
                        onChange={(e) => setGrantPermission(e.target.value)}
                        className="h-8 rounded-md border border-input bg-transparent px-2 text-xs"
                      >
                        <option value="use">use（使用）</option>
                        <option value="edit">edit（可修改）</option>
                      </select>
                      <Input
                        value={grantExpiry}
                        onChange={(e) => setGrantExpiry(e.target.value)}
                        type="datetime-local"
                        aria-label="授权到期时间"
                        className="h-8 w-auto text-xs"
                      />
                      <Button type="button" size="sm" variant="outline" className="h-8" onClick={() => void addGroupGrant()} disabled={grantBusy || selectedGrantGroups.size === 0}>
                        <Plus className="size-3.5" />
                        授权所选（{selectedGrantGroups.size}）
                      </Button>
                    </div>
                    {grantableGroups.length > 0 && (
                      <div className="grid gap-1 border-b border-border px-3 py-2 sm:grid-cols-2">
                        {grantableGroups.map((group) => (
                          <label key={group.id} className="flex min-w-0 items-center gap-2 rounded px-1 py-1 text-xs hover:bg-muted/60">
                            <input
                              type="checkbox"
                              checked={selectedGrantGroups.has(group.id)}
                              disabled={group.status !== "active"}
                              onChange={() => setSelectedGrantGroups((current) => {
                                const next = new Set(current);
                                if (next.has(group.id)) next.delete(group.id); else next.add(group.id);
                                return next;
                              })}
                            />
                            <span className="truncate">{group.name}{group.status !== "active" ? " · 已停用" : ""}</span>
                          </label>
                        ))}
                      </div>
                    )}
                    {groupGrants.length === 0 ? (
                      <div className="p-4 text-center text-xs text-muted-foreground">尚未授权给任何用户组。</div>
                    ) : (
                      <div className="divide-y divide-border">
                        {groupGrants.map((grant) => (
                          <div key={grant.id} className="flex items-center justify-between px-3 py-2">
                            <div className="min-w-0">
                              <span className="text-xs font-medium">{grant.group?.name ?? grant.group_id}</span>
                              <span className="ml-1 font-mono text-[11px] text-muted-foreground">{grant.group_id}</span>
                              <span className="ml-2 rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground">{grant.permission}</span>
                              <span className="ml-2 text-[11px] text-muted-foreground">
                                通过该组为 {grant.group?.member_count ?? 0} 名成员授予 {grant.permission} · {grant.group?.status === "disabled" ? "组已停用" : formatGrantExpiry(grant.expires_at)}
                              </span>
                            </div>
                            <Button type="button" size="sm" variant="ghost" className="h-7 w-7 p-0 text-destructive" onClick={() => void removeGroupGrant(grant.group_id)} disabled={grantBusy} aria-label={`撤销用户组 ${grant.group_id}`}><Trash2 className="size-3.5" /></Button>
                          </div>
                        ))}
                      </div>
                    )}
                  </Card>
                </section>
                )}

                {activeSection === "quality" && (
                <section>
                  <div className="mb-2 flex items-end justify-between">
                    <div>
                      <h2 className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                        <Check className="size-3.5" />
                        评估运行
                      </h2>
                      <p className="mt-1 text-xs text-muted-foreground">
                        用 design.evaluation 里的用例实测当前配置，结果按版本归档（经验池）。
                      </p>
                    </div>
                    <div className="flex items-center gap-2">
                      <label
                        className="flex cursor-pointer items-center gap-1.5 text-xs text-muted-foreground"
                        title="每天自动运行评估；有失败时自动生成改进提案进收件箱（仍需人工批准）"
                      >
                        <input
                          type="checkbox"
                          checked={autoEvolve}
                          onChange={(e) => void toggleAutoEvolve(e.target.checked)}
                        />
                        自动进化
                      </label>
                      {evalRuns.some((r) => r.status === "completed" && r.passed < r.total) && (
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          className="h-8"
                          onClick={() => void proposeImprovement()}
                          disabled={proposing}
                        >
                          {proposing ? "生成中..." : "生成改进提案"}
                        </Button>
                      )}
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        className="h-8"
                        onClick={() => void runEval()}
                        disabled={evalStarting}
                      >
                        {evalStarting ? "启动中..." : "运行评估"}
                      </Button>
                    </div>
                  </div>
                  {proposalNotice && (
                    <p className="mb-2 rounded-md bg-emerald-500/10 px-3 py-2 text-xs text-emerald-700 dark:text-emerald-400">{proposalNotice}</p>
                  )}
                  {evalError && (
                    <p className="mb-2 rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive">{evalError}</p>
                  )}
                  <Card className="overflow-hidden">
                    {evalRuns.length === 0 ? (
                      <div className="p-6 text-center text-sm text-muted-foreground">
                        暂无评估记录。智能体需要在创建向导的评估步定义用例。
                      </div>
                    ) : (
                      <div className="max-h-[280px] divide-y divide-border overflow-y-auto">
                        {evalRuns.map((run) => (
                          <div key={run.id} className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 px-3 py-2.5">
                            <div className="min-w-0">
                              <p className="text-xs font-medium">
                                {run.status === "running"
                                  ? "运行中..."
                                  : run.status === "failed"
                                    ? `失败：${run.error ?? ""}`
                                    : `${run.passed}/${run.total} 通过`}
                                {run.agent_version != null && (
                                  <span className="ml-2 text-muted-foreground">v{run.agent_version}</span>
                                )}
                              </p>
                              <p className="mt-0.5 truncate text-[11px] text-muted-foreground">
                                {run.model} · {formatMemoryDate(run.created_at)}
                                {run.status === "completed" &&
                                  Array.isArray(run.results) &&
                                  run.results.some((r) => !r.pass) && (
                                    <span className="ml-2">
                                      未通过：{run.results
                                        .filter((r) => !r.pass)
                                        .map((r) => r.category)
                                        .join(", ")}
                                    </span>
                                  )}
                              </p>
                            </div>
                            <span
                              className={cnEval(
                                run.status === "completed" && run.passed === run.total,
                                run.status,
                              )}
                            >
                              {run.status === "completed"
                                ? run.passed === run.total
                                  ? "全部通过"
                                  : "有失败"
                                : run.status}
                            </span>
                          </div>
                        ))}
                      </div>
                    )}
                  </Card>
                </section>
                )}

                {activeSection === "setup" && (
                <section>
                  <div className="mb-2 flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
                    <div>
                      <h2 className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                        <FileText className="size-3.5" />
                        工作区文件
                      </h2>
                      <p className="mt-1 text-xs text-muted-foreground">
                        Knowledge files copied into every new session workspace of this agent.
                      </p>
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="relative w-full sm:w-[260px]">
                        <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
                        <Input
                          value={fileQuery}
                          onChange={(e) => setFileQuery(e.target.value)}
                          placeholder="搜索文件"
                          className="h-8 pl-8 text-xs"
                        />
                      </div>
                      <input
                        ref={fileInputRef}
                        type="file"
                        multiple
                        className="hidden"
                        onChange={(e) => handleUploadFiles(e.target.files)}
                      />
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        className="h-8"
                        onClick={() => fileInputRef.current?.click()}
                        disabled={uploadingFiles}
                      >
                        <Upload className="size-3.5" />
                        <span className="ml-1.5 text-xs">{uploadingFiles ? "Uploading..." : "Upload"}</span>
                      </Button>
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        className="h-8"
                        onClick={() => loadFiles()}
                        disabled={filesLoading}
                      >
                        <RefreshCw className={`size-3.5 ${filesLoading ? "animate-spin" : ""}`} />
                      </Button>
                    </div>
                  </div>

                  <Card className="overflow-hidden">
                    <div className="grid grid-cols-3 border-b border-border bg-muted/20 px-3 py-2 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                      <span>路径</span>
                      <span className="text-right">大小</span>
                      <span className="text-right">操作</span>
                    </div>
                    {filesLoading && files.length === 0 ? (
                      <div className="p-6 text-sm text-muted-foreground">正在加载文件...</div>
                    ) : visibleFiles.length === 0 ? (
                      <div className="p-8 text-center">
                        <FileText className="mx-auto mb-3 size-7 text-muted-foreground/60" />
                        <p className="text-sm font-medium">
                          {files.length === 0 ? "暂无工作区文件" : "没有匹配的文件"}
                        </p>
                        <p className="mt-1 text-xs text-muted-foreground">
                          {files.length === 0
                            ? "上传文件后，每个新会话都能使用它们。"
                            : "调整搜索条件以显示更多文件。"}
                        </p>
                      </div>
                    ) : (
                      <div className="max-h-[360px] divide-y divide-border overflow-y-auto">
                        {visibleFiles.map((file) => (
                          <div
                            key={file.path}
                            className="grid grid-cols-[minmax(0,1fr)_72px_84px] items-center gap-3 px-3 py-2.5"
                          >
                            <div className="min-w-0">
                              <p className="truncate font-mono text-xs" title={file.path}>
                                {file.path}
                              </p>
                              {file.updated_at != null && (
                                <p className="mt-0.5 text-[11px] text-muted-foreground">
                                  Updated {formatMemoryDate(file.updated_at)}
                                </p>
                              )}
                            </div>
                            <span className="text-right font-mono text-xs text-muted-foreground">
                              {formatBytes(file.size_bytes)}
                            </span>
                            <div className="flex justify-end gap-1">
                              <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                className="h-8 w-8 p-0"
                                onClick={() => handleDownloadFile(file)}
                                disabled={downloadingPath === file.path}
                                aria-label={`Download ${file.path}`}
                              >
                                <Download className="size-3.5" />
                              </Button>
                              <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                className="h-8 w-8 p-0"
                                onClick={() => handleDeleteFile(file)}
                                disabled={downloadingPath === file.path}
                                aria-label={`Delete ${file.path}`}
                              >
                                <Trash2 className="size-3.5" />
                              </Button>
                            </div>
                          </div>
                        ))}
                      </div>
                    )}
                  </Card>
                </section>
                )}

                {activeSection === "setup" && (
                <section>
                  <div className="mb-2 flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
                    <div>
                      <h2 className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                        <Brain className="size-3.5" />
                        记忆
                      </h2>
                      <p className="mt-1 text-xs text-muted-foreground">
                        Review what this agent has learned, pin critical notes, and curate stale context.
                      </p>
                    </div>
                    <div className="grid grid-cols-3 overflow-hidden rounded-md border border-border bg-muted/20 text-center sm:w-[300px]">
                      <div className="px-3 py-2">
                        <div className="text-base font-semibold">{memories.length}</div>
                        <div className="text-[11px] uppercase tracking-wide text-muted-foreground">总数</div>
                      </div>
                      <div className="border-x border-border px-3 py-2">
                        <div className="text-base font-semibold">{alwaysOnCount}</div>
                        <div className="text-[11px] uppercase tracking-wide text-muted-foreground">常驻</div>
                      </div>
                      <div className="px-3 py-2">
                        <div className="text-base font-semibold">{selectedKeys.size}</div>
                        <div className="text-[11px] uppercase tracking-wide text-muted-foreground">已选</div>
                      </div>
                    </div>
                  </div>

                  <Card className="overflow-hidden">
                    <div className="border-b border-border p-3">
                      <div className="flex flex-col gap-2 lg:flex-row lg:items-center lg:justify-between">
                        <div className="relative min-w-0 flex-1">
                          <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
                          <Input
                            value={memoryQuery}
                            onChange={(e) => setMemoryQuery(e.target.value)}
                            placeholder="搜索键名或记忆内容"
                            className="h-8 pl-8 text-xs"
                          />
                        </div>
                        <div className="flex flex-wrap items-center gap-1.5">
                          {(["all", "always", "standard"] as MemoryFilter[]).map((filter) => (
                            <Button
                              key={filter}
                              type="button"
                              size="sm"
                              variant={memoryFilter === filter ? "default" : "outline"}
                              className="h-8 capitalize"
                              onClick={() => setMemoryFilter(filter)}
                            >
                              {filter === "always" ? "常驻" : filter}
                            </Button>
                          ))}
                          <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            className="h-8"
                            onClick={() => loadMemories()}
                            disabled={memoryLoading}
                          >
                            <RefreshCw className={`size-3.5 ${memoryLoading ? "animate-spin" : ""}`} />
                          </Button>
                        </div>
                      </div>
                      {selectedKeys.size > 0 && (
                        <div className="mt-3 flex flex-wrap items-center gap-2 rounded-md border border-border bg-muted/30 px-2.5 py-2">
                          <span className="text-xs text-muted-foreground">
                            {selectedKeys.size} selected
                          </span>
                          <Button type="button" size="sm" variant="outline" className="h-7" onClick={() => bulkSetAlwaysOn(true)}>
                            <Pin className="size-3.5" />
                            Always-on
                          </Button>
                          <Button type="button" size="sm" variant="outline" className="h-7" onClick={() => bulkSetAlwaysOn(false)}>
                            <PinOff className="size-3.5" />
                            Standard
                          </Button>
                          <Button type="button" size="sm" variant="outline" className="h-7 text-destructive" onClick={bulkDelete}>
                            <Trash2 className="size-3.5" />
                            Delete
                          </Button>
                          <Button type="button" size="sm" variant="ghost" className="ml-auto h-7" onClick={() => setSelectedKeys(new Set())}>
                            Clear
                          </Button>
                        </div>
                      )}
                    </div>

                    <div className="border-b border-border bg-muted/10 p-3">
                      <div className="grid gap-2 lg:grid-cols-[180px_minmax(0,1fr)_auto]">
                        <Input
                          value={newMemory.key}
                          onChange={(e) => setNewMemory((m) => ({ ...m, key: e.target.value }))}
                          placeholder="memory_key"
                          className="h-9 font-mono text-xs"
                        />
                        <Textarea
                          value={newMemory.value}
                          onChange={(e) => setNewMemory((m) => ({ ...m, value: e.target.value }))}
                          placeholder="为该智能体添加一条持久备注"
                          rows={1}
                          className="min-h-9 resize-none text-xs"
                        />
                        <div className="flex items-center gap-2">
                          <Button
                            type="button"
                            size="sm"
                            variant={newMemory.alwaysOn ? "default" : "outline"}
                            className="h-9"
                            onClick={() => setNewMemory((m) => ({ ...m, alwaysOn: !m.alwaysOn }))}
                          >
                            <Pin className="size-3.5" />
                          </Button>
                          <Button
                            type="button"
                            size="sm"
                            className="h-9"
                            onClick={addMemory}
                            disabled={!newMemory.key.trim() || !newMemory.value.trim()}
                          >
                            <Plus className="size-3.5" />
                            Add
                          </Button>
                        </div>
                      </div>
                    </div>

                    {memoryLoading && memories.length === 0 ? (
                      <div className="p-6 text-sm text-muted-foreground">正在加载记忆...</div>
                    ) : visibleMemories.length === 0 ? (
                      <div className="p-8 text-center">
                        <Brain className="mx-auto mb-3 size-7 text-muted-foreground/60" />
                        <p className="text-sm font-medium">
                          {memories.length === 0 ? "还没有记忆" : "没有匹配的记忆"}
                        </p>
                        <p className="mt-1 text-xs text-muted-foreground">
                          {memories.length === 0
                            ? "智能体会在工作中自行积累记忆，也可以在上方手动添加。"
                            : "调整搜索或筛选条件以显示更多。"}
                        </p>
                      </div>
                    ) : (
                      <div className="divide-y divide-border">
                        {visibleMemories.map((memory) => {
                          const checked = selectedKeys.has(memory.key);
                          const editing = editingKey === memory.key;
                          return (
                            <div key={memory.key} className="grid gap-3 p-3 sm:grid-cols-[auto_minmax(0,1fr)_auto] sm:items-start">
                              <input
                                type="checkbox"
                                checked={checked}
                                onChange={() => toggleSelected(memory.key)}
                                className="mt-1 size-4 rounded border-border bg-background"
                                aria-label={`Select ${memory.key}`}
                              />
                              <div className="min-w-0">
                                {editing ? (
                                  <div className="grid gap-2">
                                    <Input
                                      value={editDraft.key}
                                      onChange={(e) => setEditDraft((d) => ({ ...d, key: e.target.value }))}
                                      className="h-8 font-mono text-xs"
                                    />
                                    <Textarea
                                      value={editDraft.value}
                                      onChange={(e) => setEditDraft((d) => ({ ...d, value: e.target.value }))}
                                      rows={3}
                                      className="text-xs"
                                    />
                                  </div>
                                ) : (
                                  <>
                                    <div className="flex flex-wrap items-center gap-2">
                                      <span className="font-mono text-xs font-medium">{memory.key}</span>
                                      {isAlwaysOn(memory) && (
                                        <Badge variant="secondary" className="gap-1 text-[11px]">
                                          <Pin className="size-3" />
                                          Always-on
                                        </Badge>
                                      )}
                                      <span className="text-[11px] text-muted-foreground">
                                        Updated {formatMemoryDate(memory.updated_at)}
                                      </span>
                                    </div>
                                    <p className="mt-1 whitespace-pre-wrap break-words text-sm leading-relaxed text-muted-foreground">
                                      {memory.value}
                                    </p>
                                  </>
                                )}
                              </div>
                              <div className="flex items-center gap-1 sm:justify-end">
                                {editing ? (
                                  <>
                                    <Button
                                      type="button"
                                      size="sm"
                                      variant={editDraft.alwaysOn ? "default" : "outline"}
                                      className="h-8"
                                      onClick={() => setEditDraft((d) => ({ ...d, alwaysOn: !d.alwaysOn }))}
                                      aria-label="切换常驻"
                                    >
                                      <Pin className="size-3.5" />
                                    </Button>
                                    <Button type="button" size="sm" className="h-8" onClick={saveMemoryDraft}>
                                      <Check className="size-3.5" />
                                    </Button>
                                    <Button type="button" size="sm" variant="outline" className="h-8" onClick={() => setEditingKey(null)}>
                                      <X className="size-3.5" />
                                    </Button>
                                  </>
                                ) : (
                                  <>
                                    <Button
                                      type="button"
                                      size="sm"
                                      variant="outline"
                                      className="h-8"
                                      onClick={() => setMemoryAlwaysOn(memory, !isAlwaysOn(memory))}
                                      aria-label={isAlwaysOn(memory) ? "取消常驻" : "设为常驻"}
                                    >
                                      {isAlwaysOn(memory) ? <PinOff className="size-3.5" /> : <Pin className="size-3.5" />}
                                    </Button>
                                    <Button type="button" size="sm" variant="outline" className="h-8" onClick={() => beginEditMemory(memory)}>
                                      <Pencil className="size-3.5" />
                                    </Button>
                                    <Button
                                      type="button"
                                      size="sm"
                                      variant="outline"
                                      className="h-8 text-destructive"
                                      onClick={() => deleteMemoryRow(memory.key)}
                                    >
                                      <Trash2 className="size-3.5" />
                                    </Button>
                                  </>
                                )}
                              </div>
                            </div>
                          );
                        })}
                      </div>
                    )}
                  </Card>
                </section>
                )}

                {activeSection === "runs" && taskLauncherOpen && (
                  <Card className="border-primary/30 p-4">
                    <div className="flex items-start justify-between gap-3">
                      <div>
                        <h2 className="text-sm font-semibold">启动业务任务</h2>
                        <p className="mt-1 text-xs text-muted-foreground">
                          输入来自应用蓝图；Task、Session 和交付物会自动关联。
                        </p>
                      </div>
                      <Button type="button" size="sm" variant="ghost" onClick={() => setTaskLauncherOpen(false)}>
                        <X className="size-3.5" />
                      </Button>
                    </div>
                    <div className="mt-4 grid gap-3 sm:grid-cols-2">
                      {executionTaskInputs(agent).map((input) => (
                        <label key={input.type} className="grid gap-1.5">
                          <span className="text-xs font-medium">{input.type}</span>
                          <Input
                            value={taskInputValues[input.type] ?? ""}
                            onChange={(event) => setTaskInputValues((current) => ({
                              ...current,
                              [input.type]: event.target.value,
                            }))}
                            placeholder={input.description || input.source}
                            className="h-9 text-sm"
                          />
                          <span className="text-[11px] text-muted-foreground">
                            {input.source}{input.description ? ` · ${input.description}` : ""}
                          </span>
                        </label>
                      ))}
                    </div>
                    {taskStartError && <p className="mt-3 text-xs text-destructive">{taskStartError}</p>}
                    <div className="mt-4 flex justify-end gap-2">
                      <Button type="button" size="sm" variant="outline" onClick={() => setTaskLauncherOpen(false)}>
                        取消
                      </Button>
                      <Button type="button" size="sm" disabled={taskStarting} onClick={() => void startStructuredTask()}>
                        <Play className="size-3.5" />
                        {taskStarting ? "启动中..." : "创建 Task 并运行"}
                      </Button>
                    </div>
                  </Card>
                )}

                {activeSection === "runs" && (
                <section>
                  <div className="mb-2 flex flex-wrap items-end justify-between gap-2">
                    <div>
                      <h2 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                        业务任务（{visibleTasks.length}/{tasks.length}）
                      </h2>
                      <p className="mt-1 text-xs text-muted-foreground">
                        Task 记录业务目标与结果；Session 保留每次执行过程。
                      </p>
                    </div>
                    <div className="flex gap-1">
                      {([
                        ["all", "全部"],
                        ["active", "进行中"],
                        ["failed", "失败"],
                      ] as Array<[TaskStatusFilter, string]>).map(([value, label]) => (
                        <button
                          key={value}
                          type="button"
                          onClick={() => setTaskFilter(value)}
                          className={`rounded-md px-2 py-1 text-xs transition-colors ${
                            taskFilter === value
                              ? "bg-foreground text-background"
                              : "bg-muted text-muted-foreground hover:text-foreground"
                          }`}
                        >
                          {label}
                        </button>
                      ))}
                    </div>
                  </div>
                  <Card className="overflow-hidden">
                    {visibleTasks.length === 0 ? (
                      <div className="p-5 text-sm text-muted-foreground">
                        {tasks.length === 0
                          ? "暂无 Task。新创建的运行会自动进入任务闭环。"
                          : "没有符合当前过滤条件的任务。"}
                      </div>
                    ) : (
                      <div className="divide-y divide-border">
                        {visibleTasks.map((task) => (
                          <div key={task.id}>
                            <div className="grid gap-2 px-4 py-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
                              <div className="min-w-0">
                                <div className="flex flex-wrap items-center gap-2">
                                  <p className="truncate text-sm font-medium">{task.title}</p>
                                  <Badge variant="outline" className="text-[11px]">
                                    {task.source}
                                  </Badge>
                                </div>
                                <p className="mt-1 font-mono text-[11px] text-muted-foreground">
                                  {task.id} · {formatMemoryDate(task.created_at)}
                                  {task.current_attempt_number > 1 && ` · 第 ${task.current_attempt_number} 次尝试`}
                                </p>
                                {task.deadline_at && !["verifying", "succeeded", "failed", "cancelled"].includes(task.status) && (
                                  <p className={`mt-1 text-[11px] ${task.deadline_at < Date.now() ? "font-medium text-destructive" : "text-muted-foreground"}`}>
                                    截止：{formatMemoryDate(task.deadline_at)}
                                    {task.deadline_at < Date.now() && "（已超期）"}
                                  </p>
                                )}
                                {task.failure_reason && (
                                  <p className="mt-1 text-xs text-destructive">{task.failure_reason}</p>
                                )}
                              </div>
                              <div className="flex items-center gap-2">
                                <Badge
                                  variant={
                                    task.status === "failed"
                                      ? "destructive"
                                      : task.status === "running" || task.status === "succeeded"
                                        ? "secondary"
                                        : "outline"
                                  }
                                >
                                  {task.failure_code === "timeout" ? "timed_out" : task.status}
                                </Badge>
                                {[
                                  "queued",
                                  "running",
                                  "waiting_input",
                                  "verifying",
                                ].includes(task.status) && (
                                  <Button
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className="h-7 text-destructive"
                                    disabled={taskCancelling === task.id}
                                    onClick={() => void cancelActiveTask(task)}
                                  >
                                    <X className="size-3.5" />
                                    {taskCancelling === task.id ? "取消中..." : "取消"}
                                  </Button>
                                )}
                                <Button
                                  type="button"
                                  size="sm"
                                  variant="ghost"
                                  className="h-7"
                                  onClick={() => void openTaskDetails(task)}
                                >
                                  {selectedTaskId === task.id ? "收起" : "交付与验收"}
                                </Button>
                              </div>
                            </div>
                            {selectedTaskId === task.id && (
                              <div className="border-t border-border bg-muted/10 p-4">
                                {task.status === "failed" && (
                                  <div className="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-md border border-destructive/30 bg-destructive/5 p-3">
                                    <div>
                                      <h3 className="text-xs font-semibold">创建新的执行尝试</h3>
                                      <p className="mt-1 text-[11px] text-muted-foreground">
                                        保留当前 Task 与历史记录，使用相同输入启动新 Session。
                                      </p>
                                    </div>
                                    <Button
                                      type="button"
                                      size="sm"
                                      className="h-8"
                                      disabled={
                                        taskRetrying
                                        || task.current_attempt_number >= taskAttempts.max_attempts
                                      }
                                      title={
                                        task.current_attempt_number >= taskAttempts.max_attempts
                                          ? `已达重试上限（${taskAttempts.max_attempts} 次尝试）`
                                          : undefined
                                      }
                                      onClick={() => void retryFailedTask(task)}
                                    >
                                      <RefreshCw className={`size-3.5 ${taskRetrying ? "animate-spin" : ""}`} />
                                      {taskRetrying
                                        ? "重试中..."
                                        : task.current_attempt_number >= taskAttempts.max_attempts
                                          ? `已达上限（${taskAttempts.max_attempts}）`
                                          : "重试 Task"}
                                    </Button>
                                  </div>
                                )}
                                {task.status === "waiting_input" && (
                                  <div className="mb-4 rounded-md border border-amber-500/30 bg-amber-500/5 p-3">
                                    <h3 className="text-xs font-semibold">补充输入并继续原 Session</h3>
                                    <div className="mt-3 grid gap-2 sm:grid-cols-2">
                                      {executionTaskInputs(agent).map((input) => (
                                        <label key={input.type} className="grid gap-1">
                                          <span className="text-[11px] font-medium">{input.type}</span>
                                          <Input
                                            value={resumeInputValues[input.type] ?? ""}
                                            onChange={(event) => setResumeInputValues((current) => ({
                                              ...current,
                                              [input.type]: event.target.value,
                                            }))}
                                            placeholder={input.description}
                                            className="h-8 text-xs"
                                          />
                                        </label>
                                      ))}
                                    </div>
                                    <div className="mt-3 flex justify-end">
                                      <Button
                                        type="button"
                                        size="sm"
                                        className="h-8"
                                        disabled={taskResuming}
                                        onClick={() => void resumeWaitingTask(task)}
                                      >
                                        {taskResuming ? "继续中..." : "补充并继续"}
                                      </Button>
                                    </div>
                                  </div>
                                )}
                                {taskDetailLoading ? (
                                  <p className="text-xs text-muted-foreground">正在加载交付物...</p>
                                ) : (
                                  <div className="grid gap-4 lg:grid-cols-2">
                                    <div className="lg:col-span-2">
                                      <div className="flex items-center justify-between gap-2">
                                        <h3 className="text-xs font-semibold">
                                          执行尝试（当前 {task.current_attempt_number}/{taskAttempts.max_attempts}）
                                        </h3>
                                        <span className="text-[11px] text-muted-foreground">最新尝试优先</span>
                                      </div>
                                      {taskAttempts.sessions.length === 0 && taskAttempts.runs.length === 0 ? (
                                        <p className="mt-2 text-xs text-muted-foreground">尚无关联的执行记录。</p>
                                      ) : (
                                        <div className="mt-2 grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
                                          {taskAttempts.sessions.map((attempt) => {
                                            const artifactCount = taskAttempts.artifacts.filter(
                                              (artifact) => artifact.attempt_number === attempt.attempt_number,
                                            ).length;
                                            const checks = taskAttempts.acceptance_checks.filter(
                                              (check) => check.attempt_number === attempt.attempt_number,
                                            );
                                            const verdict = checks.some((check) => check.verdict === "failed")
                                              ? "failed"
                                              : checks.length > 0 && checks.every((check) => check.verdict === "passed")
                                                ? "passed"
                                                : "pending";
                                            return (
                                              <button
                                                key={attempt.id}
                                                type="button"
                                                className="rounded-md border border-border bg-background p-3 text-left transition-colors hover:border-primary/40"
                                                onClick={() => router.push(`/chat/?id=${encodeURIComponent(attempt.id)}`)}
                                              >
                                                <div className="flex items-center justify-between gap-2">
                                                  <span className="text-xs font-medium">Session Attempt {attempt.attempt_number}</span>
                                                  <Badge variant="outline" className="text-[10px]">{attempt.status}</Badge>
                                                </div>
                                                <p className="mt-1 truncate font-mono text-[10px] text-muted-foreground">{attempt.id}</p>
                                                <p className="mt-1 text-[11px] text-muted-foreground">
                                                  {attempt.runtime || attempt.harness} · {formatMemoryDate(attempt.created_at)}
                                                </p>
                                                <p className="mt-2 text-[11px] text-muted-foreground">
                                                  交付物 {artifactCount} · 验收 {verdict}
                                                </p>
                                              </button>
                                            );
                                          })}
                                          {taskAttempts.runs.map((attempt) => {
                                            const artifactCount = taskAttempts.artifacts.filter(
                                              (artifact) => artifact.attempt_number === attempt.attempt_number,
                                            ).length;
                                            const checks = taskAttempts.acceptance_checks.filter(
                                              (check) => check.attempt_number === attempt.attempt_number,
                                            );
                                            const verdict = checks.some((check) => check.verdict === "failed")
                                              ? "failed"
                                              : checks.length > 0 && checks.every((check) => check.verdict === "passed")
                                                ? "passed"
                                                : "pending";
                                            return (
                                              <div key={attempt.id} className="rounded-md border border-border bg-background p-3">
                                                <div className="flex items-center justify-between gap-2">
                                                  <span className="text-xs font-medium">Legacy Run Attempt {attempt.attempt_number}</span>
                                                  <Badge variant="outline" className="text-[10px]">{attempt.status}</Badge>
                                                </div>
                                                <p className="mt-1 truncate font-mono text-[10px] text-muted-foreground">{attempt.id}</p>
                                                <p className="mt-1 text-[11px] text-muted-foreground">{formatMemoryDate(attempt.started_at)}</p>
                                                <p className="mt-2 text-[11px] text-muted-foreground">
                                                  交付物 {artifactCount} · 验收 {verdict}
                                                </p>
                                                {attempt.error && <p className="mt-1 text-[11px] text-destructive">{attempt.error}</p>}
                                              </div>
                                            );
                                          })}
                                        </div>
                                      )}
                                    </div>
                                    <div>
                                      <h3 className="text-xs font-semibold">交付物（{taskArtifacts.length}）</h3>
                                      {taskArtifacts.length === 0 ? (
                                        <p className="mt-2 text-xs text-muted-foreground">
                                          暂无交付物；没有交付物的任务不能验收成功。
                                        </p>
                                      ) : (
                                        <div className="mt-2 grid gap-2">
                                          {taskArtifacts.map((artifact) => (
                                            <div key={artifact.id} className="rounded-md border border-border bg-background p-3">
                                              <div className="flex items-center justify-between gap-2">
                                                <span className="text-xs font-medium">{artifact.name}</span>
                                                <Badge variant="outline" className="text-[10px]">{artifact.artifact_type}</Badge>
                                              </div>
                                              {artifactText(artifact) && (
                                                <p className="mt-2 max-h-28 overflow-y-auto whitespace-pre-wrap text-xs text-muted-foreground">
                                                  {artifactText(artifact)}
                                                </p>
                                              )}
                                              {artifact.location && (
                                                <a href={artifact.location} className="mt-2 inline-block text-xs text-primary hover:underline">
                                                  查看来源
                                                </a>
                                              )}
                                            </div>
                                          ))}
                                        </div>
                                      )}
                                    </div>
                                    <div>
                                      <h3 className="text-xs font-semibold">完成条件</h3>
                                      {taskChecks.length === 0 ? (
                                        <p className="mt-2 text-xs text-muted-foreground">
                                          此任务没有声明完成条件，需要进行一次明确的人工整体验收。
                                        </p>
                                      ) : (
                                        <div className="mt-2 grid gap-2">
                                          {taskChecks.map((check) => (
                                            <div key={check.id} className="rounded-md border border-border bg-background p-3">
                                              <div className="flex items-start justify-between gap-2">
                                                <p className="text-xs">{check.criterion}</p>
                                                <Badge variant="outline" className="text-[10px]">{check.verdict}</Badge>
                                              </div>
                                              {check.evidence && <p className="mt-1 text-[11px] text-muted-foreground">证据：{check.evidence}</p>}
                                              {task.status === "verifying" && check.verdict === "pending" && (
                                                <div className="mt-2 flex gap-1.5">
                                                  <Button size="sm" className="h-7" disabled={acceptanceBusy || !acceptanceEvidence.trim()} onClick={() => void recordTaskAcceptance(task, check, "passed")}>通过</Button>
                                                  <Button size="sm" variant="outline" className="h-7 text-destructive" disabled={acceptanceBusy || !acceptanceEvidence.trim()} onClick={() => void recordTaskAcceptance(task, check, "failed")}>失败</Button>
                                                </div>
                                              )}
                                            </div>
                                          ))}
                                        </div>
                                      )}
                                      {task.status === "verifying" && (
                                        <div className="mt-3 grid gap-2">
                                          <Input
                                            value={acceptanceEvidence}
                                            onChange={(event) => setAcceptanceEvidence(event.target.value)}
                                            placeholder="填写可审计的验收证据"
                                            className="h-8 text-xs"
                                          />
                                          {taskChecks.length === 0 && (
                                            <div className="flex gap-1.5">
                                              <Button size="sm" className="h-7" disabled={acceptanceBusy || !acceptanceEvidence.trim()} onClick={() => void recordTaskAcceptance(task, null, "passed")}>人工通过</Button>
                                              <Button size="sm" variant="outline" className="h-7 text-destructive" disabled={acceptanceBusy || !acceptanceEvidence.trim()} onClick={() => void recordTaskAcceptance(task, null, "failed")}>人工失败</Button>
                                            </div>
                                          )}
                                        </div>
                                      )}
                                    </div>
                                  </div>
                                )}
                                {taskDetailError && (
                                  <p className="mt-3 text-xs text-destructive">{taskDetailError}</p>
                                )}
                              </div>
                            )}
                          </div>
                        ))}
                      </div>
                    )}
                  </Card>
                </section>
                )}

                {activeSection === "runs" && (
                <section>
                  <div className="mb-2">
                    <h2 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                      触发与调度（{routines.length}）
                    </h2>
                    <p className="mt-1 text-xs text-muted-foreground">
                      Routine 定义智能体何时运行；会话记录显示每次实际执行。
                    </p>
                  </div>
                  <Card className="overflow-hidden">
                    {routines.length === 0 ? (
                      <div className="p-5 text-sm text-muted-foreground">
                        当前没有 Routine，仅可由用户或 API 手动启动。
                      </div>
                    ) : (
                      <div className="divide-y divide-border">
                        {routines.map((routine) => (
                          <div
                            key={routine.id}
                            className="grid gap-2 px-4 py-3 sm:grid-cols-[minmax(0,1fr)_auto_auto_auto] sm:items-center"
                          >
                            <div className="min-w-0">
                              <p className="truncate text-sm font-medium">{routine.name}</p>
                              <p className="mt-0.5 font-mono text-[11px] text-muted-foreground">
                                {scheduleLabel(routine.cron, routine.timezone)}
                              </p>
                            </div>
                            <Badge variant={routine.status === "active" ? "secondary" : "outline"}>
                              {routine.status === "active" ? "已启用" : "已暂停"}
                            </Badge>
                            <span className="text-xs text-muted-foreground">
                              {routine.last_run_at
                                ? `最近运行 ${timeAgo(routine.last_run_at)}`
                                : "尚未运行"}
                            </span>
                            <div className="flex gap-1">
                              <Button
                                type="button"
                                size="sm"
                                variant="ghost"
                                className="h-7"
                                disabled={routineBusy === routine.id}
                                onClick={() => void toggleRoutine(routine)}
                              >
                                {routine.status === "active" ? "暂停" : "启用"}
                              </Button>
                              <Button
                                type="button"
                                size="sm"
                                variant="ghost"
                                className="h-7"
                                disabled={routineBusy === routine.id || runDisabledReason != null}
                                title={runDisabledReason ?? "立即触发一次运行"}
                                onClick={() => void runRoutineNow(routine)}
                              >
                                <Play className="size-3" />
                                立即运行
                              </Button>
                            </div>
                          </div>
                        ))}
                      </div>
                    )}
                  </Card>
                </section>
                )}

                {activeSection === "runs" && (
                <section>
                  <div className="mb-2 flex items-center justify-between">
                    <h2 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                      会话（{sessions.length}）
                    </h2>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={openSessionStart}
                      disabled={runDisabledReason != null}
                      title={runDisabledReason ?? undefined}
                    >
                      <Play className="size-3" />
                      Run
                    </Button>
                  </div>
                  {sessions.length === 0 ? (
                    <p className="text-sm text-muted-foreground">还没有会话。</p>
                  ) : (
                    <div className="flex flex-col gap-2">
                      {sessions.slice(0, MAX_VISIBLE_SESSIONS).map((s) => {
                        const status = SESSION_STATUS_BADGES[s.status ?? "idle"] ?? {
                          label: s.status ?? "idle",
                          className: "bg-muted text-muted-foreground",
                        };
                        return (
                          <Card
                            key={s.id}
                            className="flex cursor-pointer items-center justify-between gap-2 px-4 py-3 transition-colors hover:bg-muted/40"
                            onClick={() => router.push(`/chat/?id=${encodeURIComponent(s.id)}`)}
                          >
                            <div className="min-w-0">
                              <span className="flex min-w-0 items-center gap-1.5">
                                <p className="truncate text-sm font-medium">{s.title ?? "Untitled session"}</p>
                                <span className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium ${status.className}`}>
                                  {status.label}
                                </span>
                                <span className="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
                                  {s.task_id ? "任务执行" : "对话"}
                                </span>
                              </span>
                              <p className="mt-0.5 font-mono text-[11px] text-muted-foreground">{s.id}</p>
                            </div>
                            {s.time?.created && (
                              <span className="shrink-0 text-xs text-muted-foreground">
                                {timeAgo(s.time.created * 1000)}
                              </span>
                            )}
                          </Card>
                        );
                      })}
                      {sessions.length > MAX_VISIBLE_SESSIONS && (
                        <p className="text-xs text-muted-foreground">
                          仅显示最近 {MAX_VISIBLE_SESSIONS} 条，共 {sessions.length} 条。
                        </p>
                      )}
                    </div>
                  )}
                </section>
                )}
              </>
            )}
          </div>
        </main>
      </div>
    </div>
  );
}

export default function AgentDetailPage() {
  return (
    <Suspense>
      <AgentDetail />
    </Suspense>
  );
}
