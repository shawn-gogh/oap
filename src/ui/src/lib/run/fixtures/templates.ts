// Agent templates for Stage 3's "create a Run" demo flow — decoupled from
// the 8 already-tested run fixtures in fixtures/{a2a,openapi,...,scenarios}.ts
// so this slice can't regress Stage 2. Used by both RunInputForm (to render
// the input form for a schema) and fixture-client's createRun (to know what
// kind of result to synthesize once submitted).

import type { JsonSchema, RunArtifact } from "../types";

export interface RunAgentTemplate {
  agentId: string;
  agentName: string;
  providerName: string;
  inputSchema: JsonSchema | null;
  toolLabel: string;
  resultText: string;
  artifact: Pick<RunArtifact, "name" | "mediaType" | "inline">;
}

const supportedSchema: JsonSchema = {
  type: "object",
  required: ["title", "priority"],
  properties: {
    title: { type: "string", title: "任务标题" },
    details: { type: "string", title: "详细说明" },
    estimatedHours: { type: "integer", title: "预计工时（小时）" },
    urgent: { type: "boolean", title: "是否紧急" },
    priority: { type: "string", title: "优先级", enum: ["低", "中", "高"] },
    tags: { type: "array", title: "标签", items: { type: "string" } },
    attachment: {
      type: "string",
      title: "参考附件",
      contentMediaType: "application/octet-stream",
    },
    contact: {
      type: "object",
      title: "联系人",
      properties: {
        name: { type: "string", title: "姓名" },
        email: { type: "string", title: "邮箱" },
      },
    },
  },
};

// oneOf makes this fall outside the supported subset by design — proves the
// "unsupported structures use a raw JSON editor" fallback path.
const unsupportedSchema: JsonSchema = {
  type: "object",
  properties: {
    target: {
      oneOf: [
        { type: "string", title: "URL" },
        { type: "object", properties: { host: { type: "string" }, port: { type: "integer" } } },
      ],
    },
  },
};

export const RUN_AGENT_TEMPLATES: RunAgentTemplate[] = [
  {
    agentId: "agent_template_supported",
    agentName: "结构化任务智能体",
    providerName: "openapi",
    inputSchema: supportedSchema,
    toolLabel: "创建任务",
    resultText: "已按提交的结构化输入创建任务，并生成了一份摘要。",
    artifact: {
      name: "task-summary.md",
      mediaType: "text/markdown",
      inline: "# 任务摘要\n\n已根据提交内容创建任务。",
    },
  },
  {
    agentId: "agent_template_unsupported",
    agentName: "自定义目标智能体",
    providerName: "openapi",
    inputSchema: unsupportedSchema,
    toolLabel: "解析目标",
    resultText: "已按提交的原始 JSON 输入解析目标并执行。",
    artifact: {
      name: "target-resolution.json",
      mediaType: "application/json",
      inline: { resolved: true },
    },
  },
  {
    agentId: "agent_template_freeform",
    agentName: "自由文本智能体",
    providerName: "a2a",
    inputSchema: null,
    toolLabel: "处理请求",
    resultText: "已处理提交的自由文本请求。",
    artifact: {
      name: "response.txt",
      mediaType: "text/plain",
      inline: "已完成。",
    },
  },
];

export function findRunAgentTemplate(agentId: string): RunAgentTemplate | undefined {
  return RUN_AGENT_TEMPLATES.find((template) => template.agentId === agentId);
}
