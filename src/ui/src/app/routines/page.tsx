"use client";

import { useEffect, useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import {
  Pencil,
  Play,
  Plus,
  Trash2,
  Zap,
  Clock,
  Loader2,
  Info,
  Calendar,
  Sparkles,
  Bot,
  AlertCircle,
} from "lucide-react";
import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { ScheduleEditor } from "@/components/schedule-editor";
import {
  createRoutine,
  deleteRoutine,
  getAgentRunLogs,
  listAgents,
  listRoutines,
  triggerRoutine,
  updateRoutine,
} from "@/lib/api";
import { DEFAULT_TIMEZONE, scheduleLabel } from "@/lib/schedule";
import type { Agent, Routine } from "@/lib/types";

interface RoutineForm {
  agent_id: string;
  name: string;
  prompt: string;
  cron: string;
  timezone: string;
  status: string;
}

const EMPTY_FORM: RoutineForm = {
  agent_id: "",
  name: "",
  prompt: "",
  cron: "0 9 * * 1-5",
  timezone: DEFAULT_TIMEZONE,
  status: "active",
};

function timeAgo(ms?: number | null): string {
  if (!ms) return "从未运行";
  const diff = Math.max(0, Date.now() - ms);
  const mins = Math.floor(diff / 60000);
  if (mins < 60) return `${mins} 分钟前`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs} 小时前`;
  return `${Math.floor(hrs / 24)} 天前`;
}

export default function RoutinesPage() {
  const router = useRouter();
  const [agents, setAgents] = useState<Agent[]>([]);
  const [routines, setRoutines] = useState<Routine[] | null>(null);
  const [open, setOpen] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [form, setForm] = useState<RoutineForm>(EMPTY_FORM);
  const [saving, setSaving] = useState(false);
  const [triggeringId, setTriggeringId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [logsOpen, setLogsOpen] = useState(false);
  const [logsTitle, setLogsTitle] = useState("任务运行日志");
  const [logsText, setLogsText] = useState("");
  const [logsLoading, setLogsLoading] = useState(false);
  const [logsError, setLogsError] = useState<string | null>(null);

  const agentsById = useMemo(
    () => new Map(agents.map((agent) => [agent.id, agent])),
    [agents],
  );

  const load = async () => {
    try {
      const [agentList, routineList] = await Promise.all([listAgents(), listRoutines()]);
      setAgents(agentList);
      setRoutines(routineList);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "加载定时任务失败");
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const openCreate = () => {
    setEditingId(null);
    setForm({
      ...EMPTY_FORM,
      agent_id: agents[0]?.id ?? "",
    });
    setFormError(null);
    setOpen(true);
  };

  const openEdit = (routine: Routine) => {
    setEditingId(routine.id);
    setForm({
      agent_id: routine.agent_id,
      name: routine.name,
      prompt: routine.prompt,
      cron: routine.cron,
      timezone: routine.timezone || DEFAULT_TIMEZONE,
      status: routine.status || "active",
    });
    setFormError(null);
    setOpen(true);
  };

  const save = async () => {
    setSaving(true);
    setFormError(null);
    try {
      if (!form.agent_id) throw new Error("请选择关联的智能体");
      if (!form.name.trim()) throw new Error("请输入任务名称");
      if (!form.cron.trim()) throw new Error("请设置定时 Cron 表达式");
      const input = {
        agent_id: form.agent_id,
        name: form.name.trim(),
        prompt: form.prompt,
        cron: form.cron.trim(),
        timezone: form.timezone.trim() || "UTC",
        status: form.status,
      };
      if (editingId) await updateRoutine(editingId, input);
      else await createRoutine(input);
      setOpen(false);
      await load();
    } catch (err) {
      setFormError(err instanceof Error ? err.message : "保存定时任务失败");
    } finally {
      setSaving(false);
    }
  };

  const remove = async (routine: Routine) => {
    if (!confirm(`确定要移除定时任务 "${routine.name}" 吗？`)) return;
    setRoutines((current) => current?.filter((item) => item.id !== routine.id) ?? null);
    try {
      await deleteRoutine(routine.id);
    } catch (err) {
      setError(err instanceof Error ? err.message : "删除定时任务失败");
      await load();
    }
  };

  const trigger = async (routine: Routine) => {
    setTriggeringId(routine.id);
    setError(null);
    try {
      await triggerRoutine(routine.id);
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "手动触发定时任务失败");
    } finally {
      setTriggeringId(null);
    }
  };

  const openLastRun = async (routine: Routine) => {
    if (routine.last_session_id) {
      router.push(`/chat/?id=${encodeURIComponent(routine.last_session_id)}`);
      return;
    }
    if (!routine.last_run_id) return;

    setLogsOpen(true);
    setLogsTitle(`${routine.name} 运行日志`);
    setLogsText("");
    setLogsError(null);
    setLogsLoading(true);
    try {
      const logs = await getAgentRunLogs(routine.agent_id, routine.last_run_id);
      setLogsText(logs.trim() ? logs : "未捕获到任何输出日志。");
    } catch (err) {
      setLogsError(err instanceof Error ? err.message : "加载运行日志失败");
    } finally {
      setLogsLoading(false);
    }
  };

  const activeCount = useMemo(
    () => routines?.filter((r) => r.status === "active").length ?? 0,
    [routines],
  );

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden selection:bg-amber-500/20">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        {/* Header - Pure Chinese */}
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border/80 px-4 bg-background/80 backdrop-blur">
          <div className="flex items-center gap-2.5">
            <div className="flex size-7 items-center justify-center rounded-md bg-amber-500/10 text-amber-600 dark:text-amber-400 ring-1 ring-amber-500/20">
              <Zap className="size-4" />
            </div>
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold tracking-tight">定时任务调度</span>
              <span className="text-xs text-muted-foreground font-medium">/ 任务</span>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button
              size="sm"
              className="gap-1.5 bg-amber-600 text-white hover:bg-amber-700 dark:bg-amber-500 dark:hover:bg-amber-600 font-medium text-xs shadow-xs"
              onClick={openCreate}
              disabled={agents.length === 0}
            >
              <Plus className="size-4" />
              新建定时任务
            </Button>
            <ThemeToggle />
          </div>
        </header>

        <main className="min-h-0 flex-1 overflow-y-auto p-6 space-y-6">
          <div className="mx-auto max-w-5xl space-y-6">
            {/* Banner - Pure Chinese */}
            <div className="relative overflow-hidden rounded-2xl border border-border/70 bg-card p-6 shadow-xs">
              <div className="absolute right-0 top-0 -mr-16 -mt-16 size-64 rounded-full bg-amber-500/5 blur-3xl pointer-events-none" />
              <div className="relative z-10 flex flex-col gap-5 md:flex-row md:items-center md:justify-between">
                <div className="space-y-1.5 max-w-xl">
                  <div className="flex items-center gap-2">
                    <span className="inline-flex items-center gap-1 rounded-md bg-amber-500/10 px-2 py-0.5 text-xs font-medium text-amber-600 dark:text-amber-400 border border-amber-500/20">
                      <Clock className="size-3" /> CRON SCHEDULER
                    </span>
                    <span className="text-xs text-muted-foreground font-medium">自动化周期调度</span>
                  </div>
                  <h1 className="text-xl font-bold tracking-tight text-foreground">
                    智能体后台定时任务与周期调度
                  </h1>
                  <p className="text-xs text-muted-foreground leading-relaxed">
                    使用 Cron 表达式为智能体设置定时作业。系统将在后台自动唤醒智能体执行对应任务并生成审计日志。
                  </p>
                </div>

                <div className="flex items-center gap-4 shrink-0 rounded-xl bg-muted/40 p-3.5 border border-border/60">
                  <div className="text-center">
                    <div className="text-[11px] font-medium text-muted-foreground">任务总数</div>
                    <div className="text-xl font-bold font-mono text-foreground">{routines?.length ?? 0}</div>
                  </div>
                  <div className="h-7 w-px bg-border/60" />
                  <div className="text-center">
                    <div className="text-[11px] font-medium text-muted-foreground">启用中</div>
                    <div className="text-xl font-bold font-mono text-amber-600 dark:text-amber-400">{activeCount}</div>
                  </div>
                </div>
              </div>
            </div>

            {error && (
              <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-4 text-xs text-destructive font-mono">
                {error}
              </div>
            )}

            {!routines && !error && (
              <div className="space-y-3">
                {Array.from({ length: 3 }).map((_, i) => (
                  <div key={i} className="h-28 animate-pulse rounded-2xl border border-border/60 bg-muted/20" />
                ))}
              </div>
            )}

            {routines && routines.length === 0 && (
              <div className="flex flex-col items-center justify-center rounded-2xl border border-dashed border-border p-12 text-center">
                <div className="flex size-12 items-center justify-center rounded-2xl bg-amber-500/10 text-amber-500">
                  <Zap className="size-6" />
                </div>
                <h3 className="mt-3 text-sm font-semibold">暂无配置定时任务</h3>
                <p className="mt-1 text-xs text-muted-foreground max-w-xs leading-normal">
                  添加首个周期任务（例如每日自动代码审计、定时巡检日志等）。
                </p>
                <Button
                  size="sm"
                  className="gap-1.5 bg-amber-600 hover:bg-amber-700 text-white mt-4"
                  onClick={openCreate}
                  disabled={agents.length === 0}
                >
                  <Plus className="size-4" />
                  创建首个任务
                </Button>
              </div>
            )}

            {/* Routines Grid */}
            <div className="space-y-3">
              {routines?.map((routine) => {
                const agent = agentsById.get(routine.agent_id);
                const lastRun = timeAgo(routine.last_run_at);
                const canOpenLastRun = Boolean(routine.last_session_id || routine.last_run_id);
                const isActive = routine.status === "active";

                return (
                  <div
                    key={routine.id}
                    className="group relative flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4 rounded-2xl border border-border/80 bg-card p-5 transition-all duration-200 hover:-translate-y-0.5 hover:border-amber-500/40 hover:shadow-md"
                  >
                    <div className="min-w-0 flex-1 space-y-1.5">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="font-semibold text-sm text-foreground">{routine.name}</span>
                        <span
                          className={`rounded-md px-2 py-0.5 text-[10px] font-medium border ${
                            isActive
                              ? "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20"
                              : "bg-muted text-muted-foreground border-border/40"
                          }`}
                        >
                          {isActive ? "启用中" : "已暂停"}
                        </span>
                      </div>

                      <div className="flex items-center gap-2 text-xs text-muted-foreground">
                        <Bot className="size-3.5 text-muted-foreground shrink-0" />
                        <span className="font-medium text-foreground">{agent?.name ?? routine.agent_id}</span>
                      </div>

                      {routine.prompt && (
                        <p className="line-clamp-1 font-mono text-xs text-muted-foreground bg-muted/20 p-1.5 rounded border border-border/30">
                          {routine.prompt}
                        </p>
                      )}

                      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground pt-1">
                        <span className="inline-flex items-center gap-1.5 font-mono text-[11px] text-amber-600 dark:text-amber-400 font-semibold">
                          <Zap className="size-3" />
                          {scheduleLabel(routine.cron, routine.timezone)}
                        </span>
                        {canOpenLastRun ? (
                          <button
                            type="button"
                            onClick={() => void openLastRun(routine)}
                            className="text-[11px] font-mono underline decoration-border underline-offset-2 hover:text-foreground transition-colors"
                          >
                            上次运行: {lastRun}
                          </button>
                        ) : (
                          <span className="text-[11px] font-mono">上次运行: {lastRun}</span>
                        )}
                      </div>
                    </div>

                    <div className="flex shrink-0 items-center gap-1.5 self-end sm:self-center">
                      <Button
                        size="sm"
                        className="h-8 text-xs gap-1.5 bg-amber-600 hover:bg-amber-700 text-white font-medium"
                        onClick={() => void trigger(routine)}
                        disabled={triggeringId === routine.id}
                      >
                        {triggeringId === routine.id ? (
                          <>
                            <Loader2 className="size-3.5 animate-spin" />
                            触发中...
                          </>
                        ) : (
                          <>
                            <Play className="size-3.5" />
                            立即执行
                          </>
                        )}
                      </Button>
                      <Button
                        size="sm"
                        variant="outline"
                        className="h-8 px-2.5 hover:bg-muted"
                        onClick={() => openEdit(routine)}
                        aria-label="编辑任务"
                        title="编辑任务"
                      >
                        <Pencil className="size-3.5" />
                      </Button>
                      <Button
                        size="sm"
                        variant="outline"
                        className="h-8 px-2.5 text-destructive hover:bg-destructive/10 hover:text-destructive"
                        onClick={() => void remove(routine)}
                        aria-label="移除任务"
                        title="移除任务"
                      >
                        <Trash2 className="size-3.5" />
                      </Button>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        </main>
      </div>

      {/* Routine Edit/Create Dialog */}
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="w-[92vw] sm:max-w-2xl rounded-2xl">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2 text-base font-semibold">
              <Zap className="size-4 text-amber-500" />
              {editingId ? "编辑定时任务" : "创建新定时任务"}
            </DialogTitle>
          </DialogHeader>

          <div className="grid gap-4 py-2">
            <div className="grid gap-1.5">
              <Label className="text-xs font-medium text-muted-foreground uppercase">关联智能体</Label>
              <Select
                value={form.agent_id}
                onValueChange={(value) => setForm({ ...form, agent_id: value ?? "" })}
              >
                <SelectTrigger className="h-9 w-full text-xs">
                  <SelectValue>
                    {agentsById.get(form.agent_id)?.name ?? "选择执行任务的智能体"}
                  </SelectValue>
                </SelectTrigger>
                <SelectContent>
                  {agents.map((agent) => (
                    <SelectItem key={agent.id} value={agent.id} className="text-xs">
                      {agent.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            <div className="grid gap-1.5">
              <Label htmlFor="routine-name" className="text-xs font-medium text-muted-foreground uppercase">任务名称</Label>
              <Input
                id="routine-name"
                value={form.name}
                onChange={(event) => setForm({ ...form, name: event.target.value })}
                placeholder="例如: 每日早间代码库诊断"
                className="text-xs"
              />
            </div>

            <div className="grid gap-1.5">
              <Label htmlFor="routine-prompt" className="text-xs font-medium text-muted-foreground uppercase">任务指令文本</Label>
              <Textarea
                id="routine-prompt"
                value={form.prompt}
                onChange={(event) => setForm({ ...form, prompt: event.target.value })}
                rows={4}
                placeholder="告知智能体在触发时需要执行的详细任务指令..."
                className="text-xs font-mono resize-none leading-relaxed"
              />
            </div>

            <ScheduleEditor
              cron={form.cron}
              timezone={form.timezone}
              onChange={(next) => setForm({ ...form, ...next })}
            />

            <div className="grid gap-1.5">
              <Label className="text-xs font-medium text-muted-foreground uppercase">状态控制</Label>
              <Select
                value={form.status}
                onValueChange={(value) => setForm({ ...form, status: value || "active" })}
              >
                <SelectTrigger className="h-9 w-full text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="active" className="text-xs">启用 (Active)</SelectItem>
                  <SelectItem value="paused" className="text-xs">暂停 (Paused)</SelectItem>
                </SelectContent>
              </Select>
            </div>

            {formError && <div className="text-xs text-destructive rounded-md bg-destructive/10 p-2.5 font-mono">{formError}</div>}
          </div>

          <DialogFooter className="gap-2">
            <Button variant="outline" size="sm" onClick={() => setOpen(false)} disabled={saving} className="text-xs">
              取消
            </Button>
            <Button size="sm" onClick={() => void save()} disabled={saving} className="text-xs bg-amber-600 hover:bg-amber-700 text-white font-medium">
              {saving ? "提交保存中..." : "保存定时任务"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Logs View Dialog */}
      <Dialog open={logsOpen} onOpenChange={setLogsOpen}>
        <DialogContent className="flex max-h-[88vh] w-[92vw] max-w-3xl flex-col rounded-2xl">
          <DialogHeader>
            <DialogTitle className="text-sm font-semibold">{logsTitle}</DialogTitle>
          </DialogHeader>
          {logsError && <div className="text-xs text-destructive rounded-md bg-destructive/10 p-2 font-mono">{logsError}</div>}
          <pre className="min-h-48 overflow-auto rounded-xl border border-border/70 bg-muted/40 p-4 font-mono text-xs leading-relaxed text-foreground whitespace-pre-wrap">
            {logsLoading ? "正在载入运行日志..." : logsText}
          </pre>
        </DialogContent>
      </Dialog>
    </div>
  );
}
