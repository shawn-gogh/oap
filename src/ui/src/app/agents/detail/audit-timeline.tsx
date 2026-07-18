"use client";

import { useEffect, useState } from "react";
import { History } from "lucide-react";
import { Card } from "@/components/ui/card";
import { listAgentAuditEvents, type AuditLog } from "@/lib/api";

const ACTION_LABELS: Record<string, string> = {
  "agent.governance.test": "完成运行检查",
  "agent.governance.publish_requested": "申请发布",
  "agent.governance.published": "批准并发布",
  "agent.governance.rolled_back": "回滚配置",
  "agent.source.drift_detected": "检测到来源漂移",
  "agent.source.drift_accepted": "接受来源漂移",
  "agent.source.drift_rejected": "拒绝来源漂移",
  "agent.emergency_stopped": "紧急停止",
  "agent.quota.rejected": "配额拒绝",
};

function eventDetail(event: AuditLog): string | null {
  const metadata = event.metadata;
  const entries = [
    ["revision", metadata.revision],
    ["new revision", metadata.new_revision],
    ["restored from", metadata.restored_from_revision],
    ["approval", metadata.approval_id],
    ["quota", metadata.quota],
    ["current", metadata.current],
    ["limit", metadata.limit],
  ].filter((entry) => entry[1] !== undefined && entry[1] !== null);
  return entries.length > 0
    ? entries.map(([label, value]) => `${label}: ${String(value)}`).join(" · ")
    : null;
}

export function AgentAuditTimeline({ agentId }: { agentId: string }) {
  const [events, setEvents] = useState<AuditLog[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    listAgentAuditEvents(agentId)
      .then((result) => {
        if (!cancelled) setEvents(result);
      })
      .catch((reason: unknown) => {
        if (!cancelled) setError(reason instanceof Error ? reason.message : String(reason));
      });
    return () => {
      cancelled = true;
    };
  }, [agentId]);

  return (
    <section>
      <div className="mb-2">
        <h2 className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          <History className="size-3.5" />
          治理历史
        </h2>
        <p className="mt-1 text-xs text-muted-foreground">谁在何时执行了哪项治理动作。</p>
      </div>
      <Card className="overflow-hidden">
        {error ? (
          <p className="p-4 text-xs text-destructive">审计时间线加载失败：{error}</p>
        ) : !events ? (
          <div className="h-28 animate-pulse bg-muted/30" aria-label="正在加载治理历史" />
        ) : events.length === 0 ? (
          <p className="p-4 text-sm text-muted-foreground">尚无治理审计记录。</p>
        ) : (
          <ol className="divide-y divide-border">
            {events.map((event) => (
              <li key={event.id} className="grid gap-2 px-4 py-3 sm:grid-cols-[150px_1fr]">
                <time className="text-xs text-muted-foreground">
                  {new Intl.DateTimeFormat("zh-CN", {
                    month: "short",
                    day: "numeric",
                    hour: "2-digit",
                    minute: "2-digit",
                  }).format(new Date(event.created_at))}
                </time>
                <div>
                  <div className="text-sm font-medium">
                    {ACTION_LABELS[event.action] ?? event.action}
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    {event.actor_user_id}
                    {eventDetail(event) ? ` · ${eventDetail(event)}` : ""}
                  </div>
                </div>
              </li>
            ))}
          </ol>
        )}
      </Card>
    </section>
  );
}
