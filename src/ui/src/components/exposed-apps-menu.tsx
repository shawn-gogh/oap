"use client";

import { useCallback, useEffect, useState } from "react";
import { AppWindow, Copy, ExternalLink, Globe, Link2Off, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  apiErrorMessage,
  createAppShare,
  deleteExposedApp,
  listExposedApps,
  revokeAppShare,
  type ExposedApp,
} from "@/lib/api";

function appUrl(appId: string): string {
  return `${window.location.origin}/apps/${appId}/`;
}

/**
 * Header entry listing the services this session's agent exposed via
 * expose_port, with open / share / revoke / take-offline actions. Renders
 * nothing until the session has at least one active app.
 */
export function ExposedAppsMenu(_props: { sessionId?: string; agentId?: string }) {
  const [apps, setApps] = useState<ExposedApp[]>([]);
  const [busyId, setBusyId] = useState<string | null>(null);

  // Unscoped: shared-runtime MCP misattributes session/agent, so list all
  // active apps this user can manage instead of filtering by session.
  const refresh = useCallback(() => {
    listExposedApps()
      .then(setApps)
      .catch(() => {});
  }, []);

  useEffect(() => {
    refresh();
    const timer = setInterval(refresh, 15000);
    return () => clearInterval(timer);
  }, [refresh]);

  const onCopyShare = async (app: ExposedApp) => {
    setBusyId(app.id);
    try {
      const share = await createAppShare(app.id);
      await navigator.clipboard.writeText(`${window.location.origin}${share.url}`);
      toast.success("分享链接已复制（24 小时有效）");
    } catch (err) {
      toast.error(apiErrorMessage(err, "生成分享链接失败"));
    } finally {
      setBusyId(null);
    }
  };

  const onRevoke = async (app: ExposedApp) => {
    setBusyId(app.id);
    try {
      await revokeAppShare(app.id);
      toast.success("已撤销该应用的所有分享链接");
    } catch (err) {
      toast.error(apiErrorMessage(err, "撤销分享失败"));
    } finally {
      setBusyId(null);
    }
  };

  const onOffline = async (app: ExposedApp) => {
    setBusyId(app.id);
    try {
      await deleteExposedApp(app.id);
      setApps((prev) => prev.filter((item) => item.id !== app.id));
      toast.success(`已下线「${app.name ?? app.id}」`);
    } catch (err) {
      toast.error(apiErrorMessage(err, "下线应用失败"));
    } finally {
      setBusyId(null);
    }
  };

  if (apps.length === 0) return null;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        className="inline-flex h-8 items-center gap-1.5 rounded-md border border-border bg-background px-3 text-sm shadow-xs hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
        aria-label="已发布的应用"
      >
        <AppWindow className="size-3.5" />
        应用
        <span className="rounded-full bg-primary/15 px-1.5 text-[11px] font-medium text-primary">
          {apps.length}
        </span>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-80 p-1.5">
        {apps.map((app) => (
          <div key={app.id} className="rounded-md px-2 py-2 hover:bg-muted/50">
            <div className="flex items-center gap-2">
              <Globe className="size-3.5 shrink-0 text-muted-foreground" />
              <span className="min-w-0 flex-1 truncate text-sm font-medium">
                {app.name || app.id}
              </span>
              <span className="shrink-0 font-mono text-[11px] text-muted-foreground">
                :{app.port}
              </span>
            </div>
            <div className="mt-1.5 flex items-center gap-1">
              <Button
                variant="outline"
                size="sm"
                className="h-6 px-2 text-xs"
                onClick={() => window.open(appUrl(app.id), "_blank", "noopener")}
              >
                <ExternalLink className="size-3" />
                打开
              </Button>
              <Button
                variant="outline"
                size="sm"
                className="h-6 px-2 text-xs"
                disabled={busyId === app.id}
                onClick={() => void onCopyShare(app)}
              >
                <Copy className="size-3" />
                复制分享链接
              </Button>
              <Button
                variant="outline"
                size="sm"
                className="h-6 px-2 text-xs"
                disabled={busyId === app.id}
                onClick={() => void onRevoke(app)}
                title="使已发出的分享链接全部失效"
              >
                <Link2Off className="size-3" />
              </Button>
              <Button
                variant="outline"
                size="sm"
                className="h-6 px-2 text-xs text-destructive hover:text-destructive"
                disabled={busyId === app.id}
                onClick={() => void onOffline(app)}
                title="下线：停止对外暴露该端口"
              >
                <Trash2 className="size-3" />
              </Button>
            </div>
          </div>
        ))}
        <div className="border-t border-border px-2 pb-1 pt-2 text-[11px] text-muted-foreground">
          由智能体通过 expose_port 发布；分享链接无需登录即可访问。
        </div>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
