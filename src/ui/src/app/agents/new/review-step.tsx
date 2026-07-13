"use client";

import { useState } from "react";
import { CheckCircle2, Loader2, Sparkles, XCircle } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiErrorMessage, testRunAgentCase } from "@/lib/api";
import type { AgentCaseTestResult } from "@/lib/api";
import { blankDesign, evalGatePassed } from "@/lib/agent-builder";
import type { AgentDesign, AgentDraft } from "@/lib/agent-builder";
import type { Integration } from "@/lib/integrations";
import { cn } from "@/lib/utils";
import { ConfigPreview } from "./config-preview";

export function ReviewStep({
  draft,
  error,
  canCreate,
  saving,
  mcpIntegrations,
  onDraftChange,
  onBack,
  onCreate,
}: {
  draft: AgentDraft;
  error: string | null;
  canCreate: boolean;
  saving: boolean;
  mcpIntegrations: Integration[];
  onDraftChange: (next: AgentDraft) => void;
  onBack: () => void;
  onCreate: () => void;
}) {
  const design: AgentDesign = draft.design ?? blankDesign();
  const governance = design.governance ?? blankDesign().governance!;
  const attachedMcpMissingCredentials = draft.mcp_server_ids
    .map((id) => mcpIntegrations.find((integration) => integration.id === id))
    .filter((integration): integration is Integration => Boolean(integration && !integration.connected));

  const setGovernance = (patch: Partial<typeof governance>) => {
    const nextGovernance = { ...governance, ...patch };
    onDraftChange({
      ...draft,
      // timeout is enforcement, not prose: keep it in lockstep with the
      // runtime's max_runtime_minutes.
      max_runtime_minutes: nextGovernance.timeout_minutes,
      design: { ...design, governance: nextGovernance },
    });
  };

  const checks: Array<{ ok: boolean; label: string; detail: string }> = [
    {
      ok: draft.tools.length > 0 || draft.mcp_server_ids.length > 0,
      label: "工具白名单",
      detail:
        draft.tools.length > 0
          ? `已显式选择 ${draft.tools.length} 个工具：${draft.tools.map((t) => t.type).join(", ")}`
          : "未选择工具——智能体仅凭语言能力运行。",
    },
    {
      ok: governance.write_requires_approval,
      label: "写操作需要人工确认",
      detail: governance.write_requires_approval
        ? "将挂载审批 MCP；写/破坏性操作会暂停等待人工确认。"
        : "写操作将无人值守执行。仅只读智能体建议关闭。",
    },
    {
      ok: governance.credential_isolation,
      label: "凭证隔离",
      detail:
        draft.vault_keys.length > 0
          ? `${draft.vault_keys.length} 个保险库密钥在运行时注入，绝不进入 prompt。`
          : "未挂载任何凭证。",
    },
    {
      ok: governance.timeout_minutes > 0,
      label: "运行超时上限",
      detail: `单次运行上限 ${governance.timeout_minutes} 分钟。`,
    },
    {
      ok: attachedMcpMissingCredentials.length === 0,
      label: "MCP 凭证",
      detail:
        draft.mcp_server_ids.length === 0
          ? "未挂载 MCP 服务器。"
          : attachedMcpMissingCredentials.length === 0
            ? `已挂载 ${draft.mcp_server_ids.length} 个 MCP 服务器，凭证均已连接。`
            : `${attachedMcpMissingCredentials.map((item) => item.name).join("、")} 尚未配置凭证，运行时调用会失败。可先创建，再到 MCP 注册表补齐。`,
    },
    {
      ok: evalGatePassed(design),
      label: "评估已定义",
      detail: design.evaluation?.success_criteria
        ? `成功标准：${design.evaluation.success_criteria}`
        : "缺少评估定义。",
    },
  ];

  return (
    <div className="mx-auto max-w-5xl px-4 py-6">
      <h2 className="text-lg font-semibold">复核与治理</h2>
      <div className="mt-4 grid gap-6 lg:grid-cols-[minmax(320px,0.9fr)_minmax(380px,1.1fr)]">
        <section className="rounded-lg border border-border bg-card p-4">
          <h3 className="text-sm font-semibold">治理检查</h3>
          <ul className="mt-3 grid gap-2">
            {checks.map((check) => (
              <li key={check.label} className="flex items-start gap-2 rounded-md border border-border p-3">
                {check.ok ? (
                  <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-emerald-600" />
                ) : (
                  <XCircle className="mt-0.5 size-4 shrink-0 text-amber-600" />
                )}
                <span>
                  <span className="block text-sm font-medium">{check.label}</span>
                  <span className="block text-xs text-muted-foreground">{check.detail}</span>
                </span>
              </li>
            ))}
          </ul>
          <div className="mt-4 grid gap-3">
            <label className="flex cursor-pointer items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={governance.write_requires_approval}
                onChange={(e) => setGovernance({ write_requires_approval: e.target.checked })}
              />
              写操作需要人工确认
            </label>
            <div className="flex items-center gap-2">
              <Label className="text-xs text-muted-foreground">超时（分钟）</Label>
              <Input
                type="number"
                min={1}
                className="h-8 w-24 text-xs"
                value={governance.timeout_minutes}
                onChange={(e) => {
                  const next = Number.parseInt(e.target.value, 10);
                  if (Number.isFinite(next) && next > 0) setGovernance({ timeout_minutes: next });
                }}
              />
            </div>
          </div>
          {error && (
            <p className="mt-4 rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive">{error}</p>
          )}
          <div className="mt-5 flex items-center justify-between">
            <Button variant="outline" onClick={onBack}>
              返回设计
            </Button>
            <Button onClick={onCreate} disabled={!canCreate}>
              <CheckCircle2 className="size-3.5" />
              {saving ? "创建中..." : "创建智能体"}
            </Button>
          </div>
        </section>
        <section className="flex min-h-0 flex-col rounded-lg border border-border bg-card">
          <div className="border-b border-border px-5 py-3 text-sm font-semibold">最终配置</div>
          <ConfigPreview draft={draft} mcpIntegrations={mcpIntegrations} />
        </section>
      </div>
      <PreCreateTestRun draft={draft} />
    </div>
  );
}

interface TestRunCase {
  key: string;
  category: string;
  categoryLabel: string;
  input: string;
}

type TestRunOutcome =
  | { state: "running" }
  | { state: "done"; result: AgentCaseTestResult }
  | { state: "error"; message: string };

const TEST_CASE_CATEGORIES: Array<{
  key: "normal_cases" | "edge_cases" | "recovery_cases" | "safety_cases";
  category: string;
  label: string;
}> = [
  { key: "normal_cases", category: "normal", label: "正常" },
  { key: "edge_cases", category: "edge", label: "边界" },
  { key: "recovery_cases", category: "recovery", label: "恢复" },
  { key: "safety_cases", category: "safety", label: "安全" },
];

function PreCreateTestRun({ draft }: { draft: AgentDraft }) {
  const [outcomes, setOutcomes] = useState<Record<string, TestRunOutcome>>({});
  const evaluation = draft.design?.evaluation;
  const successCriteria = evaluation?.success_criteria?.trim() ?? "";

  const cases: TestRunCase[] = evaluation
    ? TEST_CASE_CATEGORIES.flatMap(({ key, category, label }) =>
        (evaluation[key] ?? []).map((input, index) => ({
          key: `${category}-${index}`,
          category,
          categoryLabel: label,
          input,
        })),
      )
    : [];
  const anyRunning = Object.values(outcomes).some((outcome) => outcome.state === "running");

  const runCase = async (testCase: TestRunCase) => {
    setOutcomes((prev) => ({ ...prev, [testCase.key]: { state: "running" } }));
    try {
      const result = await testRunAgentCase({
        system: draft.system,
        model: draft.model.trim() || undefined,
        category: testCase.category,
        caseInput: testCase.input,
        successCriteria,
      });
      setOutcomes((prev) => ({ ...prev, [testCase.key]: { state: "done", result } }));
    } catch (err) {
      setOutcomes((prev) => ({
        ...prev,
        [testCase.key]: { state: "error", message: apiErrorMessage(err, "试跑失败") },
      }));
    }
  };

  return (
    <section className="mt-6 rounded-lg border border-border bg-card p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold">创建前试跑</h3>
          <p className="mt-1 text-xs text-muted-foreground">
            用评估用例直接对话草案中的 system prompt 和模型，并按成功标准自动判定。试跑为纯模型对话，不挂载工具、MCP 和凭证。
          </p>
        </div>
      </div>
      {!successCriteria || cases.length === 0 ? (
        <p className="mt-3 rounded-md bg-amber-500/10 px-3 py-2 text-xs text-amber-700 dark:text-amber-400">
          缺少成功标准或评估用例，返回“评估 Eval”步骤补充后即可试跑。
        </p>
      ) : (
        <ul className="mt-3 grid gap-2">
          {cases.map((testCase) => {
            const outcome = outcomes[testCase.key];
            return (
              <li key={testCase.key} className="rounded-md border border-border p-3">
                <div className="flex flex-wrap items-start justify-between gap-2">
                  <div className="min-w-0 flex-1">
                    <Badge variant="outline" className="h-5 rounded-md text-[11px]">
                      {testCase.categoryLabel}
                    </Badge>
                    <p className="mt-1.5 text-sm leading-6">{testCase.input}</p>
                  </div>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={anyRunning}
                    onClick={() => void runCase(testCase)}
                  >
                    {outcome?.state === "running" ? (
                      <Loader2 className="size-3.5 animate-spin motion-reduce:animate-none" />
                    ) : (
                      <Sparkles className="size-3.5" />
                    )}
                    {outcome?.state === "done" || outcome?.state === "error" ? "重跑" : "试跑"}
                  </Button>
                </div>
                {outcome?.state === "error" && (
                  <p className="mt-2 rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive">
                    {outcome.message}
                  </p>
                )}
                {outcome?.state === "done" && (
                  <div className="mt-2 grid gap-2">
                    <div
                      className={cn(
                        "flex items-start gap-2 rounded-md px-3 py-2 text-xs",
                        outcome.result.pass
                          ? "bg-emerald-500/10 text-emerald-800 dark:text-emerald-300"
                          : "bg-destructive/10 text-destructive",
                      )}
                    >
                      {outcome.result.pass ? (
                        <CheckCircle2 className="mt-0.5 size-3.5 shrink-0" />
                      ) : (
                        <XCircle className="mt-0.5 size-3.5 shrink-0" />
                      )}
                      <span className="leading-5">{outcome.result.verdict}</span>
                    </div>
                    <details className="rounded-md border border-border bg-muted/30 px-3 py-2">
                      <summary className="cursor-pointer text-xs font-medium text-muted-foreground">
                        查看回答
                      </summary>
                      <pre className="mt-2 max-h-64 overflow-y-auto whitespace-pre-wrap text-xs leading-5">
                        {outcome.result.answer}
                      </pre>
                    </details>
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
