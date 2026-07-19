"use client";

import { useEffect, useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import { Bot, Library, Search, Users } from "lucide-react";
import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
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

  return (
    <div className="flex h-screen bg-background text-foreground">
      <Sidebar />
      <main className="min-w-0 flex-1 overflow-y-auto">
        <header className="sticky top-0 z-10 flex h-12 items-center justify-between border-b border-border bg-background/95 px-5 backdrop-blur">
          <div className="flex items-center gap-2">
            <Library className="size-4 text-muted-foreground" />
            <h1 className="text-sm font-semibold">智能体目录</h1>
          </div>
          <ThemeToggle />
        </header>
        <div className="mx-auto grid max-w-6xl gap-5 p-5">
          <section>
            <h2 className="text-xl font-semibold tracking-tight">找到适合工作的智能体</h2>
            <p className="mt-1 text-sm text-muted-foreground">
              这里只展示已经激活并满足发布治理要求的智能体，不包含运维配置和敏感信息。
            </p>
          </section>

          <Card className="grid gap-3 p-4">
            <div className="relative">
              <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="按名称、描述、标签或能力搜索"
                className="pl-9"
              />
            </div>
            <div className="flex flex-wrap gap-2">
              <Button
                size="sm"
                variant={mineOnly ? "default" : "outline"}
                onClick={() => setMineOnly((value) => !value)}
              >
                我可以使用
              </Button>
              {catalog?.tags.map((value) => (
                <Button
                  key={`tag-${value}`}
                  size="sm"
                  variant={tag === value ? "default" : "outline"}
                  onClick={() => setTag((current) => (current === value ? "" : value))}
                >
                  #{value}
                </Button>
              ))}
              {catalog?.capabilities.slice(0, 12).map((value) => (
                <Button
                  key={`capability-${value}`}
                  size="sm"
                  variant={capability === value ? "secondary" : "ghost"}
                  onClick={() =>
                    setCapability((current) => (current === value ? "" : value))
                  }
                >
                  {value}
                </Button>
              ))}
            </div>
          </Card>

          {error && <Card className="border-destructive/40 p-4 text-sm text-destructive">{error}</Card>}
          {!catalog && !error && <p className="text-sm text-muted-foreground">正在加载目录…</p>}
          {catalog && visible.length === 0 && (
            <Card className="grid place-items-center gap-2 p-10 text-center">
              <Bot className="size-8 text-muted-foreground/50" />
              <p className="text-sm font-medium">没有匹配的智能体</p>
              <p className="text-xs text-muted-foreground">清除筛选条件后再试。</p>
            </Card>
          )}
          <section className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
            {visible.map((agent) => (
              <Card key={agent.id} className="flex min-h-64 flex-col gap-4 p-4">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <h3 className="truncate font-semibold">{agent.name}</h3>
                    <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">
                      {agent.description?.trim() || "暂无描述"}
                    </p>
                  </div>
                  <Badge variant={agent.can_use ? "secondary" : "outline"}>
                    {accessLabel(agent)}
                  </Badge>
                </div>
                <div className="flex flex-wrap gap-1.5">
                  {agent.tags.map((value) => (
                    <Badge key={value} variant="outline">#{value}</Badge>
                  ))}
                  {agent.capabilities.slice(0, 6).map((value) => (
                    <Badge key={value} variant="secondary">{value}</Badge>
                  ))}
                </div>
                <div className="mt-auto grid gap-2 border-t border-border pt-3 text-xs text-muted-foreground">
                  <p>{agent.runtime} · 属主 {agent.owner_id || "平台"}</p>
                  <div className="flex items-start gap-2">
                    <Users className="mt-0.5 size-3.5 shrink-0" />
                    <span>
                      {agent.consumers.length === 0
                        ? "还没有实际使用记录"
                        : `${agent.consumers.slice(0, 3).map((consumer) => consumer.display_name).join("、")} 等 ${agent.consumers.length} 人使用 · ${agent.session_count} 个会话`}
                    </span>
                  </div>
                </div>
                <Button
                  size="sm"
                  disabled={!agent.can_use}
                  onClick={() => router.push(`/sessions/?agent=${encodeURIComponent(agent.id)}`)}
                >
                  {agent.can_use ? "使用此智能体" : "需要属主授权"}
                </Button>
              </Card>
            ))}
          </section>
        </div>
      </main>
    </div>
  );
}
