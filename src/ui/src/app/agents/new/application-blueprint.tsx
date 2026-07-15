"use client";

import { BarChart3, Plus, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import {
  applicationContractFor,
  type AgentApplicationContract,
  type AgentDashboardDefinition,
  type AgentApplicationInput,
  type AgentApplicationOutput,
  type AgentDraft,
  type AgentInteractionMode,
} from "@/lib/agent-builder";

const MODES: Array<{
  value: AgentInteractionMode;
  label: string;
  detail: string;
}> = [
  {
    value: "conversational",
    label: "对话应用",
    detail: "由用户消息触发并在会话中交付结果",
  },
  {
    value: "scheduled",
    label: "定时应用",
    detail: "由例程或定时计划周期性触发",
  },
  {
    value: "event_driven",
    label: "事件应用",
    detail: "由网络回调或消息渠道事件触发",
  },
  { value: "manual", label: "人工运行", detail: "由详情页或接口显式启动" },
];

export function ApplicationBlueprintEditor({
  draft,
  onChange,
}: {
  draft: AgentDraft;
  onChange: (next: AgentDraft) => void;
}) {
  const application = applicationContractFor(draft);
  const update = (patch: Partial<AgentApplicationContract>) =>
    onChange({ ...draft, application: { ...application, ...patch } });

  return (
    <section className="grid gap-4 rounded-lg border border-sky-400/20 bg-sky-400/5 p-4">
      <div>
        <div className="text-sm font-semibold text-editor-foreground">应用蓝图</div>
        <p className="mt-1 text-xs leading-5 text-editor-muted">
          先定义业务结果、输入输出和边界；模型、工具与 MCP 是这份契约的执行配置。
        </p>
      </div>

      <div className="grid gap-1.5">
        <Label htmlFor="application-objective" className="text-editor-muted">
          业务目标
        </Label>
        <Textarea
          id="application-objective"
          rows={2}
          value={application.objective}
          onChange={(event) => update({ objective: event.target.value })}
          placeholder="这个应用要产生什么可验证的业务结果？"
          className="border-white/10 bg-editor-surface-raised text-editor-foreground placeholder:text-editor-faint"
        />
      </div>

      <div className="grid gap-1.5">
        <Label htmlFor="application-mode" className="text-editor-muted">
          运行方式
        </Label>
        <select
          id="application-mode"
          value={application.interaction_mode}
          onChange={(event) =>
            update({
              interaction_mode: event.target.value as AgentInteractionMode,
            })
          }
          className="h-10 rounded-md border border-white/10 bg-editor-surface-raised px-3 text-sm text-editor-foreground outline-none focus:ring-2 focus:ring-ring/50"
        >
          {MODES.map((mode) => (
            <option key={mode.value} value={mode.value}>
              {mode.label} · {mode.detail}
            </option>
          ))}
        </select>
      </div>

      <LineListEditor
        label="使用者"
        values={application.audience}
        placeholder="例如：客服负责人"
        onChange={(audience) => update({ audience })}
      />

      <InputListEditor values={application.inputs} onChange={(inputs) => update({ inputs })} />

      <OutputListEditor values={application.outputs} onChange={(outputs) => update({ outputs })} />

      {application.outputs.some((output) => output.type === "interactive_dashboard") ? (
        <DashboardEditor
          value={application.dashboard ?? defaultDashboard()}
          onChange={(dashboard) => update({ dashboard })}
        />
      ) : (
        <Button
          type="button"
          variant="outline"
          onClick={() =>
            update({
              outputs: [
                ...application.outputs,
                { type: "interactive_dashboard", description: "可筛选、可复核的数据大屏。" },
              ],
              dashboard: defaultDashboard(),
            })
          }
          className="justify-start border-sky-300/20 bg-sky-300/5 text-editor-foreground hover:bg-sky-300/10 hover:text-white"
        >
          <BarChart3 className="size-4" />
          添加大屏应用输出
        </Button>
      )}

      <div className="grid gap-3 sm:grid-cols-2">
        <LineListEditor
          label="明确不做"
          values={application.non_goals}
          placeholder="例如：不直接发送邮件"
          onChange={(nonGoals) => update({ non_goals: nonGoals })}
        />
        <LineListEditor
          label="完成条件"
          values={application.completion_criteria}
          placeholder="例如：每项输入都有可复核结果"
          onChange={(completionCriteria) => update({ completion_criteria: completionCriteria })}
        />
      </div>

      <div className="grid gap-1.5">
        <Label htmlFor="application-failure" className="text-editor-muted">
          失败处理
        </Label>
        <Input
          id="application-failure"
          value={application.failure_behavior}
          onChange={(event) => update({ failure_behavior: event.target.value })}
          placeholder="依赖不可用时如何暂停、通知或降级？"
          className="border-white/10 bg-editor-surface-raised text-editor-foreground placeholder:text-editor-faint"
        />
      </div>
    </section>
  );
}

function LineListEditor({
  label,
  values,
  placeholder,
  onChange,
}: {
  label: string;
  values: string[];
  placeholder: string;
  onChange: (next: string[]) => void;
}) {
  return (
    <div className="grid gap-1.5">
      <Label className="text-editor-muted">{label}</Label>
      <Textarea
        rows={3}
        value={values.join("\n")}
        onChange={(event) => onChange(lines(event.target.value))}
        placeholder={`${placeholder}\n每行一项`}
        className="border-white/10 bg-editor-surface-raised text-editor-foreground placeholder:text-editor-faint"
      />
    </div>
  );
}

function InputListEditor({
  values,
  onChange,
}: {
  values: AgentApplicationInput[];
  onChange: (next: AgentApplicationInput[]) => void;
}) {
  return (
    <BlueprintList
      title="输入"
      addLabel="添加输入"
      onAdd={() => onChange([...values, { type: "request", source: "", description: "" }])}
    >
      {values.map((input, index) => (
        <div
          key={index}
          className="grid gap-2 rounded-md border border-white/10 bg-black/10 p-3 sm:grid-cols-[0.7fr_0.9fr_1.6fr_auto]"
        >
          <Input
            value={input.type}
            aria-label={`输入 ${index + 1} 类型`}
            placeholder="类型"
            onChange={(event) => onChange(replace(values, index, { ...input, type: event.target.value }))}
            className="border-white/10 bg-editor-surface-raised"
          />
          <Input
            value={input.source}
            aria-label={`输入 ${index + 1} 来源`}
            placeholder="来源"
            onChange={(event) =>
              onChange(
                replace(values, index, {
                  ...input,
                  source: event.target.value,
                }),
              )
            }
            className="border-white/10 bg-editor-surface-raised"
          />
          <Input
            value={input.description}
            aria-label={`输入 ${index + 1} 说明`}
            placeholder="具体输入内容"
            onChange={(event) =>
              onChange(
                replace(values, index, {
                  ...input,
                  description: event.target.value,
                }),
              )
            }
            className="border-white/10 bg-editor-surface-raised"
          />
          <RemoveButton
            label={`删除输入 ${index + 1}`}
            onClick={() => onChange(values.filter((_, itemIndex) => itemIndex !== index))}
          />
        </div>
      ))}
    </BlueprintList>
  );
}

function OutputListEditor({
  values,
  onChange,
}: {
  values: AgentApplicationOutput[];
  onChange: (next: AgentApplicationOutput[]) => void;
}) {
  return (
    <BlueprintList
      title="输出"
      addLabel="添加输出"
      onAdd={() => onChange([...values, { type: "result", description: "" }])}
    >
      {values.map((output, index) => (
        <div
          key={index}
          className="grid gap-2 rounded-md border border-white/10 bg-black/10 p-3 sm:grid-cols-[0.8fr_2fr_auto]"
        >
          <Input
            value={output.type}
            aria-label={`输出 ${index + 1} 类型`}
            placeholder="类型"
            onChange={(event) => onChange(replace(values, index, { ...output, type: event.target.value }))}
            className="border-white/10 bg-editor-surface-raised"
          />
          <Input
            value={output.description}
            aria-label={`输出 ${index + 1} 说明`}
            placeholder="可复核的交付结果"
            onChange={(event) =>
              onChange(
                replace(values, index, {
                  ...output,
                  description: event.target.value,
                }),
              )
            }
            className="border-white/10 bg-editor-surface-raised"
          />
          <RemoveButton
            label={`删除输出 ${index + 1}`}
            onClick={() => onChange(values.filter((_, itemIndex) => itemIndex !== index))}
          />
        </div>
      ))}
    </BlueprintList>
  );
}

function DashboardEditor({
  value,
  onChange,
}: {
  value: AgentDashboardDefinition;
  onChange: (next: AgentDashboardDefinition) => void;
}) {
  return (
    <div className="grid gap-3 rounded-lg border border-cyan-300/20 bg-cyan-300/5 p-4">
      <div>
        <div className="flex items-center gap-2 text-sm font-semibold text-editor-foreground">
          <BarChart3 className="size-4 text-cyan-300" />
          大屏应用配置
        </div>
        <p className="mt-1 text-xs leading-5 text-editor-muted">
          智能体运行后输出 metrics 对象和 rows 数组，平台将自动渲染指标、趋势和明细。
        </p>
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="grid gap-1.5">
          <Label className="text-editor-muted">大屏标题</Label>
          <Input
            value={value.title}
            onChange={(event) => onChange({ ...value, title: event.target.value })}
            placeholder="例如：经营分析驾驶舱"
            className="border-white/10 bg-editor-surface-raised"
          />
        </div>
        <div className="grid gap-1.5">
          <Label className="text-editor-muted">展示模板</Label>
          <select
            value={value.template}
            onChange={(event) =>
              onChange({
                ...value,
                template: event.target.value as AgentDashboardDefinition["template"],
              })
            }
            className="h-10 rounded-md border border-white/10 bg-editor-surface-raised px-3 text-sm text-editor-foreground"
          >
            <option value="analysis">分析看板</option>
            <option value="operations">运营监控</option>
            <option value="executive">管理驾驶舱</option>
          </select>
        </div>
      </div>
      <div className="grid gap-1.5">
        <Label className="text-editor-muted">用途说明</Label>
        <Input
          value={value.description}
          onChange={(event) => onChange({ ...value, description: event.target.value })}
          placeholder="说明大屏帮助用户判断什么"
          className="border-white/10 bg-editor-surface-raised"
        />
      </div>
      <div className="grid gap-3 sm:grid-cols-3">
        <LineListEditor
          label="关键指标"
          values={value.metrics}
          placeholder="例如：销售额"
          onChange={(metrics) => onChange({ ...value, metrics })}
        />
        <LineListEditor
          label="分析维度"
          values={value.dimensions}
          placeholder="例如：日期"
          onChange={(dimensions) => onChange({ ...value, dimensions })}
        />
        <LineListEditor
          label="展示组件"
          values={value.visualizations}
          placeholder="例如：趋势图"
          onChange={(visualizations) => onChange({ ...value, visualizations })}
        />
      </div>
    </div>
  );
}

function defaultDashboard(): AgentDashboardDefinition {
  return {
    title: "数据分析大屏",
    description: "展示智能体运行产生的关键指标、趋势和明细数据。",
    template: "analysis",
    metrics: ["总量", "成功量", "异常量"],
    dimensions: ["时间", "类别"],
    visualizations: ["指标卡", "趋势图", "明细表"],
  };
}

function BlueprintList({
  title,
  addLabel,
  onAdd,
  children,
}: {
  title: string;
  addLabel: string;
  onAdd: () => void;
  children: React.ReactNode;
}) {
  return (
    <div className="grid gap-2">
      <div className="flex items-center justify-between gap-3">
        <Label className="text-editor-muted">{title}</Label>
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={onAdd}
          className="h-7 border-white/10 bg-white/5 text-xs text-editor-foreground hover:bg-white/10 hover:text-white"
        >
          <Plus className="size-3" />
          {addLabel}
        </Button>
      </div>
      {children}
    </div>
  );
}

function RemoveButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <Button
      type="button"
      size="icon-sm"
      variant="ghost"
      aria-label={label}
      onClick={onClick}
      className="text-editor-faint hover:bg-white/10 hover:text-white"
    >
      <X className="size-3.5" />
    </Button>
  );
}

function lines(value: string): string[] {
  return value
    .split("\n")
    .map((item) => item.trim())
    .filter(Boolean);
}

function replace<T>(values: T[], index: number, value: T): T[] {
  return values.map((item, itemIndex) => (itemIndex === index ? value : item));
}
