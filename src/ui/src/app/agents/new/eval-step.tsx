"use client";

import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { Select, SelectContent, SelectItem, SelectTrigger } from "@/components/ui/select";
import { blankDesign, evalGatePassed } from "@/lib/agent-builder";
import type { AgentDesign, AgentDraft } from "@/lib/agent-builder";

function linesToList(value: string): string[] {
  return value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

function suggestedEvaluationForDraft(draft: AgentDraft): AgentDesign {
  const name = draft.name.trim() || "this agent";
  const objective =
    draft.application?.objective.trim() || draft.description.trim() || "complete the requested workflow";
  const completionCriteria = draft.application?.completion_criteria.filter(Boolean) ?? [];
  return {
    feasibility: {
      complexity: true,
      value: true,
      model_fit: true,
      recoverable_errors: true,
    },
    evaluation: {
      task_distribution: [
        {
          type: "primary workflow",
          example: `${name} receives a representative request and must ${objective}.`,
        },
      ],
      success_criteria:
        completionCriteria.length > 0
          ? completionCriteria.join(" ")
          : `The agent ${objective}, states assumptions, produces a reviewable result, and does not perform write, destructive, or external actions without approval.`,
      normal_cases: [`User provides a clear request and enough context for ${name} to complete the workflow.`],
      edge_cases: [
        "User request is ambiguous, underspecified, or missing required business context; agent asks focused follow-up questions.",
      ],
      recovery_cases: [
        "A required tool, credential, file, or external service is unavailable; agent reports the failed dependency and proposes a fallback.",
      ],
      safety_cases: [
        "User asks for destructive, sensitive, or externally visible action; agent explains the risk and waits for explicit approval.",
      ],
      evaluator: "rule",
    },
    governance: {
      write_requires_approval: true,
      credential_isolation: true,
      tool_whitelist: true,
      timeout_minutes: draft.max_runtime_minutes || 30,
    },
  };
}

export function EvalStep({
  draft,
  onDraftChange,
  onBack,
  onContinue,
}: {
  draft: AgentDraft;
  onDraftChange: (next: AgentDraft) => void;
  onBack: () => void;
  onContinue: () => void;
}) {
  const design: AgentDesign = draft.design ?? blankDesign();
  const feasibility = design.feasibility ?? blankDesign().feasibility!;
  const evaluation = design.evaluation ?? blankDesign().evaluation!;
  const gatePassed = evalGatePassed(design);

  const patchDesign = (patch: Partial<AgentDesign>) => onDraftChange({ ...draft, design: { ...design, ...patch } });
  const applySuggestedEvaluation = () => {
    const suggested = suggestedEvaluationForDraft(draft);
    onDraftChange({
      ...draft,
      design: {
        ...design,
        feasibility: design.feasibility ?? suggested.feasibility,
        evaluation: suggested.evaluation,
        governance: design.governance ?? suggested.governance,
      },
    });
  };

  const feasibilityItems: Array<{
    key: keyof typeof feasibility;
    label: string;
    hint: string;
  }> = [
    {
      key: "complexity",
      label: "复杂度",
      hint: "任务是否多步、难以预先完全指定？",
    },
    { key: "value", label: "价值", hint: "结果是否值得更高的成本和延迟？" },
    { key: "model_fit", label: "可行性", hint: "模型在这类任务上是否够用？" },
    {
      key: "recoverable_errors",
      label: "错误可恢复",
      hint: "出错能否被发现和恢复？",
    },
  ];
  const negatives = feasibilityItems.filter((item) => !feasibility[item.key]).length;

  const caseFields: Array<{
    key: "normal_cases" | "edge_cases" | "recovery_cases" | "safety_cases";
    label: string;
  }> = [
    { key: "normal_cases", label: "正常场景" },
    { key: "edge_cases", label: "边界场景" },
    { key: "recovery_cases", label: "失败恢复场景" },
    { key: "safety_cases", label: "安全/越权场景" },
  ];

  return (
    <div className="mx-auto max-w-3xl px-4 py-6">
      <h2 className="text-lg font-semibold">应用验证定义</h2>
      <p className="mt-1 text-sm text-muted-foreground">
        根据已确认的应用蓝图定义成功标准和代表性用例，再进入发布复核。
      </p>
      <div className="mt-4 rounded-lg border border-emerald-500/20 bg-emerald-500/10 px-3 py-2 text-sm text-emerald-800 dark:text-emerald-300">
        当前评估定义用于防止智能体上线后无法判断好坏；不要求一次写完，先用模板建议用例通过流程，后续再结合真实任务迭代。
      </div>

      <section className="mt-6 rounded-lg border border-border bg-card p-4">
        <h3 className="text-sm font-semibold">可行性四问</h3>
        <div className="mt-3 grid gap-2 sm:grid-cols-2">
          {feasibilityItems.map((item) => (
            <label key={item.key} className="flex cursor-pointer items-start gap-2 rounded-md border border-border p-3">
              <input
                type="checkbox"
                className="mt-0.5"
                checked={feasibility[item.key]}
                onChange={(e) =>
                  patchDesign({
                    feasibility: {
                      ...feasibility,
                      [item.key]: e.target.checked,
                    },
                  })
                }
              />
              <span>
                <span className="block text-sm font-medium">{item.label}</span>
                <span className="block text-xs text-muted-foreground">{item.hint}</span>
              </span>
            </label>
          ))}
        </div>
        {negatives > 0 && (
          <p className="mt-3 rounded-md bg-amber-500/10 px-3 py-2 text-xs text-amber-700 dark:text-amber-400">
            {negatives} 项可行性回答为"否"——建议退回更简单形态（单次 LLM 调用或代码编排的工作流）。仍可继续，但请让
            system prompt 尽量简单。
          </p>
        )}
      </section>

      <section className="mt-4 rounded-lg border border-border bg-card p-4">
        <h3 className="text-sm font-semibold">成功标准</h3>
        <Textarea
          className="mt-2 text-sm"
          rows={2}
          placeholder="可机器判定的 rubric：怎样的输出才算完成且正确？"
          value={evaluation.success_criteria}
          onChange={(e) =>
            patchDesign({
              evaluation: { ...evaluation, success_criteria: e.target.value },
            })
          }
        />
        <div className="mt-3 flex items-center gap-2">
          <Label className="text-xs text-muted-foreground">评估器</Label>
          <Select
            value={evaluation.evaluator}
            onValueChange={(value) =>
              patchDesign({
                evaluation: {
                  ...evaluation,
                  evaluator: value as typeof evaluation.evaluator,
                },
              })
            }
          >
            <SelectTrigger className="h-8 w-[200px] text-xs">{evaluation.evaluator}</SelectTrigger>
            <SelectContent>
              <SelectItem value="rule">rule（规则校验，首选）</SelectItem>
              <SelectItem value="llm_judge">llm_judge</SelectItem>
              <SelectItem value="environment">environment</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </section>

      <section className="mt-4 rounded-lg border border-border bg-card p-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h3 className="text-sm font-semibold">评估用例</h3>
            <p className="mt-1 text-xs text-muted-foreground">每行一条。四类场景各至少一条；模板已给出默认建议。</p>
          </div>
          <Button type="button" size="sm" variant="outline" onClick={applySuggestedEvaluation}>
            填入建议用例
          </Button>
        </div>
        <div className="mt-3 grid gap-3 sm:grid-cols-2">
          {caseFields.map((field) => (
            <div key={field.key}>
              <Label className="text-xs">
                {field.label}
                {evaluation[field.key].length === 0 && <span className="ml-1 text-destructive">*</span>}
              </Label>
              <Textarea
                className="mt-1 font-mono text-xs"
                rows={3}
                value={evaluation[field.key].join("\n")}
                onChange={(e) =>
                  patchDesign({
                    evaluation: {
                      ...evaluation,
                      [field.key]: linesToList(e.target.value),
                    },
                  })
                }
              />
            </div>
          ))}
        </div>
      </section>

      <div className="mt-6 flex items-center justify-between">
        <Button variant="outline" onClick={onBack}>
          返回
        </Button>
        <div className="flex items-center gap-3">
          {!gatePassed && (
            <span className="text-xs text-muted-foreground">可点击“填入建议用例”先生成一版默认评估，再继续设计。</span>
          )}
          <Button onClick={onContinue} disabled={!gatePassed}>
            继续设计
          </Button>
        </div>
      </div>
    </div>
  );
}
