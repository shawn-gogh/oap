"use client";

import { useEffect, useState } from "react";
import { ScrollText } from "lucide-react";

import { Sidebar } from "@/components/sidebar";
import { ThemeToggle } from "@/components/theme-toggle";
import { listAuditLogs, type AuditLog } from "@/lib/api";

function formatTime(value: number): string {
  return new Intl.DateTimeFormat("zh-CN", {
    dateStyle: "medium",
    timeStyle: "medium",
  }).format(new Date(value));
}

export default function AuditLogsPage() {
  const [logs, setLogs] = useState<AuditLog[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    listAuditLogs().then(setLogs).catch((err) => {
      setLogs([]);
      setError(err instanceof Error ? err.message : String(err));
    });
  }, []);

  return (
    <div className="flex h-screen bg-background text-foreground">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border px-4">
          <div className="flex items-center gap-2"><ScrollText className="size-4 text-muted-foreground" /><h1 className="text-sm font-semibold">审计日志</h1></div>
          <ThemeToggle />
        </header>
        <main id="main-content" className="flex-1 overflow-y-auto"><div className="mx-auto max-w-6xl px-4 py-6">
          <p className="mb-5 text-sm text-muted-foreground">记录用户、用户组、授权、密钥与网页登录会话的关键变更。</p>
          {error && <p className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error.startsWith("HTTP 403") ? "需要管理员权限。" : error}</p>}
          {logs === null ? <p className="text-sm text-muted-foreground">正在加载审计日志…</p> : logs.length === 0 ? <p className="text-sm text-muted-foreground">暂无审计记录。</p> : <div className="overflow-hidden rounded-lg border border-border bg-card">
            {logs.map((log) => <div key={log.id} className="grid gap-1 border-b border-border px-4 py-3 last:border-0 sm:grid-cols-[180px_1fr_auto] sm:items-center sm:gap-4"><div className="text-xs text-muted-foreground">{formatTime(log.created_at)}</div><div><div className="font-mono text-sm">{log.action}</div><div className="text-xs text-muted-foreground">操作者：{log.actor_user_id} · {log.target_type}：{log.target_id}</div></div><pre className="max-w-xs overflow-auto whitespace-pre-wrap text-xs text-muted-foreground">{JSON.stringify(log.metadata)}</pre></div>)}
          </div>}
        </div></main>
      </div>
    </div>
  );
}
