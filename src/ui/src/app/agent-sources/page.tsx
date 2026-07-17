"use client";

import { useEffect, useState } from "react";
import { Activity, Plus, RefreshCw, Server, Trash2 } from "lucide-react";
import { toast } from "sonner";

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
    <div className="flex min-h-screen bg-background">
      <Sidebar />
      <main className="min-w-0 flex-1">
        <header className="flex h-12 items-center justify-between border-b border-border px-4">
          <div>
            <h1 className="text-sm font-semibold tracking-tight">智能体来源</h1>
            <p className="text-xs text-muted-foreground">导入智能体时会自动登记来源平台；在这里查看同步状态、更换凭据或配置 Webhook。</p>
          </div>
          <div className="flex items-center gap-2">
            <ThemeToggle />
            <Button size="sm" onClick={() => setOpen(true)}>
              <Plus className="size-3.5" />手动添加
            </Button>
          </div>
        </header>
        <div className="mx-auto max-w-5xl p-5">
          {error && <p className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p>}
          {loading ? (
            <div className="flex flex-col gap-3" aria-label="正在加载来源平台">
              {[0, 1, 2].map((item) => (
                <div key={item} className="flex flex-col gap-2 rounded-lg border border-border p-4">
                  <div className="h-4 w-1/3 animate-pulse rounded bg-muted motion-reduce:animate-none" />
                  <div className="h-3 w-2/3 animate-pulse rounded bg-muted motion-reduce:animate-none" />
                </div>
              ))}
            </div>
          ) : connectors.length === 0 ? (
            <Card className="grid place-items-center p-12 text-center">
              <Server className="size-8 text-muted-foreground" />
              <h2 className="mt-3 text-sm font-semibold tracking-tight">尚未接入外部平台</h2>
              <p className="mt-1 max-w-md text-xs text-muted-foreground">从「智能体 → 导入」接入的平台会自动出现在这里，并获得定期同步与漂移防护；也可以手动添加。</p>
            </Card>
          ) : (
            <div className="grid gap-3">
              {connectors.map((connector) => (
                <Card key={connector.id} className="p-4">
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="min-w-0">
                      <div className="flex flex-wrap items-center gap-2">
                        <h2 className="text-sm font-semibold tracking-tight">{connector.name}</h2>
                        <Badge variant={connector.status === "healthy" ? "secondary" : connector.status === "unreachable" ? "destructive" : "outline"}>{connector.status}</Badge>
                        {connector.protocol && <Badge variant="outline">{connector.protocol}{connector.protocol_version ? ` v${connector.protocol_version}` : ""}</Badge>}
                      </div>
                      <p className="mt-1 truncate font-mono text-xs text-muted-foreground">{connector.provider} · {connector.endpoint}</p>
                      <p className="mt-2 text-xs text-muted-foreground">{connector.last_test_detail ?? "尚未执行连接测试。"}</p>
                    </div>
                    <div className="flex gap-2">
                      <Button size="sm" variant="outline" disabled={busy !== null} onClick={() => void test(connector)}>
                        {busy === connector.id ? <RefreshCw className="size-3.5 animate-spin motion-reduce:animate-none" /> : <Activity className="size-3.5" />}测试
                      </Button>
                      <Button size="icon-sm" variant="ghost" className="text-destructive" disabled={busy !== null} onClick={() => void remove(connector)} aria-label={`删除来源 ${connector.name}`}>
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

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="max-w-lg">
          <DialogHeader><DialogTitle>手动添加来源平台</DialogTitle></DialogHeader>
          <div className="grid gap-4 py-2">
            <div className="grid gap-1.5"><Label htmlFor="source-name">名称</Label><Input id="source-name" value={name} onChange={(event) => setName(event.target.value)} placeholder="生产 OpenCode" /></div>
            <div className="grid gap-1.5">
              <Label>来源提供方</Label>
              <div className="grid gap-2 sm:grid-cols-2">
                {providers.map((item) => (
                  <button key={item.id} type="button" onClick={() => setProvider(item.id)} className={`rounded-md border px-3 py-2 text-left text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50 ${provider === item.id ? "border-ring ring-2 ring-ring/20" : "border-border"}`}>
                    <span className="block font-medium">{item.name}</span><span className="font-mono text-[11px] text-muted-foreground">来源协议 · {item.capabilities.runtime_contract}</span>
                  </button>
                ))}
              </div>
            </div>
            <div className="grid gap-1.5"><Label htmlFor="source-endpoint">服务地址</Label><Input id="source-endpoint" value={endpoint} onChange={(event) => setEndpoint(event.target.value)} placeholder="https://agents.example.com" /></div>
            <div className="grid gap-1.5"><Label htmlFor="source-credential">个人凭据名称（可选）</Label><Input id="source-credential" value={credentialName} onChange={(event) => setCredentialName(event.target.value)} placeholder="provider:opencode:agent:default" /><p className="text-[11px] text-muted-foreground">仅允许引用当前属主的个人凭据，密钥不会写入连接器。</p></div>
            <div className="grid gap-1.5"><Label htmlFor="source-api-key">API 密钥（可选）</Label><Input id="source-api-key" type="password" autoComplete="new-password" value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder="创建时加密存入个人凭据" /></div>
            <div className="grid gap-1.5"><Label htmlFor="source-webhook-secret">Webhook 签名密钥（可选）</Label><Input id="source-webhook-secret" type="password" autoComplete="new-password" value={webhookSecret} onChange={(event) => setWebhookSecret(event.target.value)} placeholder="用于 HMAC-SHA256 验签和防重放" /></div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setOpen(false)}>取消</Button>
            <Button disabled={busy !== null || !name.trim() || !provider || !endpoint.trim()} onClick={() => void create()}>{busy === "create" ? "添加中…" : "添加"}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
