"use client";

import { useEffect, useMemo, useState } from "react";
import {
  KeyRound,
  Trash2,
  Pencil,
  Plus,
  Loader2,
  Eye,
  EyeOff,
  Globe,
  User,
  ShieldCheck,
  Search,
  Copy,
  Check,
  Terminal,
  ChevronDown,
  ChevronRight,
  Info,
  Lock,
  Sparkles,
} from "lucide-react";
import { toast } from "sonner";
import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { VaultBrandIcon } from "@/components/brand-kit-icons";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import {
  listVaultKeys,
  saveIntegrationKey,
  deleteIntegrationKey,
} from "@/lib/api";
import type { VaultKeyEntry } from "@/lib/types";

function timeAgo(ts?: number): string {
  if (!ts) return "";
  const secs = Math.max(0, Math.floor((Date.now() - ts) / 1000));
  if (secs < 10) return "刚刚";
  if (secs < 60) return `${secs} 秒前`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins} 分钟前`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs} 小时前`;
  return `${Math.floor(hrs / 24)} 天前`;
}

type EditorState =
  | { mode: "add"; defaultScope: "personal" | "global" }
  | { mode: "edit"; entry: VaultKeyEntry };

export default function VaultPage() {
  const [keys, setKeys] = useState<VaultKeyEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editor, setEditor] = useState<EditorState | null>(null);
  const [query, setQuery] = useState("");
  const [scopeTab, setScopeTab] = useState<"all" | "personal" | "global" | "env">("all");

  const refresh = async () => {
    try {
      setKeys(await listVaultKeys());
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  useEffect(() => {
    refresh();
  }, []);

  const onDelete = async (entry: VaultKeyEntry) => {
    setKeys((prev) => prev?.filter((k) => k.key !== entry.key || k.scope !== entry.scope) ?? null);
    await deleteIntegrationKey(entry.key, entry.scope);
    toast.success(`凭证 ${entry.key} 已物理抹除`);
    await refresh();
  };

  const globalKeys = useMemo(() => keys?.filter((k) => k.scope === "global" && k.source !== "env") ?? [], [keys]);
  const personalKeys = useMemo(() => keys?.filter((k) => k.scope === "personal" && k.source !== "env") ?? [], [keys]);
  const envKeys = useMemo(() => keys?.filter((k) => k.source === "env") ?? [], [keys]);

  const filteredKeys = useMemo(() => {
    if (!keys) return [];
    let list = keys;
    if (scopeTab === "personal") list = personalKeys;
    else if (scopeTab === "global") list = globalKeys;
    else if (scopeTab === "env") list = envKeys;

    const q = query.trim().toLowerCase();
    if (!q) return list;
    return list.filter((k) => k.key.toLowerCase().includes(q));
  }, [keys, scopeTab, query, personalKeys, globalKeys, envKeys]);

  const empty = keys !== null && keys.length === 0;

  return (
    <div className="flex h-screen overflow-hidden bg-background text-foreground selection:bg-emerald-500/20">
      <Sidebar />
      <div className="flex flex-1 flex-col overflow-hidden min-w-0">
        {/* Header - Pure Chinese */}
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border/80 px-4 bg-background/80 backdrop-blur">
          <div className="flex items-center gap-2.5">
            <div className="flex size-7 items-center justify-center rounded-md bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 ring-1 ring-emerald-500/20">
              <VaultBrandIcon size={16} />
            </div>
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold tracking-tight">凭证保险库</span>
              <span className="text-xs text-muted-foreground font-mono">/ 保险库</span>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <ThemeToggle />
          </div>
        </header>

        <main className="flex-1 overflow-y-auto p-6 space-y-6">
          <div className="mx-auto max-w-5xl space-y-6">
            {/* Banner - Pure Chinese */}
            <div className="relative overflow-hidden rounded-2xl border border-border/70 bg-card p-6 shadow-xs">
              <div className="absolute right-0 top-0 -mr-16 -mt-16 size-64 rounded-full bg-emerald-500/5 blur-3xl pointer-events-none" />
              <div className="relative z-10 flex flex-col gap-5 md:flex-row md:items-center md:justify-between">
                <div className="space-y-1.5 max-w-xl">
                  <div className="flex items-center gap-2">
                    <span className="inline-flex items-center gap-1 rounded-md bg-emerald-500/10 px-2 py-0.5 text-xs font-medium text-emerald-600 dark:text-emerald-400 border border-emerald-500/20">
                      <Lock className="size-3" /> 高级加密算法
                    </span>
                    <span className="text-xs text-muted-foreground font-medium">动态凭证隔离网络</span>
                  </div>
                  <h1 className="text-xl font-bold tracking-tight text-foreground">
                    机密凭据与接口密钥中心
                  </h1>
                  <p className="text-xs text-muted-foreground leading-relaxed">
                    在安全沙箱环境中为扩展工具与智能体任务注入机密参数。凭证明文仅存于内存脱敏区，物理隔离且安全无虞。
                  </p>
                </div>

                <div className="flex flex-wrap items-center gap-2 shrink-0">
                  <Button
                    size="sm"
                    className="gap-1.5 bg-emerald-600 text-white hover:bg-emerald-700 dark:bg-emerald-500 dark:hover:bg-emerald-600 font-medium shadow-xs"
                    onClick={() => setEditor({ mode: "add", defaultScope: "personal" })}
                  >
                    <Plus className="size-4" />
                    新建个人凭证
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    className="gap-1.5 border-border hover:bg-muted"
                    onClick={() => setEditor({ mode: "add", defaultScope: "global" })}
                  >
                    <Globe className="size-3.5 text-blue-500" />
                    新建全域凭证
                  </Button>
                </div>
              </div>
            </div>

            {/* Metrics - Pure Chinese */}
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
              <div className="rounded-xl border border-border/70 bg-card p-4">
                <div className="flex items-center justify-between text-xs text-muted-foreground">
                  <span className="font-medium">总凭证数</span>
                  <VaultBrandIcon size={14} className="text-emerald-500" />
                </div>
                <div className="mt-2 flex items-baseline justify-between">
                  <span className="text-2xl font-bold font-mono tracking-tight">{keys?.length ?? 0}</span>
                  <span className="text-[11px] text-muted-foreground">配置条目</span>
                </div>
              </div>

              <div className="rounded-xl border border-border/70 bg-card p-4">
                <div className="flex items-center justify-between text-xs text-muted-foreground">
                  <span className="font-medium">全域共享</span>
                  <Globe className="size-3.5 text-blue-500" />
                </div>
                <div className="mt-2 flex items-baseline justify-between">
                  <span className="text-2xl font-bold font-mono tracking-tight">{globalKeys.length}</span>
                  <span className="text-[11px] text-blue-600 dark:text-blue-400">共享</span>
                </div>
              </div>

              <div className="rounded-xl border border-border/70 bg-card p-4">
                <div className="flex items-center justify-between text-xs text-muted-foreground">
                  <span className="font-medium">个人私有</span>
                  <User className="size-3.5 text-emerald-500" />
                </div>
                <div className="mt-2 flex items-baseline justify-between">
                  <span className="text-2xl font-bold font-mono tracking-tight">{personalKeys.length}</span>
                  <span className="text-[11px] text-emerald-600 dark:text-emerald-400">私有</span>
                </div>
              </div>

              <div className="rounded-xl border border-border/70 bg-card p-4">
                <div className="flex items-center justify-between text-xs text-muted-foreground">
                  <span className="font-medium">环境变量</span>
                  <Terminal className="size-3.5 text-amber-500" />
                </div>
                <div className="mt-2 flex items-baseline justify-between">
                  <span className="text-2xl font-bold font-mono tracking-tight">{envKeys.length}</span>
                  <span className="text-[11px] text-amber-600 dark:text-amber-400">系统注入</span>
                </div>
              </div>
            </div>

            {/* Filter Bar - Pure Chinese */}
            <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between pt-2">
              <div className="flex items-center gap-1 rounded-xl border border-border/70 bg-muted/30 p-1">
                {(
                  [
                    { id: "all", label: "全部凭证", count: keys?.length },
                    { id: "global", label: "全域共享", count: globalKeys.length },
                    { id: "personal", label: "个人独占", count: personalKeys.length },
                    { id: "env", label: "环境变量", count: envKeys.length },
                  ] as const
                ).map((tab) => (
                  <button
                    key={tab.id}
                    onClick={() => setScopeTab(tab.id)}
                    className={`flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition-all ${
                      scopeTab === tab.id
                        ? "bg-background text-foreground shadow-2xs font-semibold"
                        : "text-muted-foreground hover:text-foreground"
                    }`}
                  >
                    <span>{tab.label}</span>
                    {tab.count !== undefined && (
                      <span className="rounded bg-muted px-1.5 py-0.2 font-mono text-[10px]">
                        {tab.count}
                      </span>
                    )}
                  </button>
                ))}
              </div>

              <div className="relative max-w-xs flex-1">
                <Search className="absolute left-3 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
                <Input
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder="按凭证名称检索..."
                  className="h-8 pl-9 text-xs bg-card"
                />
              </div>
            </div>

            {error && (
              <div className="rounded-xl border border-destructive/40 bg-destructive/10 p-3.5 text-xs text-destructive flex items-center gap-2">
                <Info className="size-4 shrink-0" />
                {error}
              </div>
            )}

            {/* Skeleton Loading */}
            {keys === null && !error && (
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                {Array.from({ length: 4 }).map((_, i) => (
                  <div key={i} className="h-28 animate-pulse rounded-xl border border-border/60 bg-muted/20" />
                ))}
              </div>
            )}

            {/* Empty State */}
            {keys !== null && empty && (
              <div className="flex flex-col items-center justify-center rounded-2xl border border-dashed border-border p-12 text-center">
                <div className="flex size-12 items-center justify-center rounded-2xl bg-emerald-500/10 text-emerald-500">
                  <VaultBrandIcon size={24} />
                </div>
                <h3 className="mt-3 text-sm font-semibold">凭证保险库暂无配置</h3>
                <p className="mt-1 text-xs text-muted-foreground max-w-xs leading-normal">
                  添加首个个人或全域接口密钥，让智能体在安全的隔离空间中调用对应的扩展工具。
                </p>
                <Button
                  size="sm"
                  className="gap-1.5 bg-emerald-600 text-white hover:bg-emerald-700 mt-4"
                  onClick={() => setEditor({ mode: "add", defaultScope: "personal" })}
                >
                  <Plus className="size-4" />
                  保存首个凭证
                </Button>
              </div>
            )}

            {/* Main Cards List */}
            {keys !== null && filteredKeys.length > 0 && (
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                {filteredKeys.map((entry) => (
                  <VaultCard
                    key={`${entry.scope}:${entry.key}`}
                    entry={entry}
                    onEdit={() => setEditor({ mode: "edit", entry })}
                    onDelete={entry.source !== "env" ? () => onDelete(entry) : undefined}
                  />
                ))}
              </div>
            )}
          </div>
        </main>
      </div>

      <VaultEditor
        state={editor}
        onClose={() => setEditor(null)}
        onSaved={() => {
          setEditor(null);
          refresh();
        }}
      />
    </div>
  );
}

function VaultCard({
  entry,
  onEdit,
  onDelete,
}: {
  entry: VaultKeyEntry;
  onEdit: () => void;
  onDelete?: () => void;
}) {
  const [copied, setCopied] = useState(false);
  const isEnv = entry.source === "env";
  const isGlobal = entry.scope === "global";
  const hasTimestamp = (entry.updated_at ?? 0) > 0;

  const copyKey = () => {
    navigator.clipboard.writeText(entry.key);
    setCopied(true);
    toast.success(`已复制凭证名称: ${entry.key}`);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div
      className={`group relative flex flex-col justify-between rounded-xl border p-4 transition-all duration-200 hover:-translate-y-0.5 hover:shadow-md ${
        isEnv
          ? "border-amber-500/30 bg-amber-500/5"
          : isGlobal
          ? "border-blue-500/25 bg-card hover:border-blue-500/50"
          : "border-border/80 bg-card hover:border-emerald-500/40"
      }`}
    >
      <div>
        <div className="flex items-start justify-between gap-2">
          <div className="flex items-center gap-2 min-w-0">
            <span className="font-mono text-sm font-semibold tracking-tight text-foreground truncate">
              {entry.key}
            </span>
            <button
              onClick={copyKey}
              className="text-muted-foreground hover:text-foreground opacity-0 group-hover:opacity-100 transition-opacity"
              title="复制凭证名称"
            >
              {copied ? <Check className="size-3.5 text-emerald-500" /> : <Copy className="size-3.5" />}
            </button>
          </div>

          <div className="flex items-center gap-1 shrink-0">
            {isEnv ? (
              <span className="rounded-md bg-amber-500/15 px-2 py-0.5 text-[10px] font-medium text-amber-600 dark:text-amber-400 border border-amber-500/20">
                环境变量
              </span>
            ) : isGlobal ? (
              <span className="rounded-md bg-blue-500/10 px-2 py-0.5 text-[10px] font-medium text-blue-600 dark:text-blue-400 border border-blue-500/20 flex items-center gap-1">
                <Globe className="size-2.5" /> 全域共享
              </span>
            ) : (
              <span className="rounded-md bg-emerald-500/10 px-2 py-0.5 text-[10px] font-medium text-emerald-600 dark:text-emerald-400 border border-emerald-500/20 flex items-center gap-1">
                <User className="size-2.5" /> 个人私有
              </span>
            )}
          </div>
        </div>

        <div className="mt-3 flex items-center justify-between text-xs text-muted-foreground">
          <div className="flex items-center gap-1 font-mono tracking-widest text-muted-foreground/70 text-[11px]">
            ••••••••••••••••
          </div>
          {hasTimestamp && <span className="text-[11px] font-mono">{timeAgo(entry.updated_at)}</span>}
        </div>
      </div>

      <div className="mt-3 pt-3 border-t border-border/40 flex items-center justify-between">
        <span className="text-[11px] text-muted-foreground">
          {isEnv ? "只读系统变量" : "安全加密保存"}
        </span>
        <div className="flex items-center gap-1">
          <Button
            size="sm"
            variant="ghost"
            className="h-7 px-2 text-xs gap-1 hover:bg-muted"
            onClick={onEdit}
            title={isEnv ? "覆盖凭证值" : "修改凭证"}
          >
            <Pencil className="size-3" />
            {isEnv ? "覆盖" : "修改"}
          </Button>
          {!isEnv && onDelete && (
            <Button
              size="sm"
              variant="ghost"
              className="h-7 px-2 text-xs gap-1 text-destructive hover:bg-destructive/10 hover:text-destructive"
              onClick={onDelete}
              title="物理抹除凭证"
            >
              <Trash2 className="size-3" />
              抹除
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}

function VaultEditor({
  state,
  onClose,
  onSaved,
}: {
  state: EditorState | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [keyName, setKeyName] = useState("");
  const [value, setValue] = useState("");
  const [scope, setScope] = useState<"personal" | "global">("personal");
  const [reveal, setReveal] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isEdit = state?.mode === "edit";
  const open = state !== null;

  useEffect(() => {
    if (state?.mode === "edit") {
      setKeyName(state.entry.key);
      setScope(state.entry.scope);
    } else if (state?.mode === "add") {
      setKeyName("");
      setScope(state.defaultScope);
    }
    setValue("");
    setReveal(false);
    setError(null);
  }, [state]);

  const onSave = async () => {
    const k = keyName.trim();
    const v = value.trim();
    if (!k || !v) return;
    setSaving(true);
    setError(null);
    try {
      await saveIntegrationKey(k, v, scope);
      toast.success(isEdit ? `凭证 ${k} 已更新` : `凭证 ${k} 已持久化保存`);
      onSaved();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => { if (!o) onClose(); }}>
      <DialogContent className="sm:max-w-md rounded-2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-base font-semibold">
            <VaultBrandIcon size={18} />
            {isEdit ? "更新凭证内容" : "写入保险库凭证"}
          </DialogTitle>
          <DialogDescription className="text-xs text-muted-foreground">
            {isEdit
              ? "输入新密钥值以覆盖存储中对应的配置。"
              : "加密保存接口密钥。保存后明文将被遮蔽，仅供运行时沙箱调用。"}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-2">
          {!isEdit && (
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                作用域
              </label>
              <div className="grid grid-cols-2 gap-2">
                <button
                  type="button"
                  onClick={() => setScope("personal")}
                  className={`flex items-center justify-center gap-2 rounded-xl border p-2.5 text-xs font-medium transition-all ${
                    scope === "personal"
                      ? "border-emerald-500/50 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 font-semibold"
                      : "border-border hover:border-foreground/20 text-muted-foreground"
                  }`}
                >
                  <User className="size-3.5" />
                  个人私有
                </button>
                <button
                  type="button"
                  onClick={() => setScope("global")}
                  className={`flex items-center justify-center gap-2 rounded-xl border p-2.5 text-xs font-medium transition-all ${
                    scope === "global"
                      ? "border-blue-500/50 bg-blue-500/10 text-blue-600 dark:text-blue-400 font-semibold"
                      : "border-border hover:border-foreground/20 text-muted-foreground"
                  }`}
                >
                  <Globe className="size-3.5" />
                  全域共享
                </button>
              </div>
            </div>
          )}

          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
              凭证名称
            </label>
            <Input
              value={keyName}
              onChange={(e) => setKeyName(e.target.value)}
              placeholder="例如: GMAIL_API_KEY / DEEPSEEK_KEY"
              className="font-mono text-xs"
              disabled={isEdit}
              autoComplete="off"
              autoFocus={!isEdit}
            />
          </div>

          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
              密钥内容
            </label>
            <div className="relative">
              <Input
                type={reveal ? "text" : "password"}
                value={value}
                onChange={(e) => setValue(e.target.value)}
                placeholder={isEdit ? "输入新的密钥内容..." : "粘贴令牌或密钥..."}
                className="pr-10 font-mono text-xs"
                autoComplete="off"
                autoFocus={isEdit}
                onKeyDown={(e) => {
                  if (e.key === "Enter") onSave();
                }}
              />
              <button
                type="button"
                onClick={() => setReveal((r) => !r)}
                className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                aria-label={reveal ? "隐藏密钥" : "显示密钥"}
              >
                {reveal ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
              </button>
            </div>
          </div>

          {error && <div className="text-xs text-destructive rounded-md bg-destructive/10 p-2.5 font-mono">{error}</div>}

          <Button
            onClick={onSave}
            disabled={saving || !keyName.trim() || !value.trim()}
            className="w-full bg-emerald-600 hover:bg-emerald-700 text-white font-medium"
          >
            {saving ? (
              <>
                <Loader2 className="size-4 animate-spin" />
                提交加密写入中…
              </>
            ) : isEdit ? (
              "提交修改"
            ) : (
              "保存至保险库"
            )}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
