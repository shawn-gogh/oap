"use client";

import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type CSSProperties,
  type PointerEvent,
} from "react";
import { AppWindow, ExternalLink, Globe, Link2, Loader2, RefreshCw, X } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { apiErrorMessage, createAppShare, listExposedApps, type ExposedApp } from "@/lib/api";

const DEFAULT_PANEL_WIDTH = 560;
const MIN_PANEL_WIDTH = 400;
const MAX_PANEL_WIDTH = 1100;

function appUrl(appId: string): string {
  return `${window.location.origin}/apps/${appId}/`;
}

/**
 * Right-side dock that embeds an app the session's agent published via
 * expose_port, served same-origin through the /apps/{id}/ reverse proxy so it
 * renders in an iframe. Sits alongside the workspace and inspector panels and,
 * like them, only one context panel shows at a time. Renders nothing when
 * closed; shows an empty state until the agent exposes something.
 */
export function BrowserPanel({
  open,
  onClose,
  sessionId,
  agentId,
}: {
  open: boolean;
  onClose: () => void;
  sessionId?: string;
  agentId?: string;
}) {
  const [apps, setApps] = useState<ExposedApp[] | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [reloadNonce, setReloadNonce] = useState(0);
  const [sharing, setSharing] = useState(false);
  const [panelWidth, setPanelWidth] = useState(DEFAULT_PANEL_WIDTH);

  const resizePanel = useCallback((width: number) => {
    setPanelWidth(Math.min(MAX_PANEL_WIDTH, Math.max(MIN_PANEL_WIDTH, width)));
  }, []);

  const startPanelResize = (event: PointerEvent<HTMLDivElement>) => {
    if (window.innerWidth < 1280) return;
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = panelWidth;
    const move = (moveEvent: globalThis.PointerEvent) => {
      resizePanel(startWidth + startX - moveEvent.clientX);
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
      document.body.style.removeProperty("cursor");
      document.body.style.removeProperty("user-select");
    };
    document.body.style.setProperty("cursor", "col-resize");
    document.body.style.setProperty("user-select", "none");
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
  };

  // Poll the exposed-app list while the panel is open. Unscoped fallback
  // mirrors ExposedAppsMenu: the shared-runtime MCP can misattribute
  // session/agent, so an empty scoped result falls back to all manageable apps.
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    const refresh = async () => {
      try {
        let list = await listExposedApps(sessionId, agentId);
        if (list.length === 0 && (sessionId || agentId)) {
          list = await listExposedApps();
        }
        if (!cancelled) setApps(list);
      } catch {
        if (!cancelled) setApps((prev) => prev ?? []);
      }
    };
    refresh();
    const timer = setInterval(refresh, 15000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [open, sessionId, agentId]);

  // Keep a valid selection: adopt the first app once one appears, and recover
  // if the selected app is taken offline.
  useEffect(() => {
    if (!apps || apps.length === 0) {
      if (activeId !== null) setActiveId(null);
      return;
    }
    if (!activeId || !apps.some((app) => app.id === activeId)) {
      setActiveId(apps[0].id);
    }
  }, [apps, activeId]);

  const activeApp = useMemo(
    () => apps?.find((app) => app.id === activeId) ?? null,
    [apps, activeId],
  );

  const onCopyShare = async () => {
    if (!activeApp) return;
    setSharing(true);
    try {
      const share = await createAppShare(activeApp.id);
      await navigator.clipboard.writeText(`${window.location.origin}${share.url}`);
      toast.success("分享链接已复制（24 小时有效）");
    } catch (err) {
      toast.error(apiErrorMessage(err, "生成分享链接失败"));
    } finally {
      setSharing(false);
    }
  };

  if (!open) return null;

  return (
    <aside
      className="fixed inset-y-0 right-0 z-40 flex w-[min(560px,calc(100vw-1rem))] min-w-0 flex-col border-l border-border/80 bg-background shadow-xl xl:relative xl:inset-auto xl:z-auto xl:h-screen xl:w-[var(--browser-panel-width)] xl:shrink-0 xl:shadow-none"
      style={{ "--browser-panel-width": `${panelWidth}px` } as CSSProperties}
    >
      {/* Resize Handle */}
      <div
        role="separator"
        aria-label="调整应用预览宽度"
        tabIndex={0}
        onPointerDown={startPanelResize}
        onDoubleClick={() => setPanelWidth(DEFAULT_PANEL_WIDTH)}
        className="group absolute inset-y-0 -left-1 z-50 hidden w-2 cursor-col-resize touch-none xl:block focus-visible:outline-none"
        title="拖动调整宽度，双击复位"
      >
        <span className="mx-auto block h-full w-px bg-transparent transition-colors group-hover:bg-blue-500 group-focus-visible:bg-blue-500" />
      </div>

      {/* Header */}
      <header className="flex h-12 shrink-0 items-center justify-between gap-2 border-b border-border/80 bg-background/80 px-4 backdrop-blur">
        <div className="flex min-w-0 items-center gap-2">
          <div className="flex size-7 shrink-0 items-center justify-center rounded-lg border border-blue-500/20 bg-blue-500/10 text-blue-600 dark:text-blue-400">
            <AppWindow className="size-4" />
          </div>
          <div className="min-w-0">
            <h2 className="text-xs font-bold tracking-tight text-foreground">应用预览</h2>
            <p className="truncate font-mono text-[10px] text-muted-foreground">
              {activeApp ? `${activeApp.name || activeApp.id} · :${activeApp.port}` : "由智能体发布"}
            </p>
          </div>
        </div>
        <Button variant="ghost" size="icon-sm" onClick={onClose} aria-label="关闭应用预览">
          <X className="size-4" />
        </Button>
      </header>

      {/* App selector + address bar */}
      {activeApp && (
        <div className="flex shrink-0 items-center gap-1.5 border-b border-border/80 bg-muted/30 px-3 py-1.5">
          {apps && apps.length > 1 ? (
            <Select value={activeId ?? undefined} onValueChange={(v) => v && setActiveId(v)}>
              <SelectTrigger className="h-7 w-[150px] shrink-0 bg-card text-xs" aria-label="选择应用">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {apps.map((app) => (
                  <SelectItem key={app.id} value={app.id} className="text-xs">
                    {app.name || app.id} :{app.port}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          ) : null}
          <div className="flex h-7 min-w-0 flex-1 items-center gap-1.5 rounded-md border border-border/70 bg-card px-2 font-mono text-[11px] text-muted-foreground">
            <Globe className="size-3 shrink-0" />
            <span className="truncate">/apps/{activeApp.id}/</span>
          </div>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={() => setReloadNonce((n) => n + 1)}
            aria-label="刷新"
            title="刷新"
          >
            <RefreshCw className="size-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={onCopyShare}
            disabled={sharing}
            aria-label="复制分享链接"
            title="复制分享链接（24 小时有效）"
          >
            {sharing ? <Loader2 className="size-3.5 animate-spin" /> : <Link2 className="size-3.5" />}
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={() => window.open(appUrl(activeApp.id), "_blank", "noopener")}
            aria-label="在新标签打开"
            title="在新标签打开"
          >
            <ExternalLink className="size-3.5" />
          </Button>
        </div>
      )}

      {/* Content */}
      {apps === null ? (
        <div className="flex flex-1 items-center justify-center gap-2 text-xs text-muted-foreground">
          <Loader2 className="size-4 animate-spin" />
          正在加载已发布的应用...
        </div>
      ) : activeApp ? (
        <iframe
          key={`${activeApp.id}:${reloadNonce}`}
          src={`/apps/${activeApp.id}/`}
          title={activeApp.name || activeApp.id}
          className="min-h-0 w-full flex-1 border-0 bg-white"
          sandbox="allow-scripts allow-forms allow-same-origin allow-popups allow-downloads allow-modals"
        />
      ) : (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
          <AppWindow className="size-6 text-muted-foreground" />
          <p className="text-sm text-muted-foreground">智能体尚未发布任何应用</p>
          <p className="text-xs text-muted-foreground">
            当智能体通过 <span className="font-mono">expose_port</span> 发布服务后，这里会自动显示。
          </p>
        </div>
      )}
    </aside>
  );
}
