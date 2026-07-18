"use client";

import { useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { updateAgent } from "@/lib/api";
import type { Agent, AgentQuotaStatus } from "@/lib/types";

type QuotaForm = {
  budget: string;
  concurrency: string;
  rate: string;
};

export function AgentQuotaPanel({
  agent,
  quota,
  onAgentUpdated,
}: {
  agent: Agent;
  quota: AgentQuotaStatus;
  onAgentUpdated: (agent: Agent) => void;
}) {
  const [form, setForm] = useState<QuotaForm>(() => quotaForm(agent));
  const [saving, setSaving] = useState(false);

  const save = async () => {
    setSaving(true);
    try {
      const config = { ...(agent.config ?? {}) };
      setNumber(config, "budget_usd_monthly", form.budget);
      setNumber(config, "max_concurrent_sessions", form.concurrency);
      setNumber(config, "rate_per_minute", form.rate);
      const updated = await updateAgent(agent.id, { config });
      onAgentUpdated(updated);
      toast.success("预算与配额已保存");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  };

  const budget = numberValue(form.budget);
  const budgetRatio = budget ? Math.min(100, (quota.month_cost_usd / budget) * 100) : 0;
  return (
    <Card className="p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold">预算与执行配额</h3>
          <p className="mt-1 text-xs text-muted-foreground">
            超限请求由网关直接拒绝，并写入治理审计时间线。
          </p>
        </div>
        <Button size="sm" onClick={() => void save()} disabled={saving}>
          {saving ? "保存中…" : "保存配额"}
        </Button>
      </div>

      <div className="mt-4 grid gap-4 md:grid-cols-3">
        <QuotaInput
          id="agent-monthly-budget"
          label="月度预算（USD）"
          value={form.budget}
          placeholder="不限"
          onChange={(budget) => setForm((current) => ({ ...current, budget }))}
          detail={`本月已用 $${quota.month_cost_usd.toFixed(4)}`}
        />
        <QuotaInput
          id="agent-max-concurrency"
          label="最大并发会话"
          value={form.concurrency}
          placeholder="不限"
          onChange={(concurrency) => setForm((current) => ({ ...current, concurrency }))}
          detail={`当前活跃 ${quota.active_sessions}`}
        />
        <QuotaInput
          id="agent-rate-limit"
          label="每分钟请求数"
          value={form.rate}
          placeholder="不限"
          onChange={(rate) => setForm((current) => ({ ...current, rate }))}
          detail={`本分钟已用 ${quota.requests_this_minute}`}
        />
      </div>

      {budget !== null && (
        <div className="mt-4">
          <div className="mb-1 flex justify-between text-xs text-muted-foreground">
            <span>月度预算消耗</span>
            <span className="tabular-nums">{budgetRatio.toFixed(1)}%</span>
          </div>
          <div className="h-2 overflow-hidden rounded-full bg-muted">
            <div className="h-full bg-primary" style={{ width: `${budgetRatio}%` }} />
          </div>
        </div>
      )}
    </Card>
  );
}

function QuotaInput({
  id,
  label,
  value,
  placeholder,
  detail,
  onChange,
}: {
  id: string;
  label: string;
  value: string;
  placeholder: string;
  detail: string;
  onChange: (value: string) => void;
}) {
  return (
    <div className="grid gap-1.5">
      <Label htmlFor={id}>{label}</Label>
      <Input
        id={id}
        type="number"
        min="0"
        step={id === "agent-monthly-budget" ? "0.01" : "1"}
        value={value}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
      />
      <p className="text-xs text-muted-foreground">{detail}</p>
    </div>
  );
}

function quotaForm(agent: Agent): QuotaForm {
  return {
    budget: configNumber(agent, "budget_usd_monthly"),
    concurrency: configNumber(agent, "max_concurrent_sessions"),
    rate: configNumber(agent, "rate_per_minute"),
  };
}

function configNumber(agent: Agent, key: string): string {
  const value = agent.config?.[key];
  return typeof value === "number" ? String(value) : "";
}

function numberValue(value: string): number | null {
  if (!value.trim()) return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}

function setNumber(config: Record<string, unknown>, key: string, value: string) {
  const parsed = numberValue(value);
  if (parsed === null) delete config[key];
  else config[key] = parsed;
}
