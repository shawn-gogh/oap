"use client";

import { Suspense, useCallback, useEffect, useMemo, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import {
  AlertCircle,
  CheckCircle2,
  Clock3,
  ExternalLink,
  Inbox as InboxIcon,
  RefreshCw,
  ShieldCheck,
  Zap,
} from "lucide-react";
import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { Button } from "@/components/ui/button";
import { ToolApprovalPanel } from "@/components/tool-approval-panel";
import { RunDrawer } from "@/components/run/RunDrawer";
import { RevisionDiffPanel } from "./revision-diff-panel";
import {
  acceptApproval,
  apiErrorMessage,
  getActiveTurn,
  listInbox,
  rejectApproval,
  retryApprovalDelivery,
  resolveInboxItem,
  type InboxFilter,
  type InboxItem,
} from "@/lib/api";

function timeAgo(ts?: number | null): string {
  if (!ts) return "";
  const secs = Math.max(0, Math.floor((Date.now() - ts) / 1000));
  if (secs < 60) return `${secs} 秒前`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins} 分钟前`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs} 小时前`;
  return `${Math.floor(hrs / 24)} 天前`;
}

function formatDate(ts?: number | null): string {
  if (!ts) return "未知";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(ts));
}

const TABS: { key: InboxFilter; label: string }[] = [
  { key: "attention", label: "待处理" },
  { key: "completed", label: "已完成" },
  { key: "all", label: "全部消息" },
];

const statusStyles: Record<string, { label: string; cls: string }> = {
  pending: { label: "待审批", cls: "border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400" },
  open: { label: "待处理问题", cls: "border-blue-500/30 bg-blue-500/10 text-blue-600 dark:text-blue-400" },
  accepted: {
    label: "已批准",
    cls: "border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400",
  },
  rejected: {
    label: "已拒绝",
    cls: "border-destructive/30 bg-destructive/10 text-destructive",
  },
  resolved: {
    label: "已解决",
    cls: "border-border bg-muted text-muted-foreground",
  },
  expired: {
    label: "已过期",
    cls: "border-border bg-muted text-muted-foreground",
  },
};

function isApprovalKind(kind: InboxItem["kind"]): boolean {
  return kind !== "issue";
}

function approvalKindLabel(item: InboxItem): string {
  const labels: Partial<Record<InboxItem["kind"], string>> = {
    approval: "兼容审批",
    business_decision: "业务决策",
    tool_permission: "运行时权限",
    runtime_permission: "运行时权限",
    unlisted_data_egress: "数据外发",
    data_egress: "数据外发",
    agent_publish: "智能体发布",
    agent_change: "智能体变更",
    platform_action: "平台操作",
  };
  return labels[item.kind] ?? item.kind;
}

function StatusTag({ item }: { item: InboxItem }) {
  const s =
    statusStyles[item.status] ?? {
      label: item.status,
      cls: "border-border bg-muted text-muted-foreground",
    };
  const Icon = item.status === "pending" || item.status === "open" ? AlertCircle : CheckCircle2;
  return (
    <span className={`inline-flex h-6 items-center gap-1.5 rounded-md border px-2 text-[11px] font-medium ${s.cls}`}>
      <Icon className="size-3" />
      {s.label}
    </span>
  );
}

function preview(item: InboxItem): string {
  if (item.body) return item.body;
  if (item.args) {
    const v = Object.values(item.args)[0];
    if (typeof v === "string") return v;
    if (v != null) return JSON.stringify(v);
  }
  return "";
}

function itemTone(item: InboxItem): string {
  if (item.status === "pending" || item.status === "open") return "bg-card";
  return "bg-background";
}

function attentionDot(item: InboxItem): string {
  if (item.status === "pending") return "bg-amber-500 animate-pulse";
  if (item.status === "open") return "bg-blue-500";
  return "bg-muted-foreground/35";
}

function EmptyState({ tab }: { tab: InboxFilter }) {
  return (
    <div className="flex h-full min-h-[360px] items-center justify-center px-6">
      <div className="max-w-sm text-center space-y-3">
        <div className="mx-auto flex size-12 items-center justify-center rounded-2xl border border-border bg-muted/30 text-muted-foreground">
          <ShieldCheck className="size-6 text-emerald-500" />
        </div>
        <div className="text-sm font-semibold text-foreground">
          {tab === "attention" ? "暂无待阻塞的人工处理任务" : "收件箱无记录"}
        </div>
        <p className="text-xs text-muted-foreground leading-relaxed">
          {tab === "attention"
            ? "当智能体的工具调用或执行流程需要人工审批确认时，消息会在此处显示。"
            : "可切换顶部标签或稍后手动刷新接收新动态。"}
        </p>
      </div>
    </div>
  );
}

function inboxItemIdParam(value: string | null): string | null {
  if (!value) return null;
  return value.startsWith("appr_") || value.startsWith("iss_") ? value : null;
}

function InboxInner() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const requestedItemId = inboxItemIdParam(searchParams.get("item"));
  const [tab, setTab] = useState<InboxFilter>(() => (requestedItemId ? "all" : "attention"));
  const [items, setItems] = useState<InboxItem[] | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [runDrawerTurn, setRunDrawerTurn] = useState<{ sessionId: string; turnId: string } | null>(null);
  const [resolvingRun, setResolvingRun] = useState(false);
  const [runResolveError, setRunResolveError] = useState<string | null>(null);

  const openRun = useCallback(async (sessionId: string) => {
    setResolvingRun(true);
    setRunResolveError(null);
    try {
      const active = await getActiveTurn(sessionId);
      if (!active) {
        setRunResolveError("该会话当前没有活跃的 Turn。");
        return;
      }
      setRunDrawerTurn({ sessionId, turnId: active.turn.id });
    } catch (e) {
      setRunResolveError(apiErrorMessage(e, "解析 Run 失败"));
    } finally {
      setResolvingRun(false);
    }
  }, []);

  const load = useCallback(async (t: InboxFilter) => {
    try {
      const list = await listInbox(t);
      setItems(list);
      setSelectedId((cur) => {
        if (requestedItemId && list.some((i) => i.id === requestedItemId)) return requestedItemId;
        return cur && list.some((i) => i.id === cur) ? cur : list[0]?.id ?? null;
      });
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [requestedItemId]);

  useEffect(() => {
    load(tab);
    const t = setInterval(() => load(tab), 4000);
    return () => clearInterval(t);
  }, [tab, load]);

  const selected = items?.find((i) => i.id === selectedId) ?? null;
  const counts = useMemo(() => {
    const list = items ?? [];
    return {
      approvals: list.filter((i) => isApprovalKind(i.kind)).length,
      issues: list.filter((i) => i.kind === "issue").length,
      blocked: list.filter((i) => i.status === "pending" || i.status === "open").length,
    };
  }, [items]);

  const onAccept = useCallback(
    async (id: string, args: Record<string, unknown>) => {
      setBusy(true);
      const sessionId = items?.find((item) => item.id === id)?.sessionId ?? null;
      try {
        const result = await acceptApproval(id, args);
        if (sessionId && result.delivery_status === "applied") {
          router.push(`/chat/?id=${encodeURIComponent(sessionId)}&resumed=true`);
          return;
        }
        await load(tab);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [items, load, router, tab],
  );

  const onReject = useCallback(
    async (id: string, feedback: string) => {
      setBusy(true);
      const sessionId = items?.find((item) => item.id === id)?.sessionId ?? null;
      try {
        const result = await rejectApproval(id, feedback);
        if (sessionId && result.delivery_status === "applied") {
          router.push(`/chat/?id=${encodeURIComponent(sessionId)}&resumed=true`);
          return;
        }
        await load(tab);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [items, load, router, tab],
  );

  const onAcceptAlways = useCallback(
    async (id: string, args: Record<string, unknown>) => {
      setBusy(true);
      const sessionId = items?.find((item) => item.id === id)?.sessionId ?? null;
      try {
        const result = await acceptApproval(id, args, "session");
        if (sessionId && result.delivery_status === "applied") {
          router.push(`/chat/?id=${encodeURIComponent(sessionId)}&resumed=true`);
          return;
        }
        await load(tab);
      } catch (error) {
        setError(error instanceof Error ? error.message : String(error));
      } finally {
        setBusy(false);
      }
    },
    [items, load, router, tab],
  );

  const onResolve = useCallback(
    async (id: string) => {
      setBusy(true);
      try {
        await resolveInboxItem(id);
        await load(tab);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [load, tab],
  );

  const onRetryDelivery = useCallback(
    async (id: string) => {
      setBusy(true);
      try {
        await retryApprovalDelivery(id);
        await load(tab);
      } catch (error) {
        setError(error instanceof Error ? error.message : String(error));
      } finally {
        setBusy(false);
      }
    },
    [load, tab],
  );

  const selectItem = useCallback(
    (id: string) => {
      setSelectedId(id);
      const params = new URLSearchParams(searchParams.toString());
      params.set("item", id);
      router.replace(`/inbox/?${params.toString()}`, { scroll: false });
    },
    [router, searchParams],
  );

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden selection:bg-blue-500/20">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        {/* Pure Chinese Header */}
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border/80 px-4 bg-background/80 backdrop-blur">
          <div className="flex items-center gap-2.5">
            <div className="flex size-7 items-center justify-center rounded-md bg-blue-500/10 text-blue-600 dark:text-blue-400 ring-1 ring-blue-500/20">
              <InboxIcon className="size-4" />
            </div>
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold tracking-tight">智能体收件箱</span>
              <span className="text-xs text-muted-foreground font-medium">/ 收件箱</span>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="icon-sm" onClick={() => load(tab)} title="刷新消息队列">
              <RefreshCw className="size-3.5" />
            </Button>
            <ThemeToggle />
          </div>
        </header>

        <main id="main-content" className="flex min-h-0 flex-1 flex-col overflow-hidden">
          {/* Top Summary Banner */}
          <section className="border-b border-border px-6 py-4 bg-card/40">
            <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
              <div className="min-w-0 space-y-1">
                <h1 className="text-xl font-bold tracking-tight leading-tight text-foreground">
                  人工审核与响应队列
                </h1>
                <p className="max-w-2xl text-xs text-muted-foreground leading-relaxed">
                  审批受限的工具调用请求、处理智能体上报的异常，或直接跳回来源会话。
                </p>
              </div>
              <div className="flex flex-wrap items-center gap-x-5 gap-y-2 text-xs">
                <div className="flex items-center gap-1.5">
                  <span className="font-bold text-foreground font-mono text-base">{items ? counts.blocked : "…"}</span>
                  <span className="text-muted-foreground">需人工干预</span>
                </div>
                <div className="h-4 w-px bg-border/60" />
                <div className="flex items-center gap-1.5">
                  <span className="font-bold text-foreground font-mono text-base">{items ? counts.approvals : "…"}</span>
                  <span className="text-muted-foreground">待审批项</span>
                </div>
                <div className="h-4 w-px bg-border/60" />
                <div className="flex items-center gap-1.5">
                  <span className="font-bold text-foreground font-mono text-base">{items ? counts.issues : "…"}</span>
                  <span className="text-muted-foreground">待解决问题</span>
                </div>
                <div className="hidden h-4 w-px bg-border/60 sm:block" />
                <div className="text-[11px] text-muted-foreground font-mono">每 4 秒自动轮询</div>
              </div>
            </div>
          </section>

          {/* Filter Bar */}
          <div className="flex items-center justify-between gap-3 border-b border-border px-4 py-2 bg-muted/10">
            <div className="flex items-center gap-1 rounded-xl border border-border/70 bg-muted/30 p-1">
              {TABS.map((t) => (
                <button
                  key={t.key}
                  onClick={() => setTab(t.key)}
                  className={`h-7 rounded-lg px-3 text-xs font-medium transition-all ${
                    tab === t.key
                      ? "bg-background text-foreground shadow-2xs font-semibold"
                      : "text-muted-foreground hover:text-foreground"
                  }`}
                >
                  {t.label}
                </button>
              ))}
            </div>
            <div className="hidden items-center gap-2 text-xs text-muted-foreground sm:flex font-mono">
              <Clock3 className="size-3.5" />
              <span>{items ? `${items.length} 条数据` : "正在加载队列..."}</span>
            </div>
          </div>

          {/* Master Detail View */}
          <div className="flex min-h-0 flex-1 flex-col md:flex-row">
            {/* Master List */}
            <div className="flex max-h-[42vh] w-full min-w-0 flex-col border-b border-border md:max-h-none md:w-[42%] md:min-w-[340px] md:border-b-0 md:border-r xl:w-[460px] bg-card/20">
              <div className="flex-1 overflow-y-auto">
                {error && (
                  <div className="m-3 rounded-xl border border-destructive/30 bg-destructive/10 px-3.5 py-2.5 text-xs text-destructive font-mono">
                    {error}
                  </div>
                )}
                {!items && !error && (
                  <div className="space-y-2 px-4 py-3" aria-label="正在加载收件箱">
                    {[...Array(5)].map((_, i) => (
                      <div key={i} className="animate-pulse rounded-xl border border-border/50 bg-muted/40 px-4 py-3">
                        <div className="flex items-start gap-2">
                          <div className="mt-2 size-1.5 shrink-0 rounded-full bg-muted-foreground/20" />
                          <div className="min-w-0 flex-1 space-y-2">
                            <div className="h-3 w-2/3 rounded bg-muted-foreground/20" />
                            <div className="h-2.5 w-1/3 rounded bg-muted-foreground/15" />
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
                {items && items.length === 0 && <EmptyState tab={tab} />}
                {items?.map((item) => {
                  const active = item.id === selectedId;
                  const itemPreview = preview(item);
                  return (
                    <button
                      key={item.id}
                      onClick={() => selectItem(item.id)}
                      className={`flex w-full border-b border-border/50 px-4 py-3.5 text-left transition-all ${itemTone(item)} ${
                        active ? "bg-muted/70 border-l-2 border-l-blue-500" : "hover:bg-muted/30"
                      }`}
                    >
                      <div className="min-w-0 flex-1">
                        <div className="flex items-start gap-2.5">
                          <div className="mt-1.5 flex size-3 shrink-0 items-center justify-center">
                            <span className={`size-2 rounded-full ${attentionDot(item)}`} />
                          </div>
                          <div className="min-w-0 flex-1">
                            <div className="flex items-center gap-2">
                              <span className="truncate text-xs font-semibold text-foreground">{item.title}</span>
                              <span className="ml-auto shrink-0 text-[10px] font-mono text-muted-foreground">
                                {timeAgo(item.createdAt)}
                              </span>
                            </div>
                            <div className="mt-1 flex min-w-0 flex-wrap items-center gap-1.5 text-[11px]">
                              <span className="text-muted-foreground font-medium">
                                {statusStyles[item.status]?.label ?? item.status}
                              </span>
                              <span className="text-muted-foreground/40">/</span>
                              <span className="truncate text-muted-foreground">
                                {item.agent ?? "通用智能体"}
                              </span>
                            </div>
                            {itemPreview && (
                              <p className="mt-1.5 line-clamp-2 text-[11px] leading-relaxed text-muted-foreground font-mono bg-muted/20 p-1.5 rounded border border-border/30">
                                {itemPreview}
                              </p>
                            )}
                          </div>
                        </div>
                      </div>
                    </button>
                  );
                })}
              </div>
            </div>

            {/* Detail View */}
            <div className="min-w-0 flex-1 overflow-y-auto p-5 bg-background">
              {!selected ? (
                <EmptyState tab={tab} />
              ) : (
                <div className="mx-auto flex w-full max-w-4xl flex-col gap-4">
                  <div className="rounded-2xl border border-border/70 bg-card shadow-2xs overflow-hidden">
                    <div className="flex flex-col gap-4 border-b border-border/70 px-5 py-4 lg:flex-row lg:items-start lg:justify-between bg-muted/20">
                      <div className="min-w-0">
                        <div className="flex flex-wrap items-center gap-2">
                          <StatusTag item={selected} />
                          {selected.escalatedAt && (
                            <span className="inline-flex h-6 items-center rounded-md border border-amber-500/30 bg-amber-500/10 px-2 text-[11px] font-medium text-amber-600 dark:text-amber-400">
                              升级至权限组 {selected.escalationRole}
                            </span>
                          )}
                          <span className="text-xs text-muted-foreground font-medium">
                            {approvalKindLabel(selected)} · {selected.enforcementOwner} 执行
                          </span>
                        </div>
                        <h2 className="mt-2 text-base font-bold tracking-tight text-foreground">{selected.title}</h2>
                        <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                          <span>智能体: {selected.agent ?? "未指定"}</span>
                          <span>创建时间: {formatDate(selected.createdAt)}</span>
                          {selected.resolvedAt && <span>解决于: {formatDate(selected.resolvedAt)}</span>}
                        </div>
                      </div>
                      {selected.sessionId && (
                        <div className="flex shrink-0 items-center gap-2">
                          {selected.status === "pending" && (
                            <Button
                              variant="outline"
                              size="sm"
                              className="h-8 text-xs gap-1.5"
                              disabled={resolvingRun}
                              onClick={() => void openRun(selected.sessionId!)}
                            >
                              <Zap className="size-3.5" />
                              {resolvingRun ? "解析中…" : "打开 Run"}
                            </Button>
                          )}
                          <Button
                            variant="outline"
                            size="sm"
                            className="h-8 text-xs gap-1.5"
                            onClick={() => router.push(`/chat/?id=${encodeURIComponent(selected.sessionId!)}`)}
                          >
                            <ExternalLink className="size-3.5" />
                            跳转至会话
                          </Button>
                        </div>
                      )}
                    </div>

                    {runResolveError && (
                      <div className="border-b border-border/70 bg-destructive/5 px-5 py-2.5 text-xs text-destructive">
                        {runResolveError}
                      </div>
                    )}

                    {selected.body && (
                      <div className="border-b border-border/70 px-5 py-3.5 bg-background/50">
                        <div className="text-[11px] font-semibold text-muted-foreground uppercase tracking-wider">智能体说明备注</div>
                        <p className="mt-1.5 whitespace-pre-wrap text-xs leading-relaxed text-foreground font-mono">{selected.body}</p>
                      </div>
                    )}

                    <div className="grid grid-cols-2 gap-px bg-border/60 text-xs md:grid-cols-4">
                      <div className="bg-card px-4 py-3">
                        <div className="text-[11px] text-muted-foreground font-medium">条目 ID</div>
                        <div className="mt-0.5 truncate font-mono text-xs">{selected.id}</div>
                      </div>
                      <div className="bg-card px-4 py-3">
                        <div className="text-[11px] text-muted-foreground font-medium">关联会话</div>
                        <div className="mt-0.5 truncate font-mono text-xs">{selected.sessionId ?? "无"}</div>
                      </div>
                      <div className="bg-card px-4 py-3">
                        <div className="text-[11px] text-muted-foreground font-medium">处理状态</div>
                        <div className="mt-0.5 font-medium">{statusStyles[selected.status]?.label ?? selected.status}</div>
                      </div>
                      <div className="bg-card px-4 py-3">
                        <div className="text-[11px] text-muted-foreground font-medium">交付状态</div>
                        <div className="mt-0.5 truncate font-mono text-xs">{selected.deliveryStatus}</div>
                      </div>
                    </div>
                  </div>

                  <RevisionDiffPanel item={selected} />

                  {isApprovalKind(selected.kind) && selected.status === "pending" && (
                    <ToolApprovalPanel
                      key={selected.id}
                      approval={{
                        id: selected.id,
                        kind: selected.kind,
                        tool: selected.title,
                        arguments: selected.args ?? {},
                        createdAt: selected.createdAt,
                        sessionId: selected.sessionId,
                        canDecide: true,
                      }}
                      onAccept={onAccept}
                      onAcceptAlways={onAcceptAlways}
                      onReject={onReject}
                      busy={busy}
                    />
                  )}

                  {isApprovalKind(selected.kind) && selected.status !== "pending" && (
                    <div className="rounded-2xl border border-border/70 bg-card p-5 space-y-3">
                      <div className="flex items-center justify-between gap-3">
                        <div className="text-xs font-semibold text-foreground">审批历史明细</div>
                        {selected.deliveryStatus === "delivery_failed" && (
                          <Button
                            size="sm"
                            variant="outline"
                            className="h-7 text-xs gap-1"
                            onClick={() => void onRetryDelivery(selected.id)}
                            disabled={busy}
                          >
                            <RefreshCw className="size-3.5" />
                            重试交付
                          </Button>
                        )}
                      </div>
                      {selected.lastDeliveryError && (
                        <div className="rounded-xl border border-destructive/30 bg-destructive/10 px-3.5 py-2.5 text-xs text-destructive font-mono">
                          {selected.lastDeliveryError}
                        </div>
                      )}
                      {selected.args && Object.keys(selected.args).length > 0 ? (
                        <div className="space-y-3">
                          {Object.entries(selected.args).map(([k, v]) => (
                            <div key={k}>
                              <div className="text-xs font-mono text-muted-foreground">{k}</div>
                              <pre className="mt-1 overflow-x-auto whitespace-pre-wrap rounded-xl border border-border/70 bg-muted/30 px-3.5 py-2.5 font-mono text-xs leading-relaxed">
                                {typeof v === "string" ? v : JSON.stringify(v, null, 2)}
                              </pre>
                            </div>
                          ))}
                        </div>
                      ) : (
                        <p className="text-xs text-muted-foreground">该操作无参数输入。</p>
                      )}
                      {selected.feedback && (
                        <div className="mt-4 border-t border-border/50 pt-3.5">
                          <div className="text-xs font-medium text-muted-foreground">人工反馈意见</div>
                          <p className="mt-1 whitespace-pre-wrap text-xs text-foreground leading-relaxed">{selected.feedback}</p>
                        </div>
                      )}
                    </div>
                  )}

                  {selected.kind === "issue" && (
                    <div className="rounded-2xl border border-border/70 bg-card p-5 space-y-3">
                      <div className="flex items-center justify-between gap-3">
                        <div>
                          <div className="text-xs font-semibold text-foreground">问题报告详情</div>
                          <div className="text-xs text-muted-foreground mt-0.5">智能体上报的人工处理说明。</div>
                        </div>
                        {selected.status === "open" && (
                          <Button size="sm" className="h-7 text-xs gap-1 bg-emerald-600 hover:bg-emerald-700 text-white" onClick={() => onResolve(selected.id)} disabled={busy}>
                            <CheckCircle2 className="size-3.5" />
                            标记已解决
                          </Button>
                        )}
                      </div>
                      <p className="whitespace-pre-wrap rounded-xl border border-border/70 bg-muted/30 px-3.5 py-2.5 text-xs leading-relaxed font-mono">
                        {selected.body || "未提供更详细的信息。"}
                      </p>
                    </div>
                  )}
                </div>
              )}
            </div>
          </div>
        </main>
      </div>

      {runDrawerTurn && (
        <RunDrawer
          sessionId={runDrawerTurn.sessionId}
          turnId={runDrawerTurn.turnId}
          open={Boolean(runDrawerTurn)}
          onOpenChange={(open) => {
            if (!open) setRunDrawerTurn(null);
          }}
        />
      )}
    </div>
  );
}

export default function InboxPage() {
  return (
    <Suspense
      fallback={
        <div className="flex h-screen items-center justify-center bg-background text-xs text-muted-foreground font-mono">
          加载消息队列...
        </div>
      }
    >
      <InboxInner />
    </Suspense>
  );
}
