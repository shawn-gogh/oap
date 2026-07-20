"use client";

import { useCallback, useEffect, useState } from "react";
import type { ReactNode } from "react";
import {
  AlertTriangle,
  BarChart3,
  Check,
  Copy,
  PanelRightClose,
  PanelRightOpen,
  RefreshCw,
  Zap,
} from "lucide-react";

import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { Button } from "@/components/ui/button";
import { getSpendLog, listSpendLogs } from "@/lib/api";
import type { SpendLog } from "@/lib/types";

const PAGE_SIZE = 50;
const TABLE_COLUMNS =
  "grid-cols-[150px_96px_104px_136px_190px_104px_108px_92px_132px_150px_132px_180px_132px]";
const LOG_URL_PARAM = "request_id";
const MAX_PROMPT_JSON_CHARS = 12000;

function formatCost(value: number | null | undefined): string {
  return `$${(value ?? 0).toFixed(8)}`;
}

function formatDate(value: string | null | undefined): string {
  if (!value) return "-";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat("zh-CN", {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
    second: "2-digit",
  }).format(date);
}

function formatDuration(ms: number | null | undefined): string {
  if (ms == null) return "-";
  return ms < 1000 ? `${ms} 毫秒` : `${(ms / 1000).toFixed(3)} 秒`;
}

function prettyJson(value: unknown): string {
  if (value == null) return "{}";
  return typeof value === "string" ? value : JSON.stringify(value, null, 2);
}

function truncateForPrompt(value: string): string {
  if (value.length <= MAX_PROMPT_JSON_CHARS) return value;
  return `${value.slice(0, MAX_PROMPT_JSON_CHARS)}\n\n[截断未完全显示的超长部分]`;
}

function currentLogUrl(requestId: string): string {
  if (typeof window === "undefined") return `/observability/logs/?${LOG_URL_PARAM}=${encodeURIComponent(requestId)}`;
  const url = new URL("/observability/logs/", window.location.origin);
  url.searchParams.set(LOG_URL_PARAM, requestId);
  return url.toString();
}

function setCurrentLogUrl(requestId: string): void {
  if (typeof window === "undefined") return;
  const url = new URL(window.location.href);
  url.pathname = "/observability/logs/";
  url.search = "";
  url.searchParams.set(LOG_URL_PARAM, requestId);
  window.history.replaceState(null, "", url.toString());
}

function shortValue(value: string | null | undefined, size = 14): string {
  if (!value) return "-";
  return value.length > size ? `${value.slice(0, size)}...` : value;
}

function metadataString(log: SpendLog | null, key: string): string | null {
  const value = log?.metadata?.[key];
  return typeof value === "string" && value.trim() ? value : null;
}

function isStreaming(log: SpendLog): boolean {
  if (!log.messages || typeof log.messages !== "object" || Array.isArray(log.messages)) {
    return false;
  }
  return (log.messages as Record<string, unknown>).stream === true;
}

function typeLabel(log: SpendLog): string {
  return log.call_type === "messages" ? "LLM 对话" : log.call_type;
}

function timeToFirstToken(log: SpendLog): string {
  return isStreaming(log) ? formatDuration(log.request_duration_ms) : "-";
}

function errorInfo(log: SpendLog | null): Record<string, unknown> | null {
  const info = log?.metadata?.error_information;
  return info && typeof info === "object" && !Array.isArray(info)
    ? (info as Record<string, unknown>)
    : null;
}

function buildDebugPrompt(log: SpendLog, error: Record<string, unknown> | null): string {
  const lines = [
    "请诊断此 OAP 网关请求日志并找出根本原因。",
    "",
    `日志地址: ${currentLogUrl(log.request_id)}`,
    `请求 ID: ${log.request_id}`,
    `状态: ${log.status ?? "未知"}`,
    `调用类型: ${log.call_type}`,
    `提供方: ${log.custom_llm_provider ?? "-"}`,
    `模型: ${log.model_group || log.model}`,
    `API 基础地址: ${log.api_base ?? "-"}`,
    `开始时间: ${log.start_time}`,
    `总耗时: ${formatDuration(log.request_duration_ms)}`,
    `输入 Tokens: ${log.prompt_tokens}`,
    `输出 Tokens: ${log.completion_tokens}`,
    `开销: ${formatCost(log.spend)}`,
  ];

  if (error) {
    lines.push(
      "",
      "捕获到的异常堆栈:",
      "```json",
      truncateForPrompt(prettyJson(error)),
      "```",
    );
  }

  lines.push(
    "",
    "请求载荷 Payload:",
    "```json",
    truncateForPrompt(prettyJson(log.messages)),
    "```",
    "",
    "响应载荷 Response:",
    "```json",
    truncateForPrompt(prettyJson(log.response)),
    "```",
    "",
    "请说明失败原因、代码定位及最小安全修复方案。",
  );

  return lines.join("\n");
}

export default function ObservabilityLogsPage() {
  const [logs, setLogs] = useState<SpendLog[]>([]);
  const [selected, setSelected] = useState<SpendLog | null>(null);
  const [selectedRequestId, setSelectedRequestId] = useState<string | null>(null);
  const [detailOpen, setDetailOpen] = useState(true);
  const [liveTail, setLiveTail] = useState(true);
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (silent = false) => {
    if (silent) setRefreshing(true);
    else setLoading(true);
    try {
      const next = await listSpendLogs({ limit: 250 });
      setLogs(next);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  }, []);

  useEffect(() => {
    const initialRequestId = new URLSearchParams(window.location.search).get(LOG_URL_PARAM);
    if (initialRequestId) setSelectedRequestId(initialRequestId);
    load();
  }, [load]);

  useEffect(() => {
    if (!liveTail) return undefined;
    const timer = setInterval(() => load(true), 15_000);
    return () => clearInterval(timer);
  }, [liveTail, load]);

  useEffect(() => {
    setPage(1);
  }, [logs.length]);

  const totalPages = Math.max(1, Math.ceil(logs.length / PAGE_SIZE));
  const currentPage = Math.min(page, totalPages);
  const pageStart = logs.length === 0 ? 0 : (currentPage - 1) * PAGE_SIZE + 1;
  const pageEnd = Math.min(currentPage * PAGE_SIZE, logs.length);
  const visibleLogs = logs.slice(pageStart === 0 ? 0 : pageStart - 1, pageEnd);

  const selectRequest = useCallback(async (requestId: string, updateUrl = true) => {
    const log = await getSpendLog(requestId);
    setSelected(log);
    setSelectedRequestId(log.request_id);
    setDetailOpen(true);
    if (updateUrl) setCurrentLogUrl(log.request_id);
  }, []);

  useEffect(() => {
    if (logs.length === 0 && !selectedRequestId) {
      setSelected(null);
      setDetailOpen(false);
      return;
    }
    const requestId = selectedRequestId ?? logs[0]?.request_id;
    if (!requestId || selected?.request_id === requestId) return;

    let cancelled = false;
    getSpendLog(requestId)
      .then((log) => {
        if (!cancelled) {
          setSelected(log);
          setSelectedRequestId(log.request_id);
          setDetailOpen(true);
        }
      })
      .catch((err) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [logs, selected?.request_id, selectedRequestId]);

  const selectedError = errorInfo(selected);

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden selection:bg-blue-500/20">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        {/* Pure Chinese Header */}
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border/80 px-4 bg-background/80 backdrop-blur">
          <div className="flex items-center gap-2.5">
            <div className="flex size-7 items-center justify-center rounded-md bg-blue-500/10 text-blue-600 dark:text-blue-400 ring-1 ring-blue-500/20">
              <BarChart3 className="size-4" />
            </div>
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold tracking-tight">调用日志与观测</span>
              <span className="text-xs text-muted-foreground font-medium">/ 调用日志</span>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              className="h-8 border-border bg-card text-xs gap-1.5"
              onClick={() => load(true)}
            >
              <RefreshCw className={`size-3.5 ${refreshing ? "animate-spin" : ""}`} />
              手动拉取
            </Button>
            <ThemeToggle />
          </div>
        </header>

        <main id="main-content" className="relative min-h-0 flex-1 overflow-hidden">
          <section className="flex h-full min-h-0 min-w-0 flex-col bg-card">
            <div className="border-b border-border px-4 py-3 bg-muted/20">
              <div className="flex flex-wrap items-center justify-between gap-4 text-xs font-mono text-muted-foreground">
                <div>显示第 {pageStart} 到 {pageEnd} 条，共 {logs.length} 条记录</div>
                <div className="flex items-center gap-3">
                  <span>页码 {currentPage} / {totalPages}</span>
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-7 text-xs"
                    disabled={currentPage <= 1}
                    onClick={() => setPage((value) => Math.max(1, value - 1))}
                  >
                    上一页
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-7 text-xs"
                    disabled={currentPage >= totalPages}
                    onClick={() => setPage((value) => Math.min(totalPages, value + 1))}
                  >
                    下一页
                  </Button>
                </div>
              </div>
            </div>

            <div className={`border-b px-4 py-2 text-xs font-mono flex items-center justify-between ${
              liveTail ? "border-emerald-500/20 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400" : "border-border bg-muted text-muted-foreground"
            }`}>
              <span>{liveTail ? "已开启实时轮询 (每 15 秒同步一次最新日志)" : "实时轮询已暂停"}</span>
              <Button variant="ghost" size="sm" className="h-6 px-2 text-xs font-medium" onClick={() => setLiveTail(v => !v)}>
                {liveTail ? "暂停轮询" : "开启轮询"}
              </Button>
            </div>

            <div className="min-h-0 flex-1 overflow-auto">
              {error && (
                <div className="m-4 rounded-xl border border-destructive/40 bg-destructive/10 p-3.5 text-xs text-destructive font-mono">
                  {error}
                </div>
              )}
              {loading && <div className="p-4 text-xs text-muted-foreground font-mono">正在加载日志列表...</div>}
              {!loading && logs.length === 0 && !error && (
                <div className="p-4 text-xs text-muted-foreground font-mono">暂无网关调用日志。</div>
              )}
              <div className="min-w-[1660px]">
                <TableHeader />
                {visibleLogs.map((log) => (
                  <LogRow
                    key={log.request_id}
                    log={log}
                    active={selected?.request_id === log.request_id}
                    onSelect={() => void selectRequest(log.request_id)}
                  />
                ))}
              </div>
            </div>
          </section>

          {selected && !detailOpen && (
            <button
              type="button"
              className="absolute right-3 top-1/2 z-20 flex -translate-y-1/2 items-center gap-2 rounded-xl border border-border bg-card px-3 py-2 text-xs font-medium text-foreground shadow-md hover:bg-muted"
              title="展开请求详细面板"
              onClick={() => setDetailOpen(true)}
            >
              <PanelRightOpen className="size-4 text-muted-foreground" />
              查看详细
            </button>
          )}

          {selected && (
            <aside
              className={`absolute inset-y-0 right-0 z-30 w-[min(760px,calc(100vw-320px))] min-w-[520px] overflow-y-auto border-l border-border bg-background shadow-[-18px_0_45px_rgba(15,23,42,0.12)] transition-transform duration-200 ease-out ${
                detailOpen ? "translate-x-0" : "pointer-events-none translate-x-full"
              }`}
            >
              <LogDetail log={selected} error={selectedError} onClose={() => setDetailOpen(false)} />
            </aside>
          )}
        </main>
      </div>
    </div>
  );
}

function TableHeader() {
  return (
    <div className={`grid ${TABLE_COLUMNS} border-b border-border bg-card px-4 py-2.5 text-xs font-semibold text-foreground`}>
      <div>调用时间</div>
      <div>类型</div>
      <div>状态</div>
      <div>会话 ID</div>
      <div>请求 ID</div>
      <div>费用开销</div>
      <div>总耗时</div>
      <div>首字延迟</div>
      <div>团队名称</div>
      <div>密钥哈希</div>
      <div>调起人</div>
      <div>接入模型</div>
      <div>令牌消耗 (Total)</div>
    </div>
  );
}

function LogRow({
  log,
  active,
  onSelect,
}: {
  log: SpendLog;
  active: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      className={`grid w-full ${TABLE_COLUMNS} items-center border-b border-border/60 px-4 py-2.5 text-left text-xs transition-colors ${
        active ? "bg-blue-500/10 border-l-2 border-l-blue-500 font-medium" : "hover:bg-muted/40"
      }`}
      onClick={onSelect}
    >
      <div className="font-mono text-muted-foreground">{formatDate(log.start_time)}</div>
      <div><TypePill value={typeLabel(log)} /></div>
      <div><StatusBadge status={log.status} compact /></div>
      <div className="truncate font-mono text-blue-600 dark:text-blue-400">{shortValue(log.session_id, 13)}</div>
      <div className="truncate font-mono text-muted-foreground">{shortValue(log.request_id, 18)}</div>
      <div className="font-mono text-muted-foreground">{log.status === "error" ? "-" : formatCost(log.spend)}</div>
      <div className="font-mono text-muted-foreground">{((log.request_duration_ms ?? 0) / 1000).toFixed(2)}s</div>
      <div className="font-mono text-muted-foreground">{timeToFirstToken(log)}</div>
      <div className="truncate text-muted-foreground">{metadataString(log, "team_name") ?? "-"}</div>
      <div className="truncate font-mono text-muted-foreground">{shortValue(log.api_key, 14)}</div>
      <div className="truncate text-muted-foreground">{log.user || "-"}</div>
      <div className="truncate text-muted-foreground font-mono">{log.model_group || log.model}</div>
      <div className="font-mono text-muted-foreground">
        {log.total_tokens.toLocaleString()}{" "}
        <span className="text-muted-foreground/70">
          ({log.prompt_tokens}+{log.completion_tokens})
        </span>
      </div>
    </button>
  );
}

function TypePill({ value }: { value: string }) {
  return (
    <span className="inline-flex items-center rounded-md bg-blue-500/10 px-2 py-0.5 text-[10px] font-medium text-blue-600 dark:text-blue-400 border border-blue-500/20">
      {value}
    </span>
  );
}

function StatusBadge({ status, compact = false }: { status: string | null; compact?: boolean }) {
  const ok = status !== "error";
  return (
    <span className={`inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-[10px] font-medium ${
      ok ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
         : "border-destructive/30 bg-destructive/10 text-destructive"
    }`}>
      {!compact && (ok ? <Check className="size-3" /> : <AlertTriangle className="size-3" />)}
      {ok ? "成功" : "失败"}
    </span>
  );
}

function LogDetail({
  log,
  error,
  onClose,
}: {
  log: SpendLog;
  error: Record<string, unknown> | null;
  onClose: () => void;
}) {
  const [copied, setCopied] = useState<string | null>(null);
  const logUrl = currentLogUrl(log.request_id);
  const debugPrompt = buildDebugPrompt(log, error);
  const copy = async (key: string, value: string) => {
    await navigator.clipboard?.writeText(value);
    setCopied(key);
    window.setTimeout(() => setCopied(null), 1200);
  };

  return (
    <div className="space-y-5 px-6 py-5">
      <div className="border-b border-border/80 pb-4">
        <div className="space-y-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <span className="text-sm font-bold text-foreground font-mono">{log.model_group || log.model}</span>
              <span className="text-xs text-muted-foreground">{log.custom_llm_provider || "-"}</span>
              <Button
                variant="ghost"
                size="icon"
                className="ml-auto h-8 w-8 text-muted-foreground hover:bg-muted"
                title="折叠请求详情"
                onClick={onClose}
              >
                <PanelRightClose className="size-4" />
              </Button>
            </div>
            <div className="mt-3 flex min-w-0 items-center gap-2">
              <h2 className="truncate font-mono text-lg font-bold leading-tight text-foreground">
                {log.request_id}
              </h2>
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7 text-blue-500 hover:bg-blue-500/10"
                title="复制请求 ID"
                onClick={() => void copy("request", log.request_id)}
              >
                {copied === "request" ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
              </Button>
            </div>
            <div className="mt-3 flex flex-wrap items-center gap-2">
              <StatusBadge status={log.status} />
              <span className="rounded-md border border-border bg-card px-2.5 py-0.5 text-xs font-mono">
                环境: {metadataString(log, "environment") ?? "默认"}
              </span>
              <span className="text-xs text-muted-foreground font-mono">{formatDate(log.start_time)}</span>
              <Button
                variant="outline"
                size="sm"
                className="h-7 text-xs border-border bg-card gap-1"
                onClick={() => void copy("url", logUrl)}
              >
                {copied === "url" ? <Check className="size-3" /> : <Copy className="size-3" />}
                复制地址
              </Button>
              <Button
                variant="outline"
                size="sm"
                className="h-7 text-xs border-border bg-card gap-1"
                onClick={() => void copy("prompt", debugPrompt)}
              >
                {copied === "prompt" ? <Check className="size-3" /> : <Copy className="size-3" />}
                生成诊断 Prompt
              </Button>
            </div>
          </div>
          <div className="grid overflow-hidden rounded-xl border border-border bg-card sm:grid-cols-4">
            <DetailStat label="费用开销" value={formatCost(log.spend)} />
            <DetailStat label="总 Tokens" value={log.total_tokens.toLocaleString()} />
            <DetailStat label="请求延迟" value={formatDuration(log.request_duration_ms)} />
            <DetailStat label="首字延迟 (TTFT)" value={timeToFirstToken(log)} />
          </div>
        </div>
      </div>

      <InfoCard title="标头与关联标签">
        <TagList log={log} />
      </InfoCard>

      <InfoCard title="请求元数据">
        <TwoColumnFields
          left={[
            ["模型 Group", log.model],
            ["调用类型", log.call_type],
            ["API 基础地址", log.api_base],
          ]}
          right={[
            ["提供方", log.custom_llm_provider],
            ["模型 ID", log.model_id],
            ["请求 IP 地址", log.requester_ip_address],
          ]}
        />
      </InfoCard>

      <InfoCard title="性能与 Token 指标">
        <TwoColumnFields
          left={[
            ["输入 Tokens", log.prompt_tokens.toLocaleString()],
            ["费用开销", formatCost(log.spend)],
            ["首字响应延迟", timeToFirstToken(log)],
            ["缓存读取 Tokens", metadataString(log, "cache_read_tokens") ?? "-"],
            ["结束时间", log.end_time],
          ]}
          right={[
            ["输出 Tokens", log.completion_tokens.toLocaleString()],
            ["总执行耗时", formatDuration(log.request_duration_ms)],
            ["缓存命中", log.cache_hit ?? "否"],
            ["缓存创建 Tokens", metadataString(log, "cache_creation_tokens") ?? "-"],
            ["发起时间", log.start_time],
          ]}
        />
      </InfoCard>

      {error && (
        <InfoCard title="异常报错追踪" tone="error">
          <div className="space-y-3">
            <ErrorField label="错误类型" value={String(error.error_type ?? "-")} />
            <ErrorField label="错误消息" value={String(error.message ?? "-")} />
            <CodeBlock value={String(error.trace ?? "")} tone="error" />
          </div>
        </InfoCard>
      )}

      <InfoCard title="费用小计">
        <div className="flex items-center justify-between text-xs">
          <span className="text-muted-foreground font-medium">当前调用总费用</span>
          <span className="font-mono text-sm font-bold text-foreground">{formatCost(log.spend)}</span>
        </div>
      </InfoCard>

      <InfoCard title="请求与响应 Payload 载荷">
        <div className="space-y-3">
          <PayloadBlock title="输入载荷 (Input)" tokens={log.prompt_tokens} value={prettyJson(log.messages)} />
          <PayloadBlock title="输出载荷 (Output)" tokens={log.completion_tokens} value={prettyJson(log.response)} />
        </div>
      </InfoCard>
    </div>
  );
}

function TagList({ log }: { log: SpendLog }) {
  const rawTags = Array.isArray(log.request_tags) ? log.request_tags : [];
  const tags = rawTags.length > 0
    ? rawTags
    : [
        `call_type: ${log.call_type}`,
        `provider: ${log.custom_llm_provider ?? "-"}`,
        `purpose: ${log.purpose}`,
        ...(log.agent_id ? [`agent: ${log.agent_id}`] : []),
        ...(log.session_id ? [`session: ${log.session_id}`] : []),
      ];
  return (
    <div className="flex flex-wrap gap-2">
      {tags.map((tag, index) => (
        <span
          key={`${String(tag)}-${index}`}
          className="rounded-md border border-border bg-muted px-2 py-0.5 font-mono text-[11px] text-muted-foreground"
        >
          {index}: {String(tag)}
        </span>
      ))}
    </div>
  );
}

function PayloadBlock({
  title,
  tokens,
  value,
}: {
  title: string;
  tokens: number;
  value: string;
}) {
  return (
    <div className="overflow-hidden rounded-xl border border-border/80 bg-card">
      <div className="flex items-center gap-3 border-b border-border/80 bg-muted/40 px-3.5 py-2 text-xs font-mono">
        <span className="font-semibold text-foreground">{title}</span>
        <span className="text-muted-foreground">Tokens: {tokens.toLocaleString()}</span>
        <Button
          variant="ghost"
          size="icon"
          className="ml-auto h-7 w-7 text-muted-foreground hover:bg-muted"
          title={`复制 ${title}`}
          onClick={() => navigator.clipboard?.writeText(value)}
        >
          <Copy className="size-3.5" />
        </Button>
      </div>
      <CodeBlock value={value} />
    </div>
  );
}

function DetailStat({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 border-r border-border/70 px-3.5 py-2.5 last:border-r-0">
      <div className="text-[10px] font-medium uppercase text-muted-foreground">{label}</div>
      <div className="mt-1 truncate font-mono text-xs font-semibold text-foreground">
        {value}
      </div>
    </div>
  );
}

function InfoCard({
  title,
  children,
  tone,
}: {
  title: string;
  children: ReactNode;
  tone?: "error";
}) {
  return (
    <section className={`overflow-hidden rounded-2xl border bg-card shadow-2xs ${
      tone === "error" ? "border-destructive/40" : "border-border/80"
    }`}>
      <div className="flex h-10 items-center gap-2 border-b border-border/70 px-4 bg-muted/20">
        <h3 className="text-xs font-semibold tracking-tight text-foreground">{title}</h3>
        {tone === "error" && (
          <span className="ml-auto rounded-md bg-destructive/10 px-2 py-0.5 text-[10px] font-medium text-destructive">
            包含捕获到的异常
          </span>
        )}
      </div>
      <div className="p-4">{children}</div>
    </section>
  );
}

function TwoColumnFields({
  left,
  right,
}: {
  left: Array<[string, string | null | undefined]>;
  right: Array<[string, string | null | undefined]>;
}) {
  return (
    <div className="grid gap-x-8 gap-y-2.5 md:grid-cols-2 text-xs font-mono">
      <div className="space-y-2.5">
        {left.map(([label, value]) => (
          <InlineMetric key={label} label={label} value={value} />
        ))}
      </div>
      <div className="space-y-2.5">
        {right.map(([label, value]) => (
          <InlineMetric key={label} label={label} value={value} />
        ))}
      </div>
    </div>
  );
}

function InlineMetric({ label, value }: { label: string; value: string | null | undefined }) {
  return (
    <div className="flex min-w-0 items-baseline gap-2">
      <span className="shrink-0 text-muted-foreground">{label}:</span>
      <span className="min-w-0 break-words font-semibold text-foreground">{value || "-"}</span>
    </div>
  );
}

function ErrorField({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-xl border border-destructive/20 bg-card p-3 font-mono">
      <div className="text-[10px] font-semibold uppercase text-destructive">{label}</div>
      <div className="mt-1 break-words text-xs font-medium text-foreground">{value}</div>
    </div>
  );
}

function CodeBlock({
  value,
  compact = false,
  tone,
}: {
  value: string;
  compact?: boolean;
  tone?: "error";
}) {
  return (
    <pre
      className={`overflow-auto p-4 font-mono text-xs leading-relaxed ${
        compact ? "max-h-[260px]" : "max-h-[540px]"
      } ${
        tone === "error"
          ? "bg-destructive/10 text-destructive"
          : "bg-muted/30 text-foreground"
      }`}
    >
      {value}
    </pre>
  );
}
