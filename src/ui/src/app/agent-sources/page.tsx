"use client";

import { useEffect, useState } from "react";
import { Activity, Plus, RefreshCw, Server, Trash2, Cpu } from "lucide-react";
import { toast } from "sonner";
import { AiGatewayBrandIcon } from "@/components/brand-kit-icons";
import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { Badge } from "@/components/ui/badge";
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
import { useConfirm } from "@/components/confirm-dialog";
import {
  apiErrorMessage,
  createAgentSourceConnector,
  deleteAgentSourceConnector,
  listAgentSourceConnectors,
  listImportProviders,
  testAgentSourceConnector,
  type AgentSourceConnector,
  type ImportProvider,
} from "@/lib/api";

export default function AgentSourcesPage() {
  const confirmAction = useConfirm();
  const [connectors, setConnectors] = useState<AgentSourceConnector[]>([]);
  const [providers, setProviders] = useState<ImportProvider[]>([]);
  const [loading, setLoading] = useState(true);
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [provider, setProvider] = useState("");
  const [endpoint, setEndpoint] = useState("");
  const [credentialName, setCredentialName] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [webhookSecret, setWebhookSecret] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const [nextConnectors, nextProviders] = await Promise.all([
        listAgentSourceConnectors(),
        listImportProviders(),
      ]);
      setConnectors(nextConnectors);
      setProviders(nextProviders);
      setProvider((current) => current || nextProviders[0]?.id || "");
    } catch (cause) {
      setError(apiErrorMessage(cause, "加载智能体来源失败"));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const create = async () => {
    setBusy("create");
    setError(null);
    try {
      const connector = await createAgentSourceConnector({
        name,
        provider,
        endpoint,
        credentialName: credentialName || undefined,
        apiKey: apiKey || undefined,
        webhookSecret: webhookSecret || undefined,
      });
      setConnectors((current) => [connector, ...current]);
      setOpen(false);
      setName("");
      setEndpoint("");
      setCredentialName("");
      setApiKey("");
      setWebhookSecret("");
      toast.success("来源平台已添加");
    } catch (cause) {
      setError(apiErrorMessage(cause, "添加来源平台失败"));
    } finally {
      setBusy(null);
    }
  };

  const test = async (connector: AgentSourceConnector) => {
    setBusy(connector.id);
    setError(null);
    try {
      const updated = await testAgentSourceConnector(connector.id);
      setConnectors((current) =>
        current.map((item) => (item.id === updated.id ? updated : item)),
      );
      toast.success(updated.status === "healthy" ? "连接测试通过" : "连接测试未通过");
    } catch (cause) {
      setError(apiErrorMessage(cause, "连接测试失败"));
    } finally {
      setBusy(null);
    }
  };

  const remove = async (connector: AgentSourceConnector) => {
    const confirmed = await confirmAction({
      title: "删除来源平台",
      description: "关联智能体会保留来源证据，但将停止自动同步。",
      confirmLabel: "确认删除",
    });
    if (!confirmed) return;
    setBusy(connector.id);
    try {
      await deleteAgentSourceConnector(connector.id);
      setConnectors((current) => current.filter((item) => item.id !== connector.id));
      toast.success("来源平台已删除");
    } catch (cause) {
      setError(apiErrorMessage(cause, "删除来源平台失败"));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden selection:bg-blue-500/20">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        {/* Anti-slop Pure Chinese Header */}
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border/80 px-4 bg-background/80 backdrop-blur">
          <div className="flex items-center gap-2.5">
            <div className="flex size-7 items-center justify-center rounded-md bg-blue-500/10 text-blue-600 dark:text-blue-400 ring-1 ring-blue-500/20">
              <AiGatewayBrandIcon size={16} />
            </div>
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold tracking-tight">AI 网关基础设施</span>
              <span className="text-xs text-muted-foreground font-medium">/ 智能体来源</span>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button size="sm" className="h-8 gap-1.5 text-xs bg-blue-600 text-white hover:bg-blue-700 font-medium" onClick={() => setOpen(true)}>
              <Plus className="size-3.5" />
              手动添加来源
            </Button>
            <ThemeToggle />
          </div>
        </header>

        <main id="main-content" className="flex-1 overflow-y-auto p-6 space-y-6">
          <div className="mx-auto flex max-w-5xl flex-col gap-6">
            {/* Command Banner - Pure Chinese */}
            <div className="relative overflow-hidden rounded-2xl border border-border/70 bg-card p-6 shadow-xs">
              <div className="absolute right-0 top-0 -mr-16 -mt-16 size-64 rounded-full bg-blue-500/5 blur-3xl pointer-events-none" />
              <div className="relative z-10 flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
                <div className="space-y-1.5 max-w-xl">
                  <div className="flex items-center gap-2">
                    <span className="inline-flex items-center gap-1 rounded-md bg-blue-500/10 px-2 py-0.5 text-xs font-medium text-blue-600 dark:text-blue-400 border border-blue-500/20">
                      <Cpu className="size-3" /> 外部平台与拓扑
                    </span>
                    <span className="text-xs text-muted-foreground font-medium">第三方 Agent 接入枢纽</span>
                  </div>
                  <h1 className="text-xl font-bold tracking-tight text-foreground">
                    智能体来源平台与协议连接器
                  </h1>
                  <p className="text-xs text-muted-foreground leading-relaxed">
                    登记与监控外部智能体来源（如 OpenCode、CrewAI、LangGraph 等）。定期同步智能体配置并提供连接保活。
                  </p>
                </div>
              </div>
            </div>

            {error && <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-3.5 text-xs font-mono text-destructive">{error}</div>}

            {loading ? (
              <div className="flex flex-col gap-3" aria-label="正在加载来源平台">
                {[0, 1, 2].map((item) => (
                  <div key={item} className="flex flex-col gap-2 rounded-2xl border border-border/70 p-4 bg-card shadow-2xs">
                    <div className="h-4 w-1/3 animate-pulse rounded bg-muted" />
                    <div className="h-3 w-2/3 animate-pulse rounded bg-muted" />
                  </div>
                ))}
              </div>
            ) : connectors.length === 0 ? (
              <Card className="grid place-items-center p-12 text-center rounded-2xl border-dashed">
                <Server className="size-8 text-muted-foreground" />
                <h2 className="mt-3 text-sm font-bold tracking-tight text-foreground">尚未接入外部来源平台</h2>
                <p className="mt-1 max-w-md text-xs text-muted-foreground leading-relaxed">
                  导入外部智能体时会自动在此处登记来源平台，并获得定期同步与状态保活。
                </p>
              </Card>
            ) : (
              <div className="grid gap-3">
                {connectors.map((connector) => (
                  <Card key={connector.id} className="p-5 rounded-2xl border-border/70 shadow-2xs">
                    <div className="flex flex-wrap items-start justify-between gap-3">
                      <div className="min-w-0 space-y-1">
                        <div className="flex flex-wrap items-center gap-2">
                          <h2 className="text-sm font-bold tracking-tight text-foreground">{connector.name}</h2>
                          <Badge variant={connector.status === "healthy" ? "secondary" : connector.status === "unreachable" ? "destructive" : "outline"} className="text-[10px]">
                            {connector.status === "healthy" ? "健康正常" : connector.status === "unreachable" ? "不可达" : connector.status}
                          </Badge>
                          {connector.protocol && <Badge variant="outline" className="text-[10px] font-mono">{connector.protocol}{connector.protocol_version ? ` v${connector.protocol_version}` : ""}</Badge>}
                        </div>
                        <p className="truncate font-mono text-xs text-muted-foreground">{connector.provider} · {connector.endpoint}</p>
                        <p className="text-xs text-muted-foreground font-mono pt-1">{connector.last_test_detail ?? "尚未执行连接测试。"}</p>
                      </div>
                      <div className="flex gap-2">
                        <Button size="sm" variant="outline" className="h-8 text-xs gap-1" disabled={busy !== null} onClick={() => void test(connector)}>
                          {busy === connector.id ? <RefreshCw className="size-3.5 animate-spin" /> : <Activity className="size-3.5" />}
                          测试连接
                        </Button>
                        <Button size="icon-sm" variant="ghost" className="h-8 w-8 text-destructive hover:bg-destructive/10" disabled={busy !== null} onClick={() => void remove(connector)} aria-label={`删除来源 ${connector.name}`}>
                          <Trash2 className="size-3.5" />
                        </Button>
                      </div>
                    </div>
                  </Card>
                ))}
              </div>
            )}
          </div>
        </main>
      </div>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="max-w-lg rounded-2xl">
          <DialogHeader><DialogTitle className="text-base font-bold">手动添加来源平台</DialogTitle></DialogHeader>
          <div className="grid gap-4 py-2">
            <div className="grid gap-1.5"><Label htmlFor="source-name" className="text-xs font-medium uppercase text-muted-foreground">平台名称</Label><Input id="source-name" value={name} onChange={(event) => setName(event.target.value)} placeholder="如: 生产 OpenCode 平台" className="h-9 text-xs" /></div>
            <div className="grid gap-1.5">
              <Label className="text-xs font-medium uppercase text-muted-foreground">来源提供方</Label>
              <div className="grid gap-2 sm:grid-cols-2">
                {providers.map((item) => (
                  <button key={item.id} type="button" onClick={() => setProvider(item.id)} className={`rounded-xl border p-3 text-left text-xs transition-all ${provider === item.id ? "border-blue-500 bg-blue-500/10 font-medium" : "border-border bg-card hover:bg-muted/40"}`}>
                    <span className="block font-bold">{item.name}</span>
                    <span className="font-mono text-[10px] text-muted-foreground block mt-0.5">协议 · {item.capabilities.runtime_contract}</span>
                  </button>
                ))}
              </div>
            </div>
            <div className="grid gap-1.5"><Label htmlFor="source-endpoint" className="text-xs font-medium uppercase text-muted-foreground">服务地址 (Endpoint)</Label><Input id="source-endpoint" value={endpoint} onChange={(event) => setEndpoint(event.target.value)} placeholder="https://agents.example.com" className="h-9 font-mono text-xs" /></div>
            <div className="grid gap-1.5"><Label htmlFor="source-credential" className="text-xs font-medium uppercase text-muted-foreground">个人凭据名称（可选）</Label><Input id="source-credential" value={credentialName} onChange={(event) => setCredentialName(event.target.value)} placeholder="provider:opencode:agent:default" className="h-9 font-mono text-xs" /><p className="text-[11px] text-muted-foreground">密钥保存在加密保险库中，不会写入连接器配置文件。</p></div>
            <div className="grid gap-1.5"><Label htmlFor="source-api-key" className="text-xs font-medium uppercase text-muted-foreground">API 密钥（可选）</Label><Input id="source-api-key" type="password" autoComplete="new-password" value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder="创建时加密存入个人凭据" className="h-9 font-mono text-xs" /></div>
            <div className="grid gap-1.5"><Label htmlFor="source-webhook-secret" className="text-xs font-medium uppercase text-muted-foreground">Webhook 签名密钥（可选）</Label><Input id="source-webhook-secret" type="password" autoComplete="new-password" value={webhookSecret} onChange={(event) => setWebhookSecret(event.target.value)} placeholder="用于 HMAC-SHA256 验签防重放" className="h-9 font-mono text-xs" /></div>
          </div>
          <DialogFooter>
            <Button variant="outline" size="sm" className="text-xs" onClick={() => setOpen(false)}>取消</Button>
            <Button size="sm" className="text-xs bg-blue-600 hover:bg-blue-700 text-white font-medium" disabled={busy !== null || !name.trim() || !provider || !endpoint.trim()} onClick={() => void create()}>{busy === "create" ? "添加中…" : "确认添加"}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
