"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertCircle,
  Check,
  CheckCircle2,
  ChevronRight,
  FileText,
  KeyRound,
  Plus,
  ServerCog,
  Trash2,
  Unplug,
  Cpu,
  Zap,
} from "lucide-react";

import { BrandIcon } from "@/components/brand-icons";
import { RuntimeProviderLogo } from "@/components/runtime-provider-logo";
import { RuntimeTemplateCard } from "@/components/runtime-template-card";
import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  apiErrorMessage,
  createRuntimeHarness,
  deleteAgentRuntimeCredential,
  deleteRuntimeHarness,
  listRuntimeHarnesses,
  saveAgentRuntimeCredential,
  updateRuntimeHarness,
} from "@/lib/api";
import {
  RUNTIME_TEMPLATES,
  runtimeTemplateById,
  runtimeTemplateIconId,
  type RuntimeTemplate,
} from "@/lib/runtime-templates";
import type { RuntimeHarness } from "@/lib/types";
import { cn } from "@/lib/utils";

const SPEC_DEFAULTS: Record<string, string> = {
  claude_managed_agents: "http://localhost:8080",
};

const SPEC_LABELS: Record<string, string> = {
  claude_managed_agents: "自托管开放 Harness（Anthropic 兼容协议）",
};

const RUNTIME_OPTIONS = [
  {
    value: "claude_managed_agents",
    label: "自托管开放 Harness（Anthropic 兼容协议）",
    apiSpec: "claude_managed_agents",
    defaultApiBase: SPEC_DEFAULTS.claude_managed_agents,
  },
];

const FALLBACK_DEFAULT_RUNTIMES: RuntimeHarness[] = [
  {
    alias: "claude_managed_agents",
    api_spec: "claude_managed_agents",
    display_name: "自托管开放 Harness",
    api_base: SPEC_DEFAULTS.claude_managed_agents,
    is_default: true,
    connected: false,
    tools: [],
  },
];

const RESERVED_ALIASES = new Set([
  "claude_managed_agents",
  "cursor",
  "gemini_antigravity",
]);

function preferredAlias(harnesses: RuntimeHarness[]): string | null {
  return harnesses.find((harness) => !harness.connected)?.alias ?? harnesses[0]?.alias ?? null;
}

function runtimeLoadError(error: unknown): string {
  const message = apiErrorMessage(error, "加载运行时失败。");
  if (message.length <= 240) return message;
  return "加载运行时失败。请检查网关 API 连接并刷新。";
}

function RuntimeLogo({ harness }: { harness: RuntimeHarness }) {
  return <RuntimeProviderLogo alias={harness.alias} apiSpec={harness.api_spec} />;
}

function StatusBadge({ connected }: { connected: boolean }) {
  if (connected) {
    return (
      <Badge className="border border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-300">
        <CheckCircle2 className="size-3" />
        已连接
      </Badge>
    );
  }

  return (
    <Badge variant="secondary" className="text-muted-foreground border border-border/40">
      <AlertCircle className="size-3" />
      需要密钥
    </Badge>
  );
}

function SummaryTile({
  label,
  value,
  tone,
}: {
  label: string;
  value: number;
  tone: "good" | "warn" | "neutral";
}) {
  return (
    <div className="rounded-xl border border-border/70 bg-card p-4 shadow-2xs">
      <div className="text-[11px] font-medium text-muted-foreground">
        {label}
      </div>
      <div
        className={cn(
          "mt-1.5 text-2xl font-bold font-mono tracking-tight",
          tone === "good" && "text-emerald-600 dark:text-emerald-400",
          tone === "warn" && "text-amber-600 dark:text-amber-400",
        )}
      >
        {value}
      </div>
    </div>
  );
}

function AddHarnessModal({
  open,
  template,
  onClose,
  onCreated,
}: {
  open: boolean;
  template: RuntimeTemplate | null;
  onClose: () => void;
  onCreated: (harnesses: RuntimeHarness[]) => void;
}) {
  const [alias, setAlias] = useState("");
  const [runtimeOption, setRuntimeOption] = useState("claude_managed_agents");
  const [apiSpec, setApiSpec] = useState("claude_managed_agents");
  const [apiBase, setApiBase] = useState(SPEC_DEFAULTS.claude_managed_agents);
  const [apiKey, setApiKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleRuntimeOptionChange = (value: string | null) => {
    const option = RUNTIME_OPTIONS.find((candidate) => candidate.value === value);
    if (!option) return;
    setRuntimeOption(option.value);
    setApiSpec(option.apiSpec);
    setApiBase(option.defaultApiBase);
  };

  const reset = useCallback(() => {
    setAlias("");
    setApiKey("");
    setRuntimeOption("claude_managed_agents");
    setApiSpec("claude_managed_agents");
    setApiBase(SPEC_DEFAULTS.claude_managed_agents);
    setError(null);
  }, []);

  useEffect(() => {
    if (!open) return;
    if (!template) {
      reset();
      return;
    }
    const matchingOption =
      RUNTIME_OPTIONS.find((option) => option.apiSpec === template.apiSpec)?.value ??
      "claude_managed_agents";
    setAlias(template.runtimeAlias);
    setApiKey("");
    setRuntimeOption(matchingOption);
    setApiSpec(template.apiSpec);
    setApiBase("");
    setError(null);
  }, [open, reset, template]);

  const handleCreate = async () => {
    const trimmedAlias = alias.trim();
    const trimmedKey = apiKey.trim();
    const trimmedBase = apiBase.trim();
    if (!trimmedAlias) {
      setError("请输入运行时别名。");
      return;
    }
    if (!/^[a-zA-Z0-9_-]+$/.test(trimmedAlias)) {
      setError("别名只能包含字母、数字、连字符和下划线。");
      return;
    }
    if (RESERVED_ALIASES.has(trimmedAlias)) {
      setError(`"${trimmedAlias}" 为保留别名。`);
      return;
    }
    if (!trimmedKey) {
      setError("请输入 API 密钥。");
      return;
    }
    if (!trimmedBase) {
      setError("请输入 API 基础地址。");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const next = await createRuntimeHarness({
        alias: trimmedAlias,
        api_spec: apiSpec,
        api_base: trimmedBase,
        api_key: trimmedKey,
      });
      onCreated(next ?? []);
      reset();
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : "创建运行时失败。");
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(isOpen) => {
        if (!isOpen) onClose();
      }}
    >
      <DialogContent className="max-w-md rounded-2xl">
        <DialogHeader>
          <DialogTitle className="text-base font-semibold">
            {template ? `接入 ${template.name} 运行时` : "新建 Agent 运行时"}
          </DialogTitle>
        </DialogHeader>
        <div className="grid gap-4 pt-2">
          {template && (
            <div className="flex items-start gap-3 rounded-xl border border-border/80 bg-muted/30 p-3">
              <span className="flex size-9 shrink-0 items-center justify-center rounded-lg border border-border bg-background text-foreground shadow-2xs">
                <BrandIcon id={runtimeTemplateIconId(template)} className="size-5" />
              </span>
              <div className="min-w-0">
                <p className="text-xs font-semibold leading-tight">{template.name}</p>
                <p className="mt-1 truncate font-mono text-[11px] text-muted-foreground">
                  {template.repoPath}
                </p>
              </div>
            </div>
          )}
          <div className="grid gap-1.5">
            <Label htmlFor="runtime-alias" className="text-xs font-medium text-muted-foreground uppercase">别名 (Alias)</Label>
            <Input
              id="runtime-alias"
              placeholder="anthropic-dev"
              value={alias}
              onChange={(event) => setAlias(event.target.value)}
              className="text-xs font-mono"
            />
          </div>
          <div className="grid gap-1.5">
            <Label className="text-xs font-medium text-muted-foreground uppercase">运行时协议</Label>
            <Select value={runtimeOption} onValueChange={handleRuntimeOptionChange}>
              <SelectTrigger className="text-xs h-9">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {RUNTIME_OPTIONS.map((option) => (
                  <SelectItem key={option.value} value={option.value} className="text-xs">
                    {option.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="grid gap-1.5">
            <Label htmlFor="runtime-api-base" className="text-xs font-medium text-muted-foreground uppercase">API 基础地址</Label>
            <Input
              id="runtime-api-base"
              value={apiBase}
              onChange={(event) => setApiBase(event.target.value)}
              className="font-mono text-xs"
            />
          </div>
          <div className="grid gap-1.5">
            <Label htmlFor="runtime-api-key" className="text-xs font-medium text-muted-foreground uppercase">API 密钥</Label>
            <div className="relative">
              <KeyRound className="absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
              <Input
                id="runtime-api-key"
                type="password"
                placeholder="输入运行时 API 密钥"
                value={apiKey}
                onChange={(event) => setApiKey(event.target.value)}
                className="pl-8 font-mono text-xs"
              />
            </div>
          </div>
          {error && <div className="text-xs text-destructive rounded-md bg-destructive/10 p-2.5 font-mono">{error}</div>}
          <div className="flex justify-end gap-2 pt-1">
            <Button variant="outline" size="sm" onClick={onClose} disabled={saving} className="text-xs">
              取消
            </Button>
            <Button size="sm" onClick={handleCreate} disabled={saving} className="text-xs bg-blue-600 hover:bg-blue-700 text-white font-medium">
              <Plus className="size-3.5" />
              {saving ? "创建中..." : "确认创建"}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function RuntimeRow({
  harness,
  selected,
  onSelect,
}: {
  harness: RuntimeHarness;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      aria-current={selected ? "true" : undefined}
      className={cn(
        "relative flex w-full min-w-0 items-start gap-3 px-4 py-3.5 pr-10 text-left transition-colors hover:bg-muted/40 sm:grid sm:grid-cols-[auto_minmax(0,1fr)_auto] sm:items-center sm:pr-4 border-b border-border/50",
        selected && "bg-muted/60 border-l-2 border-l-blue-500",
      )}
      onClick={onSelect}
    >
      <RuntimeLogo harness={harness} />
      <div className="min-w-0">
        <div className="flex min-w-0 flex-col items-start gap-1 sm:flex-row sm:flex-wrap sm:items-center sm:gap-2">
          <span className="min-w-0 font-semibold text-sm leading-tight text-foreground">{harness.display_name}</span>
          <div className="flex max-w-full flex-wrap gap-1.5">
            <Badge variant={harness.is_default ? "secondary" : "outline"} className="text-[10px]">
              {harness.is_default ? "系统默认" : "自定义"}
            </Badge>
            <Badge variant="outline" className="max-w-full text-[10px] font-mono">
              {SPEC_LABELS[harness.api_spec] ?? harness.api_spec}
            </Badge>
          </div>
        </div>
        <div className="mt-1.5 flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground font-mono">
          <span>{harness.alias}</span>
          <span className="max-w-full truncate">{harness.api_base}</span>
          {harness.masked_api_key && (
            <span>{harness.masked_api_key}</span>
          )}
        </div>
        <div className="mt-2 sm:hidden">
          <StatusBadge connected={harness.connected} />
        </div>
      </div>
      <div className="hidden items-center gap-2 sm:flex">
        <StatusBadge connected={harness.connected} />
        <ChevronRight
          className={cn(
            "size-4 text-muted-foreground transition-transform",
            selected && "rotate-90 text-foreground",
          )}
        />
      </div>
      <ChevronRight
        className={cn(
          "absolute right-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground transition-transform sm:hidden",
          selected && "rotate-90 text-foreground",
        )}
      />
    </button>
  );
}

function RuntimeSection({
  title,
  empty,
  harnesses,
  selectedAlias,
  onSelect,
  onUpdated,
}: {
  title: string;
  empty: string;
  harnesses: RuntimeHarness[];
  selectedAlias: string | null;
  onSelect: (alias: string) => void;
  onUpdated: (harnesses: RuntimeHarness[]) => void;
}) {
  return (
    <section className="space-y-2.5">
      <h2 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">{title}</h2>
      <div className="min-w-0 overflow-hidden rounded-2xl border border-border/80 bg-card shadow-2xs">
        {harnesses.length === 0 ? (
          <div className="px-5 py-6 text-xs text-muted-foreground font-mono text-center">{empty}</div>
        ) : (
          harnesses.map((harness) => {
            const selected = selectedAlias === harness.alias;
            return (
              <div key={harness.alias}>
                <RuntimeRow
                  harness={harness}
                  selected={selected}
                  onSelect={() => onSelect(harness.alias)}
                />
                {selected && <RuntimeDetails harness={harness} onUpdated={onUpdated} />}
              </div>
            );
          })
        )}
      </div>
    </section>
  );
}

function RuntimeTemplatesSection({
  templates,
  loading,
  error,
  onUse,
}: {
  templates: RuntimeTemplate[];
  loading: boolean;
  error: string | null;
  onUse: (template: RuntimeTemplate) => void;
}) {
  return (
    <section className="space-y-2.5">
      <div className="flex min-w-0 flex-wrap items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <FileText className="size-4 shrink-0 text-muted-foreground" />
          <h2 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">运行时预设模板</h2>
        </div>
        {loading && (
          <span className="text-xs text-muted-foreground font-mono" aria-live="polite">
            同步清单中...
          </span>
        )}
      </div>
      {error && (
        <div className="rounded-xl border border-amber-500/40 bg-amber-500/10 px-4 py-3 text-xs text-amber-600 dark:text-amber-400 font-mono">
          {error}
        </div>
      )}
      {templates.length === 0 ? (
        <Card className="rounded-2xl px-4 py-5 text-xs text-muted-foreground">
          暂无可用的预设模板。
        </Card>
      ) : (
        <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
          {templates.map((template) => (
            <RuntimeTemplateCard key={template.id} template={template} onUse={onUse} />
          ))}
        </div>
      )}
    </section>
  );
}

function RuntimeDetails({
  harness,
  onUpdated,
}: {
  harness: RuntimeHarness;
  onUpdated: (harnesses: RuntimeHarness[]) => void;
}) {
  const [key, setKey] = useState("");
  const [base, setBase] = useState("");
  const [saving, setSaving] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setKey("");
    setBase(harness.api_base);
    setError(null);
  }, [harness.alias, harness.api_base]);

  const trimmedKey = key.trim();
  const trimmedBase = base.trim();
  const baseChanged = trimmedBase !== harness.api_base;
  const canSave = Boolean(trimmedBase && (trimmedKey || (!harness.is_default && baseChanged)));

  const handleSave = async () => {
    if (!trimmedBase) {
      setError("API 基础地址不能为空。");
      return;
    }
    if (harness.is_default && !trimmedKey) {
      setError("请输入 API 密钥以更新该运行时。");
      return;
    }
    if (!trimmedKey && !baseChanged) return;
    setSaving(true);
    setError(null);
    try {
      let next: RuntimeHarness[];
      if (harness.is_default) {
        await saveAgentRuntimeCredential({
          runtime: harness.alias,
          apiKey: trimmedKey,
          apiBase: trimmedBase,
        });
        next = await listRuntimeHarnesses();
      } else {
        next = await updateRuntimeHarness(harness.alias, {
          ...(trimmedKey ? { api_key: trimmedKey } : {}),
          ...(baseChanged ? { api_base: trimmedBase } : {}),
        });
      }
      setKey("");
      onUpdated(next ?? []);
    } catch (err) {
      setError(err instanceof Error ? err.message : "保存运行时失败。");
    } finally {
      setSaving(false);
    }
  };

  const handleRemove = async () => {
    const message = harness.is_default
      ? `确定要移除 "${harness.display_name}" 的凭证凭据吗？`
      : `确定要删除运行时 "${harness.alias}" 吗？此操作无法撤销。`;
    if (!confirm(message)) return;
    setRemoving(true);
    setError(null);
    try {
      if (harness.is_default) {
        await deleteAgentRuntimeCredential(harness.alias);
      } else {
        await deleteRuntimeHarness(harness.alias);
      }
      const next = await listRuntimeHarnesses();
      onUpdated(next ?? []);
    } catch (err) {
      setError(err instanceof Error ? err.message : "移除运行时失败。");
    } finally {
      setRemoving(false);
    }
  };

  return (
    <div className="border-t border-border/70 bg-muted/20 px-5 py-4 sm:pl-[4.75rem]">
      <div className="grid gap-4">
        <div className="grid min-w-0 gap-3 md:grid-cols-2 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] lg:items-end">
          <div className="grid gap-1.5">
            <Label htmlFor={`runtime-key-${harness.alias}`} className="text-xs font-medium text-muted-foreground uppercase">API 密钥</Label>
            <div className="relative">
              <KeyRound className="absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
              <Input
                id={`runtime-key-${harness.alias}`}
                type="password"
                placeholder={harness.connected ? "输入新的 API 密钥..." : "输入 API 密钥"}
                value={key}
                onChange={(event) => setKey(event.target.value)}
                className="pl-8 font-mono text-xs"
              />
            </div>
          </div>

          <div className="grid gap-1.5">
            <Label htmlFor={`runtime-base-${harness.alias}`} className="text-xs font-medium text-muted-foreground uppercase">API 基础地址</Label>
            <Input
              id={`runtime-base-${harness.alias}`}
              value={base}
              onChange={(event) => setBase(event.target.value)}
              className="font-mono text-xs"
            />
          </div>

          <div className="flex flex-wrap justify-end gap-2 md:col-span-2 lg:col-span-1">
            {(!harness.is_default || harness.connected) && (
              <Button
                variant={harness.is_default ? "outline" : "destructive"}
                size="sm"
                className="h-8 text-xs gap-1"
                onClick={handleRemove}
                disabled={saving || removing}
              >
                {harness.is_default ? (
                  <Unplug className="size-3.5" />
                ) : (
                  <Trash2 className="size-3.5" />
                )}
                {removing ? "删除中..." : harness.is_default ? "清空密钥" : "删除"}
              </Button>
            )}
            <Button size="sm" className="h-8 text-xs gap-1 bg-blue-600 hover:bg-blue-700 text-white font-medium" onClick={handleSave} disabled={saving || !canSave}>
              <Check className="size-3.5" />
              {saving ? "保存中..." : harness.connected ? "更新配置" : "连接"}
            </Button>
          </div>
        </div>

        <div className="grid gap-2 rounded-xl border border-border/70 bg-card p-3 text-xs sm:grid-cols-3">
          <div className="flex items-center justify-between gap-3 sm:block">
            <span className="text-muted-foreground">类型</span>
            <div className="mt-0 font-medium sm:mt-1">
              {harness.is_default ? "系统预设" : "自定义"}
            </div>
          </div>
          <div className="flex items-center justify-between gap-3 sm:block">
            <span className="text-muted-foreground">密钥遮蔽</span>
            <div className="mt-0 font-mono text-foreground sm:mt-1">
              {harness.masked_api_key ?? "尚未配置"}
            </div>
          </div>
          <div className="flex items-center justify-between gap-3 sm:block">
            <span className="text-muted-foreground">会话能力</span>
            <div className="mt-0 font-medium sm:mt-1">
              {harness.connected ? "就绪" : "已锁定"}
            </div>
          </div>
        </div>
      </div>

      {error && <p className="mt-3 text-xs text-destructive font-mono">{error}</p>}
    </div>
  );
}

export default function RuntimesPage() {
  const [harnesses, setHarnesses] = useState<RuntimeHarness[]>([]);
  const [selectedAlias, setSelectedAlias] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showAdd, setShowAdd] = useState(false);
  const [selectedTemplate, setSelectedTemplate] = useState<RuntimeTemplate | null>(null);
  const [runtimeTemplates, setRuntimeTemplates] = useState<RuntimeTemplate[]>(RUNTIME_TEMPLATES);
  const [templatesLoading, setTemplatesLoading] = useState(true);
  const [templatesError, setTemplatesError] = useState<string | null>(null);
  const [pendingTemplateId, setPendingTemplateId] = useState<string | null>(null);
  const hasLoadedHarnessesRef = useRef(false);

  const applyHarnesses = useCallback((next: RuntimeHarness[]) => {
    const resolved = next ?? [];
    setHarnesses(resolved);
    setSelectedAlias((current) =>
      current && resolved.some((harness) => harness.alias === current)
        ? current
        : preferredAlias(resolved),
    );
  }, []);

  useEffect(() => {
    let cancelled = false;
    listRuntimeHarnesses()
      .then((next) => {
        if (cancelled) return;
        hasLoadedHarnessesRef.current = true;
        setError(null);
        applyHarnesses(next ?? []);
      })
      .catch((err) => {
        if (cancelled) return;
        setError(runtimeLoadError(err));
        if (!hasLoadedHarnessesRef.current) {
          applyHarnesses(FALLBACK_DEFAULT_RUNTIMES);
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [applyHarnesses]);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const templateId = params.get("template");
    if (!templateId) return;
    setPendingTemplateId(templateId);
    params.delete("template");
    const query = params.toString();
    window.history.replaceState(
      null,
      "",
      `${window.location.pathname}${query ? `?${query}` : ""}`,
    );
  }, []);

  useEffect(() => {
    setRuntimeTemplates(RUNTIME_TEMPLATES);
    setTemplatesError(null);
    setTemplatesLoading(false);
  }, []);

  useEffect(() => {
    if (!pendingTemplateId || templatesLoading) return;
    const template = runtimeTemplateById(pendingTemplateId, runtimeTemplates);
    if (!template) {
      const message = `未找到预设模板 "${pendingTemplateId}"。显示可用的模板清单。`;
      setTemplatesError((current) => (current ? `${current} ${message}` : message));
      setPendingTemplateId(null);
      return;
    }
    setSelectedTemplate(template);
    setShowAdd(true);
    setPendingTemplateId(null);
  }, [pendingTemplateId, runtimeTemplates, templatesLoading]);

  const defaults = useMemo(() => harnesses.filter((harness) => harness.is_default), [harnesses]);
  const custom = useMemo(() => harnesses.filter((harness) => !harness.is_default), [harnesses]);
  const connectedCount = useMemo(
    () => harnesses.filter((harness) => harness.connected).length,
    [harnesses],
  );
  const missingCount = Math.max(harnesses.length - connectedCount, 0);
  const openAddRuntime = (template: RuntimeTemplate | null = null) => {
    setSelectedTemplate(template);
    setShowAdd(true);
  };
  const closeAddRuntime = () => {
    setShowAdd(false);
    setSelectedTemplate(null);
  };

  return (
    <div className="flex h-screen bg-background text-foreground overflow-hidden selection:bg-blue-500/20">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        {/* Pure Chinese Header */}
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border/80 px-4 bg-background/80 backdrop-blur">
          <div className="flex items-center gap-2.5">
            <div className="flex size-7 items-center justify-center rounded-md bg-blue-500/10 text-blue-600 dark:text-blue-400 ring-1 ring-blue-500/20">
              <ServerCog className="size-4" />
            </div>
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold tracking-tight">Agent 运行时管理</span>
              <span className="text-xs text-muted-foreground font-medium">/ 运行时</span>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button size="sm" className="gap-1.5 bg-blue-600 text-white hover:bg-blue-700 font-medium text-xs shadow-xs" onClick={() => openAddRuntime()}>
              <Plus className="size-3.5" />
              新建运行时
            </Button>
            <ThemeToggle />
          </div>
        </header>

        <main id="main-content" className="flex-1 overflow-y-auto p-6 space-y-6">
          <div className="mx-auto flex w-full max-w-6xl flex-col gap-6">
            {/* Command Banner - Pure Chinese */}
            <div className="relative overflow-hidden rounded-2xl border border-border/70 bg-card p-6 shadow-xs">
              <div className="absolute right-0 top-0 -mr-16 -mt-16 size-64 rounded-full bg-blue-500/5 blur-3xl pointer-events-none" />
              <div className="relative z-10 flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
                <div className="space-y-1.5 max-w-xl">
                  <div className="flex items-center gap-2">
                    <span className="inline-flex items-center gap-1 rounded-md bg-blue-500/10 px-2 py-0.5 text-xs font-medium text-blue-600 dark:text-blue-400 border border-blue-500/20">
                      <Cpu className="size-3" /> 运行时环境
                    </span>
                    <span className="text-xs text-muted-foreground font-medium">智能体沙箱托管中心</span>
                  </div>
                  <h1 className="text-xl font-bold tracking-tight text-foreground">
                    智能体运行时凭证与开放 Harness
                  </h1>
                  <p className="text-xs text-muted-foreground leading-relaxed">
                    配置自托管开放 Harness、Anthropic 兼容协议端点与运行时 API 密钥。为智能体提供隔离的指令执行环境。
                  </p>
                </div>
              </div>
            </div>

            {loading && <p className="text-xs text-muted-foreground font-mono">正在加载运行时列表...</p>}
            {error && <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-4 text-xs text-destructive font-mono">{error}</div>}

            {!loading && (
              <>
                <div className="grid gap-3 sm:grid-cols-3">
                  <SummaryTile label="已受信任连接" value={connectedCount} tone="good" />
                  <SummaryTile label="需配置密钥" value={missingCount} tone="warn" />
                  <SummaryTile label="自定义运行时" value={custom.length} tone="neutral" />
                </div>

                <div className="grid min-w-0 content-start gap-6">
                  <RuntimeSection
                    title="默认预设运行时"
                    empty="暂无默认预设运行时。"
                    harnesses={defaults}
                    selectedAlias={selectedAlias}
                    onSelect={setSelectedAlias}
                    onUpdated={applyHarnesses}
                  />
                  <RuntimeTemplatesSection
                    templates={runtimeTemplates}
                    loading={templatesLoading}
                    error={templatesError}
                    onUse={openAddRuntime}
                  />
                  <RuntimeSection
                    title="自定义扩展运行时"
                    empty="尚未配置自定义运行时。"
                    harnesses={custom}
                    selectedAlias={selectedAlias}
                    onSelect={setSelectedAlias}
                    onUpdated={applyHarnesses}
                  />
                </div>
              </>
            )}
          </div>
        </main>
      </div>

      <AddHarnessModal
        open={showAdd}
        template={selectedTemplate}
        onClose={closeAddRuntime}
        onCreated={applyHarnesses}
      />
    </div>
  );
}
