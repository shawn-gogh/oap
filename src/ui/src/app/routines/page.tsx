"use client";

import { useEffect, useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import { Pencil, Play, Plus, Trash2, Zap } from "lucide-react";
import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
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
  if (!ms) return "Never";
  const diff = Math.max(0, Date.now() - ms);
  const mins = Math.floor(diff / 60000);
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  return `${Math.floor(hrs / 24)}d ago`;
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
  const [logsTitle, setLogsTitle] = useState("Routine run logs");
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
      setError(err instanceof Error ? err.message : "Failed to load routines");
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
      if (!form.agent_id) throw new Error("Agent is required");
      if (!form.name.trim()) throw new Error("Name is required");
      if (!form.cron.trim()) throw new Error("Schedule is required");
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
      setFormError(err instanceof Error ? err.message : "Failed to save routine");
    } finally {
      setSaving(false);
    }
  };

  const remove = async (routine: Routine) => {
    if (!confirm(`Delete routine "${routine.name}"?`)) return;
    setRoutines((current) => current?.filter((item) => item.id !== routine.id) ?? null);
    try {
      await deleteRoutine(routine.id);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to delete routine");
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
      setError(err instanceof Error ? err.message : "Failed to trigger routine");
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
    setLogsTitle(`${routine.name} logs`);
    setLogsText("");
    setLogsError(null);
    setLogsLoading(true);
    try {
      const logs = await getAgentRunLogs(routine.agent_id, routine.last_run_id);
      setLogsText(logs.trim() ? logs : "No logs were captured.");
    } catch (err) {
      setLogsError(err instanceof Error ? err.message : "Failed to load run logs");
    } finally {
      setLogsLoading(false);
    }
  };

  return (
    <div className="flex h-screen bg-background text-foreground">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border px-4">
          <h1 className="text-sm font-semibold">定时任务</h1>
          <div className="flex items-center gap-2">
            <Button size="sm" onClick={openCreate} disabled={agents.length === 0}>
              <Plus className="size-4" />
              New routine
            </Button>
            <ThemeToggle />
          </div>
        </header>

        <main className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto flex max-w-5xl flex-col gap-3 px-4 py-6">
            {error && (
              <Card className="border-destructive p-3">
                <p className="text-sm text-destructive">{error}</p>
              </Card>
            )}
            {!routines && !error && (
              <div className="text-sm text-muted-foreground">加载中...</div>
            )}
            {routines && routines.length === 0 && (
              <div className="py-16 text-center text-sm text-muted-foreground">
                No routines yet. Create a scheduled job for one of your agents.
              </div>
            )}
            {routines?.map((routine) => {
              const agent = agentsById.get(routine.agent_id);
              const lastRun = timeAgo(routine.last_run_at);
              const canOpenLastRun = Boolean(routine.last_session_id || routine.last_run_id);
              return (
                <Card key={routine.id} className="flex items-start justify-between gap-4 p-4">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="truncate text-sm font-medium">{routine.name}</span>
                      <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground">
                        {routine.status}
                      </span>
                    </div>
                    <p className="mt-1 text-xs text-muted-foreground">
                      {agent?.name ?? routine.agent_id}
                    </p>
                    {routine.prompt && (
                      <p className="mt-1 line-clamp-1 font-mono text-xs text-muted-foreground/80">
                        {routine.prompt}
                      </p>
                    )}
                    <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
                      <span className="inline-flex items-center gap-1.5">
                        <Zap className="size-3" />
                        <span className="font-mono">
                          {scheduleLabel(routine.cron, routine.timezone)}
                        </span>
                      </span>
                      {canOpenLastRun ? (
                        <button
                          type="button"
                          onClick={() => void openLastRun(routine)}
                          className="underline decoration-border underline-offset-2 hover:text-foreground"
                        >
                          Last run {lastRun}
                        </button>
                      ) : (
                        <span>上次运行 {lastRun}</span>
                      )}
                    </div>
                  </div>
                  <div className="flex shrink-0 items-center gap-1">
                    <Button
                      size="sm"
                      onClick={() => void trigger(routine)}
                      disabled={triggeringId === routine.id}
                    >
                      <Play className="size-3.5" />
                      {triggeringId === routine.id ? "Starting" : "Trigger"}
                    </Button>
                    <Button size="sm" variant="outline" onClick={() => openEdit(routine)} aria-label="Edit">
                      <Pencil className="size-3.5" />
                    </Button>
                    <Button size="sm" variant="outline" onClick={() => void remove(routine)} aria-label="Delete">
                      <Trash2 className="size-3.5" />
                    </Button>
                  </div>
                </Card>
              );
            })}
          </div>
        </main>
      </div>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="w-[92vw] sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>{editingId ? "Edit routine" : "New routine"}</DialogTitle>
          </DialogHeader>
          <div className="grid gap-4 py-2">
            <div className="grid gap-1.5">
              <Label>智能体</Label>
              <Select
                value={form.agent_id}
                onValueChange={(value) => setForm({ ...form, agent_id: value ?? "" })}
              >
                <SelectTrigger className="h-8 w-full">
                  <SelectValue>
                    {agentsById.get(form.agent_id)?.name ?? "Select agent"}
                  </SelectValue>
                </SelectTrigger>
                <SelectContent>
                  {agents.map((agent) => (
                    <SelectItem key={agent.id} value={agent.id}>
                      {agent.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="routine-name">名称</Label>
              <Input
                id="routine-name"
                value={form.name}
                onChange={(event) => setForm({ ...form, name: event.target.value })}
                placeholder="每日代码审查"
              />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="routine-prompt">指令</Label>
              <Textarea
                id="routine-prompt"
                value={form.prompt}
                onChange={(event) => setForm({ ...form, prompt: event.target.value })}
                rows={5}
                placeholder="告诉智能体每次运行时要做什么。"
              />
            </div>
            <ScheduleEditor
              cron={form.cron}
              timezone={form.timezone}
              onChange={(next) => setForm({ ...form, ...next })}
            />
            <div className="grid gap-1.5">
              <Label>状态</Label>
              <Select
                value={form.status}
                onValueChange={(value) => setForm({ ...form, status: value || "active" })}
              >
                <SelectTrigger className="h-8 w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="active">启用</SelectItem>
                  <SelectItem value="paused">暂停</SelectItem>
                </SelectContent>
              </Select>
            </div>
            {formError && <p className="text-sm text-destructive">{formError}</p>}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setOpen(false)} disabled={saving}>
              Cancel
            </Button>
            <Button onClick={() => void save()} disabled={saving}>
              {saving ? "Saving..." : "Save routine"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={logsOpen} onOpenChange={setLogsOpen}>
        <DialogContent className="flex max-h-[88vh] w-[92vw] max-w-3xl flex-col">
          <DialogHeader>
            <DialogTitle>{logsTitle}</DialogTitle>
          </DialogHeader>
          {logsError && <p className="text-sm text-destructive">{logsError}</p>}
          <pre className="min-h-48 overflow-auto rounded border border-border bg-muted p-3 font-mono text-xs text-muted-foreground">
            {logsLoading ? "Loading..." : logsText}
          </pre>
        </DialogContent>
      </Dialog>
    </div>
  );
}
