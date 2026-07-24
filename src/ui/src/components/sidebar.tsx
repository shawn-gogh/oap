"use client";

import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { toast } from "sonner";
import {
  Activity,
  Bot,
  ChevronDown,
  ChevronRight,
  FileText,
  History,
  Inbox,
  KeyRound,
  Library,
  MessageCircle,
  PanelLeft,
  PanelLeftClose,
  Plus,
  Puzzle,
  ScrollText,
  Server,
  ServerCog,
  Settings,
  ShieldCheck,
  Zap,
  Trash2,
  Users,
  LogOut,
  Search,
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

type SessionTone = "busy" | "failed" | "idle";

function sessionTone(session: OpencodeSession): SessionTone {
  const status = (session.status ?? "").toLowerCase();
  if (status === "busy" || status === "running" || status === "starting") {
    return "busy";
  }
  if (status === "failed" || status === "error" || status === "timed_out") return "failed";
  return "idle";
}

/** The rail only holds the most recent conversations — the full list lives on
 *  /sessions/history, which has search, filters and date grouping. */
const SIDEBAR_SESSION_LIMIT = 10;

/** Nav groups that start folded so the session list gets the vertical space.
 *  A group is force-expanded whenever it contains the active route. */
const DEFAULT_FOLDED_GROUPS = new Set(["构建", "基础设施", "观测"]);

function timeAgo(ts?: number): string {
  if (!ts) return "";
  const secs = Math.max(0, Math.floor((Date.now() - ts) / 1000));
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h`;
  return `${Math.floor(hrs / 24)}d`;
}

export function Sidebar({ activeId }: { activeId?: string | null }) {
  const embedded = useIsEmbedded();
  const router = useRouter();
  const pathname = usePathname();
  const [collapsed, setCollapsed] = useSidebarCollapsed();
  const [sessions, setSessions] = useState<OpencodeSession[] | null>(null);
  const [sessionQuery, setSessionQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [inboxCount, setInboxCount] = useState(0);
  const [currentUser, setCurrentUser] = useState<CurrentUser | null>(null);
  const [foldedGroups, setFoldedGroups] = useState<Set<string>>(() => new Set(DEFAULT_FOLDED_GROUPS));
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

  if (embedded) return null;

  const onNew = async () => {
    router.push("/chat/");
  };

  const onDelete = (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
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
      label: "AI 网关",
      icon: AiGatewayBrandIcon,
      home: "/providers/",
      description: "密钥、团队、日志、模型提供方与运行时",
      items: [
        {
          label: "密钥管理",
          href: "/keys/",
          icon: KeyRound,
          active: (path: string) => path.startsWith("/keys"),
          group: "访问控制",
        },
        {
          label: "用户管理",
          href: "/users/",
          icon: Users,
          active: (path: string) => path.startsWith("/users"),
          group: "访问控制",
        },
        {
          label: "团队管理",
          href: "/teams/",
          icon: Users,
          active: (path: string) => path.startsWith("/teams"),
          group: "访问控制",
        },
        {
          label: "LLM 提供方",
          href: "/providers/",
          icon: ServerCog,
          active: (path: string) => path.startsWith("/providers"),
          group: "基础设施",
        },
        {
          label: "Agent 运行时",
          href: "/runtimes/",
          icon: ServerCog,
          active: (path: string) => path.startsWith("/runtimes"),
          group: "基础设施",
        },
        {
          label: "智能体来源",
          href: "/agent-sources/",
          icon: Server,
          active: (path: string) => path.startsWith("/agent-sources"),
          group: "基础设施",
        },
        {
          label: "MCP 扩展服务",
          href: "/mcp-servers/",
          icon: Server,
          active: (path: string) => path.startsWith("/mcp-servers"),
          group: "基础设施",
        },
        {
          label: "调用日志",
          href: "/observability/logs/",
          icon: Activity,
          active: (path: string) => path.startsWith("/observability"),
          group: "观测",
        },
        {
          label: "审计日志",
          href: "/audit-logs/",
          icon: ScrollText,
          active: (path: string) => path.startsWith("/audit-logs"),
          group: "观测",
        },
      ],
    }] : []),
    ...(currentUser?.can_manage_groups ? [{
      label: "访问控制",
      icon: AccessControlBrandIcon,
      home: "/groups/",
      description: "用户组与授权",
      items: [{
        label: "用户组",
        href: "/groups/",
        icon: Users,
        active: (path: string) => path.startsWith("/groups"),
        group: "访问控制",
      }],
    }] : []),
    {
      label: "智能体平台",
      icon: Bot,
      home: "/chat/",
      description: "智能体、收件箱、集成与技能",
      items: [
        {
          label: "对话",
          href: "/chat/",
          icon: MessageCircle,
          active: (path) => path === "/" || path.startsWith("/chat") || path.startsWith("/sessions"),
          group: "工作台",
        },
        {
          label: "收件箱",
          href: "/inbox/",
          icon: Inbox,
          active: (path) => path.startsWith("/inbox"),
          badge: inboxCount,
          group: "工作台",
        },
        {
          label: "智能体目录",
          href: "/catalog/",
          icon: Library,
          active: (path) => path.startsWith("/catalog"),
          group: "工作台",
        },
        {
          label: "定时任务",
          href: "/routines/",
          icon: Zap,
          active: (path) => path.startsWith("/routines"),
          group: "工作台",
        },
        {
          label: "智能体",
          href: "/agents/",
          icon: Bot,
          active: (path) => path.startsWith("/agents"),
          group: "构建",
        },
        {
          label: "技能",
          href: "/skills/",
          icon: FileText,
          active: (path) => path.startsWith("/skills"),
          group: "构建",
        },
        {
          label: "规则",
          href: "/rules/",
          icon: ScrollText,
          active: (path) => path.startsWith("/rules"),
          group: "构建",
        },
        {
          label: "集成",
          href: "/integrations/",
          icon: Puzzle,
          active: (path) => path.startsWith("/integrations"),
          group: "构建",
        },
        {
          label: "凭证保险库",
          href: "/vault/",
          icon: KeyRound,
          active: (path) => path.startsWith("/vault"),
          group: "构建",
        },
      ],
    },
  ];
  const currentSection =
    sections.find((section) => section.items.some((item) => item.active(currentPath))) ??
    sections.find((section) => section.label === "Agent Platform") ??
    sections[0];
  const isAgentPlatform = currentSection.label === "Agent Platform";

  // When collapsed, force the icon-only rail at every breakpoint; when
  // expanded, keep the responsive sm: behavior (narrow on mobile, full on ≥sm).
  const labelCls = collapsed ? "hidden" : "hidden sm:inline";
  const blockCls = collapsed ? "hidden" : "hidden sm:block";
  const railJustify = collapsed ? "justify-center" : "justify-center sm:justify-start";
  const asideWidth = collapsed ? "w-16" : "w-16 sm:w-64";

  return (
    <aside className={`flex h-screen ${asideWidth} shrink-0 flex-col border-r border-border bg-background`}>
      <div className="flex h-12 items-center gap-1 border-b border-border px-2 sm:px-3">
        <ProductSwitcher
          current={currentSection}
          sections={sections}
          onSelect={(section) => router.push(section.home)}
          collapsed={collapsed}
        />
        <button
          type="button"
          onClick={() => setCollapsed(!collapsed)}
          title={collapsed ? "展开侧边栏" : "折叠侧边栏"}
          aria-label={collapsed ? "展开侧边栏" : "折叠侧边栏"}
          className={`grid size-8 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground ${
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
          className="mx-2 mt-2 grid h-8 place-items-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
        >
          <PanelLeft className="size-4" />
        </button>
      )}

      <div className="space-y-3 border-b border-border px-2 py-3 sm:px-3">
        {isAgentPlatform && (
          <Button
            onClick={onNew}
            className={`relative w-full ${railJustify}`}
            size="sm"
            aria-label="新建会话"
          >
            <Plus className="size-4" />
            <span className={labelCls}>新建会话</span>
          </Button>
        )}
        <div className="space-y-1">
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
                  onClick={() =>
                    setFoldedGroups((prev) => {
                      const next = new Set(prev);
                      if (next.has(item.group!)) next.delete(item.group!);
                      else next.add(item.group!);
                      return next;
                    })
                  }
                  aria-expanded={!folded}
                  className={`flex w-full items-center gap-1 rounded-md px-2 pb-1 pt-2 text-[11px] font-medium uppercase tracking-wide text-muted-foreground hover:text-foreground ${blockCls}`}
                >
                  <ChevronRight
                    className={`size-3 transition-transform ${folded ? "" : "rotate-90"}`}
                  />
                  {item.group}
                </button>
              )}
              <Button
                onClick={() => router.push(item.href)}
                variant="ghost"
                className={`relative w-full ${railJustify} ${foldCls} ${
                  active ? "bg-primary/10 text-primary hover:bg-primary/15 hover:text-primary" : ""
                }`}
                size="sm"
                aria-label={item.label}
                title={item.label}
              >
                <Icon className="size-4" />
                <span className={labelCls}>{item.label}</span>
                {badge > 0 && (
                  <span className="absolute ml-7 mt-[-18px] flex h-4 min-w-4 items-center justify-center rounded-full bg-amber-500 px-1 text-[11px] font-semibold text-white sm:static sm:ml-auto sm:mt-0 sm:h-5 sm:min-w-5 sm:px-1.5 sm:text-[11px]">
                    {badge}
                  </span>
                )}
              </Button>
              </div>
            );
          })}
        </div>
      </div>

      <div className={`flex-1 overflow-y-auto py-2 ${blockCls}`}>
        {isAgentPlatform && (
          <>
            <div className="flex items-center justify-between px-3 pb-1.5 pt-1">
              <span className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">最近会话</span>
              {sessions && (
                <span className="text-[10px] font-mono text-muted-foreground">{sessions.length} 条</span>
              )}
            </div>

            {sessions && (
              <div className="px-2 pb-2">
                <div className="relative">
                  <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3 text-muted-foreground" />
                  <input
                    value={sessionQuery}
                    onChange={(event) => setSessionQuery(event.target.value)}
                    placeholder="搜索历史会话..."
                    aria-label="搜索历史会话"
                    className="h-7 w-full rounded-lg border border-border/70 bg-card pl-7 pr-2 text-xs outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-blue-500/40"
                  />
                </div>
              </div>
            )}
            {error && (
              <div className="px-3 py-2 text-xs font-mono text-destructive">{error}</div>
            )}
            {!sessions && !error && (
              <div className="px-3 py-2 text-xs font-mono text-muted-foreground">正在加载历史...</div>
            )}
            {sessions && sessions.length === 0 && (
              <div className="px-3 py-2 text-xs text-muted-foreground">
                暂无历史会话。
              </div>
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
                  className={`group mx-2 flex cursor-pointer items-center gap-1.5 rounded-lg px-2.5 py-1.5 text-xs transition-all ${
                    active
                      ? "bg-blue-500/10 text-blue-600 dark:text-blue-400 font-semibold border border-blue-500/20 shadow-2xs"
                      : "hover:bg-muted/40 text-foreground"
                  }`}
                >
                  {tone === "busy" && (
                    <span
                      title="智能体思考执行中"
                      className="size-1.5 shrink-0 rounded-full bg-emerald-500 animate-ping"
                    />
                  )}
                  {tone === "failed" && (
                    <span
                      title="上次执行异常"
                      className="size-1.5 shrink-0 rounded-full bg-destructive"
                    />
                  )}
                  <span className="min-w-0 flex-1 truncate font-medium">{title}</span>
                  <span className="shrink-0 font-mono text-[10px] text-muted-foreground group-hover:hidden">
                    {timeAgo(s.time?.updated ?? s.time?.created)}
                  </span>
                  <button
                    onClick={(e) => onDelete(e, s.id)}
                    className="hidden shrink-0 rounded-md p-0.5 hover:bg-destructive/10 hover:text-destructive focus-visible:outline-none group-hover:block"
                    aria-label="删除会话"
                    title="删除会话"
                  >
                    <Trash2 className="size-3" />
                  </button>
                </div>
              );
            })}
            {sessions && sessions.length > 0 && (
              <button
                type="button"
                onClick={() => router.push("/sessions/history/")}
                className="mx-2 mt-1 flex w-[calc(100%-1rem)] items-center gap-1.5 rounded-lg px-2.5 py-1.5 text-xs text-muted-foreground hover:bg-muted/40 hover:text-foreground"
              >
                <History className="size-3.5 shrink-0" />
                <span className="truncate">查看全部会话</span>
                {hiddenSessions > 0 && (
                  <span className="ml-auto shrink-0 font-mono text-[10px]">+{hiddenSessions}</span>
                )}
              </button>
            )}
          </>
        )}
      </div>

      <div className="border-t border-border p-2 sm:p-3">
        <DropdownMenu>
          <DropdownMenuTrigger className="mb-1 flex h-8 w-full items-center justify-center gap-1 rounded-md px-2 text-sm hover:bg-muted sm:justify-start">
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
        >
          <Settings className="size-4" />
          <span className={labelCls}>系统设置</span>
        </Button>
      </div>
    </aside>
  );
}

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
  const isAgentPlatform = current.label === "智能体平台" || current.label === "Agent Platform";

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        className={`flex h-9 min-w-0 flex-1 items-center justify-center gap-2 rounded-lg px-2 text-left text-sm font-semibold outline-none hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring/50 ${
          collapsed ? "" : "sm:justify-start"
        }`}
        aria-label="切换视图"
      >
        {isAgentPlatform ? (
          <OapLogoMark size={20} />
        ) : (
          <CurrentIcon className="size-5 shrink-0" />
        )}
        <span className={`min-w-0 flex-1 truncate ${revealCls}`}>{current.label}</span>
        <ChevronDown className={`size-4 shrink-0 text-muted-foreground ${revealCls}`} />
      </DropdownMenuTrigger>
      <DropdownMenuContent side="bottom" align="start" className="w-72 p-1.5">
        {sections.map((section) => {
          const Icon = section.icon;
          const selected = section.label === current.label;
          const isSectionAgentPlatform = section.label === "智能体平台" || section.label === "Agent Platform";
          return (
            <DropdownMenuItem
              key={section.label}
              onClick={() => onSelect(section)}
              className={`items-start gap-3 px-3 py-2.5 ${selected ? "bg-accent" : ""}`}
            >
              {isSectionAgentPlatform ? (
                <OapLogoMark size={20} className="mt-0.5" />
              ) : (
                <Icon className="mt-0.5 size-5" />
              )}
              <div className="min-w-0 flex-1">
                <div className="flex items-center justify-between gap-3">
                  <span className="font-medium">{section.label}</span>
                  {selected && <span className="text-[11px] text-muted-foreground font-medium">当前选中</span>}
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
