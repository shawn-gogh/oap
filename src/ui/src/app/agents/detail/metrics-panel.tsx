"use client";

import { useEffect, useState } from "react";
import { Activity, CircleDollarSign, Gauge, Waypoints } from "lucide-react";
import { Card } from "@/components/ui/card";
import { getAgentMetrics } from "@/lib/api";
import type { AgentMetrics, AgentMeteringCoverage } from "@/lib/types";
import type { Agent } from "@/lib/types";
import { AgentQuotaPanel } from "./quota-panel";

function formatCost(value: number): string {
  if (value === 0) return "$0.00";
  if (value < 0.01) return `$${value.toFixed(4)}`;
  return `$${value.toFixed(2)}`;
}

function formatRate(value: number | null): string {
  return value === null ? "—" : `${(value * 100).toFixed(1)}%`;
}

function formatLatency(value: number | null): string {
  if (value === null) return "—";
  return value < 1000 ? `${Math.round(value)} ms` : `${(value / 1000).toFixed(1)} s`;
}

function coverageState(coverage: AgentMeteringCoverage) {
  const total = coverage.gateway_metered + coverage.provider_reported + coverage.unmetered;
  if (total === 0) return { label: "暂无运行数据", tone: "text-muted-foreground" };
  if (coverage.unmetered === 0) return { label: "成本完整可计量", tone: "text-emerald-600" };
  if (coverage.gateway_metered + coverage.provider_reported === 0) {
    return { label: "成本尚不可计量", tone: "text-amber-600" };
  }
  return { label: "成本部分可计量", tone: "text-amber-600" };
}

export function AgentMetricsPanel({
  agent,
  onAgentUpdated,
}: {
  agent: Agent;
  onAgentUpdated: (agent: Agent) => void;
}) {
  const [metrics, setMetrics] = useState<AgentMetrics | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    getAgentMetrics(agent.id, 30)
      .then((result) => {
        if (!cancelled) setMetrics(result);
      })
      .catch((reason: unknown) => {
        if (!cancelled) setError(reason instanceof Error ? reason.message : String(reason));
      });
    return () => {
      cancelled = true;
    };
  }, [agent.id]);

  const recent = metrics?.daily.slice(-7) ?? [];
  const maxCalls = Math.max(1, ...recent.map((point) => point.model_calls));

  if (error) {
    return (
      <Card className="border-dashed p-4 text-sm text-muted-foreground">
        运行指标暂时不可用：{error}
      </Card>
    );
  }
  if (!metrics) {
    return <Card className="h-36 animate-pulse bg-muted/40" aria-label="正在加载运行指标" />;
  }

  const coverage = coverageState(metrics.coverage);
  return (
    <section className="space-y-3">
      <div className="flex flex-wrap items-end justify-between gap-2">
        <div>
          <h2 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            近 30 天运行指标
          </h2>
          <p className={`mt-1 text-xs ${coverage.tone}`}>{coverage.label}</p>
        </div>
        <p className="text-xs text-muted-foreground">统计时区：{metrics.timezone}</p>
      </div>

      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        <MetricCard
          icon={Waypoints}
          label="运行次数"
          value={metrics.totals.invocations.toLocaleString()}
          detail={`${metrics.totals.model_calls.toLocaleString()} 次模型调用`}
        />
        <MetricCard
          icon={CircleDollarSign}
          label="估算成本"
          value={formatCost(metrics.totals.estimated_cost_usd)}
          detail={`${metrics.totals.total_tokens.toLocaleString()} tokens`}
        />
        <MetricCard
          icon={Activity}
          label="成功率"
          value={formatRate(metrics.totals.success_rate)}
          detail="按网关模型调用计算"
        />
        <MetricCard
          icon={Gauge}
          label="平均延迟"
          value={formatLatency(metrics.totals.average_latency_ms)}
          detail="仅统计网关可见调用"
        />
      </div>

      <Card className="grid gap-5 p-4 lg:grid-cols-[1fr_auto]">
        <div>
          <div className="mb-3 flex items-center justify-between text-xs text-muted-foreground">
            <span>最近 7 天模型调用趋势</span>
            <span>{recent.reduce((sum, point) => sum + point.model_calls, 0)} 次</span>
          </div>
          <div className="flex h-20 items-end gap-2" aria-label="最近 7 天模型调用柱状图">
            {recent.map((point) => (
              <div key={point.date} className="flex min-w-0 flex-1 flex-col items-center gap-1">
                <div
                  className="w-full rounded-sm bg-primary/70"
                  style={{ height: `${Math.max(4, (point.model_calls / maxCalls) * 56)}px` }}
                  title={`${point.date}: ${point.model_calls} 次`}
                />
                <span className="text-[10px] text-muted-foreground">{point.date.slice(5)}</span>
              </div>
            ))}
          </div>
        </div>
        <div className="grid min-w-52 content-center gap-2 text-xs">
          <CoverageRow label="gateway_metered" value={metrics.coverage.gateway_metered} />
          <CoverageRow label="provider_reported" value={metrics.coverage.provider_reported} />
          <CoverageRow label="unmetered" value={metrics.coverage.unmetered} />
        </div>
      </Card>
      <AgentQuotaPanel agent={agent} quota={metrics.quota} onAgentUpdated={onAgentUpdated} />
    </section>
  );
}

function MetricCard({
  icon: Icon,
  label,
  value,
  detail,
}: {
  icon: typeof Activity;
  label: string;
  value: string;
  detail: string;
}) {
  return (
    <Card className="p-4">
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        <Icon className="size-3.5" />
        {label}
      </div>
      <p className="mt-2 text-2xl font-semibold tabular-nums">{value}</p>
      <p className="mt-1 text-xs text-muted-foreground">{detail}</p>
    </Card>
  );
}

function CoverageRow({ label, value }: { label: string; value: number }) {
  return (
    <div className="flex items-center justify-between gap-6">
      <span className="font-mono text-muted-foreground">{label}</span>
      <span className="font-medium tabular-nums">{value}</span>
    </div>
  );
}
