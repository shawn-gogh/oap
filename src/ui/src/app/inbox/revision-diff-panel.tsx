"use client";

import { useEffect, useState } from "react";
import { GitCompareArrows } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { getAgentRevisionDiff, type AgentRevisionDiff, type InboxItem } from "@/lib/api";

const RISK_LABELS: Record<string, string> = {
  low: "低风险",
  medium: "中风险",
  high: "高风险",
  critical: "关键风险",
};

function riskLabel(risk: string): string {
  return RISK_LABELS[risk] ?? risk;
}

function integer(value: unknown): number | null {
  return typeof value === "number" && Number.isInteger(value) ? value : null;
}

function compactValue(value: unknown): string {
  if (value === undefined || value === null) return "未设置";
  const text = typeof value === "string" ? value : JSON.stringify(value);
  return text.length > 180 ? `${text.slice(0, 177)}…` : text;
}

export function RevisionDiffPanel({ item }: { item: InboxItem }) {
  const [diff, setDiff] = useState<AgentRevisionDiff | null>(null);
  const [error, setError] = useState<string | null>(null);
  const agentId =
    typeof item.args?.agent_id === "string" ? item.args.agent_id : item.agent;
  const fromVersion = integer(item.args?.base_revision) ?? 0;
  const toVersion = integer(item.args?.revision);

  useEffect(() => {
    if (item.kind !== "agent_publish" || !agentId || toVersion === null) return;
    let cancelled = false;
    getAgentRevisionDiff(agentId, fromVersion, toVersion)
      .then((result) => {
        if (!cancelled) setDiff(result);
      })
      .catch((reason: unknown) => {
        if (!cancelled) setError(reason instanceof Error ? reason.message : String(reason));
      });
    return () => {
      cancelled = true;
    };
  }, [agentId, fromVersion, item.kind, toVersion]);

  if (item.kind !== "agent_publish" || !agentId || toVersion === null) return null;
  return (
    <section className="rounded-lg border border-border bg-card">
      <div className="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
        <div className="flex items-center gap-2">
          <GitCompareArrows className="size-4 text-muted-foreground" />
          <h3 className="text-sm font-medium">发布修订差异</h3>
        </div>
        <span className="text-xs text-muted-foreground">
          v{fromVersion} → v{toVersion}
        </span>
      </div>
      {error ? (
        <p className="px-4 py-3 text-xs text-destructive">差异加载失败：{error}</p>
      ) : !diff ? (
        <div className="h-24 animate-pulse bg-muted/30" aria-label="正在加载修订差异" />
      ) : diff.findings.length === 0 ? (
        <p className="px-4 py-4 text-sm text-muted-foreground">两个版本的可审批配置没有差异。</p>
      ) : (
        <div className="divide-y divide-border">
          {diff.findings.map((finding) => (
            <div key={finding.field_path} className="grid gap-2 px-4 py-3 md:grid-cols-[180px_1fr]">
              <div>
                <div className="break-all font-mono text-xs font-medium">{finding.field_path}</div>
                <Badge
                  className="mt-1"
                  variant={finding.risk === "critical" || finding.risk === "high" ? "destructive" : "outline"}
                >
                  {riskLabel(finding.risk)}
                </Badge>
              </div>
              <div className="grid gap-2 text-xs sm:grid-cols-2">
                <DiffValue label="变更前" value={finding.previous_value} />
                <DiffValue label="变更后" value={finding.candidate_value} />
              </div>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

function DiffValue({ label, value }: { label: string; value: unknown }) {
  return (
    <div className="min-w-0 rounded-md border border-border bg-muted/30 px-3 py-2">
      <div className="mb-1 text-[10px] uppercase text-muted-foreground">{label}</div>
      <div className="break-words font-mono leading-5">{compactValue(value)}</div>
    </div>
  );
}
