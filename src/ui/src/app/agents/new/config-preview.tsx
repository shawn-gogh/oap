"use client";

import { Badge } from "@/components/ui/badge";
import { EditorChip } from "@/components/editor-chip";
import type { AgentDraft } from "@/lib/agent-builder";
import type { Integration } from "@/lib/integrations";
import { scheduleLabel } from "@/lib/schedule";

export function ConfigPreview({ draft, mcpIntegrations }: { draft: AgentDraft; mcpIntegrations: Integration[] }) {
  const selectedMcpIntegrations = draft.mcp_server_ids.map((id) => {
    const integration = mcpIntegrations.find((item) => item.id === id);
    return (
      integration ?? {
        id,
        name: id,
        description: "未知的 MCP 服务器。",
        category: "其他",
        envKey: "未知",
        mcpUrl: "",
        tools: [],
        source: "catalog" as const,
        connected: false,
        status: null,
      }
    );
  });

  return (
    <div className="min-h-0 flex-1 overflow-y-auto rounded-b-lg bg-editor-surface px-5 py-4">
      <div className="grid gap-5">
        <div>
          <div className="text-xs uppercase text-editor-faint">名称</div>
          <div className="mt-1 text-xl font-semibold text-editor-foreground">{draft.name}</div>
          <p className="mt-2 max-w-2xl text-sm leading-6 text-editor-muted">{draft.description}</p>
        </div>

        <div className="grid gap-3 sm:grid-cols-2">
          <PreviewItem label="模型" value={draft.model} />
          <PreviewItem label="运行时" value={draft.runtime} />
          <PreviewItem label="调度计划" value={scheduleLabel(draft.cron, draft.timezone)} />
          <PreviewItem
            label="工具"
            value={draft.tools
              .map((tool) => tool.type)
              .filter(Boolean)
              .join(", ")}
          />
        </div>

        {draft.application && (
          <div className="grid gap-3 rounded-lg border border-sky-300/20 bg-sky-300/5 p-4">
            <div>
              <div className="text-xs uppercase text-editor-faint">应用蓝图</div>
              <p className="mt-2 text-sm leading-6 text-editor-foreground">
                {draft.application.objective || "未定义业务目标。"}
              </p>
            </div>
            <div className="grid gap-3 sm:grid-cols-2">
              <PreviewItem label="运行方式" value={interactionModeLabel(draft.application.interaction_mode)} />
              <PreviewItem label="使用者" value={draft.application.audience.join(", ")} />
              <TokenList
                label="输入"
                values={draft.application.inputs
                  .map((input) => [input.source, input.description].filter(Boolean).join(": "))
                  .filter(Boolean)}
              />
              <TokenList
                label="输出"
                values={draft.application.outputs.map((output) => output.description || output.type).filter(Boolean)}
              />
              <TokenList label="明确不做" values={draft.application.non_goals} />
              <TokenList label="完成条件" values={draft.application.completion_criteria} />
            </div>
            <PreviewItem label="失败处理" value={draft.application.failure_behavior} />
            {draft.application.dashboard && (
              <div className="grid gap-3 rounded-lg border border-cyan-300/20 bg-cyan-300/5 p-3 sm:grid-cols-2">
                <PreviewItem label="大屏标题" value={draft.application.dashboard.title} />
                <PreviewItem
                  label="大屏模板"
                  value={dashboardTemplateLabel(draft.application.dashboard.template)}
                />
                <TokenList label="关键指标" values={draft.application.dashboard.metrics} />
                <TokenList label="分析维度" values={draft.application.dashboard.dimensions} />
                <TokenList label="展示组件" values={draft.application.dashboard.visualizations} />
              </div>
            )}
          </div>
        )}

        <div>
          <div className="text-xs uppercase text-editor-faint">系统提示词</div>
          <pre className="mt-2 max-h-80 overflow-y-auto whitespace-pre-wrap rounded-lg border border-white/10 bg-black/15 p-3 font-mono text-xs leading-6 text-editor-accent">
            {draft.system || "未设置系统提示词。"}
          </pre>
        </div>

        <div className="grid gap-3 sm:grid-cols-2">
          <TokenList label="保险库密钥" values={draft.vault_keys} />
          <TokenList label="技能 ID" values={draft.skill_ids} />
          <TokenList label="规则 ID" values={draft.rule_ids} />
          <TokenList label="子智能体" values={draft.sub_agents.map((agent) => agent.agent_id)} />
        </div>

        <div className="rounded-lg border border-white/10 bg-black/10 p-3">
          <div className="text-xs uppercase text-editor-faint">MCP 集成</div>
          {selectedMcpIntegrations.length === 0 ? (
            <div className="mt-2 text-xs text-editor-muted">无</div>
          ) : (
            <div className="mt-3 grid gap-2">
              {selectedMcpIntegrations.map((integration) => {
                const toolCount = integration.tools.filter(Boolean).length;
                return (
                  <div key={integration.id} className="rounded-md border border-white/10 bg-white/5 px-2.5 py-2">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-xs font-medium text-editor-foreground">{integration.name}</span>
                      <span className="font-mono text-[11px] text-editor-faint">{integration.id}</span>
                      <Badge
                        variant="outline"
                        className="h-5 rounded-md border-white/10 bg-white/5 text-[11px] text-editor-muted"
                      >
                        {toolCount > 0 ? `${toolCount} 个工具` : "已挂载工具集"}
                      </Badge>
                    </div>
                    <p className="mt-1 line-clamp-2 text-xs text-editor-muted">{integration.description}</p>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function PreviewItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-white/10 bg-black/10 p-3">
      <div className="text-xs uppercase text-editor-faint">{label}</div>
      <div className="mt-1 break-words font-mono text-xs text-editor-foreground">{value || "未设置"}</div>
    </div>
  );
}

function interactionModeLabel(value: string): string {
  if (value === "conversational") return "对话应用";
  if (value === "scheduled") return "定时应用";
  if (value === "event_driven") return "事件应用";
  if (value === "manual") return "人工运行";
  return value;
}

function dashboardTemplateLabel(value: string): string {
  if (value === "analysis") return "分析看板";
  if (value === "operations") return "运营监控";
  if (value === "executive") return "管理驾驶舱";
  return value;
}

function TokenList({ label, values }: { label: string; values: string[] }) {
  return (
    <div className="rounded-lg border border-white/10 bg-black/10 p-3">
      <div className="text-xs uppercase text-editor-faint">{label}</div>
      {values.length === 0 ? (
        <div className="mt-2 text-xs text-editor-muted">无</div>
      ) : (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {values.map((value) => (
            <EditorChip key={value} className="rounded-md text-editor-foreground">
              {value}
            </EditorChip>
          ))}
        </div>
      )}
    </div>
  );
}
