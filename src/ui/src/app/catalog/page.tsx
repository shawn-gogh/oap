"use client";

import { useEffect, useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import {
  Bot,
  Library,
  Search,
  Users,
  Sparkles,
  Zap,
  Filter,
  Check,
  ArrowRight,
  ShieldCheck,
  Cpu,
  Lock,
} from "lucide-react";
import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  apiErrorMessage,
  listAgentCatalog,
  type AgentCatalogItem,
  type AgentCatalogResponse,
} from "@/lib/api";

function includes(value: string, query: string): boolean {
  return value.toLocaleLowerCase().includes(query);
}

function matches(
  agent: AgentCatalogItem,
  query: string,
  tag: string,
  capability: string,
  mineOnly: boolean,
): boolean {
  const normalized = query.trim().toLocaleLowerCase();
  const searchable = [
    agent.name,
    agent.description ?? "",
    agent.owner_id ?? "",
    ...agent.tags,
    ...agent.capabilities,
  ];
  return (
    (!normalized || searchable.some((value) => includes(value, normalized))) &&
    (!tag || agent.tags.includes(tag)) &&
    (!capability || agent.capabilities.includes(capability)) &&
    (!mineOnly || agent.can_use)
  );
}

function accessLabel(agent: AgentCatalogItem): string {
  if (agent.access === "owner") return "我创建的";
  if (agent.access === "granted") return "已授权";
  if (agent.access === "admin") return "管理员可用";
  return "未授权";
}

export default function AgentCatalogPage() {
  const router = useRouter();
  const [catalog, setCatalog] = useState<AgentCatalogResponse | null>(null);
  const [query, setQuery] = useState("");
  const [tag, setTag] = useState("");
  const [capability, setCapability] = useState("");
  const [mineOnly, setMineOnly] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    listAgentCatalog()
      .then(setCatalog)
      .catch((caught) => setError(apiErrorMessage(caught, "加载智能体目录失败")));
  }, []);

  const visible = useMemo(
    () =>
      catalog?.agents.filter((agent) => matches(agent, query, tag, capability, mineOnly)) ?? [],
    [capability, catalog, mineOnly, query, tag],
  );

  const availableCount = useMemo(
    () => catalog?.agents.filter((a) => a.can_use).length ?? 0,
    [catalog],
  );

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden selection:bg-blue-500/20">
      <Sidebar />
      <div className="flex flex-1 flex-col min-w-0 overflow-hidden">
        {/* Pure Chinese Header */}
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border/80 px-4 bg-background/80 backdrop-blur">
          <div className="flex items-center gap-2.5">
            <div className="flex size-7 items-center justify-center rounded-md bg-blue-500/10 text-blue-600 dark:text-blue-400 ring-1 ring-blue-500/20">
              <Library className="size-4" />
            </div>
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold tracking-tight">智能体目录</span>
              <span className="text-xs text-muted-foreground font-medium">/ 智能体</span>
            </div>
          </div>
          <ThemeToggle />
        </header>

        <main className="flex-1 overflow-y-auto p-6 space-y-6">
          <div className="mx-auto w-full max-w-6xl space-y-6">
            {/* Command Banner */}
            <div className="relative overflow-hidden rounded-2xl border border-border/70 bg-card p-6 shadow-xs">
              <div className="absolute right-0 top-0 -mr-16 -mt-16 size-64 rounded-full bg-blue-500/5 blur-3xl pointer-events-none" />
              <div className="relative z-10 flex flex-col gap-5 md:flex-row md:items-center md:justify-between">
                <div className="space-y-1.5 max-w-xl">
                  <div className="flex items-center gap-2">
                    <span className="inline-flex items-center gap-1 rounded-md bg-blue-500/10 px-2 py-0.5 text-xs font-medium text-blue-600 dark:text-blue-400 border border-blue-500/20">
                      <Cpu className="size-3" /> 智能体矩阵
                    </span>
                    <span className="text-xs text-muted-foreground font-medium">已激活且治理达标实例</span>
                  </div>
                  <h1 className="text-xl font-bold tracking-tight text-foreground">
                    智能体目录与探索中心
                  </h1>
                  <p className="text-xs text-muted-foreground leading-relaxed">
                    检索与使用已发布并满足治理规范的智能体。快速开启对话会话，调用特定领域的自动化能力。
                  </p>
                </div>

                <div className="flex items-center gap-4 shrink-0 rounded-xl bg-muted/40 p-3.5 border border-border/60">
                  <div className="text-center">
                    <div className="text-[11px] font-medium text-muted-foreground">目录总数</div>
                    <div className="text-xl font-bold font-mono text-foreground">{catalog?.agents.length ?? 0}</div>
                  </div>
                  <div className="h-7 w-px bg-border/60" />
                  <div className="text-center">
                    <div className="text-[11px] font-medium text-muted-foreground">可用数量</div>
                    <div className="text-xl font-bold font-mono text-blue-600 dark:text-blue-400">{availableCount}</div>
                  </div>
                </div>
              </div>
            </div>

            {/* Filter Bar */}
            <div className="rounded-2xl border border-border/70 bg-card p-4 space-y-3 shadow-2xs">
              <div className="relative">
                <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  aria-label="搜索智能体目录"
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder="按名称、描述、标签或能力关键词检索..."
                  className="pl-9 text-xs bg-background"
                />
              </div>

              <div className="flex flex-wrap items-center gap-2 pt-1 border-t border-border/40">
                <Button
                  size="sm"
                  variant={mineOnly ? "default" : "outline"}
                  className="h-7 text-xs gap-1"
                  onClick={() => setMineOnly((value) => !value)}
                >
                  <Check className={`size-3 ${mineOnly ? "opacity-100" : "opacity-0"}`} />
                  我可使用的智能体
                </Button>

                {catalog?.tags.map((value) => (
                  <Button
                    key={`tag-${value}`}
                    size="sm"
                    variant={tag === value ? "default" : "outline"}
                    className="h-7 text-xs font-mono"
                    onClick={() => setTag((current) => (current === value ? "" : value))}
                  >
                    #{value}
                  </Button>
                ))}

                {catalog?.capabilities.slice(0, 8).map((value) => (
                  <Button
                    key={`capability-${value}`}
                    size="sm"
                    variant={capability === value ? "secondary" : "ghost"}
                    className="h-7 text-xs font-medium"
                    onClick={() =>
                      setCapability((current) => (current === value ? "" : value))
                    }
                  >
                    {value}
                  </Button>
                ))}
              </div>
            </div>

            {error && (
              <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-4 text-xs text-destructive">
                {error}
              </div>
            )}

            {!catalog && !error && (
              <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
                {Array.from({ length: 6 }).map((_, i) => (
                  <div key={i} className="h-48 animate-pulse rounded-2xl border border-border/60 bg-muted/20" />
                ))}
              </div>
            )}

            {catalog && visible.length === 0 && (
              <div className="flex flex-col items-center justify-center rounded-2xl border border-dashed border-border p-12 text-center">
                <div className="flex size-12 items-center justify-center rounded-2xl bg-blue-500/10 text-blue-500">
                  <Bot className="size-6" />
                </div>
                <h3 className="mt-3 text-sm font-semibold">没有匹配的智能体实例</h3>
                <p className="mt-1 text-xs text-muted-foreground">
                  请尝试重置筛选条件或变更检索关键词。
                </p>
              </div>
            )}

            {/* Agents Grid - Anti-slop Bento Cards */}
            <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
              {visible.map((agent) => (
                <div
                  key={agent.id}
                  className="group relative flex flex-col justify-between rounded-2xl border border-border/80 bg-card p-5 transition-all duration-200 hover:-translate-y-0.5 hover:border-blue-500/40 hover:shadow-md"
                >
                  <div>
                    <div className="flex items-start justify-between gap-3">
                      <div className="flex items-center gap-3 min-w-0">
                        <div className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-blue-500/10 text-blue-600 dark:text-blue-400 border border-blue-500/20 group-hover:scale-105 transition-transform">
                          <Bot className="size-5" />
                        </div>
                        <div className="min-w-0">
                          <h3 className="truncate font-semibold text-sm text-foreground group-hover:text-blue-600 dark:group-hover:text-blue-400 transition-colors">
                            {agent.name}
                          </h3>
                          <span className="text-[11px] font-mono text-muted-foreground">
                            {agent.runtime || "默认解耦环境"}
                          </span>
                        </div>
                      </div>

                      <span
                        className={`shrink-0 rounded-md px-2 py-0.5 text-[10px] font-medium border ${
                          agent.can_use
                            ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20"
                            : "bg-muted text-muted-foreground border-border/40"
                        }`}
                      >
                        {accessLabel(agent)}
                      </span>
                    </div>

                    <p className="mt-3 line-clamp-2 text-xs text-muted-foreground leading-relaxed">
                      {agent.description?.trim() || "自动化智能体任务处理实例。"}
                    </p>

                    <div className="mt-3.5 flex flex-wrap gap-1.5">
                      {agent.tags.map((value) => (
                        <span key={value} className="rounded-md bg-muted/60 px-2 py-0.5 text-[10px] font-mono text-muted-foreground border border-border/40">
                          #{value}
                        </span>
                      ))}
                      {agent.capabilities.slice(0, 4).map((value) => (
                        <span key={value} className="rounded-md bg-blue-500/5 px-2 py-0.5 text-[10px] text-blue-600 dark:text-blue-400 border border-blue-500/15">
                          {value}
                        </span>
                      ))}
                    </div>
                  </div>

                  <div className="mt-5 pt-3.5 border-t border-border/50 space-y-3">
                    <div className="flex items-center justify-between text-xs text-muted-foreground">
                      <div className="flex items-center gap-1.5">
                        <Users className="size-3.5 shrink-0 text-muted-foreground" />
                        <span className="text-[11px]">
                          {agent.consumers.length === 0
                            ? "暂无会话记录"
                            : `${agent.consumers.length} 人使用 · ${agent.session_count} 次会话`}
                        </span>
                      </div>
                      <span className="text-[11px] font-mono text-muted-foreground">
                        属主: {agent.owner_id || "系统平台"}
                      </span>
                    </div>

                    <Button
                      size="sm"
                      disabled={!agent.can_use}
                      className={`w-full h-8 text-xs font-medium gap-1.5 transition-all ${
                        agent.can_use
                          ? "bg-blue-600 hover:bg-blue-700 text-white"
                          : "bg-muted text-muted-foreground cursor-not-allowed"
                      }`}
                      onClick={() => router.push(`/sessions/?agent=${encodeURIComponent(agent.id)}`)}
                    >
                      {agent.can_use ? (
                        <>
                          发起对话会话
                          <ArrowRight className="size-3.5" />
                        </>
                      ) : (
                        <>
                          <Lock className="size-3.5" />
                          需要权限授权
                        </>
                      )}
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </main>
      </div>
    </div>
  );
}
