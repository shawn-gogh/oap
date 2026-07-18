# 智能体纳管平台改进计划

依据本轮纳管模块系统性审查的结论制定。核心判断：
**系统在"管定义"上已过及格线（生命周期流水线、漂移治理、最小权限执行），
下一个台阶是"管行为、管成本"——这两样恰好是网关最有先天优势做的事。**

分支约定：`claude/latest-project-commit-wfh4xl`，每阶段独立 PR 合入 main。

---

## 已完成（截至 PR #11）

### 纳管流程安全修复（PR #9）

| 项 | 内容 | 关键文件 |
|---|---|---|
| ① | 回滚不再自动激活：`restore_snapshot` 移除硬编码 `status='active'`，恢复运行必须走 `activate`（预检 + 治理门禁） | `src/db/managed_agents/registry/repository.rs` |
| ② | `resume` 拒绝治理挂起/已退役的智能体 | `src/http/managed_agents/registry/resume.rs` |
| ③ | 会话拦截门把 `paused` 纳入检查，紧急停止对原生智能体真正生效 | `src/http/managed_agents/mod.rs::assert_agent_interactive` |
| ④ | import 强制执行 preview 阻断规则（共享 `import_issues()`）；身份键 trim 规范化 | `src/http/managed_agents/import.rs` |
| ⑤ | 无连接器来源纳入定时同步与健康检查（LEFT JOIN + 仅显式停用排除）；漂移来源保持 5 分钟同步节奏 | `src/db/managed_agents/sources/repository.rs::list_due_sources` |
| ⑥ | 重复导入的变更统一走漂移评审（`record_drift_candidate()` 单一入口），不再直接改写配置 | `src/http/managed_agents/source_management.rs` |
| ⑦ | 预检联邦来源探测与同步路径使用同一凭据解析（连接器凭据优先） | `registry/preflight.rs`、`source_management.rs::discovery_api_key_for_agent` |

### 连接器体验（PR #9）

- 导入时静默物化连接器（`ensure_connector_for_import`）："导入智能体"成为唯一用户入口，连接器降级为自动产物
- `/agent-sources` 页面文案从"创建连接器"教学口吻改为"已接入平台"管理口吻

### 纳管面板体验重构（PR #10）

- P0：四套状态机合并为一句人话状态 + 唯一主按钮（`deriveGovernanceUx`）；其余操作收进「更多」菜单
- P1：漂移评审对话框（逐字段对比 + 风险徽章 + 审计原因）、BYO 密钥对话框；检查通过提示自带「申请发布」
- P2：治理面板拆出 `governance-panel.tsx`（938 行），详情页 3466 → 2551 行

### 结果闭环与回归测试（PR #11）

- 导入对话框展示逐项结果：`blocked`（含原因）/ `drift_pending`（含治理面板直达链接）
- 审批流集成测试恢复（自建授权智能体 + 显式驱动 turn 状态机）
- 新增 `governance_state_machine_gates`、`import_blocking_and_drift_review` 集成测试；回滚测试断言"不自动激活"

---

## P0：行为可观测（下一步）

> 原则：网关已经看见了全部数据流，缺的只是聚合与展示。P0 不引入新的执行路径，纯读侧。

### P0-1 智能体运行指标聚合

- **目标**：回答"这个智能体今天花了多少钱、跑了多少次、成功率多少"。
- **方案要点**：
  - 网关代理层已有每次模型调用的 token 用量与模型 id（`standard_logging` 载荷）；按 `agent_id × 日` 聚合出 tokens / 估算成本 / 调用次数 / 平均延迟 / 错误率
  - 新表 `LiteLLM_AgentUsageDailyTable`（或复用现有 spend 日志按 agent 维度查询，先查现有 schema 再定）
  - API：`GET /api/agents/{id}/metrics?days=30`
- **前端**：详情页「总览」加指标卡（tabular-nums），治理面板健康区加 7 日趋势 sparkline
- **验收**：会话产生的每次模型调用都能归因到 agent；指标与 spend 日志抽样核对一致
- **关键文件**：`src/proxy/`（logging 载荷）、`src/http/managed_agents/`（新指标端点）、`src/ui/src/app/agents/detail/`

### P0-2 发布审批的修订 diff

- **目标**：审批人看得到"批的是什么"。
- **方案要点**：
  - 修订快照已全量入库（`registry/revisions`）；新端点 `GET /api/agents/{id}/revisions/{a}/diff/{b}` 复用 `agent-draft-diff` 的字段级 diff 思路（后端实现，返回字段路径 + 前后值 + 风险级，风险分级复用 `drift_findings` 的字段表）
  - 收件箱审批详情页嵌入 diff 视图（复用漂移评审对话框的对比组件）
- **验收**：`agent_publish` 类审批项展开即见与上一已发布版本的差异；无已发布版本时与"空"对比
- **关键文件**：`src/db/managed_agents/registry/revisions.rs`、`src/http/managed_agents/governance.rs`、`src/ui/src/app/inbox/`

### P0-3 审批与健康告警外推

- **目标**：审批请求和健康恶化不再依赖有人盯收件箱。
- **方案要点**：Mattermost 双向通道已存在（`src/http/managed_agents/mattermost/`）；在 `request_publish`、健康自动暂停、高风险漂移三个事件点发通知（含直达链接）
- **验收**：三类事件在配置了通道的部署中触达 Mattermost；未配置时静默跳过
- **关键文件**：`src/http/managed_agents/mattermost/`、`governance.rs`、`source_management.rs`

### P0-4 审计时间线 UI

- **目标**：把已有的审计数据变成"可追溯性看得见"。
- **方案要点**：`audit::record` 已覆盖全部治理动作；新端点按 agent 过滤审计流，治理面板加「历史」区（时间线：谁、何时、做了什么、关联版本/快照）
- **验收**：导入/测试/审批/发布/回滚/紧急停止在时间线上连续可读
- **关键文件**：`src/db/managed_agents/audit.rs`、`src/ui/src/app/agents/detail/governance-panel.tsx`

---

## P1：强制型防护

> 原则：把"依赖运行时自觉"的控制点搬到网关强制执行。

### P1-1 出站流量网关强制

- **目标**：出站域名白名单从"自我报告审批"升级为"代理层强制"。
- **方案要点**：为运行时会话提供网关出口代理（复用 MCP 代理基础设施），拒绝非白名单域名；`match_domain_whitelist` 已是共享判定函数，直接复用
- **风险**：各运行时接入方式不同（环境变量 HTTP_PROXY / 沙箱网络策略），需按 harness 分别落地；先做 e2b 沙箱（`e2b_sandbox_params.envs` 可注入代理配置）
- **关键文件**：`src/db/managed_agents/settings/repository.rs::match_domain_whitelist`、`src/http/sessions/runtime_provision*`

### P1-2 预算与配额

- **目标**：按智能体设置月度成本上限 / 并发上限 / 速率限制，超限在网关层截断。
- **方案要点**：
  - 配置落在 agent config（`budget_usd_monthly`、`max_concurrent_sessions`、`rate_per_minute`）
  - 执行点：`enqueue_prompt_text_with_runtime_model`（已是所有 prompt 的单一咽喉）+ 会话创建
  - 依赖 P0-1 的用量聚合做预算判断；超限拒绝时给出人话错误（剩余额度、重置时间）
- **验收**：超预算的智能体新 prompt 被 429/400 拒绝并附说明；面板显示预算消耗进度
- **关键文件**：`src/http/sessions.rs`、`src/http/managed_agents/registry/`

### P1-3 黄金用例回归纳入发布门禁

- **目标**："发布"意味着行为验证过，不只是连通性。
- **方案要点**：
  - eval 框架已在（`eval_runs.rs`，PASS/FAIL judge 契约）；为导入的智能体允许定义黄金用例（输入 + 期望要点）
  - `request_publish` 前置检查：有黄金用例的智能体必须最近一次 eval 通过（复用 `evalGatePassed` 语义）；无用例时降级为提示而非阻断（避免一刀切劝退）
- **验收**：定义了用例的智能体，eval 未过时申请发布返回 400 并指引运行评估
- **关键文件**：`src/http/managed_agents/eval_runs.rs`、`governance.rs::request_publish`

### P1-4 行为基线与异常告警（探索）

- **目标**：定义未漂移但行为漂移时能被发现。
- **方案要点**：以 7 日窗口统计每 agent 的工具调用分布 / 出站域名集合 / 平均 token；偏离基线（新域名、调用量激增）产生 `behavior_drift` 类健康记录，进收件箱
- **说明**：先做规则型（新出站域名 = 必告警），不做统计学异常检测；数据源依赖 P0-1 与工具审批记录
- **关键文件**：`src/http/managed_agents/source_scheduler.rs`（挂在既有定时器上）

---

## P2：组织化与生态

| 项 | 内容 | 要点 |
|---|---|---|
| P2-1 角色分离 | 导入者 / 审批者 / 运维者角色，替代"admin 全能" | 基于现有 `api_keys.role` 扩展；审批者不能审批自己导入的智能体（现在只是审计标记 `self_approval`，升级为可配置的硬阻断） |
| P2-2 定期复审 | 发布有效期（如 90 天），到期自动降级为"待复审"并通知 | 挂在 source_scheduler；复审 = 重跑治理测试 + 轻量审批 |
| P2-3 智能体目录 | 消费侧视图：标签、能力搜索、"谁在用"；与运维视图分离 | 前端为主；agent config 加 `tags` |
| P2-4 更多来源适配器 | LangGraph / CrewAI / OpenAI Assistants 导入 | 复用 `ImportAgentsProvider` trait，每个适配器独立文件 |
| P2-5 审计导出与留存 | 审计流 CSV/JSON 导出、留存策略 | 合规场景需要 |

---

## 技术债（穿插处理，不阻塞主线）

| 项 | 内容 |
|---|---|
| TD-1 | 代码体积检查 baseline 化：`scripts/check_code_size.py` 支持豁免清单（存量 21 处违规入 baseline 文件），新增违规才失败——恢复 CI"全绿=正常"的信号语义 |
| TD-2 | SSRF TOCTOU：`validate_connector_endpoint` 校验后 DNS 可被 rebinding 绕过；自定义 resolver 钉住已校验 IP，或文档化自托管网络边界假设 |
| TD-3 | `source_hash` 剥离 name/description 等低风险字段，远端改文案不再重置整条发布流水线 |
| TD-4 | 回滚后面板明示"当前运行版本落后于远端 vN"（`governance.source_hash` 与运行配置的偏离可见化） |
| TD-5 | 前端 3 个存量 `Date.now` 渲染纯度 lint 错误（任务超期显示、健康新鲜度） |
| TD-6 | 详情页继续拆分（任务面板、记忆面板各约 400-500 行） |
| TD-7 | 连接器页展示 webhook 接收地址（可复制），外部平台管理员才知道往哪推 |

---

## UI 风格准则（增量约束，非重做）

在既有设计系统（蓝色主色、11-15px 五级字号阶梯、StatusDot、EmptyState）之上，明确**"控制塔"气质**：

1. **状态优先**：列表页状态列前置；全站统一 StatusDot + 人话标签词表；语义色（绿/琥珀/红）只表达状态，禁止装饰性彩色徽章
2. **减少卡片嵌套**：平面分区 + 分隔线为主，Card 只留给可交互单元（治理面板已示范，推广到 inbox / observability）
3. **数字排版**：指标用 `tabular-nums`；标识符才用 mono
4. **信任线索**：版本号徽章、审计署名条（谁在何时批准）作为可见的设计元素，而非藏在日志里
5. **动效克制**：预算只花在状态变迁瞬间（暂停→恢复、漂移出现）的过渡；继续尊重 `prefers-reduced-motion`

---

## 实施顺序建议

```
P0-1 指标聚合 ──┬─→ P1-2 预算配额
P0-2 审批 diff  │
P0-3 告警外推   ├─→ P1-4 行为基线
P0-4 审计时间线 ┘
TD-1 baseline 化（随任意 PR 顺带）
P1-1 出站强制（独立线，先 e2b）
P1-3 eval 门禁（独立线）
P2 视 P0/P1 落地情况排期
```

每项完成标准：后端 `fmt`/`clippy -D warnings`/全量测试绿，前端 `tsc`/测试/`build` 绿，行为有集成测试覆盖,经 PR 合入 main 并确认 CI。
