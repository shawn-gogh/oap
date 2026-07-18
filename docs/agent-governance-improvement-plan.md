# 智能体纳管平台改进计划

依据本轮纳管模块系统性审查的结论制定。核心判断：
**系统在"管定义"上已过及格线（生命周期流水线、漂移治理、最小权限执行），
下一个台阶是"管行为、管成本"——这两样恰好是网关最有先天优势做的事。**

实施约定：以 `main` 为基线，每个可独立验收的交付单元使用独立 PR。

## 实施基线修正（2026-07-18）

代码核对后，后续实施按以下依赖顺序推进：

1. 工程基线：代码体积 baseline、前端 CI、运行时治理覆盖矩阵
2. 可信归因：为网关模型调用写入可信的 agent/session/invocation 上下文
3. 指标与审计：区分 gateway_metered/provider_reported/unmetered
4. 通知、配额预算、真实运行时 Eval、行为漂移
5. 组织角色、定期复审、目录和更多来源适配器

原 P0-1 不是纯读侧：SpendLog 虽然预留 `agent_id` 和 `session_id`，当前写入链路尚未填充。
原 P1-1 的 CONNECT 出站代理已经在本地 opencode workspace session 落地，后续任务改为补齐
其他运行时的覆盖矩阵和绕过测试。当前 Eval 是单次模型回答加 LLM Judge，只能先做软门禁，
完成真实运行时回归后才能成为强发布门禁。

运行时能力的当前事实源见
[`agent-runtime-governance-coverage.md`](./agent-runtime-governance-coverage.md)。

## 本轮实施进度（2026-07-18）

- 阶段 0 已完成：代码体积 baseline、前端 CI、前端存量 lint 修复、运行时治理覆盖矩阵。
- 阶段 1 已完成首个可验收切片：workspace opencode 模型调用注入 session 上下文；网关鉴权后
  服务端解析 agent 和当前主 invocation；SpendLog 持久化并展示
  `session_id / agent_id / invocation_id / purpose`。
- 阶段 1 的数据库升级使用独立 `0058` 迁移，未修改历史迁移校验和；真实 PostgreSQL
  集成测试已覆盖从模型请求到 SpendLog 的归因闭环。
- 阶段 2 已完成首个可验收切片：`GET /api/agents/{id}/metrics?days=30` 按 UTC 日聚合
  运行次数、模型调用、tokens、估算成本、成功率和平均延迟；详情页展示 30 天指标卡、
  7 天趋势及 `gateway_metered / provider_reported / unmetered` 覆盖数量。
- 指标首版直接查询 SpendLog 与主 invocation，并通过 `0059` 增加查询索引；暂不引入日汇总表，
  等真实数据量证明需要后再增加异步汇总。
- 阶段 3 已完成：发布申请记录基准 revision，审批详情通过
  `GET /api/agents/{id}/revisions/{from}/diff/{to}` 展示可审批字段的前后值、变更类型和风险；
  `GET /api/agents/{id}/audit` 提供 agent 过滤的治理审计流，治理页按时间线展示操作者、
  动作和关联 revision/approval。
- 修订 diff 排除 ID、创建时间、会话引用等运行态噪音，只比较模型、指令、工具、运行时、
  MCP、密钥、调度及策略等配置；`0060` 为 agent 审计时间线补充复合索引。
- 阶段 4 已完成：Mattermost 连接配置新增治理通知频道；申请发布、连续三次健康检查失败导致
  自动暂停、高风险来源漂移导致自动暂停时，主动向目标频道发送包含控制台直达链接的通知。
- 通知采用非阻断语义：治理状态和审计记录先完成，Mattermost 未配置时静默跳过，凭证或网络
  投递失败仅写服务端告警，不回滚审批、暂停或漂移快照。
- 阶段 5 已完成：Agent config 支持 `budget_usd_monthly`、`max_concurrent_sessions` 和
  `rate_per_minute`；网关在会话创建及统一 Prompt 入口强制执行，超限返回 429 并写入
  `agent.quota.rejected` 审计事件。
- 月度预算直接读取当前 UTC 月生产 SpendLog；速率限制使用 `0061` PostgreSQL 原子分钟桶；
  并发配额统计非终态、非空闲会话，并通过 Agent 级创建锁避免同进程并发穿透。
- 指标 API 同时返回预算剩余、月度/分钟重置时间、当前活跃会话与分钟请求数；Agent 总览支持
  修改三项限制并展示月度预算进度。
- P1-3 完成：发布接口现在计算当前 revision 的黄金用例门禁；定义了完整
  `design.evaluation` 的智能体，必须由最近一次同版本 eval 全量通过后才能申请发布。
- 未运行、运行中、执行失败、用例未全过以及定义不完整都会返回 400，并记录
  `agent.governance.publish_blocked` 审计事件；无黄金用例时不阻断，但发布响应和治理面板会明确提示。
- 治理 API 和界面展示门禁状态、最近运行摘要，并在纳管流水线中增加“回归”阶段。
- P2-1 完成：API 密钥新增 `importer`、`approver`、`operator` 三类治理角色；导入、审批和
  运行运维权限已拆分。发布和数据外发审批不再要求全能管理员，审批者可独立处理。
- 自审批默认硬阻断，导入者不能用同一用户身份审批自己的发布申请；管理员也不能绕过。
  管理设置允许显式关闭职责分离，并记录设置变更审计。
- P2-2 完成：发布和复审通过时按管理设置写入有效期，默认 90 天；既有已发布和已回滚记录
  通过 `0065` 回填有效期。调度器原子认领到期记录并切换为 `review_due`，暂停新会话和任务，
  写入审计并发送 Mattermost 通知，重复调度不会重复处理。
- 复审复用“运行检查 → 黄金回归门禁 → 审批发布”链路；审批通过后恢复 active 并重置有效期。
  设置页可配置 1–3650 天复审周期，智能体详情显示截止时间和下一步操作。
- P2-3 完成：新增独立消费侧智能体目录，只展示 active 且满足发布治理要求的安全摘要，不返回
  system、凭据或完整 config。支持名称、描述、标签和能力搜索，以及“我可以使用”筛选。
- Agent config 支持最多 20 个规范化 `tags` 和 `capabilities`；目录能力同时汇总工具、技能和
  MCP 绑定。目录按真实会话聚合使用者、会话数和最近使用时间，并区分属主、已授权与未授权状态。
- 下一切片：P2-4 更多来源适配器。

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
- **完成情况**：已新增 `notification_channel_id` 配置和统一主动通知模块；三类消息分别包含
  revision/approval、连续失败次数/暂停原因、风险级/漂移字段/快照 ID，并复用 `AppState`
  HTTP 客户端和 Vault Bot Token。格式、REST 发帖和发布申请端到端投递均有测试覆盖。

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
- **完成情况**：配置保存时校验正数及整数类型；新会话与带初始 Prompt 的运行均进入配额检查，
  后续 Prompt 在创建 turn 前检查。429 文案包含当前值、上限与重置时间；拒绝审计进入 Agent
  时间线。真实 PostgreSQL 集成测试覆盖并发、分钟速率、月度成本、指标状态和审计闭环。

### P1-3 黄金用例回归纳入发布门禁

- **目标**："发布"意味着行为验证过，不只是连通性。
- **方案要点**：
  - eval 框架已在（`eval_runs.rs`，PASS/FAIL judge 契约）；为导入的智能体允许定义黄金用例（输入 + 期望要点）
  - `request_publish` 前置检查：有黄金用例的智能体必须最近一次 eval 通过（复用 `evalGatePassed` 语义）；无用例时降级为提示而非阻断（避免一刀切劝退）
- **验收**：定义了用例的智能体，eval 未过时申请发布返回 400 并指引运行评估
- **关键文件**：`src/http/managed_agents/eval_runs.rs`、`governance.rs::request_publish`
- **完成情况**：以 `design.evaluation` 的成功标准及正常、边界、恢复、安全四类用例作为完整
  黄金用例定义。发布前查询当前 revision 最近一次 eval；只有 `completed` 且
  `passed == total > 0` 才通过。部分定义、无同版本运行、运行中、运行失败或未全过均阻断，
  无用例则返回非阻断 warning。`0062` 为同版本最新运行查询增加复合索引；治理详情同步返回
  `eval_gate`，前端展示“回归”阶段和具体指引。

### P1-4 行为基线与异常告警（探索）

- **目标**：定义未漂移但行为漂移时能被发现。
- **方案要点**：以 7 日窗口统计每 agent 的工具调用分布 / 出站域名集合 / 平均 token；偏离基线（新域名、调用量激增）产生 `behavior_drift` 类健康记录，进收件箱
- **说明**：先做规则型（新出站域名 = 必告警），不做统计学异常检测；数据源依赖 P0-1 与工具审批记录
- **关键文件**：`src/http/managed_agents/source_scheduler.rs`（挂在既有定时器上）

---

## P2：组织化与生态

| 项 | 内容 | 要点 |
|---|---|---|
| P2-1 角色分离（已完成） | 导入者 / 审批者 / 运维者角色，替代"admin 全能" | `api_keys.role` 与 Web Session 保留角色；外部导入仅允许 importer/admin，发布与数据外发由 approver/admin 审批，跨属主健康检查、紧急停止和退役允许 operator/admin。自审批默认硬阻断，可由管理员显式关闭并审计 |
| P2-2 定期复审（已完成） | 发布有效期默认 90 天，到期自动降级为“待复审”并暂停新工作 | `0065` 增加发布时间和复审截止时间；source scheduler 原子切换 `review_due`、写审计并通知。复审复用运行检查、黄金回归和 approver 审批，完成后恢复运行并重置有效期 |
| P2-3 智能体目录（已完成） | 独立消费侧视图：标签、能力搜索、“谁在用”；与运维视图分离 | `/api/agent-catalog` 仅返回可发布消费的安全摘要，config 支持 `tags` / `capabilities`；能力合并工具、技能和 MCP，真实会话聚合使用者；未授权条目可发现但不能直接启动 |
| P2-4 更多来源适配器 | LangGraph / CrewAI / OpenAI Assistants 导入 | 复用 `ImportAgentsProvider` trait，每个适配器独立文件 |
| P2-5 审计导出与留存 | 审计流 CSV/JSON 导出、留存策略 | 合规场景需要 |

---

## 技术债（穿插处理，不阻塞主线）

| 项 | 内容 |
|---|---|
| TD-1 | 代码体积检查 baseline 化：`scripts/check_code_size.py` 支持 JSON baseline（当前存量 41 个文件、88 个函数例外），新增或增长违规失败——恢复 CI"全绿=正常"的信号语义 |
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
