"use client";

import { useEffect, useState } from "react";
import {
  Server,
  Plus,
  Pencil,
  Trash2,
  Loader2,
  Search,
  Info,
  X,
  Zap,
  Save,
  RotateCcw,
  Check,
  ChevronLeft,
  ChevronRight,
  ExternalLink,
  KeyRound,
  LockKeyhole,
  Cpu,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { AiGatewayBrandIcon } from "@/components/brand-kit-icons";
import { BrandIcon } from "@/components/brand-icons";
import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  listMcpServers,
  createMcpServer,
  updateMcpServer,
  deleteMcpServer,
  listMcpServerTools,
  testMcpServerTools,
  discoverMcpToolsFromUrl,
  getMcpProxyBaseUrl,
  saveMcpProxyBaseUrl,
} from "@/lib/api";
import type { McpProxyBaseUrlSetting, McpToolDef } from "@/lib/api";
import type { McpServer } from "@/lib/types";
import { cn } from "@/lib/utils";

type VariableScope = "instance" | "per_user";

interface VariableDef {
  name: string;
  description: string;
  scope: VariableScope;
  value: string;
}

interface FormState {
  server_name: string;
  alias: string;
  description: string;
  url: string;
  transport: string;
  variables: VariableDef[];
  static_headers: { name: string; value: string }[];
  allowed_tools: string[];
  allowed_tools_text: string;
  available_on_public_internet: boolean;
}

const EMPTY_FORM: FormState = {
  server_name: "",
  alias: "",
  description: "",
  url: "",
  transport: "sse",
  variables: [],
  static_headers: [],
  allowed_tools: [],
  allowed_tools_text: "",
  available_on_public_internet: true,
};

const GMAIL_TEMPLATE_TOOLS = [
  "search_threads",
  "get_thread",
  "create_draft",
  "list_drafts",
  "list_labels",
];

const GMAIL_TEMPLATE_SCOPES = [
  "https://www.googleapis.com/auth/gmail.readonly",
  "https://www.googleapis.com/auth/gmail.compose",
];

const GMAIL_TEMPLATE_RESOURCE = "https://gmailmcp.googleapis.com/mcp";

type McpServersTab = "servers" | "templates";

const MCP_SERVERS_TABS: { id: McpServersTab; label: string }[] = [
  { id: "servers", label: "已注册服务器" },
  { id: "templates", label: "扩展服务模板" },
];

const GMAIL_TEMPLATE_STEPS: {
  title: string;
  eyebrow: string;
  icon: LucideIcon;
  body: string;
  details: string[];
  code: string;
  link?: { label: string; href: string };
}[] = [
  {
    title: "Google 云项目准备",
    eyebrow: "Cloud APIs",
    icon: Server,
    body: "创建或选择拥有 OAuth 客户端的 Google Cloud 项目，启用 Gmail 服务。",
    details: [
      "如果 CLI 未认证，请先运行 gcloud auth login",
      "执行 gcloud services enable gmail.googleapis.com",
      "确保配置正确的访问控制权限与 API 密钥",
    ],
    code: "gcloud services enable gmail.googleapis.com gmailmcp.googleapis.com --project=PROJECT_ID",
  },
  {
    title: "OAuth 授权范围配置",
    eyebrow: "Google Auth Platform",
    icon: LockKeyhole,
    body: "配置授权屏幕与权限范围，供智能体调起使用。",
    details: [
      "为智能体应用开启 Gmail readonly 和 compose 权限范围",
      "可实现只读、检索以及生成草稿功能",
    ],
    code: GMAIL_TEMPLATE_SCOPES.join("\n"),
    link: {
      label: "打开 Google Auth 控制台",
      href: "https://console.cloud.google.com/auth/overview",
    },
  },
];

export default function McpServersPage() {
  const [servers, setServers] = useState<McpServer[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [proxySetting, setProxySetting] = useState<McpProxyBaseUrlSetting | null>(null);
  const [proxyDraft, setProxyDraft] = useState("");
  const [proxySaving, setProxySaving] = useState(false);
  const [proxyError, setProxyError] = useState<string | null>(null);
  const [editorServer, setEditorServer] = useState<McpServer | null | "new">(null);
  const [confirmDelete, setConfirmDelete] = useState<McpServer | null>(null);
  const [activeTab, setActiveTab] = useState<McpServersTab>("servers");
  const [gmailTemplateOpen, setGmailTemplateOpen] = useState(false);
  const [gmailTemplateStep, setGmailTemplateStep] = useState(0);

  const refresh = async () => {
    try {
      setServers(await listMcpServers());
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const refreshProxySetting = async () => {
    try {
      const setting = await getMcpProxyBaseUrl();
      setProxySetting(setting);
      setProxyDraft(setting.proxy_base_url ?? "");
      setProxyError(null);
    } catch (e) {
      setProxyError(e instanceof Error ? e.message : String(e));
    }
  };

  useEffect(() => {
    refresh();
    refreshProxySetting();
  }, []);

  const onAddServer = () => {
    setActiveTab("servers");
    setEditorServer("new");
  };

  const onDelete = async (s: McpServer) => {
    setConfirmDelete(s);
  };

  const onConfirmDelete = async () => {
    if (!confirmDelete) return;
    const s = confirmDelete;
    setConfirmDelete(null);
    setServers((prev) => prev?.filter((x) => x.server_id !== s.server_id) ?? null);
    try {
      await deleteMcpServer(s.server_id);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      await refresh();
    }
  };

  return (
    <div className="flex h-screen overflow-hidden bg-background text-foreground selection:bg-blue-500/20">
      <Sidebar />
      <div className="flex flex-1 flex-col overflow-hidden">
        {/* Anti-slop Pure Chinese Header */}
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border/80 px-4 bg-background/80 backdrop-blur">
          <div className="flex items-center gap-2.5">
            <div className="flex size-7 items-center justify-center rounded-md bg-blue-500/10 text-blue-600 dark:text-blue-400 ring-1 ring-blue-500/20">
              <AiGatewayBrandIcon size={16} />
            </div>
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold tracking-tight">AI 网关基础设施</span>
              <span className="text-xs text-muted-foreground font-medium">/ MCP 扩展服务</span>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button size="sm" className="h-8 gap-1.5 text-xs bg-blue-600 text-white hover:bg-blue-700 font-medium" onClick={onAddServer}>
              <Plus className="size-3.5" />
              添加 MCP 服务器
            </Button>
            <ThemeToggle />
          </div>
        </header>

        <main className="flex-1 overflow-y-auto p-6 space-y-6">
          <div className="max-w-5xl space-y-6 mx-auto">
            {/* Command Banner - Pure Chinese */}
            <div className="relative overflow-hidden rounded-2xl border border-border/70 bg-card p-6 shadow-xs">
              <div className="absolute right-0 top-0 -mr-16 -mt-16 size-64 rounded-full bg-blue-500/5 blur-3xl pointer-events-none" />
              <div className="relative z-10 flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
                <div className="space-y-1.5 max-w-xl">
                  <div className="flex items-center gap-2">
                    <span className="inline-flex items-center gap-1 rounded-md bg-blue-500/10 px-2 py-0.5 text-xs font-medium text-blue-600 dark:text-blue-400 border border-blue-500/20">
                      <Cpu className="size-3" /> MCP 协议扩展
                    </span>
                    <span className="text-xs text-muted-foreground font-medium">智能体工具能力扩展</span>
                  </div>
                  <h1 className="text-xl font-bold tracking-tight text-foreground">
                    MCP (Model Context Protocol) 扩展服务器管理
                  </h1>
                  <p className="text-xs text-muted-foreground leading-relaxed">
                    配置并注册外部 MCP 交互服务器，支持 SSE 与 HTTP 传输，为智能体提供工具能力、上下文数据与 API 代理。
                  </p>
                </div>
              </div>
            </div>

            <McpServersTabs activeTab={activeTab} onChange={setActiveTab} />

            {activeTab === "servers" && (
              <div className="space-y-4">
                {error && (
                  <div className="rounded-xl border border-destructive/40 bg-destructive/10 px-4 py-3 text-xs font-mono text-destructive">
                    {error}
                  </div>
                )}

                {servers === null && !error && (
                  <div className="space-y-2">
                    {[...Array(4)].map((_, i) => (
                      <div
                        key={i}
                        className="h-12 rounded-xl border border-border/70 bg-card/60 animate-pulse"
                      />
                    ))}
                  </div>
                )}

                {servers !== null && servers.length === 0 && (
                  <div className="flex flex-col items-center justify-center gap-3 py-16 text-center rounded-2xl border border-dashed border-border/80 bg-card">
                    <Server className="size-10 text-muted-foreground/40" />
                    <p className="text-sm font-semibold text-foreground">尚未注册任何 MCP 服务器</p>
                    <p className="text-xs text-muted-foreground max-w-sm">点击下方按钮接入你的第一个 MCP 扩展服务。</p>
                    <Button size="sm" className="mt-2 text-xs bg-blue-600 hover:bg-blue-700 text-white font-medium" onClick={onAddServer}>
                      <Plus className="size-3.5" />
                      注册首个 MCP 服务器
                    </Button>
                  </div>
                )}

                {servers !== null && servers.length > 0 && (
                  <div className="rounded-2xl border border-border/70 overflow-hidden bg-card shadow-2xs">
                    <table className="min-w-[640px] w-full text-xs">
                      <thead>
                        <tr className="border-b border-border bg-muted/40 font-semibold">
                          <th className="px-4 py-3 text-left">服务器名称</th>
                          <th className="px-4 py-3 text-left">服务地址 (URL)</th>
                          <th className="px-4 py-3 text-left">传输协议</th>
                          <th className="px-4 py-3 text-left">特性属性</th>
                          <th className="px-4 py-3 text-left">运行状态</th>
                          <th className="px-4 py-3 text-right">操作</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-border/60">
                        {servers.map((s) => (
                          <ServerRow
                            key={s.server_id}
                            server={s}
                            onEdit={() => setEditorServer(s)}
                            onDelete={() => onDelete(s)}
                          />
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
              </div>
            )}
          </div>
        </main>
      </div>

      {/* Confirm delete dialog */}
      <Dialog open={confirmDelete !== null} onOpenChange={(o) => { if (!o) setConfirmDelete(null); }}>
        <DialogContent className="sm:max-w-sm rounded-2xl">
          <DialogHeader>
            <DialogTitle className="text-base font-bold">删除 MCP 服务器？</DialogTitle>
            <DialogDescription className="text-xs text-muted-foreground pt-1">
              将永久移除该 MCP 扩展服务器连接，该操作不可撤销。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter className="gap-2 sm:gap-0 pt-2">
            <Button variant="outline" size="sm" className="text-xs" onClick={() => setConfirmDelete(null)}>
              取消
            </Button>
            <Button size="sm" variant="destructive" className="text-xs" onClick={() => void onConfirmDelete()}>
              确认删除
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function McpServersTabs({
  activeTab,
  onChange,
}: {
  activeTab: McpServersTab;
  onChange: (tab: McpServersTab) => void;
}) {
  return (
    <div
      role="tablist"
      aria-label="MCP server sections"
      className="inline-flex rounded-xl border border-border/80 bg-muted/30 p-1"
    >
      {MCP_SERVERS_TABS.map((tab) => {
        const active = activeTab === tab.id;
        return (
          <button
            key={tab.id}
            id={`mcp-tab-${tab.id}`}
            type="button"
            role="tab"
            aria-selected={active}
            onClick={() => onChange(tab.id)}
            className={cn(
              "min-w-28 rounded-lg px-3.5 py-1.5 text-xs font-semibold transition-all text-muted-foreground",
              active && "bg-background text-foreground shadow-2xs border border-border/60",
            )}
          >
            {tab.label}
          </button>
        );
      })}
    </div>
  );
}

function ServerRow({
  server,
  onEdit,
  onDelete,
}: {
  server: McpServer;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const displayName = server.alias ?? server.server_name ?? server.server_id;
  const status = server.status ?? "unknown";

  return (
    <tr className="group bg-card hover:bg-muted/30 transition-colors">
      <td className="px-4 py-3">
        <div className="font-semibold text-xs text-foreground">{displayName}</div>
        {server.description && (
          <div className="text-[11px] text-muted-foreground mt-0.5 line-clamp-1">
            {server.description}
          </div>
        )}
      </td>
      <td className="px-4 py-3">
        <span className="font-mono text-xs text-muted-foreground truncate max-w-xs block">
          {server.url ?? "-"}
        </span>
      </td>
      <td className="px-4 py-3">
        <Badge variant="outline" className="text-[10px] uppercase font-mono">
          {server.transport}
        </Badge>
      </td>
      <td className="px-4 py-3">
        <div className="flex flex-wrap gap-1">
          {server.is_byok && (
            <Badge className="text-[10px] bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/30">
              自定义凭据 (BYOK)
            </Badge>
          )}
          {server.available_on_public_internet && (
            <Badge className="text-[10px] bg-blue-500/10 text-blue-600 dark:text-blue-400 border-blue-500/30">
              公网可用
            </Badge>
          )}
        </div>
      </td>
      <td className="px-4 py-3">
        <Badge
          variant={status === "active" ? "secondary" : "outline"}
          className={`text-[10px] ${
            status === "active"
              ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/30"
              : "text-muted-foreground"
          }`}
        >
          {status === "active" ? "就绪中" : status}
        </Badge>
      </td>
      <td className="px-4 py-3">
        <div className="flex items-center justify-end gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
          <Button
            size="sm"
            variant="ghost"
            className="h-7 w-7 p-0"
            onClick={onEdit}
            aria-label="编辑服务器"
          >
            <Pencil className="size-3.5" />
          </Button>
          <Button
            size="sm"
            variant="ghost"
            className="h-7 w-7 p-0 text-destructive hover:bg-destructive/10"
            onClick={onDelete}
            aria-label="删除服务器"
          >
            <Trash2 className="size-3.5" />
          </Button>
        </div>
      </td>
    </tr>
  );
}
