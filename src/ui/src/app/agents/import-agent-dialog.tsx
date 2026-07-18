"use client";

import { useEffect, useMemo, useState } from "react";
import { FileUp, Search } from "lucide-react";

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
  imported: { label: "已导入", className: "text-emerald-600 dark:text-emerald-400" },
  unchanged: { label: "已是最新", className: "text-muted-foreground" },
  drift_pending: { label: "变更待评审", className: "text-amber-600 dark:text-amber-400" },
  blocked: { label: "已阻断", className: "text-destructive" },
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
        setError("请先选择一个 .zip 智能体包。");
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
        setError("请至少选择一个 .md 智能体文件。");
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
      setError("请至少选择一个智能体。");
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
        setError(`预检发现 ${blocking.length} 个阻断问题，请修复后再导入。`);
        return;
      }
      const requiresApproval = previewItems.flatMap((item) => item.issues).some(
        (issue) => issue.severity === "approval_required",
      );
      if (requiresApproval && !previewConfirmed) {
        setPreviewConfirmed(true);
        setError("预检发现需要人工映射和审批的高风险字段。请核对下方结果后再次点击导入。");
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
      // Anything other than a clean import/unchanged needs the user to see
      // why: blocked items never entered the registry, drift_pending items
      // kept their approved config and await review on the governance panel.
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
    reader.onerror = () => setError("读取智能体包失败。");
    reader.readAsDataURL(file);
  };

  const loadAgentFiles = async (files: FileList | null) => {
    if (!files || files.length === 0) return;
    setError(null);
    const markdownFiles = Array.from(files).filter((file) => /\.(md|markdown)$/i.test(file.name));
    if (markdownFiles.length === 0) {
      setAgentFiles([]);
      setError("请选择 .md 或 .markdown 文件。");
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
      <DialogContent className="w-[94vw] sm:max-w-3xl max-h-[88vh] grid-rows-[auto_minmax(0,1fr)_auto] gap-0 p-0">
        <DialogHeader className="px-6 pt-6 pb-4 border-b border-border">
          <DialogTitle>纳管外部智能体</DialogTitle>
        </DialogHeader>
        <div className="flex flex-col gap-4 px-6 py-4 overflow-y-auto">
          {importResults && (
            <p className="text-sm text-muted-foreground">
              导入请求已处理完成。以下结果需要你关注——其余项已正常导入。
            </p>
          )}
          {!importResults && (
          <>
            <div className="grid grid-cols-3 rounded-lg border border-border bg-muted/30 p-1">
              {[
                { value: "remote" as const, label: "远程运行时" },
                { value: "files" as const, label: "Markdown 文件" },
                { value: "bundle" as const, label: "智能体包（.zip）" },
              ].map((option) => (
                <button
                  key={option.value}
                  type="button"
                  onClick={() => {
                    setMode(option.value);
                    setError(null);
                  }}
                  className={cn(
                    "h-8 rounded-md px-3 text-sm font-medium text-muted-foreground transition-colors",
                    mode === option.value
                      ? "bg-background text-foreground shadow-sm"
                      : "hover:text-foreground",
                  )}
                >
                  {option.label}
                </button>
              ))}
            </div>
            <div className="rounded-lg border border-border bg-muted/30 px-4 py-3 text-sm">
              <p className="font-medium">纳管流程：导入 → 测试 → 审批发布 → 授权 → 监控</p>
              <p className="mt-1 text-xs text-muted-foreground">
                导入只生成草稿版本，运行检查通过并由管理员审批后才可运行。共享凭据按导入人隔离存储，不会成为全局凭据。
              </p>
            </div>
            <div className="grid gap-1.5">
              <Label>{mode === "remote" ? "来源平台" : "执行运行时"}</Label>
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
                            "flex w-full items-center gap-3 rounded-lg border border-border bg-background p-3 text-left transition-colors hover:bg-muted/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
                            selected && "border-ring bg-muted/60 ring-2 ring-ring/20",
                          )}
                        >
                          <RuntimeProviderLogo alias={provider.id} apiSpec={provider.api_spec} />
                          <span className="min-w-0 flex-1">
                            <span className="block text-sm font-medium leading-tight">
                              {provider.name}
                            </span>
                            <span className="mt-0.5 block truncate font-mono text-[11px] text-muted-foreground">
                              {provider.id} · {provider.capabilities.runtime_contract}
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
                            "flex w-full items-center gap-3 rounded-lg border border-border bg-background p-3 text-left transition-colors hover:bg-muted/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
                            selected && "border-ring bg-muted/60 ring-2 ring-ring/20",
                          )}
                        >
                          <RuntimeProviderLogo alias={runtime.alias} apiSpec={runtime.api_spec} />
                          <span className="min-w-0 flex-1">
                            <span className="block text-sm font-medium leading-tight">
                              {runtime.display_name}
                            </span>
                            <span className="mt-0.5 block truncate font-mono text-[11px] text-muted-foreground">
                              {runtime.alias} · {runtime.connected ? "已连接" : "未连接"}
                            </span>
                          </span>
                        </button>
                      );
                    })}
                {providersLoading && (
                  <div className="rounded-lg border border-dashed border-border px-3 py-4 text-sm text-muted-foreground">
                    正在加载纳管来源…
                  </div>
                )}
                {!providersLoading &&
                  (mode === "remote" ? providers.length === 0 : runtimes.length === 0) && (
                  <div className="rounded-lg border border-dashed border-border px-3 py-4 text-sm text-muted-foreground">
                    {mode === "remote" ? "暂无可用的导入来源。" : "暂无兼容的执行运行时。"}
                  </div>
                )}
              </div>
            </div>
            {mode === "remote" ? (
              <>
                <div className="grid gap-1.5">
                  <Label htmlFor="import-endpoint">{providerName} 服务地址</Label>
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
                    placeholder="https://deployment.kb.us-central1.gcp.cloud.es.io"
                  />
                </div>
                <div className="grid gap-1.5">
                  <Label htmlFor="import-key">{providerName} API 密钥</Label>
                  <Input
                    id="import-key"
                    type="password"
                    autoComplete="current-password"
                    value={apiKey}
                    onChange={(e) => setApiKey(e.target.value)}
                    placeholder="API 密钥"
                  />
                </div>
                <div className="grid gap-1.5">
                  <Label>凭据策略</Label>
                  <div className="grid grid-cols-3 rounded-lg border border-border bg-muted/30 p-1">
                    {[
                      { value: "shared" as const, label: "属主隔离密钥" },
                      { value: "byo" as const, label: "运行时自带密钥" },
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
                          "h-8 rounded-md px-3 text-sm font-medium text-muted-foreground transition-colors",
                          credentialMode === option.value
                            ? "bg-background text-foreground shadow-sm"
                            : "hover:text-foreground",
                        )}
                      >
                        {option.label}
                      </button>
                    ))}
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <Button
                    type="button"
                    variant="outline"
                    onClick={discover}
                    disabled={loading || !providerId || !endpoint.trim() || !apiKey.trim()}
                  >
                    {loading ? "连接中..." : "连接并发现"}
                  </Button>
                  {externalAgents.length > 0 && (
                    <span className="text-xs text-muted-foreground">
                      已发现 {externalAgents.length} 个智能体
                    </span>
                  )}
                </div>
              </>
            ) : mode === "bundle" ? (
              <div className="grid gap-2">
                <Label htmlFor="agent-bundle-zip">智能体包（.zip）</Label>
                <label className="flex cursor-pointer items-center justify-center gap-2 rounded-lg border border-dashed border-border bg-muted/20 px-3 py-8 text-sm text-muted-foreground hover:bg-muted/40">
                  <FileUp className="size-4" />
                  <span>{bundle ? bundle.filename : "选择 .zip 智能体包"}</span>
                  <input
                    id="agent-bundle-zip"
                    type="file"
                    accept=".zip,application/zip"
                    className="sr-only"
                    onChange={(event) => loadBundle(event.target.files)}
                  />
                </label>
                <p className="text-xs text-muted-foreground">
                  zip 内的 agent .md（frontmatter + prompt）导入为智能体；其余文件作为知识文件进入智能体工作区，自动种子到每个新会话。
                </p>
              </div>
            ) : (
              <div className="grid gap-2">
                <Label htmlFor="opencode-agent-files">OpenCode 智能体 Markdown</Label>
                <label className="flex cursor-pointer items-center justify-center gap-2 rounded-lg border border-dashed border-border bg-muted/20 px-3 py-8 text-sm text-muted-foreground hover:bg-muted/40">
                  <FileUp className="size-4" />
                  <span>{agentFiles.length ? `已选择 ${agentFiles.length} 个文件` : "选择 .md 文件"}</span>
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
                  <div className="max-h-44 divide-y divide-border overflow-y-auto rounded-md border border-border">
                    {agentFiles.map((file) => (
                      <div key={file.filename} className="px-3 py-2">
                        <div className="truncate text-sm font-medium">{file.filename}</div>
                        <div className="text-xs text-muted-foreground">{file.content.length} 个字符</div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}
            {mode === "remote" && externalAgents.length > 0 && (
              <div className="grid gap-2">
                <div className="flex items-center gap-2">
                  <div className="relative flex-1">
                    <Search className="absolute left-2 top-2 size-4 text-muted-foreground" />
                    <Input
                      value={query}
                      onChange={(e) => setQuery(e.target.value)}
                      placeholder="搜索智能体"
                      aria-label="搜索已发现的智能体"
                      className="pl-8"
                    />
                  </div>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => setSelectedIds(filteredAgents.map((agent) => agent.id))}
                  >
                    全选
                  </Button>
                </div>
                <div className="max-h-72 divide-y divide-border overflow-y-auto rounded-md border border-border">
                  {filteredAgents.map((agent) => (
                    <label
                      key={agent.id}
                      className="flex cursor-pointer items-start gap-2 px-3 py-2 hover:bg-muted/50"
                    >
                      <input
                        type="checkbox"
                        className="mt-1"
                        checked={selectedIds.includes(agent.id)}
                        onChange={() => toggleAgent(agent.id)}
                      />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-sm font-medium">{agent.name}</span>
                        <span className="block truncate font-mono text-[11px] text-muted-foreground">
                          {agent.id}
                        </span>
                        {agent.description && (
                          <span className="mt-0.5 block line-clamp-2 text-xs text-muted-foreground">
                            {agent.description}
                          </span>
                        )}
                      </span>
                    </label>
                  ))}
                  {filteredAgents.length === 0 && (
                    <p className="px-3 py-8 text-center text-sm text-muted-foreground">
                      没有匹配的智能体。
                    </p>
                  )}
                </div>
              </div>
            )}
            {mode === "remote" && preview.length > 0 && (
              <div className="rounded-lg border border-border bg-muted/20 p-3">
                <div className="flex items-center justify-between gap-3">
                  <p className="text-sm font-medium">导入预检</p>
                  <span className="text-xs text-muted-foreground">
                    {preview.length} 个规范化结果
                  </span>
                </div>
                <div className="mt-2 max-h-40 space-y-2 overflow-y-auto">
                  {preview.map((item) => (
                    <div key={item.external_id} className="rounded-md border border-border bg-background px-3 py-2">
                      <div className="flex items-center justify-between gap-2 text-xs">
                        <span className="truncate font-mono">{item.external_id}</span>
                        <span className={item.can_import ? "text-emerald-600" : "text-destructive"}>
                          {item.can_import ? "可导入" : "已阻断"}
                        </span>
                      </div>
                      {item.issues.map((issue) => (
                        <p key={`${issue.code}-${issue.field}`} className="mt-1 text-xs text-muted-foreground">
                          [{issue.severity}] {issue.field}：{issue.message}
                        </p>
                      ))}
                      {item.issues.length === 0 && (
                        <p className="mt-1 text-xs text-muted-foreground">未发现兼容性问题。</p>
                      )}
                    </div>
                  ))}
                </div>
              </div>
            )}
          </>
          )}
          {importResults && (
            <div className="rounded-lg border border-border bg-muted/20 p-3">
              <p className="text-sm font-medium">导入结果</p>
              <div className="mt-2 max-h-56 space-y-2 overflow-y-auto">
                {importResults.map((result) => {
                  const meta = IMPORT_RESULT_META[result.status] ?? {
                    label: result.status,
                    className: "text-muted-foreground",
                  };
                  return (
                    <div
                      key={result.external_id}
                      className="rounded-md border border-border bg-background px-3 py-2"
                    >
                      <div className="flex items-center justify-between gap-2 text-xs">
                        <span className="truncate font-mono">{result.external_id}</span>
                        <span className={`shrink-0 font-medium ${meta.className}`}>{meta.label}</span>
                      </div>
                      {result.status === "drift_pending" && (
                        <p className="mt-1 text-xs text-muted-foreground">
                          智能体保持已审批的配置不变；远端变更已生成候选快照，
                          {result.agent_id ? (
                            <a
                              href={`/agents/detail/?id=${encodeURIComponent(result.agent_id)}`}
                              className="underline underline-offset-2 hover:text-foreground"
                            >
                              前往治理面板评审
                            </a>
                          ) : (
                            "请在治理面板评审"
                          )}
                          。
                        </p>
                      )}
                      {result.status === "blocked" &&
                        result.issues.map((issue, index) => (
                          <p key={`${issue.code ?? index}`} className="mt-1 text-xs text-destructive">
                            {issue.message ?? issue.code}
                          </p>
                        ))}
                    </div>
                  );
                })}
              </div>
            </div>
          )}
          {error && <p className="text-sm text-destructive">{error}</p>}
        </div>
        <DialogFooter className="m-0 rounded-b-xl px-6 py-4">
          {importResults ? (
            <Button onClick={() => close(false)}>完成</Button>
          ) : (
            <>
              <Button variant="outline" onClick={() => close(false)} disabled={saving}>
                取消
              </Button>
              <Button
                onClick={importSelected}
                disabled={saving || (mode === "remote" ? selectedIds.length === 0 : mode === "bundle" ? !bundle : agentFiles.length === 0)}
              >
                {saving
                  ? "导入中…"
                  : `${mode === "remote" && previewConfirmed ? "确认" : ""}导入${mode === "remote" ? ` ${selectedIds.length}` : mode === "bundle" ? "" : ` ${agentFiles.length}`}`}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
