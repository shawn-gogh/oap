"use client";

import { useEffect, useMemo, useState } from "react";
import {
  FileUp,
  Search,
  CheckCircle2,
  AlertTriangle,
  Server,
  FileCode,
  Archive,
  Cpu,
  ArrowRight,
  ShieldAlert,
  Radio,
} from "lucide-react";

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
import { RuntimeProviderLogo } from "@/components/runtime-provider-logo";
import {
  discoverProviderAgents,
  importAgentBundle,
  importOpencodeAgentFiles,
  importProviderAgents,
  listImportProviders,
  listRuntimeHarnesses,
  previewProviderAgents,
  type ExternalAgent,
  type ImportItemResult,
  type ImportPreviewItem,
  type ImportProvider,
} from "@/lib/api";
import type { Agent, RuntimeHarness } from "@/lib/types";
import { cn } from "@/lib/utils";

const IMPORT_RESULT_META: Record<string, { label: string; className: string }> = {
  imported: { label: "已成功导入", className: "text-emerald-600 dark:text-emerald-400 bg-emerald-500/10 border-emerald-500/30" },
  unchanged: { label: "配置已是最新", className: "text-muted-foreground bg-muted border-border" },
  drift_pending: { label: "配置漂移待评审", className: "text-amber-600 dark:text-amber-400 bg-amber-500/10 border-amber-500/30" },
  blocked: { label: "触发安全阻断", className: "text-destructive bg-destructive/10 border-destructive/30" },
};

interface ImportAgentDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onImported: (agents: Agent[]) => void;
}

export function ImportAgentDialog({ open, onOpenChange, onImported }: ImportAgentDialogProps) {
  const [mode, setMode] = useState<"remote" | "files" | "bundle">("remote");
  const [providers, setProviders] = useState<ImportProvider[]>([]);
  const [runtimes, setRuntimes] = useState<RuntimeHarness[]>([]);
  const [providersLoading, setProvidersLoading] = useState(false);
  const [providerId, setProviderId] = useState("");
  const [runtimeId, setRuntimeId] = useState("");
  const [endpoint, setEndpoint] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [credentialMode, setCredentialMode] = useState<"shared" | "byo">("shared");
  const [externalAgents, setExternalAgents] = useState<ExternalAgent[]>([]);
  const [agentFiles, setAgentFiles] = useState<Array<{ filename: string; content: string }>>([]);
  const [bundle, setBundle] = useState<{ filename: string; base64: string } | null>(null);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<ImportPreviewItem[]>([]);
  const [previewConfirmed, setPreviewConfirmed] = useState(false);
  const [importResults, setImportResults] = useState<ImportItemResult[] | null>(null);

  useEffect(() => {
    if (!open) return;
    setProvidersLoading(true);
    Promise.all([listImportProviders(), listRuntimeHarnesses()])
      .then(([sourceProviders, runtimeHarnesses]) => {
        const compatibleRuntimes = runtimeHarnesses.filter(
          (runtime) => runtime.api_spec === "claude_managed_agents",
        );
        setProviders(sourceProviders);
        setRuntimes(compatibleRuntimes);
        setProviderId(sourceProviders[0]?.id ?? "");
        setRuntimeId(
          compatibleRuntimes.find((runtime) => runtime.connected)?.alias ??
            compatibleRuntimes[0]?.alias ??
            "",
        );
      })
      .catch(() => {
        setProviders([]);
        setRuntimes([]);
        setProviderId("");
        setRuntimeId("");
      })
      .finally(() => setProvidersLoading(false));
  }, [open]);

  const selectedProvider = providers.find((provider) => provider.id === providerId);
  const providerName = selectedProvider?.name ?? providerId;
  const filteredAgents = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) return externalAgents;
    return externalAgents.filter((agent) =>
      `${agent.name} ${agent.description ?? ""} ${agent.id}`.toLowerCase().includes(normalized),
    );
  }, [externalAgents, query]);

  const reset = () => {
    setMode("remote");
    setEndpoint("");
    setApiKey("");
    setCredentialMode("shared");
    setExternalAgents([]);
    setAgentFiles([]);
    setBundle(null);
    setSelectedIds([]);
    setQuery("");
    setError(null);
    setPreview([]);
    setPreviewConfirmed(false);
    setImportResults(null);
  };

  const close = (nextOpen: boolean) => {
    onOpenChange(nextOpen);
    if (!nextOpen) reset();
  };

  const discover = async () => {
    setLoading(true);
    setError(null);
    try {
      const discovered = await discoverProviderAgents({ providerId, endpoint, apiKey });
      setExternalAgents(discovered);
      setSelectedIds(discovered.map((agent) => agent.id));
      setPreview([]);
      setPreviewConfirmed(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  const importSelected = async () => {
    if (mode === "bundle") {
      if (!bundle) {
        setError("请先选择一个 ZIP 格式智能体压缩包。");
        return;
      }
      setSaving(true);
      setError(null);
      try {
        const imported = await importAgentBundle({
          filename: bundle.filename,
          contentBase64: bundle.base64,
          runtime: runtimeId || undefined,
        });
        onImported(imported);
        close(false);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setSaving(false);
      }
      return;
    }
    if (mode === "files") {
      if (agentFiles.length === 0) {
        setError("请至少选择一个 Markdown 格式智能体配置文件。");
        return;
      }
      setSaving(true);
      setError(null);
      try {
        const imported = await importOpencodeAgentFiles({
          runtime: runtimeId || undefined,
          files: agentFiles,
        });
        onImported(imported);
        close(false);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setSaving(false);
      }
      return;
    }

    const selected = externalAgents.filter((agent) => selectedIds.includes(agent.id));
    if (selected.length === 0) {
      setError("请至少选择一个待纳管的智能体。");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const previewItems = await previewProviderAgents({
        providerId,
        endpoint,
        credentialMode,
        agents: selected.map((agent) => ({
          externalId: agent.id,
          name: agent.name,
          description: agent.description,
          model: agent.model,
          raw: agent.raw,
        })),
      });
      setPreview(previewItems);
      const blocking = previewItems.flatMap((item) => item.issues).filter(
        (issue) => issue.severity === "blocking",
      );
      if (blocking.length > 0) {
        setError(`预检发现 ${blocking.length} 个阻断项问题，请修复后再行导入。`);
        return;
      }
      const requiresApproval = previewItems.flatMap((item) => item.issues).some(
        (issue) => issue.severity === "approval_required",
      );
      if (requiresApproval && !previewConfirmed) {
        setPreviewConfirmed(true);
        setError("预检发现存在需要授权审计的高风险属性。请核对下方预检清单后再次点击确认导入。");
        return;
      }
      const { agents: importedAgents, results } = await importProviderAgents({
        providerId,
        endpoint,
        apiKey: credentialMode === "shared" ? apiKey : undefined,
        credentialMode,
        agents: selected.map((agent) => ({
          externalId: agent.id,
          name: agent.name,
          description: agent.description,
          model: agent.model,
          raw: agent.raw,
        })),
      });
      onImported(importedAgents);
      const needsAttention = results.some(
        (result) => result.status === "blocked" || result.status === "drift_pending",
      );
      if (needsAttention) {
        setImportResults(results);
        setPreview([]);
        setError(null);
      } else {
        close(false);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  const toggleAgent = (id: string) => {
    setPreview([]);
    setPreviewConfirmed(false);
    setSelectedIds((current) =>
      current.includes(id) ? current.filter((value) => value !== id) : [...current, id],
    );
  };

  const loadBundle = (files: FileList | null) => {
    const file = files?.[0];
    if (!file) return;
    setError(null);
    const reader = new FileReader();
    reader.onload = () => {
      const result = String(reader.result ?? "");
      const base64 = result.includes(",") ? result.slice(result.indexOf(",") + 1) : result;
      setBundle({ filename: file.name, base64 });
    };
    reader.onerror = () => setError("读取智能体压缩包失败。");
    reader.readAsDataURL(file);
  };

  const loadAgentFiles = async (files: FileList | null) => {
    if (!files || files.length === 0) return;
    setError(null);
    const markdownFiles = Array.from(files).filter((file) => /\.(md|markdown)$/i.test(file.name));
    if (markdownFiles.length === 0) {
      setAgentFiles([]);
      setError("请选择 Markdown (.md) 扩展名配置文件。");
      return;
    }
    try {
      const loaded = await Promise.all(
        markdownFiles.map(async (file) => ({
          filename: file.name,
          content: await file.text(),
        })),
      );
      setAgentFiles(loaded);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent className="w-[94vw] sm:max-w-3xl max-h-[90vh] grid-rows-[auto_minmax(0,1fr)_auto] gap-0 p-0 rounded-2xl border-border/80 bg-background text-foreground shadow-2xl selection:bg-blue-500/20">
        {/* Anti-slop Header */}
        <DialogHeader className="px-6 py-4 border-b border-border/80 bg-card/60 backdrop-blur">
          <div className="flex items-center gap-2.5">
            <div className="flex size-8 items-center justify-center rounded-xl bg-blue-500/10 text-blue-600 dark:text-blue-400 border border-blue-500/20 shadow-2xs">
              <Cpu className="size-4" />
            </div>
            <div>
              <DialogTitle className="text-base font-bold tracking-tight">纳管外部智能体与接入中心</DialogTitle>
              <p className="text-xs text-muted-foreground font-medium pt-0.5">登记第三方 Agent 拓扑，实现凭据隔离、运行保活与漂移监控</p>
            </div>
          </div>
        </DialogHeader>

        <div className="flex flex-col gap-5 px-6 py-5 overflow-y-auto space-y-1">
          {importResults && (
            <div className="rounded-xl border border-blue-500/30 bg-blue-500/10 p-3.5 text-xs text-blue-700 dark:text-blue-300">
              纳管请求处理完毕。以下为需要管理员关注的节点状态，其余项目已自动归档登记。
            </div>
          )}

          {!importResults && (
            <>
              {/* Channel Selector Cards */}
              <div className="grid grid-cols-3 gap-2 p-1 rounded-xl bg-muted/40 border border-border/70">
                {[
                  { value: "remote" as const, label: "远程 Agent 协议发现", icon: Server },
                  { value: "files" as const, label: "Markdown 描述配置", icon: FileCode },
                  { value: "bundle" as const, label: "ZIP 智能体包上传", icon: Archive },
                ].map((option) => {
                  const Icon = option.icon;
                  const active = mode === option.value;
                  return (
                    <button
                      key={option.value}
                      type="button"
                      onClick={() => {
                        setMode(option.value);
                        setError(null);
                      }}
                      className={cn(
                        "flex items-center justify-center gap-2 h-9 rounded-lg px-3 text-xs font-semibold transition-all",
                        active
                          ? "bg-background text-foreground shadow-2xs border border-border/80 font-bold"
                          : "text-muted-foreground hover:text-foreground hover:bg-muted/50",
                      )}
                    >
                      <Icon className="size-3.5" />
                      <span>{option.label}</span>
                    </button>
                  );
                })}
              </div>

              {/* Governance Workflow Banner - Pure Chinese */}
              <div className="rounded-xl border border-border/70 bg-card p-3.5 text-xs space-y-1.5 shadow-2xs">
                <div className="flex items-center gap-2 font-bold text-foreground">
                  <span className="flex size-5 items-center justify-center rounded-md bg-blue-500/10 text-blue-500 text-[10px]">1</span>
                  <span>零信任纳管链路：注册草稿 ➔ 安全预检 ➔ 静态审计 ➔ 凭据隔离授权</span>
                </div>
                <p className="text-[11px] text-muted-foreground leading-relaxed pl-7">
                  纳管智能体将优先处于隔离草稿模式。通过健康保活与管理审批后方可对外调度。
                </p>
              </div>

              {/* Source/Runtime Selection */}
              <div className="grid gap-2">
                <Label className="text-xs font-bold text-muted-foreground uppercase tracking-wide">
                  {mode === "remote" ? "纳管来源平台" : "绑定执行运行时"}
                </Label>
                <div className="grid gap-2">
                  {mode === "remote"
                    ? providers.map((provider) => {
                        const selected = provider.id === providerId;
                        return (
                          <button
                            key={provider.id}
                            type="button"
                            onClick={() => {
                              setProviderId(provider.id);
                              setExternalAgents([]);
                              setSelectedIds([]);
                              setPreview([]);
                              setPreviewConfirmed(false);
                            }}
                            className={cn(
                              "flex w-full items-center gap-3 rounded-xl border border-border/80 bg-card p-3 text-left transition-all hover:bg-muted/40",
                              selected && "border-blue-500 bg-blue-500/5 ring-1 ring-blue-500/30",
                            )}
                          >
                            <RuntimeProviderLogo alias={provider.id} apiSpec={provider.api_spec} />
                            <span className="min-w-0 flex-1">
                              <span className="block text-xs font-bold leading-tight text-foreground">
                                {provider.name}
                              </span>
                              <span className="mt-0.5 block truncate font-mono text-[10px] text-muted-foreground">
                                {provider.id} · 契约 {provider.capabilities.runtime_contract}
                              </span>
                            </span>
                          </button>
                        );
                      })
                    : runtimes.map((runtime) => {
                        const selected = runtime.alias === runtimeId;
                        return (
                          <button
                            key={runtime.alias}
                            type="button"
                            onClick={() => setRuntimeId(runtime.alias)}
                            className={cn(
                              "flex w-full items-center gap-3 rounded-xl border border-border/80 bg-card p-3 text-left transition-all hover:bg-muted/40",
                              selected && "border-blue-500 bg-blue-500/5 ring-1 ring-blue-500/30",
                            )}
                          >
                            <RuntimeProviderLogo alias={runtime.alias} apiSpec={runtime.api_spec} />
                            <span className="min-w-0 flex-1">
                              <span className="block text-xs font-bold leading-tight text-foreground">
                                {runtime.display_name}
                              </span>
                              <span className="mt-0.5 block truncate font-mono text-[10px] text-muted-foreground">
                                {runtime.alias} · {runtime.connected ? "保活连通正常" : "未建立连接"}
                              </span>
                            </span>
                          </button>
                        );
                      })}
                  {providersLoading && (
                    <div className="rounded-xl border border-dashed border-border/80 p-4 text-xs font-mono text-muted-foreground text-center">
                      正在抓取纳管来源元数据...
                    </div>
                  )}
                  {!providersLoading &&
                    (mode === "remote" ? providers.length === 0 : runtimes.length === 0) && (
                    <div className="rounded-xl border border-dashed border-border/80 p-4 text-xs font-mono text-muted-foreground text-center">
                      {mode === "remote" ? "暂无可纳管的外部平台来源。" : "暂无兼容的本地执行运行时。"}
                    </div>
                  )}
                </div>
              </div>

              {/* Mode Specific Inputs */}
              {mode === "remote" ? (
                <div className="space-y-3 pt-1">
                  <div className="grid gap-1.5">
                    <Label htmlFor="import-endpoint" className="text-xs font-medium uppercase text-muted-foreground">{providerName} 服务地址 (Endpoint)</Label>
                    <Input
                      id="import-endpoint"
                      value={endpoint}
                      onChange={(e) => {
                        setEndpoint(e.target.value);
                        setExternalAgents([]);
                        setSelectedIds([]);
                        setPreview([]);
                        setPreviewConfirmed(false);
                      }}
                      placeholder="https://agents-cluster.example.com"
                      className="h-9 font-mono text-xs"
                    />
                  </div>
                  <div className="grid gap-1.5">
                    <Label htmlFor="import-key" className="text-xs font-medium uppercase text-muted-foreground">{providerName} API 访问密钥</Label>
                    <Input
                      id="import-key"
                      type="password"
                      autoComplete="current-password"
                      value={apiKey}
                      onChange={(e) => setApiKey(e.target.value)}
                      placeholder="请输入平台 Api Key"
                      className="h-9 font-mono text-xs"
                    />
                  </div>
                  <div className="grid gap-1.5">
                    <Label className="text-xs font-medium uppercase text-muted-foreground">凭据托管与隔离策略</Label>
                    <div className="grid grid-cols-2 gap-2">
                      {[
                        { value: "shared" as const, label: "属主加密凭据", desc: "密钥存入个人凭据池，隔离访问" },
                        { value: "byo" as const, label: "运行时透传", desc: "直接由底层 Runtime 管理密钥" },
                      ].map((option) => (
                        <button
                          key={option.value}
                          type="button"
                          onClick={() => {
                            setCredentialMode(option.value);
                            setPreview([]);
                            setPreviewConfirmed(false);
                          }}
                          className={cn(
                            "rounded-xl border p-2.5 text-left text-xs transition-all",
                            credentialMode === option.value
                              ? "border-blue-500 bg-blue-500/10 font-medium"
                              : "border-border/80 bg-card hover:bg-muted/30",
                          )}
                        >
                          <span className="block font-bold">{option.label}</span>
                          <span className="text-[10px] text-muted-foreground block mt-0.5">{option.desc}</span>
                        </button>
                      ))}
                    </div>
                  </div>
                  <div className="flex items-center justify-between pt-1">
                    <Button
                      type="button"
                      size="sm"
                      className="h-8 text-xs bg-blue-600 hover:bg-blue-700 text-white font-medium gap-1.5"
                      onClick={discover}
                      disabled={loading || !providerId || !endpoint.trim() || !apiKey.trim()}
                    >
                      <Radio className="size-3.5" />
                      {loading ? "连接探针探查中..." : "测试连接并发现智能体"}
                    </Button>
                    {externalAgents.length > 0 && (
                      <span className="font-mono text-xs text-muted-foreground">
                        在目标端点成功发现 <strong className="text-foreground">{externalAgents.length}</strong> 个智能体
                      </span>
                    )}
                  </div>
                </div>
              ) : mode === "bundle" ? (
                <div className="grid gap-2 pt-1">
                  <Label htmlFor="agent-bundle-zip" className="text-xs font-medium uppercase text-muted-foreground">选择智能体压缩包 (.zip)</Label>
                  <label className="flex cursor-pointer flex-col items-center justify-center gap-2 rounded-2xl border border-dashed border-border/80 bg-card p-8 text-xs text-muted-foreground hover:bg-muted/30 transition-all">
                    <FileUp className="size-6 text-blue-500" />
                    <span className="font-semibold text-foreground">{bundle ? bundle.filename : "点击或拖拽上传 .zip 归档文件"}</span>
                    <span className="text-[11px]">归档包内的描述文件与 Prompt 将自动解析为 Agent，关联知识库将同步导入工作区。</span>
                    <input
                      id="agent-bundle-zip"
                      type="file"
                      accept=".zip,application/zip"
                      className="sr-only"
                      onChange={(event) => loadBundle(event.target.files)}
                    />
                  </label>
                </div>
              ) : (
                <div className="grid gap-2 pt-1">
                  <Label htmlFor="opencode-agent-files" className="text-xs font-medium uppercase text-muted-foreground">选择 Markdown 智能体配置文件 (.md)</Label>
                  <label className="flex cursor-pointer flex-col items-center justify-center gap-2 rounded-2xl border border-dashed border-border/80 bg-card p-8 text-xs text-muted-foreground hover:bg-muted/30 transition-all">
                    <FileUp className="size-6 text-blue-500" />
                    <span className="font-semibold text-foreground">{agentFiles.length ? `已选择 ${agentFiles.length} 个描述文件` : "点击或拖拽上传一个或多个 .md / .markdown 文件"}</span>
                    <input
                      id="opencode-agent-files"
                      type="file"
                      multiple
                      accept=".md,.markdown,text/markdown,text/plain"
                      className="sr-only"
                      onChange={(event) => void loadAgentFiles(event.target.files)}
                    />
                  </label>
                  {agentFiles.length > 0 && (
                    <div className="max-h-44 divide-y divide-border/60 overflow-y-auto rounded-xl border border-border/80 bg-card">
                      {agentFiles.map((file) => (
                        <div key={file.filename} className="px-3.5 py-2.5 flex items-center justify-between text-xs">
                          <span className="truncate font-medium text-foreground">{file.filename}</span>
                          <span className="font-mono text-[10px] text-muted-foreground">{file.content.length} 字符</span>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              )}

              {/* Agent List Discovery */}
              {mode === "remote" && externalAgents.length > 0 && (
                <div className="grid gap-2 pt-2">
                  <div className="flex items-center gap-2">
                    <div className="relative flex-1">
                      <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground" />
                      <Input
                        value={query}
                        onChange={(e) => setQuery(e.target.value)}
                        placeholder="检索发现的智能体名称或描述..."
                        aria-label="搜索已发现的智能体"
                        className="pl-8 h-8 text-xs"
                      />
                    </div>
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      className="h-8 text-xs"
                      onClick={() => setSelectedIds(filteredAgents.map((agent) => agent.id))}
                    >
                      全选
                    </Button>
                  </div>
                  <div className="max-h-60 divide-y divide-border/60 overflow-y-auto rounded-xl border border-border/80 bg-card">
                    {filteredAgents.map((agent) => (
                      <label
                        key={agent.id}
                        className="flex cursor-pointer items-start gap-3 p-3 hover:bg-muted/30 transition-colors"
                      >
                        <input
                          type="checkbox"
                          className="mt-0.5 rounded border-border"
                          checked={selectedIds.includes(agent.id)}
                          onChange={() => toggleAgent(agent.id)}
                        />
                        <span className="min-w-0 flex-1 space-y-0.5">
                          <span className="block truncate text-xs font-bold text-foreground">{agent.name}</span>
                          <span className="block truncate font-mono text-[10px] text-muted-foreground">
                            {agent.id}
                          </span>
                          {agent.description && (
                            <span className="block line-clamp-2 text-[11px] text-muted-foreground leading-relaxed pt-0.5">
                              {agent.description}
                            </span>
                          )}
                        </span>
                      </label>
                    ))}
                    {filteredAgents.length === 0 && (
                      <p className="px-3 py-8 text-center text-xs text-muted-foreground">
                        没有找到匹配的智能体。
                      </p>
                    )}
                  </div>
                </div>
              )}

              {mode === "remote" && !importResults && error && (
                <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-3 text-xs font-mono text-destructive">
                  {error}
                </div>
              )}

              {/* Pre-Flight Preview */}
              {mode === "remote" && preview.length > 0 && (
                <div className="rounded-xl border border-border/80 bg-card p-4 space-y-2.5 shadow-2xs">
                  <div className="flex items-center justify-between gap-3">
                    <div className="flex items-center gap-2 text-xs font-bold text-foreground">
                      <ShieldAlert className="size-4 text-blue-500" />
                      <span>纳管安全与规范化预检</span>
                    </div>
                    <span className="font-mono text-[11px] text-muted-foreground">
                      {preview.length} 个预检条目
                    </span>
                  </div>
                  <div className="max-h-40 space-y-2 overflow-y-auto pt-1">
                    {preview.map((item) => (
                      <div key={item.external_id} className="rounded-lg border border-border/60 bg-muted/20 p-2.5">
                        <div className="flex items-center justify-between gap-2 text-xs">
                          <span className="truncate font-mono font-bold text-foreground">{item.external_id}</span>
                          <span className={cn("text-[10px] font-bold px-1.5 py-0.5 rounded-md border", item.can_import ? "text-emerald-600 bg-emerald-500/10 border-emerald-500/30" : "text-destructive bg-destructive/10 border-destructive/30")}>
                            {item.can_import ? "预检通过" : "已触发表格阻断"}
                          </span>
                        </div>
                        {item.issues.map((issue) => (
                          <p key={`${issue.code}-${issue.field}`} className="mt-1 text-[11px] text-muted-foreground font-mono">
                            [{issue.severity}] {issue.field}：{issue.message}
                          </p>
                        ))}
                        {item.issues.length === 0 && (
                          <p className="mt-1 text-[11px] text-muted-foreground">规范校验通过，未发现风险阻断。</p>
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </>
          )}

          {importResults && (
            <div className="rounded-xl border border-border/80 bg-card p-4 space-y-2.5 shadow-2xs">
              <p className="text-xs font-bold text-foreground">纳管处理结果</p>
              <div className="max-h-60 space-y-2 overflow-y-auto pt-1">
                {importResults.map((result) => {
                  const meta = IMPORT_RESULT_META[result.status] ?? {
                    label: result.status,
                    className: "text-muted-foreground border-border",
                  };
                  return (
                    <div
                      key={result.external_id}
                      className="rounded-lg border border-border/60 bg-muted/20 p-3"
                    >
                      <div className="flex items-center justify-between gap-2 text-xs">
                        <span className="truncate font-mono font-bold text-foreground">{result.external_id}</span>
                        <span className={cn("shrink-0 font-bold text-[10px] px-2 py-0.5 rounded-md border", meta.className)}>
                          {meta.label}
                        </span>
                      </div>
                      {result.status === "drift_pending" && (
                        <p className="mt-1.5 text-xs text-muted-foreground leading-relaxed">
                          智能体已保持已通过审计的配置不变；远端变更已被拦截并生成快照，
                          {result.agent_id ? (
                            <a
                              href={`/agents/detail/?id=${encodeURIComponent(result.agent_id)}`}
                              className="text-blue-600 dark:text-blue-400 font-bold underline underline-offset-2 ml-1"
                            >
                              前往治理面板审核
                            </a>
                          ) : (
                            "请在治理面板评审"
                          )}
                          。
                        </p>
                      )}
                      {result.status === "blocked" &&
                        result.issues.map((issue, index) => (
                          <p key={`${issue.code ?? index}`} className="mt-1 text-xs font-mono text-destructive">
                            {issue.message ?? issue.code}
                          </p>
                        ))}
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {mode !== "remote" && error && (
            <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-3 text-xs font-mono text-destructive">
              {error}
            </div>
          )}
        </div>

        <DialogFooter className="px-6 py-3.5 border-t border-border/80 bg-card/60 backdrop-blur">
          {importResults ? (
            <Button size="sm" className="text-xs bg-blue-600 hover:bg-blue-700 text-white font-medium" onClick={() => close(false)}>
              完成并返回
            </Button>
          ) : (
            <div className="flex items-center gap-2">
              <Button size="sm" variant="outline" className="text-xs" onClick={() => close(false)} disabled={saving}>
                取消
              </Button>
              <Button
                size="sm"
                className="text-xs bg-blue-600 hover:bg-blue-700 text-white font-medium gap-1.5"
                onClick={importSelected}
                disabled={saving || (mode === "remote" ? selectedIds.length === 0 : mode === "bundle" ? !bundle : agentFiles.length === 0)}
              >
                <ArrowRight className="size-3.5" />
                {saving
                  ? "纳管注册中…"
                  : `${mode === "remote" && previewConfirmed ? "确认" : ""}纳管导入${mode === "remote" ? ` (${selectedIds.length})` : mode === "bundle" ? "" : ` (${agentFiles.length})`}`}
              </Button>
            </div>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
