"use client";

import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { toast } from "sonner";
import {
  Activity,
  Bot,
  Boxes,
  ChevronDown,
  ChevronRight,
  FileText,
  History,
  Inbox,
  KeyRound,
  Library,
  MessageCircle,
  MoreHorizontal,
  PanelLeft,
  PanelLeftClose,
  Plus,
  Puzzle,
  ScrollText,
  Search,
  Server,
  ServerCog,
  Settings,
  SlidersHorizontal,
  Zap,
  Trash2,
  Users,
  LogOut,
} from "lucide-react";
import { useSidebarCollapsed } from "@/lib/use-sidebar-collapsed";
import type { LucideIcon } from "lucide-react";
import { usePathname } from "next/navigation";
import { Button } from "@/components/ui/button";
import { AccessControlBrandIcon, AiGatewayBrandIcon } from "@/components/brand-kit-icons";
import { OapLogoMark } from "@/components/oap-logo";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  apiErrorMessage,
  clearStoredMasterKey,
  deleteSession,
  getCurrentUser,
  listSessions,
  logout,
  listInbox,
  type CurrentUser,
} from "@/lib/api";
import type { OpencodeSession } from "@/lib/types";

/** Stable internal identity for a workspace section. Logic keys off this;
 *  only `label` is localized, so renaming the Chinese label never breaks a
 *  behavioural check (the old code compared against the English string
 *  "Agent Platform" and silently failed once the label became 中文). */
type SectionId = "agent-platform" | "ai-gateway" | "access-control";

type NavItem = {
  label: string;
  href: string;
  icon: LucideIcon;
  active: (pathname: string) => boolean;
  badge?: number;
  /** Sub-group header; a heading renders whenever it changes from the
   *  previous item's group. */
  group?: string;
};

type NavSection = {
  id: SectionId;
  label: string;
  icon: React.ComponentType<{ size?: number; className?: string }>;
  home: string;
  description: string;
  items: NavItem[];
};

function useIsEmbedded(): boolean {
  const [embedded, setEmbedded] = useState(false);
  useEffect(() => { setEmbedded(window.self !== window.top); }, []);
  return embedded;
}

type SessionTone = "busy" | "failed" | "cancelled" | "idle";

function sessionTone(session: OpencodeSession): SessionTone {
  const status = (session.status ?? "").toLowerCase();
  if (status === "busy" || status === "running" || status === "starting") return "busy";
  if (status === "failed" || status === "error" || status === "timed_out") return "failed";
  if (status === "cancelled" || status === "canceled" || status === "aborted" || status === "stopped" || status === "interrupted") {
    return "cancelled";
  }
  return "idle";
}

/** The rail holds only the most recent conversations; the full list lives on
 *  /sessions/history, which has search, filters and date grouping. */
const SIDEBAR_SESSION_LIMIT = 7;

/** Groups that start folded. Core building blocks (智能体资产) stay open; only
 *  secondary configuration is tucked away. Persisted per-user in localStorage
 *  so a manual expand survives refresh. */
const DEFAULT_FOLDED_GROUPS = ["高级配置", "观测"];
const FOLD_STORAGE_KEY = "sidebar-folded-groups";

function useFoldedGroups() {
  const [folded, setFolded] = useState<Set<string>>(() => {
    if (typeof window === "undefined") return new Set(DEFAULT_FOLDED_GROUPS);
    try {
      const raw = localStorage.getItem(FOLD_STORAGE_KEY);
      if (raw) return new Set(JSON.parse(raw) as string[]);
    } catch { /* fall through to defaults */ }
    return new Set(DEFAULT_FOLDED_GROUPS);
  });
  const toggle = (group: string) =>
    setFolded((prev) => {
      const next = new Set(prev);
      if (next.has(group)) next.delete(group);
      else next.add(group);
      try { localStorage.setItem(FOLD_STORAGE_KEY, JSON.stringify([...next])); } catch { /* ignore */ }
      return next;
    });
  return [folded, toggle] as const;
}

/** Compact, Chinese relative time: 刚刚 / 6 分 / 2 小时 / 昨天 / 3 天. */
function timeAgo(ts?: number): string {
  if (!ts) return "";
  const mins = Math.floor((Date.now() - ts) / 60000);
  if (mins < 1) return "刚刚";
  if (mins < 60) return `${mins} 分`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs} 小时`;
  const days = Math.floor(hrs / 24);
  if (days === 1) return "昨天";
  if (days < 30) return `${days} 天`;
  return new Date(ts).toLocaleDateString("zh-CN", { month: "numeric", day: "numeric" });
}

function isTypingTarget(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el) return false;
  const tag = el.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || el.isContentEditable;
}

export function Sidebar({ activeId }: { activeId?: string | null }) {
  const embedded = useIsEmbedded();
  const router = useRouter();
  const pathname = usePathname();
  const [collapsed, setCollapsed] = useSidebarCollapsed();
  const [sessions, setSessions] = useState<OpencodeSession[] | null>(null);
  const [sessionQuery, setSessionQuery] = useState("");
  const [searchOpen, setSearchOpen] = useState(false);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [inboxCount, setInboxCount] = useState(0);
  const [currentUser, setCurrentUser] = useState<CurrentUser | null>(null);
  const [foldedGroups, toggleFoldedGroup] = useFoldedGroups();
  const load = async () => {
    try {
      const list = await listSessions();
      // Hide the registry's internal companion sessions (created automatically
      // when an agent is registered) - they duplicate every chat session in
      // the sidebar and aren't meant for direct conversation.
      setSessions(list.filter((s) => !s.title?.startsWith("agent-builder-")));
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  useEffect(() => {
    load();
    const t = setInterval(load, 15000);
    return () => clearInterval(t);
  }, []);

  // Poll the needs-attention count for the unread badge.
  useEffect(() => {
    const loadCount = () =>
      listInbox("attention")
        .then((items) => setInboxCount(items.length))
        .catch(() => {});
    loadCount();
    const t = setInterval(loadCount, 15000);
    return () => clearInterval(t);
  }, [pathname]);

  useEffect(() => {
    getCurrentUser().then(setCurrentUser).catch(() => setCurrentUser(null));
  }, []);

  // Global shortcuts: "/" reveals the session search, ⌘/Ctrl+N starts a new
  // conversation. Both no-op while typing in a field.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (isTypingTarget(event.target)) return;
      if (event.key === "/" && !event.metaKey && !event.ctrlKey) {
        event.preventDefault();
        setSearchOpen(true);
        requestAnimationFrame(() => searchInputRef.current?.focus());
      } else if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "n") {
        event.preventDefault();
        router.push("/chat/");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [router]);

  // Undo-window delete: the session leaves the list immediately, but the
  // backend delete only fires after the toast expires. 撤销 cancels it.
  const pendingDeleteTimers = useRef(new Map<string, number>());

  const query = sessionQuery.trim().toLowerCase();
  const matched = query
    ? sessions?.filter(
        (s) =>
          (s.title ?? "").toLowerCase().includes(query) || s.id.toLowerCase().includes(query),
      )
    : sessions;
  // Running sessions pin to the top — they are the ones worth jumping back to;
  // the rest keep the API's most-recent-first order.
  const ordered = matched
    ? [
        ...matched.filter((s) => sessionTone(s) === "busy"),
        ...matched.filter((s) => sessionTone(s) !== "busy"),
      ]
    : matched;
  const visibleSessions = ordered?.slice(0, SIDEBAR_SESSION_LIMIT);
  const hiddenSessions = (ordered?.length ?? 0) - (visibleSessions?.length ?? 0);

  const requestDelete = (id: string) => {
    const removed = sessions?.find((s) => s.id === id);
    setSessions((prev) => prev?.filter((s) => s.id !== id) ?? null);
    if (id === activeId) router.push("/chat/");

    const timer = window.setTimeout(() => {
      pendingDeleteTimers.current.delete(id);
      deleteSession(id).catch((err) => {
        setSessions((prev) => (removed && prev ? [removed, ...prev] : prev));
        toast.error(apiErrorMessage(err, "删除会话失败"));
      });
    }, 5000);
    pendingDeleteTimers.current.set(id, timer);

    toast(`已删除会话「${removed?.title?.trim() || id.slice(0, 12)}」`, {
      duration: 5000,
      action: {
        label: "撤销",
        onClick: () => {
          const pending = pendingDeleteTimers.current.get(id);
          if (pending !== undefined) {
            window.clearTimeout(pending);
            pendingDeleteTimers.current.delete(id);
          }
          setSessions((prev) => (removed && prev ? [removed, ...prev] : prev));
        },
      },
    });
  };

  const currentPath = pathname ?? "";
  const sections: NavSection[] = [
    ...(currentUser?.is_admin ? [{
      id: "ai-gateway" as const,
      label: "AI 网关",
      icon: AiGatewayBrandIcon,
      home: "/providers/",
      description: "密钥、团队、日志、模型提供方与运行时",
      items: [
        { label: "密钥管理", href: "/keys/", icon: KeyRound, active: (p: string) => p.startsWith("/keys"), group: "访问控制" },
        { label: "用户管理", href: "/users/", icon: Users, active: (p: string) => p.startsWith("/users"), group: "访问控制" },
        { label: "团队管理", href: "/teams/", icon: Users, active: (p: string) => p.startsWith("/teams"), group: "访问控制" },
        { label: "LLM 提供方", href: "/providers/", icon: ServerCog, active: (p: string) => p.startsWith("/providers"), group: "基础设施" },
        { label: "Agent 运行时", href: "/runtimes/", icon: ServerCog, active: (p: string) => p.startsWith("/runtimes"), group: "基础设施" },
        { label: "智能体来源", href: "/agent-sources/", icon: Server, active: (p: string) => p.startsWith("/agent-sources"), group: "基础设施" },
        { label: "MCP 扩展服务", href: "/mcp-servers/", icon: Server, active: (p: string) => p.startsWith("/mcp-servers"), group: "基础设施" },
        { label: "调用日志", href: "/observability/logs/", icon: Activity, active: (p: string) => p.startsWith("/observability"), group: "观测" },
        { label: "审计日志", href: "/audit-logs/", icon: ScrollText, active: (p: string) => p.startsWith("/audit-logs"), group: "观测" },
      ],
    }] : []),
    ...(currentUser?.can_manage_groups ? [{
      id: "access-control" as const,
      label: "访问控制",
      icon: AccessControlBrandIcon,
      home: "/groups/",
      description: "用户组与授权",
      items: [
        { label: "用户组", href: "/groups/", icon: Users, active: (p: string) => p.startsWith("/groups"), group: "访问控制" },
      ],
    }] : []),
    {
      id: "agent-platform" as const,
      label: "智能体平台",
      icon: Bot,
      home: "/chat/",
      description: "对话、智能体资产与高级配置",
      items: [
        { label: "对话", href: "/chat/", icon: MessageCircle, active: (p) => p === "/" || p.startsWith("/chat") || p.startsWith("/sessions"), group: "工作台" },
        { label: "收件箱", href: "/inbox/", icon: Inbox, active: (p) => p.startsWith("/inbox"), badge: inboxCount, group: "工作台" },
        { label: "定时任务", href: "/routines/", icon: Zap, active: (p) => p.startsWith("/routines"), group: "工作台" },
        { label: "智能体管理", href: "/agents/", icon: Bot, active: (p) => p.startsWith("/agents"), group: "智能体资产" },
        { label: "智能体目录", href: "/catalog/", icon: Library, active: (p) => p.startsWith("/catalog"), group: "智能体资产" },
        { label: "技能", href: "/skills/", icon: FileText, active: (p) => p.startsWith("/skills"), group: "智能体资产" },
        { label: "集成", href: "/integrations/", icon: Puzzle, active: (p) => p.startsWith("/integrations"), group: "智能体资产" },
        { label: "规则", href: "/rules/", icon: ScrollText, active: (p) => p.startsWith("/rules"), group: "高级配置" },
        { label: "凭证保险库", href: "/vault/", icon: KeyRound, active: (p) => p.startsWith("/vault"), group: "高级配置" },
      ],
    },
  ];
  const currentSection =
    sections.find((section) => section.items.some((item) => item.active(currentPath))) ??
    sections.find((section) => section.id === "agent-platform") ??
    sections[0];
  const isAgentPlatform = currentSection.id === "agent-platform";

  // Remember the last route visited within each workspace so switching back
  // returns you where you were, not to the section home.
  const lastPathBySection = useRef<Partial<Record<SectionId, string>>>({});
  useEffect(() => {
    if (currentPath) lastPathBySection.current[currentSection.id] = currentPath;
  }, [currentPath, currentSection.id]);

  if (embedded) return null;

  const onNew = () => router.push("/chat/");

  // When collapsed, force the icon-only rail at every breakpoint; when
  // expanded, keep the responsive sm: behavior (narrow on mobile, full on ≥sm).
  const labelCls = collapsed ? "hidden" : "hidden sm:inline";
  const blockCls = collapsed ? "hidden" : "hidden sm:flex";
  const railJustify = collapsed ? "justify-center" : "justify-center sm:justify-start";
  const asideWidth = collapsed ? "w-16" : "w-16 sm:w-64";

  return (
    <aside className={`flex h-screen ${asideWidth} shrink-0 flex-col border-r border-border bg-background`}>
      <div className="flex h-12 items-center gap-1 border-b border-border px-2 sm:px-3">
        <ProductSwitcher
          current={currentSection}
          sections={sections}
          onSelect={(section) =>
            router.push(lastPathBySection.current[section.id] ?? section.home)
          }
          collapsed={collapsed}
        />
        <button
          type="button"
          onClick={() => setCollapsed(!collapsed)}
          title={collapsed ? "展开侧边栏" : "折叠侧边栏"}
          aria-label={collapsed ? "展开侧边栏" : "折叠侧边栏"}
          className={`grid size-8 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50 ${
            collapsed ? "hidden" : ""
          }`}
        >
          <PanelLeftClose className="size-4" />
        </button>
      </div>
      {collapsed && (
        <button
          type="button"
          onClick={() => setCollapsed(false)}
          title="展开侧边栏"
          aria-label="展开侧边栏"
          className="mx-2 mt-2 grid h-8 place-items-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
        >
          <PanelLeft className="size-4" />
        </button>
      )}

      <div className="space-y-2 border-b border-border px-2 py-3 sm:px-3">
        {isAgentPlatform && (
          <Button
            onClick={onNew}
            className={`relative h-10 w-full gap-2 rounded-lg ${railJustify}`}
            size="sm"
            aria-label="新建会话"
            title="新建会话 (⌘N)"
          >
            <Plus className="size-4" />
            <span className={labelCls}>新建会话</span>
            {!collapsed && (
              <kbd className="ml-auto hidden rounded border border-primary-foreground/30 px-1 font-mono text-[10px] font-medium text-primary-foreground/80 sm:inline">
                ⌘N
              </kbd>
            )}
          </Button>
        )}
        <nav className="space-y-0.5" aria-label={currentSection.label}>
          {currentSection.items.map((item, index) => {
            const Icon = item.icon;
            const badge = item.badge ?? 0;
            const previousGroup = currentSection.items[index - 1]?.group;
            const showGroupHeader = Boolean(item.group) && item.group !== previousGroup;
            const active = item.active(currentPath);
            // A folded group still shows the item that matches the current
            // route, so you never lose your place in the nav.
            const groupHasActive =
              Boolean(item.group) &&
              currentSection.items.some((other) => other.group === item.group && other.active(currentPath));
            const folded = Boolean(item.group) && foldedGroups.has(item.group!) && !groupHasActive;
            // Only fold where the group header is actually visible (expanded
            // rail at ≥sm); the icon-only rail has no header to unfold with, so
            // it keeps showing every item.
            const foldCls = folded && !collapsed ? "sm:hidden" : "";
            return (
              <div key={item.href}>
                {showGroupHeader && (
                  <button
                    type="button"
                    onClick={() => toggleFoldedGroup(item.group!)}
                    aria-expanded={!folded}
                    className={`w-full items-center gap-1 rounded-md px-2 pb-1 pt-3 text-xs font-medium text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50 ${
                      collapsed ? "hidden" : "hidden sm:flex"
                    }`}
                  >
                    <ChevronRight className={`size-3 shrink-0 transition-transform ${folded ? "" : "rotate-90"}`} />
                    <span className="truncate">{item.group}</span>
                  </button>
                )}
                <Button
                  onClick={() => router.push(item.href)}
                  variant="ghost"
                  className={`relative h-9 w-full ${railJustify} ${foldCls} ${
                    active
                      ? "bg-muted font-medium text-foreground hover:bg-muted"
                      : "text-muted-foreground hover:bg-muted/50 hover:text-foreground"
                  }`}
                  size="sm"
                  aria-current={active ? "page" : undefined}
                  aria-label={item.label}
                  title={item.label}
                >
                  {active && (
                    <span className="absolute left-0 top-1/2 h-5 w-0.5 -translate-y-1/2 rounded-full bg-primary" />
                  )}
                  <span className="relative shrink-0">
                    <Icon className="size-4" />
                    {/* Collapsed rail has no label, so surface the badge as a
                        dot on the icon itself. */}
                    {badge > 0 && collapsed && (
                      <span className="absolute -right-1 -top-1 size-2 rounded-full bg-amber-500 ring-2 ring-background" />
                    )}
                  </span>
                  <span className={labelCls}>{item.label}</span>
                  {badge > 0 && !collapsed && (
                    <span className="ml-auto hidden h-5 min-w-5 items-center justify-center rounded-full bg-amber-500 px-1.5 text-[11px] font-semibold text-white sm:flex">
                      {badge}
                    </span>
                  )}
                </Button>
              </div>
            );
          })}
        </nav>
      </div>

      {isAgentPlatform && (
        <div className={`min-h-0 flex-1 flex-col ${blockCls}`}>
          <div className="flex shrink-0 items-center justify-between gap-2 px-3 pb-1.5 pt-2">
            <span className="text-xs font-medium text-muted-foreground">最近会话</span>
            <div className="flex items-center gap-1">
              {sessions && !searchOpen && (
                <span className="font-mono text-[10px] text-muted-foreground">{sessions.length}</span>
              )}
              <button
                type="button"
                onClick={() => {
                  setSearchOpen((open) => {
                    const next = !open;
                    if (!next) setSessionQuery("");
                    else requestAnimationFrame(() => searchInputRef.current?.focus());
                    return next;
                  });
                }}
                aria-label={searchOpen ? "收起搜索" : "搜索会话"}
                title="搜索会话 (/)"
                className={`grid size-6 place-items-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50 ${
                  searchOpen ? "bg-muted text-foreground" : ""
                }`}
              >
                <Search className="size-3.5" />
              </button>
            </div>
          </div>

          {searchOpen && (
            <div className="shrink-0 px-2 pb-2">
              <input
                ref={searchInputRef}
                value={sessionQuery}
                onChange={(event) => setSessionQuery(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Escape") {
                    setSessionQuery("");
                    setSearchOpen(false);
                  }
                }}
                placeholder="搜索会话标题或 ID..."
                aria-label="搜索历史会话"
                className="h-7 w-full rounded-lg border border-border/70 bg-card px-2.5 text-xs outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-blue-500/40"
              />
            </div>
          )}

          <div className="min-h-0 flex-1 space-y-0.5 overflow-y-auto px-2 pb-2">
            {error && <div className="px-1 py-2 text-xs font-mono text-destructive">{error}</div>}
            {!sessions && !error && (
              <div className="px-1 py-2 text-xs font-mono text-muted-foreground">正在加载历史...</div>
            )}
            {sessions && sessions.length === 0 && (
              <div className="px-1 py-2 text-xs text-muted-foreground">暂无历史会话。</div>
            )}
            {sessions && sessions.length > 0 && visibleSessions?.length === 0 && (
              <div className="px-1 py-2 text-xs text-muted-foreground">没有匹配的会话。</div>
            )}
            {visibleSessions?.map((s) => {
              const short = s.id.slice(0, 12);
              const title = s.title?.trim() || short;
              const active = s.id === activeId;
              const tone = sessionTone(s);
              return (
                <div
                  key={s.id}
                  onClick={() => router.push(`/chat/?id=${encodeURIComponent(s.id)}`)}
                  title={`${title}\n${short} · ${timeAgo(s.time?.updated ?? s.time?.created)}`}
                  className={`group relative flex cursor-pointer items-center gap-1.5 rounded-md px-2.5 py-1.5 text-xs transition-colors ${
                    active
                      ? "bg-muted font-medium text-foreground"
                      : "text-foreground hover:bg-muted/50"
                  }`}
                >
                  {active && (
                    <span className="absolute left-0 top-1/2 h-4 w-0.5 -translate-y-1/2 rounded-full bg-primary" />
                  )}
                  {tone === "busy" && (
                    <span className="size-1.5 shrink-0 rounded-full bg-emerald-500" aria-hidden />
                  )}
                  {tone === "failed" && (
                    <span className="size-1.5 shrink-0 rounded-full bg-destructive" aria-hidden />
                  )}
                  {tone === "cancelled" && (
                    <span className="size-1.5 shrink-0 rounded-full bg-muted-foreground/50" aria-hidden />
                  )}
                  <span className="min-w-0 flex-1 truncate">{title}</span>
                  {tone === "busy" ? (
                    <span className="shrink-0 whitespace-nowrap text-[10px] font-medium text-emerald-600 dark:text-emerald-400 group-hover:hidden">
                      运行中
                    </span>
                  ) : (
                    <span className="shrink-0 whitespace-nowrap font-mono text-[10px] text-muted-foreground group-hover:hidden">
                      {timeAgo(s.time?.updated ?? s.time?.created)}
                    </span>
                  )}
                  <DropdownMenu>
                    <DropdownMenuTrigger
                      onClick={(e) => e.stopPropagation()}
                      aria-label="会话操作"
                      title="会话操作"
                      // Always rendered (not hover-only) so touch devices can
                      // reach it; subtle until hover/focus/open.
                      className="shrink-0 rounded-md p-0.5 text-muted-foreground opacity-40 hover:bg-muted hover:text-foreground focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50 group-hover:opacity-100 data-[popup-open]:opacity-100"
                    >
                      <MoreHorizontal className="size-3.5" />
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end" className="w-40">
                      <DropdownMenuItem
                        onClick={(e) => {
                          e.stopPropagation();
                          requestDelete(s.id);
                        }}
                        className="text-destructive focus:text-destructive"
                      >
                        <Trash2 className="size-3.5" />
                        删除会话
                      </DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
              );
            })}
          </div>

          {sessions && sessions.length > 0 && (
            <button
              type="button"
              onClick={() => router.push("/sessions/history/")}
              className="flex shrink-0 items-center gap-1.5 border-t border-border px-3 py-2 text-xs text-muted-foreground hover:bg-muted/40 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/50"
            >
              <History className="size-3.5 shrink-0" />
              <span className="truncate">查看全部会话</span>
              {hiddenSessions > 0 && (
                <span className="ml-auto shrink-0 font-mono text-[10px]">+{hiddenSessions}</span>
              )}
            </button>
          )}
        </div>
      )}
      {/* When the recent-session region is not on screen (other workspace, or
          collapsed rail) this spacer pushes the account footer to the bottom. */}
      {(!isAgentPlatform || collapsed) && <div className="flex-1" />}

      <div className="border-t border-border p-2 sm:p-3">
        <DropdownMenu>
          <DropdownMenuTrigger className="mb-1 flex h-8 w-full items-center justify-center gap-1 rounded-md px-2 text-sm hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50 sm:justify-start">
            <Users className="size-4" />
            <span className="hidden min-w-0 flex-1 truncate text-left sm:inline">
              {currentUser?.display_name || "账户"}
            </span>
            <ChevronDown className="hidden size-3.5 sm:inline" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-56">
            <div className="px-2 py-1.5 text-xs text-muted-foreground">
              <div className="truncate font-medium text-foreground">{currentUser?.display_name || "账户"}</div>
              <div className="truncate font-mono">{currentUser?.id}</div>
            </div>
            <DropdownMenuItem
              onClick={() => {
                void logout().finally(() => {
                  clearStoredMasterKey();
                  router.replace("/login/");
                });
              }}
            >
              <LogOut className="size-4" />
              退出登录
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
        <Button
          onClick={() => router.push("/settings/")}
          variant={pathname?.startsWith("/settings") ? "secondary" : "ghost"}
          className={`w-full ${railJustify}`}
          size="sm"
          aria-label="系统设置"
          title="系统设置"
        >
          <Settings className="size-4" />
          <span className={labelCls}>系统设置</span>
        </Button>
      </div>
    </aside>
  );
}

/** Icon shown next to a group inside the workspace-switcher dropdown, so the
 *  three workspaces read as distinct destinations. */
const SECTION_GLYPH: Record<SectionId, LucideIcon> = {
  "agent-platform": Boxes,
  "ai-gateway": SlidersHorizontal,
  "access-control": Users,
};

function ProductSwitcher({
  current,
  sections,
  onSelect,
  collapsed,
}: {
  current: NavSection;
  sections: NavSection[];
  onSelect: (section: NavSection) => void;
  collapsed: boolean;
}) {
  const CurrentIcon = current.icon;
  const revealCls = collapsed ? "hidden" : "hidden sm:block";
  const isAgentPlatform = current.id === "agent-platform";

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        className={`flex h-9 min-w-0 flex-1 items-center justify-center gap-2 rounded-lg px-2 text-left text-sm font-semibold outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring/50 ${
          collapsed ? "" : "sm:justify-start"
        }`}
        title="切换工作区"
        aria-label="切换工作区"
      >
        {isAgentPlatform ? <OapLogoMark size={20} /> : <CurrentIcon className="size-5 shrink-0" />}
        <span className={`min-w-0 flex-1 truncate ${revealCls}`}>{current.label}</span>
        <ChevronDown className={`size-4 shrink-0 text-muted-foreground ${revealCls}`} />
      </DropdownMenuTrigger>
      <DropdownMenuContent side="bottom" align="start" className="w-72 p-1.5">
        <DropdownMenuLabel className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
          切换工作区
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        {sections.map((section) => {
          const Glyph = SECTION_GLYPH[section.id];
          const selected = section.id === current.id;
          return (
            <DropdownMenuItem
              key={section.id}
              onClick={() => onSelect(section)}
              className={`items-start gap-3 px-3 py-2.5 ${selected ? "bg-accent" : ""}`}
            >
              {section.id === "agent-platform" ? (
                <OapLogoMark size={20} className="mt-0.5" />
              ) : (
                <Glyph className="mt-0.5 size-5" />
              )}
              <div className="min-w-0 flex-1">
                <div className="flex items-center justify-between gap-3">
                  <span className="font-medium">{section.label}</span>
                  {selected && <span className="text-[11px] font-medium text-muted-foreground">当前选中</span>}
                </div>
                <p className="mt-0.5 text-xs text-muted-foreground">{section.description}</p>
              </div>
            </DropdownMenuItem>
          );
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
