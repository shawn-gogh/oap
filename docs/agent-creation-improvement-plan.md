# 智能体创建流程改进计划与进度

依据 `agent-creation-flow-critical-review.md` 的评审结论制定。核心原则：
**UI 说了的必须做到，做不到的必须改口**——先消除虚假确定性，再补真实验证能力。

分支：`fix/agent-creation-honesty`（推送在 userrepo/oap）。

---

## 已完成

### 阶段 A：诚实性修复 + 最小授权（commit `9ec868e0`）

| 项 | 内容 | 关键文件 |
|---|---|---|
| A1 | 工具目录收窄：`bash/write/edit/web_fetch/code_execution` 不再默认启用；每个有副作用的工具带 `risk` 中文说明，UI 勾选框下方渲染 | `src/http/agent_runtime_tools.rs`（唯一事实源）、`src/ui/src/app/agents/new/draft-controls.tsx` |
| A2 | 前端兜底 `DEFAULT_TOOLS` 收窄为 `read/glob/grep` | `src/ui/src/lib/agent-builder.ts` |
| A3 | 审批能力位 `approval_enforcement`：当前所有 Runtime 为 `"advisory"`（原生工具在 Runtime 内执行，LAP 不在执行边界）；复核页文案不再声称"写操作会暂停等待确认" | `agent_runtime_tools.rs::approval_enforcement()`、`review-step.tsx` |
| A4 | 试跑结果语义降级：PASS 显示为中性色 +"提示词测试通过（未验证工具与真实数据）" | `review-step.tsx` |
| A5 | 步骤条接口显示修正为 `POST /api/agents` | `steps-bar.tsx` |

**已知行为变化**：tools 为空的存量 agent 在 session 创建时不再被注入全套默认工具。

### 阶段 B：draft/active 生命周期 + 激活预检（commit `5e3cbebe`）

| 项 | 内容 | 关键文件 |
|---|---|---|
| B1 | 新建 agent 默认 `status='draft'`（原为 `'paused'`；存量不迁移、不受影响） | `src/db/managed_agents/registry/repository.rs` |
| B2 | draft 门禁 `assert_agent_runnable`：拒绝手动 run（`runs/create`）和 routine 触发；聊天 session 仍允许（测试用） | `src/http/managed_agents/mod.rs`、`runs/create.rs`、`routines/trigger.rs` |
| B3 | `GET /api/agents/{id}/preflight`：四态报告（`verified` / `exists_only` / `unverified` / `failed`），检查 Runtime 解析+凭证、模型已配置、工具兼容性、vault key 存在性、MCP 引用存在性 | `src/http/managed_agents/registry/preflight.rs` |
| B4 | `POST /api/agents/{id}/activate`：预检有 `failed` 则拒绝并列出原因；`resume` 不允许 draft→active 绕过 | 同上、`resume.rs` |
| B5 | 详情页 draft 横幅：预检报告 + 重新预检 + 激活按钮（`can_activate` 门控） | `src/ui/src/app/agents/detail/page.tsx`、`src/ui/src/lib/api.ts` |

**预检四态语义（新增检查必须遵守）**：
- `verified` — 此刻真实解析/连通过
- `exists_only` — 记录存在但正确性未证明（如 vault key 有值但可能是错的）
- `unverified` — 该配置下检查未实现，如实说"没查"
- `failed` — 缺失或不可用，阻止激活

---

## 未完成（按优先级）

### ~~阶段 C：P0 收尾~~（已完成，commit `cefc3d06`）
- [x] C1 创建成功 toast 说明草稿语义，跳转详情页（预检面板在详情页顶部）
- [x] C2 Slack/Teams 事件路径确认绕过 `runs/create`，已补 draft 忽略（记日志、不回错给渠道）
- [x] C3 agents 列表页 draft 显示「草稿」徽章

### ~~阶段 D：在线连通性预检~~（已完成，见分支提交）
- [x] D1 MCP 冒烟：复用 `mcp_registry::tools::tools_for_server` 调 `tools/list`，以 agent owner 凭证执行，8s 超时；错误归因区分认证失败(401/403)/协议错误(HTTP n)/网络不可达/超时。偏差说明：未做结果缓存——预检仅由用户手动触发（详情页按钮），无轮询场景，缓存收益不成立；若后续加自动预检再补
- [x] D2 Runtime health：自定义 harness 对 api_base 发 GET 探测（任何 HTTP 响应即视为连通）；内置 SaaS Runtime 不探测（探测厂商 API 无意义且增加抖动），保持解析+凭证即 verified
- [x] D3 模型可用性：模型在网关 `model_list` 中 → `verified`；不在 → `unverified`（外部 Runtime 厂商侧解析无法验证，如实说明），不误报 failed

### 阶段 E：opencode wrapper 强制审批（中大，独立分支做）
- [ ] E1 approval token 语义实现：绑定 `(user, agent_id, session_id, tool_name, args_hash)`，一次性、短 TTL（15min）、参数变化重批、子智能体不继承；签发/消费写审计表（schema 与后续迁移一起规划）
- [ ] E2 LAP 侧校验点：平台 MCP handler（`platform_mcps/approval.rs` 旁）+ MCP 代理层
- [ ] E3 opencode wrapper 接 permission hook → LAP 审批 API；完成后该 Runtime 的 `approval_enforcement` 改为 `"enforced"`，复核页自动切换到"平台强制"文案（前端分支已就位）
- [ ] E4 副作用分类：内置工具静态保守分类（bash 一律按 write+）；MCP 工具用规范 `annotations`（`readOnlyHint`/`destructiveHint`），缺失默认高风险。第一版明确不做参数级判断

### 阶段 F：P1 体验（中）
- [x] F1 推荐模型内联差异提示（已完成）：生成结果模型与用户已选不同且在可选模型列表中时，配置页显示内联横幅 [使用建议]/[保留当前]，默认保留用户选择；建议模型不在 `models` 列表则不提示（避免推荐不可用模型）。未做：推荐理由展示（后端 drafting 接口暂不返回理由）
- [ ] F2 可审批的字段级 diff：基于现有 `diffAgentDrafts`（`agent-draft-diff.ts`）加应用前预览、逐字段接受/拒绝、数组增删明细；高风险变更（新增 write 类工具、放宽审批）必须显式确认
- [ ] F3 Fit 四问改造：删除自我评估式问题，只保留驱动配置的事实问题（是否自动执行外部副作用→审批策略、是否定时→routine、失败是否可检测→通知），答案必须落到配置差异
- [ ] F4 默认隐藏 Runtime/Harness/YAML 到高级模式

### 阶段 G：P2 长期（大，按产品节奏）
- [ ] G1 模板可安装化：数据源 + MCP 依赖 + 凭证向导 + 连接测试 + 示例任务
- [ ] G2 成本/时延预算展示
- [ ] G3 上线验收清单 + 首次真实任务验证
- [ ] G4 Degraded 状态与持续健康监测（依赖失效自动降级）
- [ ] G5 失败通知、恢复策略、重复执行控制

---

## 关键架构事实（新分支开工前必读）

1. **LAP 不在原生工具执行边界**：`bash/write/edit` 在外部 Runtime（Claude Managed Agents、opencode wrapper）内部执行，不回流 Rust。LAP 只能强制拦截自己 serve 的平台 MCP 和经代理的 MCP。见 `src/http/sessions/runtime_provision.rs`。
2. **工具集合成链路**：`agent_runtime_tools.rs` 静态表 → `/api/runtimes` + `/api/harnesses` + gemini provisioning 兜底 → 前端 `defaultToolsForRuntime`。改默认值只需改静态表 + 前端 `DEFAULT_TOOLS` 兜底两处。
3. **agent.status 消费点**：目前只有 `assert_agent_runnable`（runs/create、routines/trigger）和 pause/resume/activate 端点。routine 自身另有独立 status。新增运行入口必须调用 `assert_agent_runnable`。
4. **推送规则**：所有分支只推 `userrepo`（shawn-gogh/oap），不推上游、不建 fork。
