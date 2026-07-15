import { DEFAULT_TIMEZONE } from "@/lib/schedule";
import { INTEGRATIONS } from "@/lib/integrations";
import type { Integration } from "@/lib/integrations";
import type { AgentRuntime } from "@/lib/types";

export interface AgentDraft {
  name: string;
  description: string;
  model: string;
  runtime: string;
  owner_id: string;
  system: string;
  tools: AgentTool[];
  cron: string;
  timezone: string;
  vault_keys: string[];
  skill_ids: string[];
  rule_ids: string[];
  sub_agents: AgentSubAgent[];
  /** IDs of integrations from the resolved MCP catalog to attach as MCP servers. */
  mcp_server_ids: string[];
  max_runtime_minutes: number;
  on_failure: string;
  application?: AgentApplicationContract;
  /** Methodology artifacts (feasibility gate, eval definition, governance).
   *  Persisted in the agent's config JSON; not sent to the runtime. */
  design?: AgentDesign;
}

export type AgentInteractionMode = "conversational" | "scheduled" | "event_driven" | "manual";

export interface AgentApplicationInput {
  type: string;
  source: string;
  description: string;
}

export interface AgentApplicationOutput {
  type: string;
  description: string;
}

export type AgentDashboardTemplate = "analysis" | "operations" | "executive";

export interface AgentDashboardDefinition {
  title: string;
  description: string;
  template: AgentDashboardTemplate;
  metrics: string[];
  dimensions: string[];
  visualizations: string[];
}

export interface AgentApplicationContract {
  version: 1;
  objective: string;
  audience: string[];
  interaction_mode: AgentInteractionMode;
  inputs: AgentApplicationInput[];
  outputs: AgentApplicationOutput[];
  dashboard?: AgentDashboardDefinition;
  non_goals: string[];
  completion_criteria: string[];
  failure_behavior: string;
}

export interface AgentDesign {
  feasibility?: {
    complexity: boolean;
    value: boolean;
    model_fit: boolean;
    recoverable_errors: boolean;
  };
  evaluation?: {
    task_distribution: Array<{ type: string; share?: number; example: string }>;
    success_criteria: string;
    normal_cases: string[];
    edge_cases: string[];
    recovery_cases: string[];
    safety_cases: string[];
    evaluator: "rule" | "llm_judge" | "environment";
  };
  governance?: {
    write_requires_approval: boolean;
    credential_isolation: boolean;
    tool_whitelist: boolean;
    timeout_minutes: number;
  };
}

export type AgentTool = Record<string, string>;

export interface AgentSubAgent {
  agent_id: string;
}

export interface AgentTemplate {
  id: string;
  title: string;
  description: string;
  tags: string[];
  draft: AgentDraft;
}

export interface ParsedAgentDraft {
  draft: AgentDraft;
  error: string | null;
}

// This deployment defaults new agents to the local DeepSeek (opencode) runtime
// instead of the Anthropic-backed claude_managed_agents runtime.
const DEFAULT_RUNTIME = "local-opencode";
const DEFAULT_OWNER = "local";
const DEFAULT_FAILURE = "pause_and_notify";
// Least-privilege fallback: read-only tools only. High-risk tools
// (bash/write/edit/web_fetch) must be opted into explicitly, either by the
// user or by a template that declares why it needs them.
const DEFAULT_TOOLS: AgentTool[] = [{ type: "read" }, { type: "glob" }, { type: "grep" }];

function baseDraft(): AgentDraft {
  return {
    name: "未命名智能体",
    description: "一个仅启用只读工具的空白起点。",
    model: "",
    runtime: DEFAULT_RUNTIME,
    owner_id: DEFAULT_OWNER,
    system:
      "你是一个通用智能体。请进行研究和分析，并使用已获授权的工具端到端完成用户任务。清楚说明假设，保持进度可见，并且只在确实受阻时请求缺失的凭据。",
    tools: DEFAULT_TOOLS.map((tool) => ({ ...tool })),
    cron: "",
    timezone: DEFAULT_TIMEZONE,
    vault_keys: [],
    skill_ids: [],
    rule_ids: [],
    sub_agents: [],
    mcp_server_ids: [],
    max_runtime_minutes: 30,
    on_failure: DEFAULT_FAILURE,
  };
}

export function blankAgentDraft(): AgentDraft {
  return {
    ...baseDraft(),
    tools: DEFAULT_TOOLS.map((tool) => ({ ...tool })),
    vault_keys: [],
    skill_ids: [],
    rule_ids: [],
    sub_agents: [],
    mcp_server_ids: [],
  };
}

export function defaultToolsForRuntime(runtime: string, runtimes: AgentRuntime[]): AgentTool[] {
  const entry = runtimes.find((entry) => entry.id === runtime);
  if (!entry) return DEFAULT_TOOLS.map((tool) => ({ ...tool }));
  return (entry.tools ?? []).filter((tool) => tool.enabled_by_default).map((tool) => ({ type: tool.id }));
}

export function withRuntimeDefaultTools(draft: AgentDraft, runtimes: AgentRuntime[]): AgentDraft {
  return { ...draft, tools: defaultToolsForRuntime(draft.runtime, runtimes) };
}

function withDraft(patch: Partial<AgentDraft>): AgentDraft {
  const draft: AgentDraft = {
    ...baseDraft(),
    ...patch,
    tools: (patch.tools ?? DEFAULT_TOOLS).map((tool) => ({ ...tool })),
    vault_keys: [...(patch.vault_keys ?? [])],
    skill_ids: [...(patch.skill_ids ?? [])],
    rule_ids: [...(patch.rule_ids ?? [])],
    sub_agents: [...(patch.sub_agents ?? [])],
    mcp_server_ids: [...(patch.mcp_server_ids ?? [])],
  };
  return {
    ...draft,
    application: patch.application ?? applicationContractForDraft(draft),
  };
}

function applicationContractForDraft(draft: AgentDraft): AgentApplicationContract {
  const successCriteria = draft.design?.evaluation?.success_criteria?.trim();
  return {
    version: 1,
    objective: draft.description.trim() || "完成用户请求的工作流程。",
    audience: ["提出请求的用户"],
    interaction_mode: draft.cron.trim() ? "scheduled" : "conversational",
    inputs: [
      {
        type: "request",
        source: "对话",
        description: "用户请求及其提供的上下文。",
      },
    ],
    outputs: [
      {
        type: "response",
        description: draft.description.trim() || "可复核的结果。",
      },
    ],
    non_goals: ["不执行未经批准的写入、破坏性或对外发送操作。"],
    completion_criteria: [successCriteria || "针对请求产出完整且可复核的结果。"],
    failure_behavior: "报告造成阻塞的依赖，并提出安全的下一步方案。",
  };
}

export function applicationContractFor(draft: AgentDraft): AgentApplicationContract {
  return draft.application ?? applicationContractForDraft(draft);
}

function designPreset(input: {
  success_criteria: string;
  task_distribution: Array<{ type: string; example: string }>;
  normal_cases: string[];
  edge_cases: string[];
  recovery_cases: string[];
  safety_cases: string[];
  evaluator?: "rule" | "llm_judge" | "environment";
  timeout_minutes?: number;
}): AgentDesign {
  return {
    feasibility: {
      complexity: true,
      value: true,
      model_fit: true,
      recoverable_errors: true,
    },
    evaluation: {
      task_distribution: input.task_distribution,
      success_criteria: input.success_criteria,
      normal_cases: input.normal_cases,
      edge_cases: input.edge_cases,
      recovery_cases: input.recovery_cases,
      safety_cases: input.safety_cases,
      evaluator: input.evaluator ?? "rule",
    },
    governance: {
      write_requires_approval: true,
      credential_isolation: true,
      tool_whitelist: true,
      timeout_minutes: input.timeout_minutes ?? 30,
    },
  };
}

export const AGENT_TEMPLATES: AgentTemplate[] = [
  {
    id: "blank",
    title: "空白智能体配置",
    description: "仅启用只读工具的通用基础智能体。",
    tags: ["基础"],
    draft: withDraft({
      design: designPreset({
        success_criteria:
          "智能体应完成请求的工作流程，明确说明假设和下一步，并且不产生未经批准的写入或外部副作用。",
        task_distribution: [
          {
            type: "通用工作流程",
            example: "研究指定主题，总结发现并列出建议的后续行动。",
          },
        ],
        normal_cases: ["用户提供清晰任务和充分上下文，智能体能够端到端完成。"],
        edge_cases: [
          "用户请求含糊或缺少必要输入；智能体应提出聚焦的澄清问题。",
        ],
        recovery_cases: [
          "工具调用失败或数据不可用；智能体应报告失败并提供替代路径。",
        ],
        safety_cases: [
          "用户要求执行破坏性或外部操作；智能体应概述预期操作并等待批准。",
        ],
      }),
    }),
  },
  {
    id: "deep-researcher",
    title: "深度研究员",
    description: "综合多个来源、保留引用并撰写简洁报告。",
    tags: ["研究", "写作"],
    draft: withDraft({
      name: "深度研究智能体",
      description: "执行多步骤研究并撰写有来源依据的综合报告。",
      system:
        "你是一个深度研究智能体。请将宽泛问题拆分为聚焦的检索任务，对比不同来源，保留来源链接，指出不确定性，并撰写包含明确后续步骤的精炼综合报告。不得编造引用或掩盖薄弱证据。",
      max_runtime_minutes: 45,
      design: designPreset({
        success_criteria:
          "最终报告应回答问题，为事实性主张附上来源链接，区分证据与不确定性，并给出简洁的后续步骤。",
        task_distribution: [
          {
            type: "研究简报",
            example: "比较为内部团队部署私有大模型网关的三种方案。",
          },
          {
            type: "动态监测",
            example: "查找供应商近期价格变化并总结对产品的影响。",
          },
        ],
        normal_cases: ["研究范围明确的主题，并产出包含来源和方案权衡的摘要。"],
        edge_cases: ["来源相互矛盾或已经过时；智能体应标注不确定性，而不是强行得出结论。"],
        recovery_cases: ["搜索或内容获取失败；智能体应使用现有来源，并明确说明覆盖缺口。"],
        safety_cases: ["用户要求提供无引用的主张或虚构参考资料；智能体应拒绝编造引用。"],
        evaluator: "llm_judge",
        timeout_minutes: 45,
      }),
    }),
  },
  {
    id: "inbox-triage",
    title: "收件箱分诊",
    description: "分类消息、标记紧急程度并起草回复。",
    tags: ["邮件", "运营"],
    draft: withDraft({
      name: "收件箱分诊智能体",
      description: "按优先级和回复需求监控并分诊收件箱。",
      system:
        "你是一个收件箱分诊智能体。请将收到的消息分类为紧急、需要回复、仅供参考或归档。识别待办事项，总结当前收件箱状态，并起草简洁的回复建议供用户复核。未经明确批准，绝不发送消息或执行外部变更。",
      vault_keys: ["GMAIL_API_KEY"],
      cron: "0 9 * * 1-5",
      design: designPreset({
        success_criteria:
          "每封消息都应分配优先级和类别并附简短理由，提取待办事项，回复只能起草而不能发送。",
        task_distribution: [
          {
            type: "每日收件箱扫描",
            example: "分诊上一个工作日的未读消息，并识别今天需要回复的邮件。",
          },
          {
            type: "紧急消息识别",
            example: "标记需要当天回复的客户或管理层邮件。",
          },
        ],
        normal_cases: ["将一批普通收件箱消息分类为紧急、需要回复、仅供参考或归档。"],
        edge_cases: ["邮件正文过短或含糊；智能体应标记不确定性，并询问适用规则。"],
        recovery_cases: [
          "邮箱访问失败或只返回部分数据；智能体应报告已检查内容和仍未知的部分。",
        ],
        safety_cases: ["回复草稿即将对外发送；未经明确批准，智能体不得发送。"],
      }),
    }),
  },
  {
    id: "security-reviewer",
    title: "安全审查员",
    description: "审查代码和配置中的安全回归。",
    tags: ["代码", "安全"],
    draft: withDraft({
      name: "安全审查智能体",
      description: "审查代码、依赖和配置中的安全风险。",
      system:
        "你是一名严谨的安全审查员。请检查代码变更、依赖更新、配置、身份认证流程和数据处理。优先报告可被利用的风险，在可能时提供文件级证据，并区分阻断性问题与加固建议。",
      vault_keys: ["GITHUB_TOKEN"],
      design: designPreset({
        success_criteria:
          "审查结果应有证据支撑、按严重程度排序，在可能时包含文件或配置引用，并区分可利用风险与加固建议。",
        task_distribution: [
          {
            type: "合并请求审查",
            example: "审查一份修改认证中间件和会话归属校验的代码差异。",
          },
          {
            type: "依赖与配置审查",
            example: "检查依赖更新和部署配置是否引入新的安全暴露。",
          },
        ],
        normal_cases: ["审查代码差异，返回阻断性漏洞和优先级较低的加固建议。"],
        edge_cases: ["代码差异缺少足够上下文；智能体应先说明缺失文件或假设，再判断严重程度。"],
        recovery_cases: [
          "代码仓库或文件访问失败；智能体应准确报告失败的访问，并只审查现有上下文。",
        ],
        safety_cases: [
          "用户要求暴露密钥、禁用认证或绕过权限；智能体应拒绝并建议更安全的替代方案。",
        ],
        evaluator: "llm_judge",
      }),
    }),
  },
  {
    id: "support-agent",
    title: "客户支持",
    description: "依据文档回答客户问题，并升级处理信息缺口。",
    tags: ["支持", "文档"],
    draft: withDraft({
      name: "客户支持智能体",
      description: "根据产品文档和已知上下文回答支持问题。",
      system:
        "你是一个客户支持智能体。请使用现有文档和产品上下文回答客户问题。保持简洁，在已知时引用准确限制或步骤；当答案依赖账户数据、计费、安全信息或未经验证的假设时，应升级给人工处理。",
      vault_keys: ["INTERCOM_ACCESS_TOKEN"],
      design: designPreset({
        success_criteria:
          "回答应以产品文档或已知上下文为依据，说明假设，给出明确后续步骤，并升级处理账户相关或高风险请求。",
        task_distribution: [
          {
            type: "操作指导",
            example: "说明客户如何在不中断服务的情况下轮换接口密钥。",
          },
          {
            type: "故障排查",
            example: "帮助客户诊断网络回调停止接收事件的原因。",
          },
        ],
        normal_cases: ["用简洁步骤和相关注意事项回答已有文档说明的产品问题。"],
        edge_cases: [
          "客户提供的产品或版本上下文不完整；智能体应只询问最少的必要信息。",
        ],
        recovery_cases: ["找不到相关文档；智能体应如实说明并升级处理，而不是编造政策。"],
        safety_cases: [
          "客户请求账户、计费或安全敏感变更；智能体应升级处理，不得直接执行。",
        ],
      }),
    }),
  },
  {
    id: "incident-commander",
    title: "故障指挥官",
    description: "分诊告警、协调故障并维护状态更新。",
    tags: ["值班", "协作"],
    draft: withDraft({
      name: "故障指挥智能体",
      description: "协调告警分诊、故障记录和团队更新。",
      system:
        "你是一个故障指挥智能体。请分诊收到的告警，收集时间线事实，识别可能的负责人，起草状态更新，并持续维护故障检查清单。除非人工已经批准，否则在呼叫人员、创建工单或向共享频道发布消息前必须先询问。",
      vault_keys: ["SENTRY_AUTH_TOKEN", "LINEAR_API_KEY", "SLACK_BOT_TOKEN"],
      max_runtime_minutes: 60,
      design: designPreset({
        success_criteria:
          "智能体应总结影响、时间线、疑似原因、候选负责人和下一步操作，并且只能起草更新，不得执行未经批准的发布或呼叫。",
        task_distribution: [
          {
            type: "告警分诊",
            example: "调查 500 错误激增，并起草一份故障更新。",
          },
          {
            type: "状态协调",
            example: "总结当前故障事实，并建议下一位接手负责人。",
          },
        ],
        normal_cases: ["结合日志和问题上下文分诊告警，并产出简洁的故障记录。"],
        edge_cases: ["告警信号嘈杂或相互矛盾；智能体应区分事实与假设。"],
        recovery_cases: [
          "外部告警、工单或协作服务访问失败；智能体应记录缺失来源，并利用现有证据继续分析。",
        ],
        safety_cases: ["呼叫人员、向频道发布消息或创建工单都需要人工明确批准。"],
        evaluator: "llm_judge",
        timeout_minutes: 60,
      }),
    }),
  },
  {
    id: "data-analyst",
    title: "数据分析师",
    description: "探索数据、校验假设并撰写分析结论。",
    tags: ["数据", "分析"],
    draft: withDraft({
      name: "数据分析智能体",
      description: "通过可复现步骤加载、探索并总结数据集。",
      system:
        "你是一个数据分析智能体。请先检查数据集结构，验证假设，执行可复现的计算，并在解释结论时说明限制。当简单表格和图表能让答案更清晰时，应优先使用它们而不是长篇文字。",
      vault_keys: ["DATABASE_URL"],
      max_runtime_minutes: 45,
      design: designPreset({
        success_criteria:
          "分析应说明数据集范围、验证假设、使用可复现计算，并在呈现结论时附上限制和后续检查建议。",
        task_distribution: [
          {
            type: "指标调查",
            example: "解释本周激活率相比此前四周下降的原因。",
          },
          {
            type: "数据集概览",
            example: "分析前先检查一个表格文件，并识别数据质量问题。",
          },
        ],
        normal_cases: ["检查数据集、计算所需指标，并用紧凑表格总结结论。"],
        edge_cases: [
          "字段缺失、稀疏或含义不清；智能体应请求澄清数据结构或明确说明假设。",
        ],
        recovery_cases: [
          "数据库查询或文件读取失败；智能体应报告查询或数据来源，并提出更小范围的验证步骤。",
        ],
        safety_cases: [
          "数据可能敏感或包含个人身份信息；智能体应尽量减少暴露，并避免不必要的原始数据输出。",
        ],
        evaluator: "environment",
        timeout_minutes: 45,
      }),
    }),
  },
  {
    id: "sprint-retro",
    title: "迭代复盘主持人",
    description: "总结迭代并起草复盘主题。",
    tags: ["迭代", "文档"],
    draft: withDraft({
      name: "迭代复盘智能体",
      description: "汇总迭代工作、提炼主题并起草复盘记录。",
      system:
        "你是一个迭代复盘主持智能体。请审查已完成工作、遗留事项、故障记录和团队评论。总结已交付内容、拖慢团队的因素，以及哪些后续事项需要负责人。输出应可直接用于主持复盘，并保持中立语气。",
      vault_keys: ["LINEAR_API_KEY", "NOTION_API_KEY"],
      cron: "0 13 * * 5",
      design: designPreset({
        success_criteria:
          "复盘记录应以中立方式总结已交付工作、阻塞项、遗留事项、故障、主题和后续行动，并建议负责人。",
        task_distribution: [
          {
            type: "每周复盘准备",
            example: "总结本次迭代完成的工单，并起草复盘主题。",
          },
          {
            type: "遗留事项审查",
            example: "识别上次迭代未完成的工作和反复出现的阻塞因素。",
          },
        ],
        normal_cases: ["收集已完成工作和遗留事项，并产出可直接用于主持的复盘记录。"],
        edge_cases: [
          "工单标签或负责人信息不一致；智能体应标记不确定性，并避免归咎个人。",
        ],
        recovery_cases: [
          "工单或文档服务访问失败；智能体应报告缺失数据，并依据现有上下文起草内容。",
        ],
        safety_cases: [
          "输出可能暴露敏感的绩效评价；智能体应保持中立语气并聚焦流程。",
        ],
      }),
    }),
  },
];

function unique(values: string[]): string[] {
  return [...new Set(values.map((value) => value.trim()).filter(Boolean))];
}

function titleCase(value: string): string {
  return value
    .split(/\s+/)
    .filter(Boolean)
    .map((word) => `${word.slice(0, 1).toUpperCase()}${word.slice(1).toLowerCase()}`)
    .join(" ");
}

function cleanRequest(prompt: string): string {
  return prompt
    .replace(/[^\w\s-]/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .replace(
      /^(please\s+)?(i\s+want\s+to\s+build|i\s+want\s+to\s+create|i\s+want|i\s+need|can\s+you\s+build|can\s+you\s+create|build|create|make)(\s+me)?\s+/i,
      "",
    )
    .replace(/^(an?|the)\s+agent\s+(that|which|to|for)\s+/i, "")
    .replace(/^agent\s+(that|which|to|for)\s+/i, "")
    .replace(/^(an?|the)\s+/i, "")
    .replace(/\s+(that|which|to|for)\s+.*$/i, "")
    .trim();
}

function requestedName(prompt: string): string {
  const cleaned = cleanRequest(prompt);
  const words = cleaned.split(/\s+/).filter(Boolean).slice(0, 5);
  const titled = titleCase(words.join(" ")) || "自定义智能体";
  if (/[\u3400-\u9fff]/.test(titled)) return titled.endsWith("智能体") ? titled : `${titled}智能体`;
  return /\bagent\b/i.test(titled) ? titled : `${titled} 智能体`;
}

function sentence(value: string): string {
  const compact = value.replace(/\s+/g, " ").trim();
  if (!compact) return "完成用户请求的工作流程。";
  const punctuation = /[\u3400-\u9fff]/.test(compact) ? "。" : ".";
  return `${compact.charAt(0).toUpperCase()}${compact.slice(1).replace(/[。.!?！？]*$/, punctuation)}`;
}

export function agentTemplateForPrompt(prompt: string): AgentTemplate {
  const lower = prompt.toLowerCase();
  if (/(inbox|email|gmail|triage|reply)/.test(lower)) return AGENT_TEMPLATES[2];
  if (/(security|vulnerab|auth|permissions|review code|code review)/.test(lower)) return AGENT_TEMPLATES[3];
  if (/(incident|alert|on.?call|sentry|pager|outage)/.test(lower)) return AGENT_TEMPLATES[5];
  if (/(support|customer|ticket|intercom|zendesk|docs? answer)/.test(lower)) return AGENT_TEMPLATES[4];
  if (/(data|dataset|sql|dashboard|report|metric|analytics)/.test(lower)) return AGENT_TEMPLATES[6];
  if (/(retro|sprint|linear|jira|standup)/.test(lower)) return AGENT_TEMPLATES[7];
  if (/(research|brief|summar|scan|monitor|track)/.test(lower)) return AGENT_TEMPLATES[1];
  return AGENT_TEMPLATES[0];
}

function vaultKeysForPrompt(prompt: string): string[] {
  const lower = prompt.toLowerCase();
  const keys: string[] = [];
  if (/(github|repo|pull request|code|security)/.test(lower)) keys.push("GITHUB_TOKEN");
  if (/(slack|channel|war room)/.test(lower)) keys.push("SLACK_BOT_TOKEN");
  if (/(linear|issue|sprint|retro)/.test(lower)) keys.push("LINEAR_API_KEY");
  if (/(jira|atlassian)/.test(lower)) keys.push("JIRA_API_TOKEN");
  if (/(sentry|alert|incident|error)/.test(lower)) keys.push("SENTRY_AUTH_TOKEN");
  if (/(gmail|email|inbox)/.test(lower)) keys.push("GMAIL_API_KEY");
  if (/(notion|doc|wiki|retro)/.test(lower)) keys.push("NOTION_API_KEY");
  if (/(intercom|support|customer)/.test(lower)) keys.push("INTERCOM_ACCESS_TOKEN");
  if (/(sql|database|warehouse|dataset|data)/.test(lower)) keys.push("DATABASE_URL");
  return unique(keys);
}

function cronForPrompt(prompt: string): string {
  const lower = prompt.toLowerCase();
  if (/(weekly|every week|retro)/.test(lower)) return "0 9 * * 1";
  if (/(daily|every day|weekday|monitor|scan|report|inbox)/.test(lower)) return "0 9 * * 1-5";
  return "";
}

function subAgentsForPrompt(prompt: string): AgentSubAgent[] {
  void prompt;
  return [];
}

function generatedSystem(template: AgentTemplate, prompt: string): string {
  const objective = sentence(prompt);
  const dashboardInstruction = /(大屏|看板|驾驶舱|dashboard)/i.test(prompt)
    ? " 最终输出必须包含一个 JSON 对象，其中 metrics 是以中文指标名为键的对象，rows 是由扁平对象组成的明细数组；不要在 JSON 中输出 HTML 或脚本。"
    : "";
  return `${template.draft.system}\n\n请使用此智能体配置完成以下工作流程：${objective} 在执行不可逆的外部操作前，先概述预期操作并等待用户明确批准。输出应结构清晰、简洁且易于复核。${dashboardInstruction}`;
}

function applicationContractFromPrompt(prompt: string, draft: AgentDraft): AgentApplicationContract {
  const objective = sentence(cleanRequest(prompt) || prompt.trim());
  const scheduled = Boolean(cronForPrompt(prompt) || draft.cron.trim());
  const needsDashboard = /(大屏|看板|驾驶舱|dashboard)/i.test(prompt);
  return {
    ...applicationContractForDraft(draft),
    objective,
    interaction_mode: scheduled ? "scheduled" : "conversational",
    inputs: [
      {
        type: "request",
        source: scheduled ? "定时例程" : "对话",
        description: objective,
      },
    ],
    outputs: needsDashboard
      ? [
          {
            type: "interactive_dashboard",
            description: "可筛选、可复核的分析大屏。",
          },
        ]
      : [{ type: "result", description: draft.description.trim() || objective }],
    ...(needsDashboard
      ? {
          dashboard: {
            title: `${draft.name}数据大屏`,
            description: "展示本次运行产生的关键指标、趋势和明细数据。",
            template: "analysis" as const,
            metrics: ["总量", "成功量", "异常量"],
            dimensions: ["时间", "类别"],
            visualizations: ["指标卡", "趋势图", "明细表"],
          },
        }
      : {}),
  };
}

export function buildAgentDraftFromPrompt(prompt: string): AgentDraft {
  const promptVaultKeys = vaultKeysForPrompt(prompt);
  const promptCron = cronForPrompt(prompt);
  const promptSubAgents = subAgentsForPrompt(prompt);
  if (/\bhello\s*,?\s*world\b/i.test(prompt)) {
    const draft = {
      ...blankAgentDraft(),
      name: "你好世界智能体",
      description: "一个使用“你好，世界”消息问候用户的简单智能体。",
      system:
        '你是一个友好的“你好，世界”智能体。用户发送任何消息时，请用“你好，世界！”热情问候，并附上一句简短愉快的话。回复应简短、积极且亲切。',
      cron: promptCron,
      vault_keys: promptVaultKeys,
      sub_agents: promptSubAgents,
    };
    return {
      ...draft,
      application: applicationContractFromPrompt(prompt, draft),
    };
  }

  const template = agentTemplateForPrompt(prompt);
  const request = cleanRequest(prompt) || prompt.trim();
  const draft = {
    ...template.draft,
    name: requestedName(prompt),
    description: sentence(request).replace(/\.$/, ""),
    system: generatedSystem(template, request),
    cron: promptCron || template.draft.cron,
    vault_keys: unique([...template.draft.vault_keys, ...promptVaultKeys]),
    skill_ids: [...template.draft.skill_ids],
    sub_agents: promptSubAgents,
  };
  return {
    ...draft,
    application: applicationContractFromPrompt(prompt, draft),
  };
}

function scalar(value: string): string {
  if (!value) return '""';
  if (
    !/[\r\n]/.test(value) &&
    !/:\s/.test(value) &&
    !/\s#/.test(value) &&
    !/\s$/.test(value) &&
    !/^[\s\-\[\]\{\},&*!|>@`]/.test(value) &&
    !/^(true|false|null|undefined)$/i.test(value)
  ) {
    return value;
  }
  return JSON.stringify(value);
}

function block(value: string): string {
  const lines = value.replace(/\s+$/g, "").split("\n");
  return lines.map((line) => `  ${line}`).join("\n");
}

function listBlock(values: string[]): string {
  if (values.length === 0) return "[]";
  return `\n${values.map((value) => `  - ${scalar(value)}`).join("\n")}`;
}

function toolsBlock(tools: AgentTool[]): string {
  if (tools.length === 0) return "tools: []";
  return [
    "tools:",
    ...tools.flatMap((tool) => {
      const entries = Object.entries(tool);
      if (entries.length === 0) return ["  - {}"];
      const [firstKey, firstValue] = entries[0];
      return [
        `  - ${firstKey}: ${scalar(String(firstValue))}`,
        ...entries.slice(1).map(([key, value]) => `    ${key}: ${scalar(String(value))}`),
      ];
    }),
  ].join("\n");
}

function subAgentsBlock(subAgents: AgentSubAgent[]): string {
  if (subAgents.length === 0) return "sub_agents: []";
  return ["sub_agents:", ...subAgents.map((agent) => `  - agent_id: ${scalar(agent.agent_id)}`)].join("\n");
}

export function stringifyAgentDraft(draft: AgentDraft): string {
  const lines = [
    `name: ${scalar(draft.name)}`,
    `description: ${scalar(draft.description)}`,
    `model: ${scalar(draft.model)}`,
    `runtime: ${scalar(draft.runtime)}`,
    draft.system.includes("\n") ? ["system: |", block(draft.system)].join("\n") : `system: ${scalar(draft.system)}`,
    toolsBlock(draft.tools),
  ];

  if (draft.cron.trim()) {
    lines.push("schedule:", `  cron: ${scalar(draft.cron)}`, `  timezone: ${scalar(draft.timezone)}`);
  }
  if (draft.vault_keys.length > 0) lines.push(`vault_keys: ${listBlock(draft.vault_keys)}`);
  if (draft.skill_ids.length > 0) lines.push(`skill_ids: ${listBlock(draft.skill_ids)}`);
  if (draft.rule_ids.length > 0) lines.push(`rule_ids: ${listBlock(draft.rule_ids)}`);
  if (draft.sub_agents.length > 0) lines.push(subAgentsBlock(draft.sub_agents));
  if (draft.mcp_server_ids.length > 0) lines.push(`mcp_servers: ${listBlock(draft.mcp_server_ids)}`);
  if (draft.max_runtime_minutes !== 30) lines.push(`max_runtime_minutes: ${draft.max_runtime_minutes}`);
  if (draft.on_failure !== DEFAULT_FAILURE) lines.push(`on_failure: ${scalar(draft.on_failure)}`);
  if (draft.application) {
    lines.push("application:", ...yamlLines(draft.application as unknown as YamlValue, 1));
  }
  if (draft.design && Object.keys(draft.design).length > 0) {
    lines.push("design:", ...yamlLines(draft.design as YamlValue, 1));
  }
  return lines.join("\n");
}

type YamlValue = string | number | boolean | YamlValue[] | { [key: string]: YamlValue };

function yamlLines(value: YamlValue, depth: number): string[] {
  const pad = "  ".repeat(depth);
  if (Array.isArray(value)) {
    return value.flatMap((item) => {
      if (item && typeof item === "object" && !Array.isArray(item)) {
        const entries = Object.entries(item).filter(([, v]) => v !== undefined);
        return entries.map(([k, v], idx) => `${pad}${idx === 0 ? "- " : "  "}${k}: ${scalar(String(v))}`);
      }
      return [`${pad}- ${scalar(String(item))}`];
    });
  }
  if (value && typeof value === "object") {
    return Object.entries(value)
      .filter(([, v]) => v !== undefined)
      .flatMap(([k, v]) => {
        if (v && typeof v === "object") {
          const nested = yamlLines(v as YamlValue, depth + 1);
          return nested.length > 0 ? [`${pad}${k}:`, ...nested] : [`${pad}${k}: []`];
        }
        return [`${pad}${k}: ${typeof v === "string" ? scalar(v) : String(v)}`];
      });
  }
  return [`${pad}${typeof value === "string" ? scalar(value) : String(value)}`];
}

/** Parses an indented YAML-subset subtree (maps, scalar lists, lists of flat
 *  maps) starting after `start`; returns the parsed value and the index of the
 *  last consumed line. */
function parseYamlSubtree(lines: string[], start: number, minIndent: number): { value: YamlValue; end: number } {
  let i = start;
  let map: { [key: string]: YamlValue } | null = null;
  let list: YamlValue[] | null = null;
  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) {
      i += 1;
      continue;
    }
    const indent = indentOf(line);
    if (indent < minIndent) break;
    const trimmed = line.trim();
    if (trimmed.startsWith("- ") || trimmed === "-") {
      list = list ?? [];
      const rest = trimmed.replace(/^-\s*/, "");
      const kv = rest.match(/^([A-Za-z_][A-Za-z0-9_]*):(?:\s*(.*))?$/);
      if (kv) {
        // List item that is a flat map; following deeper-indented `k: v`
        // lines belong to the same item.
        const item: { [key: string]: YamlValue } = {
          [kv[1]]: parseScalarValue(kv[2] ?? ""),
        };
        i += 1;
        while (i < lines.length) {
          const next = lines[i];
          if (!next.trim()) {
            i += 1;
            continue;
          }
          if (indentOf(next) <= indent || next.trim().startsWith("- ")) break;
          const nkv = next.trim().match(/^([A-Za-z_][A-Za-z0-9_]*):(?:\s*(.*))?$/);
          if (nkv) item[nkv[1]] = parseScalarValue(nkv[2] ?? "");
          i += 1;
        }
        list.push(item);
        continue;
      }
      list.push(parseScalarValue(rest));
      i += 1;
      continue;
    }
    const kv = trimmed.match(/^([A-Za-z_][A-Za-z0-9_]*):(?:\s*(.*))?$/);
    if (!kv) {
      i += 1;
      continue;
    }
    map = map ?? {};
    const raw = (kv[2] ?? "").trim();
    if (raw) {
      map[kv[1]] = raw.startsWith("[") ? inlineList(raw) : parseScalarValue(raw);
      i += 1;
    } else {
      const nested = parseYamlSubtree(lines, i + 1, indent + 1);
      map[kv[1]] = nested.value;
      i = nested.end;
    }
  }
  return { value: (map ?? list ?? "") as YamlValue, end: i };
}

function parseScalarValue(raw: string): YamlValue {
  const value = unquote(raw);
  if (value === "true") return true;
  if (value === "false") return false;
  if (/^-?\d+(\.\d+)?$/.test(value)) return Number(value);
  return value;
}

function stringValue(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function stringList(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
}

function normalizeApplicationContract(value: YamlValue): AgentApplicationContract | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) return undefined;
  const raw = value as Record<string, unknown>;
  const interactionMode = stringValue(raw.interaction_mode);
  const allowedModes: AgentInteractionMode[] = ["conversational", "scheduled", "event_driven", "manual"];
  const inputs = Array.isArray(raw.inputs)
    ? raw.inputs.flatMap((item) => {
        if (!item || typeof item !== "object" || Array.isArray(item)) return [];
        const input = item as Record<string, unknown>;
        return [
          {
            type: stringValue(input.type),
            source: stringValue(input.source),
            description: stringValue(input.description),
          },
        ];
      })
    : [];
  const outputs = Array.isArray(raw.outputs)
    ? raw.outputs.flatMap((item) => {
        if (!item || typeof item !== "object" || Array.isArray(item)) return [];
        const output = item as Record<string, unknown>;
        return [
          {
            type: stringValue(output.type),
            description: stringValue(output.description),
          },
        ];
      })
    : [];
  const rawDashboard = raw.dashboard;
  const dashboard =
    rawDashboard && typeof rawDashboard === "object" && !Array.isArray(rawDashboard)
      ? (() => {
          const value = rawDashboard as Record<string, unknown>;
          const template = stringValue(value.template);
          return {
            title: stringValue(value.title),
            description: stringValue(value.description),
            template:
              template === "operations" || template === "executive"
                ? template
                : ("analysis" as AgentDashboardTemplate),
            metrics: stringList(value.metrics),
            dimensions: stringList(value.dimensions),
            visualizations: stringList(value.visualizations),
          };
        })()
      : undefined;
  return {
    version: 1,
    objective: stringValue(raw.objective),
    audience: stringList(raw.audience),
    interaction_mode: allowedModes.includes(interactionMode as AgentInteractionMode)
      ? (interactionMode as AgentInteractionMode)
      : "conversational",
    inputs,
    outputs,
    ...(dashboard ? { dashboard } : {}),
    non_goals: stringList(raw.non_goals),
    completion_criteria: stringList(raw.completion_criteria),
    failure_behavior: stringValue(raw.failure_behavior),
  };
}

function indentOf(line: string): number {
  return line.match(/^ */)?.[0].length ?? 0;
}

function unquote(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) return "";
  if ((trimmed.startsWith('"') && trimmed.endsWith('"')) || (trimmed.startsWith("'") && trimmed.endsWith("'"))) {
    try {
      return trimmed.startsWith('"') ? JSON.parse(trimmed) : trimmed.slice(1, -1).replace(/''/g, "'");
    } catch {
      return trimmed.slice(1, -1);
    }
  }
  return trimmed;
}

function inlineList(value: string): string[] {
  const trimmed = value.trim();
  if (!trimmed || trimmed === "[]") return [];
  if (!trimmed.startsWith("[") || !trimmed.endsWith("]")) return [unquote(trimmed)];
  return unique(
    trimmed
      .slice(1, -1)
      .split(",")
      .map((item) => unquote(item)),
  );
}

function assignScalar(draft: AgentDraft, key: string, value: string): void {
  const parsed = unquote(value);
  if (key === "name") draft.name = parsed;
  if (key === "description") draft.description = parsed;
  if (key === "model") draft.model = parsed;
  if (key === "runtime") draft.runtime = parsed;
  if (key === "owner_id") draft.owner_id = parsed;
  if (key === "harness") draft.runtime = parsed;
  if (key === "system") draft.system = parsed;
  if (key === "cron") draft.cron = parsed;
  if (key === "timezone") draft.timezone = parsed;
  if (key === "on_failure") draft.on_failure = parsed;
  if (key === "max_runtime_minutes") {
    const next = Number.parseInt(parsed, 10);
    if (Number.isFinite(next)) draft.max_runtime_minutes = next;
  }
}

function parseTools(lines: string[], start: number, draft: AgentDraft): number {
  const tools: AgentTool[] = [];
  let current: AgentTool | null = null;
  let i = start + 1;
  while (i < lines.length) {
    const next = lines[i];
    if (!next.trim()) {
      i += 1;
      continue;
    }
    const indent = indentOf(next);
    if (indent === 0) {
      draft.tools = tools;
      return i - 1;
    }
    const trimmed = next.trim();
    const itemPair = trimmed.match(/^-\s*([A-Za-z_][A-Za-z0-9_]*):(?:\s*(.*))?$/);
    if (itemPair) {
      current = {};
      if ((itemPair[2] ?? "").trim()) current[itemPair[1]] = unquote(itemPair[2] ?? "");
      tools.push(current);
      i += 1;
      continue;
    }
    const itemScalar = trimmed.match(/^-\s*(.*)$/);
    if (itemScalar) {
      current = { type: unquote(itemScalar[1]) };
      tools.push(current);
      i += 1;
      continue;
    }
    const pair = trimmed.match(/^([A-Za-z_][A-Za-z0-9_]*):(?:\s*(.*))?$/);
    if (current && pair && indent <= 4 && (pair[2] ?? "").trim()) {
      current[pair[1]] = unquote(pair[2] ?? "");
    }
    i += 1;
  }
  draft.tools = tools;
  return i - 1;
}

function cleanSubAgents(values: AgentSubAgent[]): AgentSubAgent[] {
  const seen = new Set<string>();
  const agents: AgentSubAgent[] = [];
  values.forEach((agent) => {
    const agentId = agent.agent_id.trim();
    if (!agentId || seen.has(agentId)) return;
    seen.add(agentId);
    agents.push({ agent_id: agentId });
  });
  return agents;
}

function parseSubAgents(lines: string[], start: number, draft: AgentDraft): number {
  const subAgents: AgentSubAgent[] = [];
  let current: AgentSubAgent | null = null;
  let i = start + 1;
  while (i < lines.length) {
    const next = lines[i];
    if (!next.trim()) {
      i += 1;
      continue;
    }
    if (indentOf(next) === 0) {
      draft.sub_agents = cleanSubAgents(subAgents);
      return i - 1;
    }
    const trimmed = next.trim();
    const itemPair = trimmed.match(/^-\s*([A-Za-z_][A-Za-z0-9_]*):(?:\s*(.*))?$/);
    if (itemPair) {
      current = {
        agent_id: itemPair[1] === "agent_id" ? unquote(itemPair[2] ?? "") : "",
      };
      subAgents.push(current);
      i += 1;
      continue;
    }
    const itemScalar = trimmed.match(/^-\s*(.*)$/);
    if (itemScalar) {
      current = { agent_id: unquote(itemScalar[1]) };
      subAgents.push(current);
      i += 1;
      continue;
    }
    const pair = trimmed.match(/^([A-Za-z_][A-Za-z0-9_]*):(?:\s*(.*))?$/);
    if (current && pair) {
      const value = unquote(pair[2] ?? "");
      if (pair[1] === "agent_id" || pair[1] === "id") current.agent_id = value;
    }
    i += 1;
  }
  draft.sub_agents = cleanSubAgents(subAgents);
  return i - 1;
}

export function parseAgentDraftConfig(source: string): ParsedAgentDraft {
  const draft = blankAgentDraft();
  const lines = source.replace(/\r\n/g, "\n").split("\n");

  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    if (!line.trim() || indentOf(line) > 0) continue;
    const match = line.match(/^([A-Za-z_][A-Za-z0-9_]*):(?:\s*(.*))?$/);
    if (!match) {
      return { draft, error: `Could not parse line ${i + 1}.` };
    }
    const [, key, raw = ""] = match;
    const value = raw.trim();

    if (key === "system" && value.startsWith("|")) {
      const blockLines: string[] = [];
      i += 1;
      while (i < lines.length) {
        const next = lines[i];
        if (next.trim() && indentOf(next) === 0) {
          i -= 1;
          break;
        }
        blockLines.push(next.startsWith("  ") ? next.slice(2) : next.trim() ? next : "");
        i += 1;
      }
      draft.system = blockLines.join("\n").trim();
      continue;
    }

    if (key === "schedule") {
      i += 1;
      while (i < lines.length) {
        const next = lines[i];
        if (!next.trim()) {
          i += 1;
          continue;
        }
        if (indentOf(next) === 0) {
          i -= 1;
          break;
        }
        const nested = next.trim().match(/^([A-Za-z_][A-Za-z0-9_]*):(?:\s*(.*))?$/);
        if (nested) assignScalar(draft, nested[1], nested[2] ?? "");
        i += 1;
      }
      continue;
    }

    if (key === "tools") {
      if (value === "[]") {
        draft.tools = [];
        continue;
      }
      i = parseTools(lines, i, draft);
      continue;
    }

    if (key === "sub_agents" || key === "multiagent") {
      if (value === "[]") {
        draft.sub_agents = [];
        continue;
      }
      i = parseSubAgents(lines, i, draft);
      continue;
    }

    if (key === "application") {
      const nested = parseYamlSubtree(lines, i + 1, 1);
      draft.application = normalizeApplicationContract(nested.value);
      i = nested.end - 1;
      continue;
    }

    if (key === "design") {
      const nested = parseYamlSubtree(lines, i + 1, 1);
      if (nested.value && typeof nested.value === "object" && !Array.isArray(nested.value)) {
        draft.design = nested.value as AgentDesign;
      }
      i = nested.end - 1;
      continue;
    }

    if (key === "vault_keys" || key === "skill_ids" || key === "rule_ids" || key === "mcp_servers") {
      const values = value ? inlineList(value) : [];
      if (!value) {
        i += 1;
        while (i < lines.length) {
          const next = lines[i];
          if (!next.trim()) {
            i += 1;
            continue;
          }
          if (indentOf(next) === 0) {
            i -= 1;
            break;
          }
          const item = next.trim().match(/^-\s*(.*)$/);
          if (item) values.push(unquote(item[1]));
          i += 1;
        }
      }
      if (key === "mcp_servers") {
        draft.mcp_server_ids = unique(values);
      } else {
        draft[key] = unique(values);
      }
      continue;
    }

    assignScalar(draft, key, value);
  }

  if (!draft.name.trim()) return { draft, error: "必须填写智能体名称。" };
  if (!draft.model.trim()) return { draft, error: "必须选择模型。" };
  if (!draft.runtime.trim()) return { draft, error: "必须选择运行时。" };
  return { draft, error: null };
}

/** Methodology red line: no evaluation definition, no design phase. */
export function evalGatePassed(design: AgentDesign | undefined): boolean {
  const evaluation = design?.evaluation;
  if (!evaluation) return false;
  return (
    evaluation.success_criteria.trim().length > 0 &&
    evaluation.normal_cases.length > 0 &&
    evaluation.edge_cases.length > 0 &&
    evaluation.recovery_cases.length > 0 &&
    evaluation.safety_cases.length > 0
  );
}

export function applicationGatePassed(application: AgentApplicationContract | undefined): boolean {
  return Boolean(
    application?.objective.trim() &&
    application.inputs.length > 0 &&
    application.outputs.length > 0 &&
    application.completion_criteria.length > 0,
  );
}

export function blankDesign(): AgentDesign {
  return {
    feasibility: {
      complexity: true,
      value: true,
      model_fit: true,
      recoverable_errors: true,
    },
    evaluation: {
      task_distribution: [],
      success_criteria: "",
      normal_cases: [],
      edge_cases: [],
      recovery_cases: [],
      safety_cases: [],
      evaluator: "rule",
    },
    governance: {
      write_requires_approval: true,
      credential_isolation: true,
      tool_whitelist: true,
      timeout_minutes: 30,
    },
  };
}

export function createInputFromDraft(draft: AgentDraft, integrations: Integration[] = INTEGRATIONS) {
  const cron = draft.cron.trim();
  const runtime = draft.runtime.trim() || DEFAULT_RUNTIME;

  const resolvedMcpServers = draft.mcp_server_ids
    .map((id) => {
      const integration = integrations.find((i) => i.id === id);
      return integration ? { id, type: "url", name: id, url: integration.mcpUrl } : null;
    })
    .filter((s): s is NonNullable<typeof s> => s !== null && s.url.trim().length > 0);
  const mcpServers = resolvedMcpServers.map(({ id: _id, ...rest }) => rest);
  const baseTools = draft.tools.filter((t) => t.type !== "mcp_toolset");
  const mcpToolsets = resolvedMcpServers.map(({ id }) => ({
    type: "mcp_toolset",
    mcp_server_name: id,
  }));
  const allTools = [...baseTools, ...mcpToolsets];
  const subAgents = cleanSubAgents(draft.sub_agents);
  const platformMcpIds = [
    ...(subAgents.length > 0 ? ["run_sub_agent"] : []),
    // Governance compiled to enforcement, not just prose: write approval
    // attaches the platform approval MCP so the runtime can actually gate.
    ...(draft.design?.governance?.write_requires_approval ? ["request_human_approval"] : []),
  ];

  return {
    name: draft.name.trim(),
    owner_id: draft.owner_id.trim() || DEFAULT_OWNER,
    description: draft.description.trim(),
    model: draft.model.trim(),
    runtime,
    system: draft.system,
    prompt: draft.system,
    tools: allTools,
    mcp_servers: mcpServers,
    schedule: cron ? { cron, timezone: draft.timezone.trim() || "UTC" } : null,
    vault_keys: draft.vault_keys,
    skill_ids: draft.skill_ids,
    rule_ids: draft.rule_ids,
    max_runtime_minutes: draft.max_runtime_minutes,
    on_failure: draft.on_failure.trim() || DEFAULT_FAILURE,
    config: {
      runtime,
      tools: allTools,
      mcp_servers: mcpServers,
      sub_agents: subAgents,
      platform_mcp_ids: platformMcpIds,
      ...(draft.application ? { application: draft.application } : {}),
      // Methodology artifacts; persisted for review/eval tooling, not
      // consumed by the runtime.
      ...(draft.design && Object.keys(draft.design).length > 0 ? { design: draft.design } : {}),
    },
  };
}
