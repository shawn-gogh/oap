"use client";

import { useEffect, useMemo, useState } from "react";
import { Search, Check, Puzzle, Zap, ShieldCheck, Filter, ArrowUpRight } from "lucide-react";
import { toast } from "sonner";
import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { IntegrationsBrandIcon } from "@/components/brand-kit-icons";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { IntegrationDialog } from "@/components/integration-dialog";
import { BrandIcon } from "@/components/brand-icons";
import { listPublicMcpServers, listMcpUserCredentials } from "@/lib/api";
import { serverIconId } from "@/lib/integrations";
import type { McpServer } from "@/lib/types";

/** Derive a display name from an MCP server record. */
function serverDisplayName(s: McpServer): string {
  return s.alias ?? s.server_name ?? s.server_id;
}

/** Derive the category to group this server under. */
function serverCategory(s: McpServer): string {
  const info = s.mcp_info as { category?: string } | undefined;
  return info?.category ?? "其他扩展";
}

/** Priority order for category headers. Unlisted categories fall to the end. */
const CATEGORY_ORDER = ["谷歌工具", "微软服务", "开发工具", "生产力工具", "其他扩展"];

function categoryIndex(cat: string): number {
  const i = CATEGORY_ORDER.indexOf(cat);
  return i === -1 ? CATEGORY_ORDER.length : i;
}

function groupByCategory(servers: McpServer[]): [string, McpServer[]][] {
  const groups = new Map<string, McpServer[]>();
  for (const s of servers) {
    const cat = serverCategory(s);
    const arr = groups.get(cat) ?? [];
    arr.push(s);
    groups.set(cat, arr);
  }
  return [...groups.entries()].sort(
    (a, b) => categoryIndex(a[0]) - categoryIndex(b[0]),
  );
}

export default function IntegrationsPage() {
  const [servers, setServers] = useState<McpServer[]>([]);
  const [connected, setConnected] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<"all" | "connected" | "unconnected">("all");
  const [active, setActive] = useState<McpServer | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);

  const refresh = async () => {
    const [srvs, creds] = await Promise.all([
      listPublicMcpServers().catch(() => [] as McpServer[]),
      listMcpUserCredentials().catch(() => [] as { server_id: string }[]),
    ]);
    setServers(srvs);
    setConnected(new Set(creds.map((c) => c.server_id)));
    setLoading(false);
  };

  useEffect(() => {
    void refresh();
  }, []);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const status = params.get("mcp_oauth");
    if (!status) return;
    const serverLabel =
      params.get("server_id")?.replace(/[-_]+/g, " ").trim() || "集成服务";
    if (status === "connected") {
      toast.success(`已成功连接 ${serverLabel}`);
    } else if (status === "failed") {
      toast.error(params.get("error") ?? `${serverLabel} 连接失败`);
    }
    params.delete("mcp_oauth");
    params.delete("server_id");
    params.delete("error");
    const nextQuery = params.toString();
    const nextUrl = nextQuery ? `${window.location.pathname}?${nextQuery}` : window.location.pathname;
    window.history.replaceState(null, "", nextUrl);
    void refresh();
  }, []);

  const groups = useMemo(() => {
    const q = query.trim().toLowerCase();
    return groupByCategory(servers)
      .map(([cat, items]) => {
        let filtered = items;
        if (statusFilter === "connected") {
          filtered = filtered.filter((it) => connected.has(it.server_id));
        } else if (statusFilter === "unconnected") {
          filtered = filtered.filter((it) => !connected.has(it.server_id));
        }

        if (q) {
          filtered = filtered.filter(
            (it) =>
              serverDisplayName(it).toLowerCase().includes(q) ||
              (it.description ?? "").toLowerCase().includes(q),
          );
        }
        return [cat, filtered] as [string, McpServer[]];
      })
      .filter(([, items]) => items.length > 0);
  }, [query, statusFilter, servers, connected]);

  const openDialog = (it: McpServer) => {
    setActive(it);
    setDialogOpen(true);
  };

  const connectedCount = useMemo(() => {
    return servers.filter((s) => connected.has(s.server_id)).length;
  }, [servers, connected]);

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden selection:bg-blue-500/20">
      <Sidebar />
      <div className="flex flex-1 flex-col min-w-0 overflow-hidden">
        {/* Header - Pure Chinese */}
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border/80 px-4 bg-background/80 backdrop-blur">
          <div className="flex items-center gap-2.5">
            <div className="flex size-7 items-center justify-center rounded-md bg-blue-500/10 text-blue-600 dark:text-blue-400 ring-1 ring-blue-500/20">
              <IntegrationsBrandIcon size={16} />
            </div>
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold tracking-tight">扩展集成</span>
              <span className="text-xs text-muted-foreground font-medium">/ 集成</span>
            </div>
          </div>
          <ThemeToggle />
        </header>

        <main id="main-content" className="flex-1 overflow-y-auto p-6 space-y-6">
          <div className="mx-auto w-full max-w-5xl space-y-6">
            {/* Banner - Pure Chinese */}
            <div className="relative overflow-hidden rounded-2xl border border-border/70 bg-card p-6 shadow-xs">
              <div className="absolute right-0 top-0 -mr-16 -mt-16 size-64 rounded-full bg-blue-500/5 blur-3xl pointer-events-none" />
              <div className="relative z-10 flex flex-col gap-5 md:flex-row md:items-center md:justify-between">
                <div className="space-y-1.5 max-w-xl">
                  <div className="flex items-center gap-2">
                    <span className="inline-flex items-center gap-1 rounded-md bg-blue-500/10 px-2 py-0.5 text-xs font-medium text-blue-600 dark:text-blue-400 border border-blue-500/20">
                      <Zap className="size-3" /> 扩展工具枢纽
                    </span>
                    <span className="text-xs text-muted-foreground font-medium">协议驱动工具链</span>
                  </div>
                  <h1 className="text-xl font-bold tracking-tight text-foreground">
                    模型上下文扩展与生态集成
                  </h1>
                  <p className="text-xs text-muted-foreground leading-relaxed">
                    统一托管模型上下文协议 (MCP) 扩展节点。通过动态授权或凭证握手，即刻为智能体开启原生代码、工具与数据获取能力。
                  </p>
                </div>

                <div className="flex items-center gap-4 shrink-0 rounded-xl bg-muted/40 p-3.5 border border-border/60">
                  <div className="text-center">
                    <div className="text-[11px] font-medium text-muted-foreground">扩展总数</div>
                    <div className="text-xl font-bold font-mono text-foreground">{servers.length}</div>
                  </div>
                  <div className="h-7 w-px bg-border/60" />
                  <div className="text-center">
                    <div className="text-[11px] font-medium text-muted-foreground">已授权连接</div>
                    <div className="text-xl font-bold font-mono text-blue-600 dark:text-blue-400">{connectedCount}</div>
                  </div>
                </div>
              </div>
            </div>

            {/* Filter Bar - Pure Chinese */}
            <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
              <div className="flex items-center gap-1 rounded-xl border border-border/70 bg-muted/30 p-1">
                {(
                  [
                    { id: "all", label: "全部扩展", count: servers.length },
                    { id: "connected", label: "已连接", count: connectedCount },
                    { id: "unconnected", label: "未连接", count: servers.length - connectedCount },
                  ] as const
                ).map((tab) => (
                  <button
                    key={tab.id}
                    onClick={() => setStatusFilter(tab.id)}
                    className={`flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition-all ${
                      statusFilter === tab.id
                        ? "bg-background text-foreground shadow-2xs font-semibold"
                        : "text-muted-foreground hover:text-foreground"
                    }`}
                  >
                    <span>{tab.label}</span>
                    <span className="rounded bg-muted px-1.5 py-0.2 font-mono text-[10px]">
                      {tab.count}
                    </span>
                  </button>
                ))}
              </div>

              <div className="relative max-w-xs flex-1">
                <Search className="absolute left-3 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
                <Input
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder="按扩展名称或描述检索..."
                  className="h-8 pl-9 text-xs bg-card"
                />
              </div>
            </div>

            {/* Skeletons */}
            {loading && (
              <div className="grid grid-cols-1 gap-3.5 sm:grid-cols-2">
                {Array.from({ length: 4 }).map((_, i) => (
                  <div key={i} className="h-28 animate-pulse rounded-xl border border-border/60 bg-muted/20" />
                ))}
              </div>
            )}

            {/* Empty Server List */}
            {!loading && servers.length === 0 && (
              <div className="flex flex-col items-center justify-center rounded-2xl border border-dashed border-border p-12 text-center">
                <div className="flex size-12 items-center justify-center rounded-2xl bg-blue-500/10 text-blue-500">
                  <IntegrationsBrandIcon size={24} />
                </div>
                <h3 className="mt-3 text-sm font-semibold">暂未注册扩展服务</h3>
                <p className="mt-1 text-xs text-muted-foreground max-w-xs leading-normal">
                  请联系平台管理员在后台注入新的扩展服务配置文件。
                </p>
              </div>
            )}

            {!loading && servers.length > 0 && groups.length === 0 && (
              <div className="rounded-xl border border-dashed border-border py-12 text-center text-xs text-muted-foreground">
                未找到匹配“{query}”的扩展服务。
              </div>
            )}

            {/* Integration Groups */}
            <div className="space-y-6">
              {groups.map(([cat, items]) => (
                <section key={cat} className="space-y-3">
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-semibold tracking-wider text-muted-foreground">
                      {cat}
                    </span>
                    <div className="h-px flex-1 bg-border/50" />
                  </div>

                  <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                    {items.map((it) => {
                      const isConnected = connected.has(it.server_id);
                      const displayName = serverDisplayName(it);
                      const isOAuth = it.auth_type === "oauth2" || Boolean(it.authorization_url);
                      return (
                        <div
                          key={it.server_id}
                          className={`group relative flex items-start gap-3.5 rounded-xl border p-4 transition-all duration-200 hover:-translate-y-0.5 hover:shadow-md ${
                            isConnected
                              ? "border-blue-500/30 bg-card hover:border-blue-500/60"
                              : "border-border/80 bg-card hover:border-foreground/25"
                          }`}
                        >
                          <div className="relative flex size-10 shrink-0 items-center justify-center overflow-hidden rounded-xl border border-border/80 bg-muted/40 shadow-2xs">
                            <BrandIcon id={serverIconId(it)} className="size-5" />
                            {isConnected && (
                              <span className="absolute right-1 top-1 size-2 rounded-full bg-emerald-500 ring-2 ring-background animate-pulse" />
                            )}
                          </div>

                          <div className="min-w-0 flex-1">
                            <div className="flex items-center gap-1.5">
                              <span className="font-semibold text-sm leading-tight text-foreground group-hover:text-blue-600 dark:group-hover:text-blue-400 transition-colors">
                                {displayName}
                              </span>
                              {isOAuth && (
                                <span className="rounded bg-blue-500/10 px-1.5 py-0.2 text-[10px] font-medium text-blue-600 dark:text-blue-400 border border-blue-500/20">
                                  动态授权
                                </span>
                              )}
                            </div>
                            <p className="mt-1 line-clamp-2 text-xs text-muted-foreground leading-relaxed">
                              {it.description || "提供自动化扩展与工具集合。"}
                            </p>
                          </div>

                          <Button
                            size="sm"
                            variant={isConnected ? "secondary" : "outline"}
                            className={`shrink-0 h-8 px-3 text-xs gap-1 font-medium transition-all ${
                              isConnected
                                ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/30 hover:bg-emerald-500/20"
                                : "hover:border-blue-500/50 hover:bg-blue-500/5 hover:text-blue-600 dark:hover:text-blue-400"
                            }`}
                            onClick={() => openDialog(it)}
                          >
                            {isConnected ? (
                              <>
                                <Check className="size-3.5" />
                                已连接
                              </>
                            ) : (
                              <>
                                <PlugIcon className="size-3.5" />
                                {isOAuth ? "授权" : "配置"}
                              </>
                            )}
                          </Button>
                        </div>
                      );
                    })}
                  </div>
                </section>
              ))}
            </div>
          </div>
        </main>
      </div>

      <IntegrationDialog
        server={active}
        open={dialogOpen}
        connected={active ? connected.has(active.server_id) : false}
        onOpenChange={setDialogOpen}
        onChange={refresh}
      />
    </div>
  );
}

function PlugIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg
      {...props}
      xmlns="http://www.w3.org/2000/svg"
      width="24"
      height="24"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M12 22v-5" />
      <path d="M9 8V2" />
      <path d="M15 8V2" />
      <path d="M18 8v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4V8Z" />
    </svg>
  );
}
