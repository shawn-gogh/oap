"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { toast } from "sonner";
import { Bot, History, Loader2, MessagesSquare, Plus, Search, Trash2, X } from "lucide-react";
import { BrandIcon } from "@/components/brand-icons";
import { EmptyState } from "@/components/empty-state";
import { Sidebar } from "@/components/sidebar";
import { StatusDot, type StatusDotTone } from "@/components/status-dot";
import { ThemeToggle } from "@/components/theme-toggle";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { apiErrorMessage, deleteSession, listAgents, listSessions } from "@/lib/api";
import { runtimeBrandIconId } from "@/lib/runtime-branding";
import type { Agent, OpencodeSession } from "@/lib/types";

/** How many rows render before the "加载更多" button appears. Sessions come
 *  back unpaginated from GET /session, so the cap is purely about keeping the
 *  DOM small on accounts with hundreds of sessions. */
const PAGE_SIZE = 60;

type StatusFilter = "all" | "busy" | "failed" | "idle";
type RangeFilter = "all" | "today" | "7d" | "30d";
type SortKey = "updated" | "created";

const STATUS_LABEL: Record<Exclude<StatusFilter, "all">, string> = {
  busy: "进行中",
  failed: "异常",
  idle: "已结束",
};

const STATUS_TONE: Record<Exclude<StatusFilter, "all">, StatusDotTone> = {
  busy: "success",
  failed: "error",
  idle: "idle",
};

function sessionStatus(session: OpencodeSession): Exclude<StatusFilter, "all"> {
  const status = (session.status ?? "").toLowerCase();
  if (status === "busy" || status === "running" || status === "starting") return "busy";
  if (status === "failed" || status === "error" || status === "timed_out") return "failed";
  return "idle";
}

function sessionTime(session: OpencodeSession, sort: SortKey): number {
  if (sort === "created") return session.time?.created ?? 0;
  return session.time?.updated ?? session.time?.created ?? 0;
}

function relativeTime(ts: number): string {
  if (!ts) return "";
  const minutes = Math.round((Date.now() - ts) / 60000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.round(hours / 24);
  if (days < 30) return `${days} 天前`;
  return new Date(ts).toLocaleDateString("zh-CN");
}

function exactTime(ts: number): string {
  return ts ? new Date(ts).toLocaleString("zh-CN") : "";
}

function startOfDay(date: Date): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

/** Bucket label for the date group a session belongs to. The buckets are
 *  coarse on purpose — scanning "今天 / 昨天 / 本周" is faster than reading
 *  dates on every row. */
function bucketLabel(ts: number): string {
  if (!ts) return "时间未知";
  const today = startOfDay(new Date());
  const day = 86_400_000;
  if (ts >= today) return "今天";
  if (ts >= today - day) return "昨天";
  if (ts >= today - 7 * day) return "本周";
  if (ts >= today - 30 * day) return "本月";
  const date = new Date(ts);
  return `${date.getFullYear()} 年 ${date.getMonth() + 1} 月`;
}

function rangeFloor(range: RangeFilter): number {
  const today = startOfDay(new Date());
  if (range === "today") return today;
  if (range === "7d") return today - 7 * 86_400_000;
  if (range === "30d") return today - 30 * 86_400_000;
  return 0;
}

export default function SessionHistoryPage() {
  const router = useRouter();
  const [sessions, setSessions] = useState<OpencodeSession[] | null>(null);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [agentFilter, setAgentFilter] = useState("all");
  const [runtimeFilter, setRuntimeFilter] = useState("all");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [rangeFilter, setRangeFilter] = useState<RangeFilter>("all");
  const [sort, setSort] = useState<SortKey>("updated");
  const [limit, setLimit] = useState(PAGE_SIZE);
  const [selected, setSelected] = useState<Set<string>>(new Set());

  useEffect(() => {
    const load = async () => {
      try {
        // The registry creates an "agent-builder-*" companion session per
        // registered agent; those mirror real chats and only add noise here.
        const list = await listSessions();
        setSessions(list.filter((s) => !s.title?.startsWith("agent-builder-")));
        setError(null);
      } catch (e) {
        setError(apiErrorMessage(e, "加载会话历史失败"));
      }
    };
    load();
    const timer = setInterval(load, 20000);
    return () => clearInterval(timer);
  }, []);

  useEffect(() => {
    listAgents()
      .then(setAgents)
      .catch(() => setAgents([]));
  }, []);

  const agentName = useMemo(() => {
    const map = new Map<string, string>();
    for (const agent of agents) map.set(agent.id, agent.name);
    return map;
  }, [agents]);

  // Undo-window delete: rows disappear at once, the backend call only fires
  // after the toast expires. Mirrors the sidebar's behaviour.
  const pendingDeletes = useRef(new Map<string, number>());
  useEffect(() => () => pendingDeletes.current.forEach((timer) => window.clearTimeout(timer)), []);

  const removeSessions = (ids: string[]) => {
    if (ids.length === 0) return;
    const removed = sessions?.filter((s) => ids.includes(s.id)) ?? [];
    setSessions((prev) => prev?.filter((s) => !ids.includes(s.id)) ?? null);
    setSelected(new Set());

    const restore = () => setSessions((prev) => (prev ? [...removed, ...prev] : prev));
    const timer = window.setTimeout(() => {
      ids.forEach((id) => pendingDeletes.current.delete(id));
      Promise.all(ids.map((id) => deleteSession(id))).catch((err) => {
        restore();
        toast.error(apiErrorMessage(err, "删除会话失败"));
      });
    }, 5000);
    ids.forEach((id) => pendingDeletes.current.set(id, timer));

    const label =
      ids.length === 1
        ? `已删除会话「${removed[0]?.title?.trim() || ids[0].slice(0, 12)}」`
        : `已删除 ${ids.length} 个会话`;
    toast(label, {
      duration: 5000,
      action: {
        label: "撤销",
        onClick: () => {
          window.clearTimeout(timer);
          ids.forEach((id) => pendingDeletes.current.delete(id));
          restore();
        },
      },
    });
  };

  const runtimeOptions = useMemo(() => {
    const set = new Set<string>();
    for (const session of sessions ?? []) if (session.runtime) set.add(session.runtime);
    return [...set].sort();
  }, [sessions]);

  const agentOptions = useMemo(() => {
    const set = new Set<string>();
    for (const session of sessions ?? []) {
      const id = session.agent_id ?? session.agent;
      if (id) set.add(id);
    }
    return [...set].sort((a, b) => (agentName.get(a) ?? a).localeCompare(agentName.get(b) ?? b));
  }, [sessions, agentName]);

  const filtered = useMemo(() => {
    if (!sessions) return null;
    const needle = query.trim().toLowerCase();
    const floor = rangeFloor(rangeFilter);
    return sessions
      .filter((session) => {
        if (needle) {
          const haystack = `${session.title ?? ""} ${session.id}`.toLowerCase();
          if (!haystack.includes(needle)) return false;
        }
        if (agentFilter !== "all" && (session.agent_id ?? session.agent) !== agentFilter) return false;
        if (runtimeFilter !== "all" && session.runtime !== runtimeFilter) return false;
        if (statusFilter !== "all" && sessionStatus(session) !== statusFilter) return false;
        if (floor && sessionTime(session, sort) < floor) return false;
        return true;
      })
      .sort((a, b) => sessionTime(b, sort) - sessionTime(a, sort));
  }, [sessions, query, agentFilter, runtimeFilter, statusFilter, rangeFilter, sort]);

  useEffect(() => setLimit(PAGE_SIZE), [query, agentFilter, runtimeFilter, statusFilter, rangeFilter, sort]);

  const visible = filtered?.slice(0, limit) ?? null;

  // Date buckets, in the order the sorted list produced them.
  const groups = useMemo(() => {
    const buckets: { label: string; items: OpencodeSession[] }[] = [];
    for (const session of visible ?? []) {
      const label = bucketLabel(sessionTime(session, sort));
      const last = buckets[buckets.length - 1];
      if (last?.label === label) last.items.push(session);
      else buckets.push({ label, items: [session] });
    }
    return buckets;
  }, [visible, sort]);

  const filtersActive =
    Boolean(query.trim()) ||
    agentFilter !== "all" ||
    runtimeFilter !== "all" ||
    statusFilter !== "all" ||
    rangeFilter !== "all";

  const resetFilters = () => {
    setQuery("");
    setAgentFilter("all");
    setRuntimeFilter("all");
    setStatusFilter("all");
    setRangeFilter("all");
  };

  const toggleSelected = (id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  return (
    <div className="flex h-screen overflow-hidden bg-background text-foreground">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border/80 bg-background/80 px-4 backdrop-blur">
          <div className="flex items-center gap-2.5">
            <div className="flex size-7 items-center justify-center rounded-md bg-blue-500/10 text-blue-600 ring-1 ring-blue-500/20 dark:text-blue-400">
              <History className="size-4" />
            </div>
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold tracking-tight">会话历史</span>
              {filtered && (
                <span className="text-xs font-medium text-muted-foreground">
                  / {filtered.length} 个会话
                </span>
              )}
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button size="sm" className="gap-1.5 text-xs font-medium" onClick={() => router.push("/chat/")}>
              <Plus className="size-4" />
              新建会话
            </Button>
            <ThemeToggle />
          </div>
        </header>

        <div className="shrink-0 space-y-2 border-b border-border/80 px-4 py-3">
          <div className="flex flex-wrap items-center gap-2">
            <div className="relative min-w-[220px] flex-1">
              <Search className="absolute left-3 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="搜索会话标题或 ID..."
                aria-label="搜索会话历史"
                className="h-9 w-full rounded-xl border border-border/70 bg-card pl-9 pr-3 text-sm outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-blue-500/40"
              />
            </div>

            <Select value={agentFilter} onValueChange={(value) => setAgentFilter(value ?? "all")}>
              <SelectTrigger className="h-9 w-[170px] rounded-xl text-xs" aria-label="按智能体筛选">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">全部智能体</SelectItem>
                {agentOptions.map((id) => (
                  <SelectItem key={id} value={id}>
                    {agentName.get(id) ?? id}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>

            {runtimeOptions.length > 0 && (
              <Select value={runtimeFilter} onValueChange={(value) => setRuntimeFilter(value ?? "all")}>
                <SelectTrigger className="h-9 w-[170px] rounded-xl text-xs" aria-label="按运行时筛选">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">全部运行时</SelectItem>
                  {runtimeOptions.map((runtime) => (
                    <SelectItem key={runtime} value={runtime}>
                      {runtime}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}

            <Select
              value={statusFilter}
              onValueChange={(value) => setStatusFilter(value as StatusFilter)}
            >
              <SelectTrigger className="h-9 w-[130px] rounded-xl text-xs" aria-label="按状态筛选">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">全部状态</SelectItem>
                <SelectItem value="busy">进行中</SelectItem>
                <SelectItem value="failed">异常</SelectItem>
                <SelectItem value="idle">已结束</SelectItem>
              </SelectContent>
            </Select>

            <Select value={rangeFilter} onValueChange={(value) => setRangeFilter(value as RangeFilter)}>
              <SelectTrigger className="h-9 w-[120px] rounded-xl text-xs" aria-label="按时间筛选">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">全部时间</SelectItem>
                <SelectItem value="today">今天</SelectItem>
                <SelectItem value="7d">近 7 天</SelectItem>
                <SelectItem value="30d">近 30 天</SelectItem>
              </SelectContent>
            </Select>

            <Select value={sort} onValueChange={(value) => setSort(value as SortKey)}>
              <SelectTrigger className="h-9 w-[130px] rounded-xl text-xs" aria-label="排序方式">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="updated">最近活动</SelectItem>
                <SelectItem value="created">创建时间</SelectItem>
              </SelectContent>
            </Select>

            {filtersActive && (
              <Button variant="ghost" size="sm" className="gap-1 text-xs" onClick={resetFilters}>
                <X className="size-3.5" />
                清除筛选
              </Button>
            )}
          </div>

          {selected.size > 0 && (
            <div className="flex items-center gap-3 rounded-xl border border-border/70 bg-muted/40 px-3 py-2 text-xs">
              <span className="font-medium">已选中 {selected.size} 个会话</span>
              <Button
                variant="ghost"
                size="sm"
                className="gap-1 text-xs text-destructive hover:bg-destructive/10 hover:text-destructive"
                onClick={() => removeSessions([...selected])}
              >
                <Trash2 className="size-3.5" />
                批量删除
              </Button>
              <Button variant="ghost" size="sm" className="text-xs" onClick={() => setSelected(new Set())}>
                取消选择
              </Button>
            </div>
          )}
        </div>

        <main className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
          <div className="mx-auto max-w-4xl space-y-6">
            {error && (
              <div className="rounded-xl border border-destructive/30 bg-destructive/5 px-4 py-3 font-mono text-xs text-destructive">
                {error}
              </div>
            )}

            {!sessions && !error && (
              <div className="flex items-center justify-center gap-2 py-16 text-sm text-muted-foreground">
                <Loader2 className="size-4 animate-spin" />
                正在加载会话历史...
              </div>
            )}

            {filtered && filtered.length === 0 && (
              <EmptyState
                icon={MessagesSquare}
                title={filtersActive ? "没有匹配的会话" : "暂无会话历史"}
                hint={filtersActive ? "试试放宽筛选条件或清空搜索词。" : "新建一个会话后，历史会出现在这里。"}
                action={
                  filtersActive ? (
                    <Button variant="outline" size="sm" onClick={resetFilters}>
                      清除筛选
                    </Button>
                  ) : (
                    <Button size="sm" onClick={() => router.push("/chat/")}>
                      新建会话
                    </Button>
                  )
                }
              />
            )}

            {groups.map((group) => (
              <section key={group.label} className="space-y-1.5">
                <div className="sticky top-0 z-10 -mx-1 bg-background/95 px-1 py-1.5 backdrop-blur">
                  <h2 className="text-[11px] font-bold uppercase tracking-wider text-muted-foreground">
                    {group.label}
                    <span className="ml-2 font-mono font-normal">{group.items.length}</span>
                  </h2>
                </div>
                <div className="overflow-hidden rounded-xl border border-border/70 bg-card">
                  {group.items.map((session, index) => {
                    const status = sessionStatus(session);
                    const ts = sessionTime(session, sort);
                    const id = session.agent_id ?? session.agent;
                    const isSelected = selected.has(session.id);
                    return (
                      <div
                        key={session.id}
                        onClick={() => router.push(`/chat/?id=${encodeURIComponent(session.id)}`)}
                        role="button"
                        tabIndex={0}
                        onKeyDown={(event) => {
                          if (event.key === "Enter" || event.key === " ") {
                            event.preventDefault();
                            router.push(`/chat/?id=${encodeURIComponent(session.id)}`);
                          }
                        }}
                        className={`group flex cursor-pointer items-center gap-3 px-3 py-2.5 transition-colors ${
                          index > 0 ? "border-t border-border/60" : ""
                        } ${isSelected ? "bg-blue-500/5" : "hover:bg-muted/50"}`}
                      >
                        <input
                          type="checkbox"
                          checked={isSelected}
                          onClick={(event) => event.stopPropagation()}
                          onChange={() => toggleSelected(session.id)}
                          aria-label={`选择会话 ${session.title || session.id}`}
                          className="size-3.5 shrink-0 accent-blue-600"
                        />
                        <span className="flex size-7 shrink-0 items-center justify-center rounded-lg border border-border bg-muted">
                          {session.runtime ? (
                            <BrandIcon id={runtimeBrandIconId(session.runtime, null)} className="size-3.5" />
                          ) : (
                            <Bot className="size-3.5 text-muted-foreground" />
                          )}
                        </span>
                        <div className="min-w-0 flex-1">
                          <div className="flex min-w-0 items-center gap-2">
                            <StatusDot tone={STATUS_TONE[status]} label={STATUS_LABEL[status]} />
                            <span className="truncate text-sm font-medium" title={session.id}>
                              {session.title?.trim() || session.id.slice(0, 12)}
                            </span>
                          </div>
                          <div className="mt-0.5 flex min-w-0 items-center gap-2 text-[11px] text-muted-foreground">
                            {id && <span className="truncate">{agentName.get(id) ?? id}</span>}
                            {id && session.runtime && <span aria-hidden>·</span>}
                            {session.runtime && <span className="truncate">{session.runtime}</span>}
                          </div>
                        </div>
                        <span
                          className="shrink-0 text-xs text-muted-foreground tabular-nums"
                          title={exactTime(ts)}
                        >
                          {relativeTime(ts)}
                        </span>
                        <button
                          type="button"
                          onClick={(event) => {
                            event.stopPropagation();
                            removeSessions([session.id]);
                          }}
                          className="shrink-0 rounded-md p-1 opacity-0 transition-opacity hover:bg-destructive/10 hover:text-destructive focus-visible:opacity-100 focus-visible:outline-none group-hover:opacity-100"
                          aria-label="删除会话"
                          title="删除会话"
                        >
                          <Trash2 className="size-3.5" />
                        </button>
                      </div>
                    );
                  })}
                </div>
              </section>
            ))}

            {filtered && filtered.length > limit && (
              <div className="flex justify-center pb-4">
                <Button variant="outline" size="sm" onClick={() => setLimit((prev) => prev + PAGE_SIZE)}>
                  加载更多（还有 {filtered.length - limit} 个）
                </Button>
              </div>
            )}
          </div>
        </main>
      </div>
    </div>
  );
}
