"use client";

import { useEffect, useMemo, useState } from "react";
import { History } from "lucide-react";
import { Card } from "@/components/ui/card";
import { listAgentAuditEvents, type AuditLog } from "@/lib/api";

const ACTION_LABELS: Record<string, string> = {
  "agent.governance.test": "完成运行检查",
  "agent.governance.publish_requested": "申请发布",
  "agent.governance.published": "批准并发布",
  "agent.governance.publish_blocked": "发布被阻断",
  "agent.governance.review_due": "复审到期",
  "agent.governance.rolled_back": "回滚配置",
  "agent.drift.accepted": "接受来源漂移",
  "agent.drift.rejected": "拒绝来源漂移",
  "agent.source.drift_candidate": "发现来源漂移",
  "agent.source.imported": "导入来源",
  "agent.source.synced": "同步来源",
  "agent.source.checked": "来源检查",
  "agent.runtime.conformance_checked": "契约检查",
  "agent.health.checked": "健康检查",
  "agent.emergency_stopped": "紧急停止",
  "agent.retired": "退役智能体",
  "agent.quota.rejected": "配额拒绝",
};

// Automated, high-frequency background checks (the platform runs these every
// few minutes on its own) — they drown out the handful of events an admin
// actually did, so keep them out of the default view.
const ROUTINE_ACTIONS = new Set([
  "agent.health.checked",
  "agent.source.synced",
  "agent.source.checked",
  "agent.runtime.conformance_checked",
]);

const DETAIL_LABELS: Record<string, string> = {
  revision: "版本",
  new_revision: "新版本",
  restored_from_revision: "恢复自版本",
  approval_id: "审批",
  quota: "配额项",
  current: "当前值",
  limit: "限额",
};

const VISIBLE_LIMIT = 20;

function eventDetail(event: AuditLog): string | null {
  const metadata = event.metadata;
  const pair = (label: string, value: unknown): [string, unknown] => [label, value];
  const entries = [
    pair("revision", metadata.revision),
    pair("new_revision", metadata.new_revision),
    pair("restored_from_revision", metadata.restored_from_revision),
    pair("approval_id", metadata.approval_id),
    pair("quota", metadata.quota),
    pair("current", metadata.current),
    pair("limit", metadata.limit),
  ].filter((entry) => entry[1] !== undefined && entry[1] !== null);
  return entries.length > 0
    ? entries.map(([label, value]) => `${DETAIL_LABELS[label] ?? label}：${String(value)}`).join(" · ")
    : null;
}

export function AgentAuditTimeline({ agentId }: { agentId: string }) {
  const [events, setEvents] = useState<AuditLog[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showRoutine, setShowRoutine] = useState(false);
  const [expanded, setExpanded] = useState(false);

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

  const routineCount = useMemo(
    () => events?.filter((event) => ROUTINE_ACTIONS.has(event.action)).length ?? 0,
    [events],
  );
  const filtered = useMemo(
    () => (showRoutine ? events : events?.filter((event) => !ROUTINE_ACTIONS.has(event.action))) ?? [],
    [events, showRoutine],
  );
  const visible = expanded ? filtered : filtered.slice(0, VISIBLE_LIMIT);

  return (
    <section>
      <div className="mb-2 flex items-end justify-between gap-3">
        <div>
          <h2 className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            <History className="size-3.5" />
            治理历史
          </h2>
          <p className="mt-1 text-xs text-muted-foreground">谁在何时执行了哪项治理动作。</p>
        </div>
        {routineCount > 0 && (
          <label className="flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground">
            <input
              type="checkbox"
              checked={showRoutine}
              onChange={(event) => setShowRoutine(event.target.checked)}
            />
            显示自动检查记录（{routineCount}）
          </label>
        )}
      </div>
      <Card className="overflow-hidden">
        {error ? (
          <p className="p-4 text-xs text-destructive">审计时间线加载失败：{error}</p>
        ) : !events ? (
          <div className="h-28 animate-pulse bg-muted/30" aria-label="正在加载治理历史" />
        ) : filtered.length === 0 ? (
          <p className="p-4 text-sm text-muted-foreground">尚无治理审计记录。</p>
        ) : (
          <>
            <ol className="divide-y divide-border">
              {visible.map((event) => (
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
            {!expanded && filtered.length > VISIBLE_LIMIT && (
              <button
                type="button"
                onClick={() => setExpanded(true)}
                className="w-full border-t border-border px-4 py-2 text-center text-xs text-muted-foreground hover:bg-muted/40"
              >
                展开剩余 {filtered.length - VISIBLE_LIMIT} 条
              </button>
            )}
          </>
        )}
      </Card>
    </section>
  );
}
