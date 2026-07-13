"use client";

import {
  Activity,
  ArrowRight,
  CheckCircle2,
  CircleAlert,
  Database,
  FileText,
  KeyRound,
  ShieldCheck,
  Workflow,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import type { AgentPreflightReport, EvalRun } from "@/lib/api";
import type { AgentApplicationContract } from "@/lib/agent-builder";
import type { Agent, AgentTask, OpencodeSession, Routine } from "@/lib/types";

export type AgentDashboardSection =
  "overview" | "setup" | "runs" | "quality" | "governance";

const MODE_LABELS: Record<string, string> = {
  conversational: "对话应用",
  scheduled: "定时应用",
  event_driven: "事件应用",
  manual: "人工运行",
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function strings(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter(
        (item): item is string =>
          typeof item === "string" && item.trim().length > 0,
      )
    : [];
}

export function applicationContractFromAgent(
  agent: Agent,
): AgentApplicationContract | null {
  const raw =
    isRecord(agent.config) && isRecord(agent.config.application)
      ? agent.config.application
      : null;
  if (!raw || typeof raw.objective !== "string" || !raw.objective.trim())
    return null;

  const inputs = Array.isArray(raw.inputs)
    ? raw.inputs.flatMap((value) => {
        if (!isRecord(value)) return [];
        return [
          {
            type: typeof value.type === "string" ? value.type : "input",
            source: typeof value.source === "string" ? value.source : "",
            description:
              typeof value.description === "string" ? value.description : "",
          },
        ];
      })
    : [];
  const outputs = Array.isArray(raw.outputs)
    ? raw.outputs.flatMap((value) => {
        if (!isRecord(value)) return [];
        return [
          {
            type: typeof value.type === "string" ? value.type : "output",
            description:
              typeof value.description === "string" ? value.description : "",
          },
        ];
      })
    : [];
  const mode =
    typeof raw.interaction_mode === "string" ? raw.interaction_mode : "manual";

  return {
    version: 1,
    objective: raw.objective.trim(),
    audience: strings(raw.audience),
    interaction_mode:
      mode === "conversational" ||
      mode === "scheduled" ||
      mode === "event_driven" ||
      mode === "manual"
        ? mode
        : "manual",
    inputs,
    outputs,
    non_goals: strings(raw.non_goals),
    completion_criteria: strings(raw.completion_criteria),
    failure_behavior:
      typeof raw.failure_behavior === "string" ? raw.failure_behavior : "",
  };
}

function recentSession(
  sessions: OpencodeSession[],
): OpencodeSession | undefined {
  return [...sessions].sort(
    (a, b) => (b.time?.created ?? 0) - (a.time?.created ?? 0),
  )[0];
}

function formatTime(timestamp: number | undefined): string {
  if (!timestamp) return "暂无";
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(timestamp));
}

export function AgentApplicationOverview({
  agent,
  runtime,
  sessions,
  tasks,
  routines,
  evalRuns,
  filesCount,
  memoryCount,
  alwaysOnCount,
  credentialCount,
  grantCount,
  preflightReport,
  onSelectSection,
}: {
  agent: Agent;
  runtime: string;
  sessions: OpencodeSession[];
  tasks: AgentTask[];
  routines: Routine[];
  evalRuns: EvalRun[];
  filesCount: number;
  memoryCount: number;
  alwaysOnCount: number;
  credentialCount: number;
  grantCount: number;
  preflightReport: AgentPreflightReport | null;
  onSelectSection: (section: AgentDashboardSection) => void;
}) {
  const application = applicationContractFromAgent(agent);
  const latestSession = recentSession(sessions);
  const latestTask = [...tasks].sort((a, b) => b.created_at - a.created_at)[0];
  const latestEval = [...evalRuns].sort(
    (a, b) => b.created_at - a.created_at,
  )[0];
  const activeRoutines = routines.filter(
    (routine) => routine.status === "active",
  );
  const failedChecks =
    preflightReport?.checks.filter((check) => check.verdict === "failed") ?? [];
  const uncertainChecks =
    preflightReport?.checks.filter(
      (check) =>
        check.verdict === "exists_only" || check.verdict === "unverified",
    ) ?? [];
  const setupReady = preflightReport
    ? failedChecks.length === 0 &&
      (agent.status !== "draft" || preflightReport.can_activate)
    : agent.status === "active";
  const setupTitle =
    failedChecks.length > 0
      ? "执行环境需要处理"
      : uncertainChecks.length > 0
        ? "具备运行条件，部分能力未验证"
        : setupReady
          ? "执行环境已验证"
          : "执行环境尚未就绪";

  return (
    <div className="grid gap-6">
      <section className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <SummaryCard
          label="生命周期"
          value={agent.status === "draft" ? "草稿" : "运行中"}
          detail={
            agent.status === "draft"
              ? "通过预检后可激活"
              : "可创建会话与接受调度"
          }
          tone={agent.status === "draft" ? "warning" : "success"}
        />
        <SummaryCard
          label="运行方式"
          value={MODE_LABELS[application?.interaction_mode ?? ""] ?? "未声明"}
          detail={
            activeRoutines.length > 0
              ? `${activeRoutines.length} 个启用 Routine`
              : "无启用 Routine"
          }
        />
        <SummaryCard
          label="最近任务"
          value={latestTask?.title ?? latestSession?.title ?? "暂无任务"}
          detail={
            latestTask
              ? `${latestTask.status} · ${formatTime(latestTask.created_at)}`
              : formatTime(
                  latestSession?.time?.created
                    ? latestSession.time.created * 1000
                    : undefined,
                )
          }
        />
        <SummaryCard
          label="最近质量结果"
          value={
            latestEval
              ? `${latestEval.passed}/${latestEval.total} 通过`
              : "尚未评估"
          }
          detail={
            latestEval
              ? `${latestEval.model} · ${latestEval.status}`
              : "运行评估以建立质量基线"
          }
          tone={
            latestEval?.status === "completed" &&
            latestEval.passed === latestEval.total
              ? "success"
              : undefined
          }
        />
      </section>

      <section>
        <div className="mb-2 flex items-center justify-between gap-3">
          <div>
            <h2 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
              应用蓝图
            </h2>
            <p className="mt-1 text-xs text-muted-foreground">
              描述业务结果与边界，模型和工具是它的执行配置。
            </p>
          </div>
          <Button
            size="sm"
            variant="outline"
            onClick={() => onSelectSection("setup")}
          >
            查看执行配置
          </Button>
        </div>
        {application ? (
          <Card className="overflow-hidden">
            <div className="border-b border-border p-5">
              <div className="flex flex-wrap items-center gap-2">
                <Badge variant="secondary">
                  {MODE_LABELS[application.interaction_mode]}
                </Badge>
                {application.audience.map((audience) => (
                  <Badge key={audience} variant="outline">
                    {audience}
                  </Badge>
                ))}
              </div>
              <p className="mt-3 text-base font-medium leading-7">
                {application.objective}
              </p>
            </div>
            <div className="grid md:grid-cols-2">
              <BlueprintList
                title="输入"
                items={application.inputs.map((input) =>
                  [input.type, input.description || input.source]
                    .filter(Boolean)
                    .join(" · "),
                )}
              />
              <BlueprintList
                title="输出"
                items={application.outputs.map((output) =>
                  [output.type, output.description].filter(Boolean).join(" · "),
                )}
                className="border-t md:border-l md:border-t-0"
              />
              <BlueprintList
                title="完成条件"
                items={application.completion_criteria}
                className="border-t"
              />
              <BlueprintList
                title="明确不做"
                items={application.non_goals}
                className="border-t md:border-l"
              />
            </div>
            {application.failure_behavior && (
              <div className="border-t border-border bg-muted/20 px-5 py-3 text-xs">
                <span className="font-medium">失败处理：</span>
                <span className="text-muted-foreground">
                  {application.failure_behavior}
                </span>
              </div>
            )}
          </Card>
        ) : (
          <Card className="border-dashed p-5">
            <p className="text-sm font-medium">
              这是旧版智能体，尚未声明应用蓝图。
            </p>
            <p className="mt-1 text-xs text-muted-foreground">
              现有配置仍可运行；编辑智能体即可补充业务目标、输入输出与完成条件。
            </p>
          </Card>
        )}
      </section>

      <section className="grid gap-3 lg:grid-cols-[1.2fr_0.8fr]">
        <Card className="p-4">
          <div className="flex items-start justify-between gap-3">
            <div className="flex items-start gap-3">
              {setupReady && uncertainChecks.length === 0 ? (
                <CheckCircle2 className="mt-0.5 size-5 text-emerald-600" />
              ) : (
                <CircleAlert className="mt-0.5 size-5 text-amber-600" />
              )}
              <div>
                <h3 className="text-sm font-semibold">{setupTitle}</h3>
                <p className="mt-1 text-xs text-muted-foreground">
                  {runtime} · {String(agent.model ?? "未选择模型")}
                </p>
              </div>
            </div>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => onSelectSection("setup")}
            >
              Setup <ArrowRight className="size-3.5" />
            </Button>
          </div>
          {preflightReport && (
            <div className="mt-4 grid gap-2 sm:grid-cols-2">
              {preflightReport.checks.slice(0, 6).map((check) => (
                <div
                  key={check.id}
                  className="flex items-start gap-2 rounded-md bg-muted/30 px-3 py-2 text-xs"
                >
                  {check.verdict === "failed" ? (
                    <CircleAlert className="mt-0.5 size-3.5 shrink-0 text-destructive" />
                  ) : check.verdict === "verified" ? (
                    <CheckCircle2 className="mt-0.5 size-3.5 shrink-0 text-emerald-600" />
                  ) : (
                    <CircleAlert className="mt-0.5 size-3.5 shrink-0 text-amber-600" />
                  )}
                  <span>
                    <span className="font-medium">{check.label}</span>
                    <span className="ml-1 text-muted-foreground">
                      {check.detail}
                    </span>
                  </span>
                </div>
              ))}
            </div>
          )}
        </Card>

        <Card className="grid grid-cols-2 overflow-hidden">
          <ResourceMetric
            icon={FileText}
            label="工作区文件"
            value={filesCount}
          />
          <ResourceMetric
            icon={Database}
            label="记忆"
            value={memoryCount}
            detail={`${alwaysOnCount} 条常驻`}
          />
          <ResourceMetric
            icon={KeyRound}
            label="凭证声明"
            value={credentialCount}
            className="border-t"
          />
          <ResourceMetric
            icon={ShieldCheck}
            label="使用授权"
            value={grantCount}
            className="border-t"
          />
        </Card>
      </section>

      <section className="grid gap-3 sm:grid-cols-3">
        <ActionCard
          icon={Activity}
          title="运行记录"
          detail={`${tasks.length} 个任务 · ${sessions.length} 个会话`}
          onClick={() => onSelectSection("runs")}
        />
        <ActionCard
          icon={Workflow}
          title="质量与改进"
          detail={`${evalRuns.length} 次评估`}
          onClick={() => onSelectSection("quality")}
        />
        <ActionCard
          icon={ShieldCheck}
          title="治理与授权"
          detail={`${grantCount} 项授权`}
          onClick={() => onSelectSection("governance")}
        />
      </section>
    </div>
  );
}

function SummaryCard({
  label,
  value,
  detail,
  tone,
}: {
  label: string;
  value: string;
  detail: string;
  tone?: "success" | "warning";
}) {
  return (
    <Card className="p-4">
      <p className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
        {label}
      </p>
      <p
        className={`mt-2 truncate text-sm font-semibold ${tone === "success" ? "text-emerald-600" : tone === "warning" ? "text-amber-600" : ""}`}
      >
        {value}
      </p>
      <p className="mt-1 truncate text-xs text-muted-foreground">{detail}</p>
    </Card>
  );
}

function BlueprintList({
  title,
  items,
  className = "",
}: {
  title: string;
  items: string[];
  className?: string;
}) {
  return (
    <div className={`border-border p-5 ${className}`}>
      <h3 className="text-xs font-semibold text-muted-foreground">{title}</h3>
      {items.length > 0 ? (
        <ul className="mt-2 grid gap-1.5 text-sm">
          {items.map((item, index) => (
            <li key={`${item}-${index}`} className="flex gap-2">
              <span className="text-muted-foreground">•</span>
              <span>{item}</span>
            </li>
          ))}
        </ul>
      ) : (
        <p className="mt-2 text-xs text-muted-foreground">未声明</p>
      )}
    </div>
  );
}

function ResourceMetric({
  icon: Icon,
  label,
  value,
  detail,
  className = "",
}: {
  icon: typeof FileText;
  label: string;
  value: number;
  detail?: string;
  className?: string;
}) {
  return (
    <div className={`border-border p-4 odd:border-r ${className}`}>
      <Icon className="size-4 text-muted-foreground" />
      <p className="mt-2 text-lg font-semibold">{value}</p>
      <p className="text-xs text-muted-foreground">
        {label}
        {detail ? ` · ${detail}` : ""}
      </p>
    </div>
  );
}

function ActionCard({
  icon: Icon,
  title,
  detail,
  onClick,
}: {
  icon: typeof Activity;
  title: string;
  detail: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex items-center gap-3 rounded-lg border border-border bg-card p-4 text-left transition-colors hover:bg-muted/40"
    >
      <Icon className="size-5 text-muted-foreground" />
      <span className="min-w-0 flex-1">
        <span className="block text-sm font-medium">{title}</span>
        <span className="block text-xs text-muted-foreground">{detail}</span>
      </span>
      <ArrowRight className="size-4 text-muted-foreground" />
    </button>
  );
}
