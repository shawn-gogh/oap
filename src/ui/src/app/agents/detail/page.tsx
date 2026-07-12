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
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { VaultCredentialsEditor } from "@/components/vault-credentials-editor";
import {
  DEFAULT_VAULT_USER,
  agentFileDownloadUrl,
  createAgentGrant,
  createAgentGroupGrant,
  createImprovementProposal,
  deleteAgentGrant,
  deleteAgentGroupGrant,
  listAgentGrants,
  listAgentGroupGrants,
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
  getAgent,
  listAgentFiles,
  uploadAgentFile,
  listMemory,
  listSessions,
  listVaultKeysForUser,
  storeMemory,
  updateAgent,
} from "@/lib/api";
import { scheduleLabel } from "@/lib/schedule";
import type {
  Agent,
  AgentRuntimeId,
  Memory,
  OpencodeSession,
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

function isAgentRuntimeId(value: unknown): value is AgentRuntimeId {
  return value === "claude_managed_agents" || value === "cursor" || value === "gemini_antigravity";
}

function runtimeFromAgent(agent: Agent): string {
  const config = agent.config;
  if (config && typeof config === "object" && !Array.isArray(config)) {
    const runtime = (config as { runtime?: unknown }).runtime;
    if (isAgentRuntimeId(runtime)) return runtime;
  }
  if (isAgentRuntimeId(agent.harness)) return agent.harness;
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

function fileNameFromPath(filePath: string): string {
  return filePath.split("/").filter(Boolean).at(-1) || "agent-file";
}

type MemoryFilter = "all" | "always" | "standard";

function AgentDetail() {
  const router = useRouter();
  const confirmAction = useConfirm();
  const searchParams = useSearchParams();
  const id = decodeURIComponent(searchParams.get("id") ?? "");

  const [agent, setAgent] = useState<Agent | null>(null);
  const [sessions, setSessions] = useState<OpencodeSession[]>([]);
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
  const [grantUser, setGrantUser] = useState("");
  const [grantUserQuery, setGrantUserQuery] = useState("");
  const [grantGroup, setGrantGroup] = useState("");
  const [grantGroupQuery, setGrantGroupQuery] = useState("");
  const [grantPermission, setGrantPermission] = useState("use");
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

  useEffect(() => {
    if (!id) return;
    (async () => {
      try {
        const ag = await getAgent(id);
        const owner = vaultUserFromAgent(ag);
        const [allSessions, memoryRows, fileRows, keyRows] = await Promise.all([
          listSessions().catch(() => []),
          listMemory(id).catch(() => []),
          listAgentFiles(id).catch(() => []),
          listVaultKeysForUser(owner).catch(() => []),
        ]);
        setVaultUserId(owner);
        setAgent(ag);
        setSessions(allSessions.filter((s) => s.agent_id === id || s.agent === id || s.harness === id));
        setMemories(memoryRows);
        setFiles(fileRows);
        setStoredKeyEntries(keyRows);
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

  const openSessionStart = () => {
    if (!id) return;
    router.push(`/sessions/?agent=${encodeURIComponent(id)}`);
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
    const user = grantUser.trim();
    if (!user || grantBusy) return;
    setGrantBusy(true);
    setGrantError(null);
    try {
      await createAgentGrant(id, user, grantPermission);
      setGrants(await listAgentGrants(id));
      setGrantUser("");
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
    const group = grantGroup.trim();
    if (!group || grantBusy) return;
    setGrantBusy(true);
    setGrantError(null);
    try {
      await createAgentGroupGrant(id, group, grantPermission);
      setGroupGrants(await listAgentGroupGrants(id));
      setGrantGroup("");
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
                <Button size="sm" variant="default" onClick={openSessionStart}>
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
                <Button size="sm" variant="outline" onClick={handleDelete} aria-label="Delete">
                  <Trash2 className="size-3.5" />
                </Button>
              </>
            )}
            <ThemeToggle />
          </div>
        </header>

        <main className="flex-1 overflow-y-auto">
          <div className="mx-auto flex max-w-4xl flex-col gap-6 px-4 py-6">
            {error && (
              <Card className="border-destructive p-3">
                <p className="text-sm text-destructive">{error}</p>
              </Card>
            )}
            {loading && <div className="text-sm text-muted-foreground">Loading...</div>}

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

                <section>
                  <h2 className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Configuration
                  </h2>
                  <Card className="p-4">
                    <dl className="grid gap-x-6 gap-y-3 text-sm sm:grid-cols-[140px_1fr]">
                      <dt className="font-medium text-muted-foreground">ID</dt>
                      <dd className="break-all font-mono text-xs text-muted-foreground">{agent.id}</dd>

                      {agent.model && (
                        <>
                          <dt className="font-medium text-muted-foreground">Model</dt>
                          <dd className="font-mono text-xs">{String(agent.model)}</dd>
                        </>
                      )}

                      {agent.owner_id && (
                        <>
                          <dt className="font-medium text-muted-foreground">Owner</dt>
                          <dd className="font-mono text-xs">{String(agent.owner_id)}</dd>
                        </>
                      )}

                      <dt className="font-medium text-muted-foreground">Default runtime</dt>
                      <dd className="font-mono text-xs">{runtimeFromAgent(agent)}</dd>

                      <dt className="font-medium text-muted-foreground">Run schedule</dt>
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

                {grantsVisible && (
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
                    <div className="flex items-center gap-2 border-b border-border px-3 py-2.5">
                      <Input
                        value={grantUserQuery}
                        onChange={(e) => {
                          setGrantUserQuery(e.target.value);
                          setGrantUser("");
                        }}
                        placeholder="搜索姓名、邮箱或用户 ID"
                        className="h-8 max-w-[200px] text-xs"
                      />
                      <select
                        value={grantUser}
                        onChange={(e) => setGrantUser(e.target.value)}
                        className="h-8 max-w-[240px] rounded-md border border-input bg-transparent px-2 text-xs"
                      >
                        <option value="">从搜索结果选择用户</option>
                        {grantableUsers.map((user) => (
                          <option key={user.id} value={user.id} disabled={user.status !== "active"}>
                            {user.display_name} ({user.id}){user.status !== "active" ? " · 已禁用" : ""}
                          </option>
                        ))}
                      </select>
                      <select
                        value={grantPermission}
                        onChange={(e) => setGrantPermission(e.target.value)}
                        className="h-8 rounded-md border border-input bg-transparent px-2 text-xs"
                      >
                        <option value="use">use（使用）</option>
                        <option value="edit">edit（可修改）</option>
                      </select>
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        className="h-8"
                        onClick={() => void addGrant()}
                        disabled={grantBusy || !grantUser.trim()}
                      >
                        <Plus className="size-3.5" />
                        授权
                      </Button>
                    </div>
                    {grants.length === 0 ? (
                      <div className="p-4 text-center text-xs text-muted-foreground">
                        尚未授权给任何用户，仅 owner 与 admin 可见。
                      </div>
                    ) : (
                      <div className="divide-y divide-border">
                        {grants.map((grant) => (
                          <div key={grant.id} className="flex items-center justify-between px-3 py-2">
                            <div className="min-w-0">
                              <span className="font-mono text-xs">{grant.grantee_user_id}</span>
                              <span className="ml-2 rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground">
                                {grant.permission}
                              </span>
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

                {grantsVisible && (
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
                          setGrantGroup("");
                        }}
                        placeholder="搜索用户组"
                        className="h-8 max-w-[180px] text-xs"
                      />
                      <select
                        value={grantGroup}
                        onChange={(e) => setGrantGroup(e.target.value)}
                        className="h-8 max-w-[220px] rounded-md border border-input bg-transparent px-2 text-xs"
                      >
                        <option value="">从搜索结果选择用户组</option>
                        {grantableGroups.map((group) => (
                          <option key={group.id} value={group.id} disabled={group.status !== "active"}>
                            {group.name}{group.status !== "active" ? " · 已禁用" : ""}
                          </option>
                        ))}
                      </select>
                      <select
                        value={grantPermission}
                        onChange={(e) => setGrantPermission(e.target.value)}
                        className="h-8 rounded-md border border-input bg-transparent px-2 text-xs"
                      >
                        <option value="use">use（使用）</option>
                        <option value="edit">edit（可修改）</option>
                      </select>
                      <Button type="button" size="sm" variant="outline" className="h-8" onClick={() => void addGroupGrant()} disabled={grantBusy || !grantGroup.trim()}>
                        <Plus className="size-3.5" />
                        授权用户组
                      </Button>
                    </div>
                    {groupGrants.length === 0 ? (
                      <div className="p-4 text-center text-xs text-muted-foreground">尚未授权给任何用户组。</div>
                    ) : (
                      <div className="divide-y divide-border">
                        {groupGrants.map((grant) => (
                          <div key={grant.id} className="flex items-center justify-between px-3 py-2">
                            <div className="min-w-0"><span className="font-mono text-xs">{grant.group_id}</span><span className="ml-2 rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground">{grant.permission}</span></div>
                            <Button type="button" size="sm" variant="ghost" className="h-7 w-7 p-0 text-destructive" onClick={() => void removeGroupGrant(grant.group_id)} disabled={grantBusy} aria-label={`撤销用户组 ${grant.group_id}`}><Trash2 className="size-3.5" /></Button>
                          </div>
                        ))}
                      </div>
                    )}
                  </Card>
                </section>
                )}

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

                <section>
                  <div className="mb-2 flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
                    <div>
                      <h2 className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                        <FileText className="size-3.5" />
                        Workspace Files
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
                          placeholder="Search files"
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
                      <span>Path</span>
                      <span className="text-right">Size</span>
                      <span className="text-right">Actions</span>
                    </div>
                    {filesLoading && files.length === 0 ? (
                      <div className="p-6 text-sm text-muted-foreground">Loading files...</div>
                    ) : visibleFiles.length === 0 ? (
                      <div className="p-8 text-center">
                        <FileText className="mx-auto mb-3 size-7 text-muted-foreground/60" />
                        <p className="text-sm font-medium">
                          {files.length === 0 ? "No workspace files" : "No matching files"}
                        </p>
                        <p className="mt-1 text-xs text-muted-foreground">
                          {files.length === 0
                            ? "Upload files to make them available in every new session."
                            : "Adjust the search to broaden the file list."}
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

                <section>
                  <div className="mb-2 flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
                    <div>
                      <h2 className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                        <Brain className="size-3.5" />
                        Memory
                      </h2>
                      <p className="mt-1 text-xs text-muted-foreground">
                        Review what this agent has learned, pin critical notes, and curate stale context.
                      </p>
                    </div>
                    <div className="grid grid-cols-3 overflow-hidden rounded-md border border-border bg-muted/20 text-center sm:w-[300px]">
                      <div className="px-3 py-2">
                        <div className="text-base font-semibold">{memories.length}</div>
                        <div className="text-[11px] uppercase tracking-wide text-muted-foreground">Total</div>
                      </div>
                      <div className="border-x border-border px-3 py-2">
                        <div className="text-base font-semibold">{alwaysOnCount}</div>
                        <div className="text-[11px] uppercase tracking-wide text-muted-foreground">Always-on</div>
                      </div>
                      <div className="px-3 py-2">
                        <div className="text-base font-semibold">{selectedKeys.size}</div>
                        <div className="text-[11px] uppercase tracking-wide text-muted-foreground">Selected</div>
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
                            placeholder="Search keys or memory text"
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
                              {filter === "always" ? "Always-on" : filter}
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
                          placeholder="Add a durable note for this agent"
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
                      <div className="p-6 text-sm text-muted-foreground">Loading memories...</div>
                    ) : visibleMemories.length === 0 ? (
                      <div className="p-8 text-center">
                        <Brain className="mx-auto mb-3 size-7 text-muted-foreground/60" />
                        <p className="text-sm font-medium">
                          {memories.length === 0 ? "No memories yet" : "No matching memories"}
                        </p>
                        <p className="mt-1 text-xs text-muted-foreground">
                          {memories.length === 0
                            ? "The agent can add memories as it works, or you can seed one above."
                            : "Adjust the search or filter to broaden the list."}
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
                                      aria-label="Toggle always-on"
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
                                      aria-label={isAlwaysOn(memory) ? "Disable always-on" : "Make always-on"}
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

                <section>
                  <div className="mb-2 flex items-center justify-between">
                    <h2 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                      Sessions ({sessions.length})
                    </h2>
                    <Button size="sm" variant="outline" onClick={openSessionStart}>
                      <Play className="size-3" />
                      Run
                    </Button>
                  </div>
                  {sessions.length === 0 ? (
                    <p className="text-sm text-muted-foreground">No sessions yet.</p>
                  ) : (
                    <div className="flex flex-col gap-2">
                      {sessions.map((s) => (
                        <Card
                          key={s.id}
                          className="flex cursor-pointer items-center justify-between gap-2 px-4 py-3 transition-colors hover:bg-muted/40"
                          onClick={() => router.push(`/chat/?id=${encodeURIComponent(s.id)}`)}
                        >
                          <div className="min-w-0">
                            <p className="truncate text-sm font-medium">{s.title ?? "Untitled session"}</p>
                            <p className="mt-0.5 font-mono text-[11px] text-muted-foreground">{s.id}</p>
                          </div>
                          {s.time?.created && (
                            <span className="shrink-0 text-xs text-muted-foreground">
                              {timeAgo(s.time.created * 1000)}
                            </span>
                          )}
                        </Card>
                      ))}
                    </div>
                  )}
                </section>
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
