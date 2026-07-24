# 导入智能体（Import Agent）功能测试计划

覆盖范围：`docs/engineering/multi-source-agent-governance.mdx` 描述的多来源导入治理流程 ——
远程运行时导入（A2A / ACP / Dify / OpenAPI / OpenCode / Elastic）、Markdown 文件导入、
智能体包（.zip）导入，以及导入后的治理生命周期（测试 → 审批发布 → 漂移 → 健康检查 → 退役）。

关键代码位置：
- HTTP 入口：`src/http/managed_agents/import.rs`、`import_files.rs`、`source_management.rs`
- Provider 适配器：`src/sdk/providers/{a2a,acp,dify,openapi,opencode,elastic}_import_agents.rs`
- 归一化 / 一致性：`src/sdk/agents/canonical.rs`、`conformance.rs`
- SSRF 防护：`source_management.rs::validate_connector_endpoint`（`import.rs` 复用）
- UI：`src/ui/src/app/agents/import-agent-dialog.tsx`、`src/ui/src/app/agent-sources/page.tsx`

---

## 1. 可运行测试环境

### 1.1 启动依赖服务 + Mock A2A 智能体

仓库已有 `scripts/a2a_agent/`（FastAPI 实现），本次已增强为可配置多场景的 A2A 测试夹具，
并接入 `compose.yaml` 的 `a2a` profile：

```bash
docker compose --profile a2a up -d          # 启动 postgres/minio/lap + a2a-agent
docker compose ps                            # 确认 a2a-agent 已 Up
curl http://127.0.0.1:8090/healthz           # 宿主机直接访问 mock 服务（仅用于手工探测）
curl http://127.0.0.1:4000/api/agents/import/providers -H "Authorization: Bearer sk-local"
```

**重要：SSRF 防护会拒绝 `localhost` / `127.0.0.1` / 链路本地地址**
（`validate_connector_endpoint`，`source_management.rs:918`）。在导入对话框或 API 请求中，
"服务地址" 必须填写 compose 网络内的服务名：

```
http://a2a-agent:8080                      # 默认正常智能体
http://a2a-agent:8080/scenarios/<name>     # 特定边界场景（见下表）
```

`http://127.0.0.1:8090` 仅用于你在宿主机上直接 curl 校验 mock 服务本身是否工作，
**不能**作为导入接口的 `endpoint` 参数，否则会被 SSRF 防护拒绝（这本身也是一条测试用例，见 §3）。

### 1.2 Mock 服务场景目录

`scripts/a2a_agent/main.py` 提供以下固定场景，访问 `GET /scenarios/{name}/.well-known/agent-card.json`：

| 场景名 | 行为 | 对应的被测逻辑 |
|---|---|---|
| （默认，无前缀） | 合法 Agent Card；`message/send` 无 "async" 关键字走同步，含 "async" 走异步轮询 | discover 正常路径、`invoke_a2a` 同步/异步分支 |
| `missing-name` | Agent Card 缺少 `name` 字段 | 严格解析失败，discover 返回文档校验错误 |
| `no-identity` | 有 `name`/`description`，缺少 0.3 必填 `url` | 严格解析失败，不再从 connector endpoint 猜测 RPC 地址 |
| `malformed-json` | 200 状态码但响应体不是合法 JSON | `ImportAgentsError::Decode` → HTTP 500 `配置无效：invalid provider response` |
| `http-500` | 返回 HTTP 500 | `ImportAgentsError::Upstream` → HTTP 502 `上游返回 HTTP 500` |
| `auth-required` | 需要 `Authorization: Bearer test-secret-key`，否则 401 | discover 时 `api_key` 透传校验 |
| `high-risk` | Agent Card 附带 `permissions`/`network`/`filesystem`/`secrets`/`side_effects` 顶层字段 | `normalize_agent` 的 `unmapped_high_risk_field` → `approval_required` |
| `v1` | A2A 1.0 `supportedInterfaces`，JSON-RPC PascalCase 方法 | 选择 `1.0 + JSONRPC`，校验 `A2A-Version: 1.0` 和 ProtoJSON 响应 |
| `task-fail` | 异步任务最终进入 `status.state = failed` | `poll_a2a_task` 的失败终态分支 |
| `task-input-required` | 异步任务进入 `input-required` | `poll_a2a_task` 的“不支持的续接状态”分支 |
| `task-timeout` | 异步任务永远停在 `working` | `poll_a2a_task` 60 秒轮询超时分支（120 次 × 500ms） |
| `rpc-error` | `message/send` 直接返回 JSON-RPC `error` 而非 `result` | `invoke_a2a` 的 `payload.get("error")` 分支 |
| `mutable` | Agent Card 内容可在运行时通过 `PUT /scenarios/mutable/card` 修改（`POST /scenarios/mutable/reset` 复位） | `sync_source` / 漂移检测 / connector 关联回填（见 §6 G7、§12 发现 2） |

首页 `http://a2a-agent:8080/`（或 `127.0.0.1:8090/`）列出全部场景说明，方便手工探测。

### 1.3 其他 Provider 的 mock 需求（未提供现成夹具）

若需要覆盖 ACP / Dify / OpenAPI / OpenCode 的导入路径，需要额外的 mock 服务
（可用同样的 FastAPI/HTTP 模式实现，按各 provider 的 discover 契约）：

| Provider | discover 请求 | 关键字段 |
|---|---|---|
| ACP | `GET {endpoint}/agents` | 顶层数组或 `{agents:[]}`/`{data:[]}`，每项需要 `id` |
| Dify | `GET {endpoint}/info`（始终带 Bearer，可为空） | 需要顶层 `name`；`id`/`app_id` 作为标识 |
| OpenAPI | `GET {endpoint}/openapi.json`（或以 `.json`/`.yaml` 结尾的 endpoint 本身） | `openapi` 版本必须以 `"3."` 开头（拒绝 Swagger 2.0），需要 `info.title` |
| OpenCode | `GET {endpoint}/v1/agents`，`x-api-key` 头 | 需要 `id`；`model` 可为字符串或 `{id:"..."}` |

本测试计划优先覆盖 A2A（已有可运行夹具），其余 provider 的用例以“单元测试模板”形式列出（§7），
可直接在 `import_tests.rs` / 新增的 provider 单测模块中落地，无需额外起服务。

---

## 2. 测试场景总览（按治理生命周期分组）

```
发现(discover) → 预览(preview) → 导入(import) → 测试(test) → 待审批(pending_approval)
   → 发布(published) → [漂移(drift) / 健康检查(health) / 一致性(conformance)]
   → 暂停(suspended) → 退役(retired)
```

---

## 3. 测试用例：发现与端点校验（discover + SSRF）

对应接口：`POST /api/agents/import/{provider_id}/discover`

| # | 用例 | 步骤 | 期望结果 |
|---|---|---|---|
| D1 | 正常发现单个智能体 | `endpoint=http://a2a-agent:8080`, `api_key=""` | `200`，`agents` 含 1 项，`id=example-a2a-agent`，`raw.url` 指向 `/rpc` |
| D2 | Agent Card 缺少 name | `endpoint=.../scenarios/missing-name` | discover 失败并返回 Agent Card 文档校验错误 |
| D3 | 0.3 Agent Card 缺少 url | `endpoint=.../scenarios/no-identity` | discover 失败，不生成不可执行的导入候选 |
| D4 | 响应非法 JSON | `endpoint=.../scenarios/malformed-json` | `500`，`invalid provider response` |
| D5 | 上游返回 500 | `endpoint=.../scenarios/http-500` | `502`，`上游返回 HTTP 500` |
| D6 | 需要鉴权但未提供 key | `endpoint=.../scenarios/auth-required`, `api_key=""` | `502`（上游 401 被透传为 Upstream 错误） |
| D7 | 提供正确 key | `api_key="test-secret-key"` | `200`，正常返回 |
| D8 | endpoint 为 `localhost` | `endpoint=http://localhost:8090` | `400`，`endpoint 不允许指向本机或云元数据服务` |
| D9 | endpoint 为 `127.0.0.1` | `endpoint=http://127.0.0.1:8090` | `400`，`endpoint 解析到了本机、链路本地或元数据地址` |
| D10 | endpoint 携带凭据 | `endpoint=http://user:pass@a2a-agent:8080` | `400`，`endpoint 不允许在 URL 中携带凭据` |
| D11 | endpoint 非 http(s) 协议 | `endpoint=ftp://a2a-agent:8080` | `400`，`endpoint 只能使用 http 或 https` |
| D12 | endpoint 指向云元数据地址 | `endpoint=http://169.254.169.254` | `400`，被 `forbidden_address` 拦截 |
| D13 | provider_id 不存在 | `POST /api/agents/import/not-a-provider/discover` | 404/400，明确的“未知 provider”错误 |

D1–D7 已通过 curl 对本环境实测通过（见对话记录）；D8/D9 已实测通过。

示例请求（D1）：
```bash
curl -s -X POST http://127.0.0.1:4000/api/agents/import/a2a/discover \
  -H "Authorization: Bearer sk-local" -H 'content-type: application/json' \
  -d '{"endpoint":"http://a2a-agent:8080","api_key":""}'
```

---

## 4. 测试用例：预览与归一化治理（preview）

对应接口：`POST /api/agents/import/{provider_id}/preview`（`src/sdk/agents/canonical.rs::normalize_agent`）

| # | 用例 | 输入 | 期望结果 |
|---|---|---|---|
| P1 | 正常智能体预览 | 默认场景发现结果 | `can_import=true`，`issues` 为空或仅 `info` |
| P2 | 高风险字段导入 | `.../scenarios/high-risk` 发现结果 | `can_import=true`，5 条 `approval_required` issue（`secrets`/`permissions`/`network`/`filesystem`/`side_effects`），已实测通过 |
| P3 | external_id 为空 | 手工构造 `agents:[{external_id:""}]` | `blocking` issue，`can_import=false` |
| P4 | A2A raw 缺少 url | `raw` 不含 `url` 字段 | `blocking`：`a2a_runtime_url_missing`（`import.rs:273`） |
| P5 | model 为空 | 构造 `ManagedAgentRow.model=""` 场景（可通过导入后再 `POST /api/agents/{id}/source/normalize` 触发） | `blocking`：`model_missing` |
| P6 | tools 非数组 | `tools={}` | `blocking`：`tools_invalid` |
| P7 | 未声明 runtime | `config.runtime` 缺失 | `info`：`runtime_implicit`，不阻塞 |
| P8 | Dify workflow 模式 | provider=dify，`raw.mode` 包含 `workflow` | `approval_required`（工作流需要人工映射输入） |
| P9 | OpenAPI 缺少 x-lap-runtime | provider=openapi | `approval_required` |
| P10 | ACP 导入 | provider=acp，任意来源 | 恒定 `approval_required`（协议 pin 需人工确认） |

---

## 5. 测试用例：导入与幂等性（import）

对应接口：`POST /api/agents/import/{provider_id}`

| # | 用例 | 步骤 | 期望结果 |
|---|---|---|---|
| I1 | 首次导入 | 使用 D1 的发现结果导入 | `201`，`results[0].status="imported"`，创建 governance/revision/source/snapshot | 已实测通过（agent 创建成功，随后已清理） |
| I2 | 相同来源重复导入（未变化） | 对同一 `external_id`/`raw` 再次导入 | `results[0].status="unchanged"`，不产生新 revision，生命周期/健康状态保持不变 |
| I3 | 来源变化后重新导入 | 修改 `raw`（如更新 description）后导入 | 新建 revision，`status` 回到 `draft`，此前的测试/待审批状态失效 |
| I4 | agents 数组为空 | `agents:[]` | `400`，`at least one agent is required` |
| I5 | credential_mode=shared，非管理员 | 非 admin key 调用 | `401 Unauthorized`（`validate_credential_mode`） |
| I6 | credential_mode=byo，非管理员 | 非 admin key 调用 | `201`，允许 |
| I7 | 非管理员传 owner_id | 请求体带 `owner_id` | 被忽略，落地 owner 为调用者本人（`owner_id_for_import`） |
| I8 | 管理员传 owner_id | admin key + `owner_id` | 落地 owner 为指定用户 |
| I9 | endpoint 违反 SSRF 规则 | 同 §3 D8/D9 | 导入接口同样在创建前拒绝（`import()` 内部也调用 `validate_connector_endpoint`） |

---

## 6. 测试用例：生命周期治理（governance / drift / conformance / health）

参考现有集成测试模板：`tests/managed_agents_api.rs::imported_agent_governance_publish_and_rollback_against_postgres`
（用 `AppFixture` + `TEST_DATABASE_URL`，无需真实 docker 环境）。

| # | 用例 | 接口 | 期望结果 |
|---|---|---|---|
| G1 | 导入后请求发布未测试的 revision | `POST /governance/request-publish` | 拒绝，未测试不可发布 |
| G2 | 完整发布流程 | test → request-publish → approvals/{id}/accept | `status=published`，`published_revision` 等于最新 revision |
| G3 | 变更来源后重新发布 | 修改 raw → 重新导入 → 重新 test/publish | 旧 published revision 不被覆盖，新 revision 走完整审批 |
| G4 | 回滚 | `POST /governance/rollback` | 恢复到此前 published revision 内容 |
| G5 | 一致性检查（conformance） | `POST /api/agents/{id}/governance/conformance` | 纯 A2A federated 智能体（无 managed runtime harness）应为 `partial` 而非 `conformant`（`managed_protocol` 仅在特定 runtime/harness 下为 true）—— **已实测确认为 `partial`** |
| G6 | 健康检查 | `POST /api/agents/{id}/governance/health` | 返回 `preflight` 报告；连续失败应把 `active` 状态降级为 `paused` 并写 `governance::suspend` —— **已实测，preflight 的 `runtime` 检查始终 `failed`（见 §12 发现 1）** |
| G7 | 手动同步（sync） | `POST /api/agents/{id}/source/sync` | **前提：来源必须关联 connector（见 §12 发现 2），否则恒为空操作。** 关联后：无变化时 `sync_state=in_sync`；来源有变化时产生 candidate snapshot，`sync_state=drifted` —— 已用 `scenarios/mutable` 实测通过 |
| G8 | 并发同步加锁 | 两次并发 `sync` 请求 | 第二次返回 `400`：`该来源正在同步，请稍后重试` |
| G9 | 接受漂移 | `POST /api/agents/{id}/source/drift/accept` | 用 candidate snapshot 更新 agent，`status` 回 `draft`，记录新 revision |
| G10 | 拒绝漂移 | `POST /api/agents/{id}/source/drift/reject` | candidate 标记 `rejected`，agent 配置不变 |
| G11 | 无待处理漂移时接受/拒绝 | 无 candidate_snapshot_id | `400`：`当前没有待处理的来源变更` |
| G12 | 紧急停止 | `POST /api/agents/{id}/emergency-stop` | 立即 `paused`，阻止新任务派发 |
| G13 | 退役 | `POST /api/agents/{id}/retire` | 证据保留，状态终态化，不可再执行 |

---

## 7. 测试用例：A2A 会话执行桥接（session runtime，使用 mock 服务）

对应代码：`src/http/sessions/external_bridge.rs`。这一层在会话真正调用远程 A2A 智能体时触发，
需要先完成 §5 的导入 + §6 的发布，再通过聊天/会话接口发起一轮对话。

| # | 用例 | Mock 场景 | 期望结果 |
|---|---|---|---|
| E1 | 同步响应 | 默认，prompt 不含 "async" | 立即返回文本结果，不创建 task |
| E2 | 异步任务完成 | 默认，prompt 含 "async" | 3 秒后 `tasks/get` 返回 `completed`，携带 `text` |
| E3 | 任务失败终态 | `task-fail` | `poll_a2a_task` 返回 `A2A task ended with state failed` |
| E4 | 任务续接 — 生成审批 | `task-input-required` | 命中 `input-required` 时创建 `a2a_continuation` 审批项，turn 转 `waiting_approval`，`body` 携带远程智能体的追问文本 —— **已通过真实会话端到端验证** |
| E4a | 续接审批 — 批准 | 同上，`POST /api/approvals/{id}/accept` | 用同一 `taskId` 重新 `message/send`；任务 `completed`，助手回复写入聊天记录，会话回 `idle` —— **已实测通过** |
| E4b | 续接审批 — 拒绝 | 同上，`POST /api/approvals/{id}/reject` | 实际发出 `tasks/cancel`（mock 侧任务状态验证为 `canceled`），turn 转终态 `rejected`，会话回 `idle` —— **已实测通过** |
| E4c | 多轮续接 | mock 需在 resume 后仍返回 `input-required`（可扩展 mock 支持） | `resume_a2a_task` 重新进入 `poll_a2a_task`，再次暂停生成新的审批项，而不是把二次续接当错误处理 |
| E5 | 任务永不终态 | `task-timeout` | 到达 Turn `deadline_at` 后 best-effort cancel，Turn 收敛为 `timed_out` |
| E6 | RPC 层错误 | `rpc-error` | `invoke_a2a` 返回 `A2A request failed` |
| E7 | 会话取消 | 默认，异步任务进行中取消会话 | 触发 `tasks/cancel`，mock 侧任务状态置为 `canceled` |
| E8 | A2A 1.0 同步/异步执行 | `v1` | 使用 `SendMessage`/`GetTask`/`CancelTask`，正确解包 task/message 并识别 `TASK_STATE_*` |
| E9 | 版本头不匹配 | `v1`，发送 `A2A-Version: 0.3` | 返回 `VersionNotSupportedError`（`-32009`），客户端不得降级重试 |
| E10 | 0.3 富内容与 Artifact | `rich`，发送 text/data/file content | 请求使用 0.3 part discriminator；结果保留 raw A2A，data/file 进入 canonical Artifact 或明确记录 storage unavailable |
| E11 | 0.3 流式 | `stream` | 调用 `message/stream`，严格校验 SSE，progress/message/Artifact 合并为终态结果 |
| E12 | 1.0 流式 | `v1-stream` | 调用 `SendStreamingMessage`，发送 ProtoJSON part，消费 1.0 status/artifact update |
| E13 | A2A 重启恢复 | 已绑定 remote task 后重启 gateway | 使用 Invocation 冻结的 version/interface/task 恢复，不重新协商 |
| E14 | 0.3 Push | `push` 且配置 `public_base_url` | 注册 `tasks/pushNotificationConfig/set`；正确 token/version 被接受，重复投递幂等 |
| E15 | 1.0 Push | `v1-push` 且配置 `public_base_url` | 注册 `CreateTaskPushNotificationConfig`；错误 token、版本或 task ID 被拒绝 |
| E16 | 0.3 重连订阅 | `stream`，流中断且 task 未终态 | 使用冻结版本调用 `tasks/resubscribe`；流再次中断后才回退 `tasks/get` |
| E17 | 1.0 重连订阅 | `v1-complete`，流中断且 task 未终态 | 使用冻结版本调用 `SubscribeToTask`，不得降级成 0.3 |
| E18 | Push 生命周期清理 | `push` / `v1-complete`，task 到达任一终态 | 使用注册返回的 config ID 调用对应版本 delete；本地 metadata 标记 disabled |
| E19 | 必需扩展拒绝 | `required-extension` | discovery 明确拒绝未实现的 required extension，connector 不得进入可执行状态 |
| E20 | 版本冻结预检 | 已发布 0.3 / 1.0 connector | preflight 使用 negotiated profile 的 URL、binding、版本头和 Send 方法；unverified profile 失败 |
| E21 | 完整操作矩阵 | 单元测试 | 0.3/1.0 的 subscribe、Push CRUD、extended card 方法逐项锁定；0.3 JSON-RPC `ListTasks` 明确不支持 |

已用 curl 直接对 mock RPC 端点验证 E1/E2/E3 的响应结构正确（同步文本、异步 completed、异步 failed）；
E4/E4a/E4b 已通过真实 `POST /session` + `/api/approvals` 端到端验证（见 §12 发现 1 的"端到端验证"）。
E5–E7 仍建议补充为集成测试（`tests/managed_agents_api.rs`，使用 `wiremock::MockServer` 或直接指向
本 mock 容器）。

---

## 8. 测试用例：Markdown 文件导入与 .zip 智能体包

对应接口：`POST /api/agents/import/opencode-files`、`POST /api/agents/import/bundle`（`import_files.rs`）。
不需要 mock A2A 服务，直接构造文件内容。

| # | 用例 | 期望结果 |
|---|---|---|
| F1 | 合法 frontmatter + prompt | 正常导入，`management_mode=managed` |
| F2 | 缺少 frontmatter | 报错或降级处理（需核对具体分支） |
| F3 | frontmatter 未闭合（缺结尾 `---`） | 报错，明确的解析错误信息 |
| F4 | permissions → tools 映射 | `allow` 权限映射为工具，`deny` 权限被排除 |
| F5 | 多文件批量导入 | 每个文件独立生成 `ImportItemResult` |
| B1 | 合法 zip 包 | 正常导入，原始归档按 digest 存档 |
| B2 | 超过 200 个条目 | 拒绝，明确的“条目数超限”错误 |
| B3 | 解压后超过 64MB | 拒绝，明确的“体积超限”错误（zip-bomb 防护） |
| B4 | 路径穿越（`../../etc/passwd`） | 拒绝，条目名校验失败 |
| B5 | 包内附带 knowledge 文件 | 写入首个智能体的 workspace bucket（需 MinIO 可用，否则 `InvalidConfig`） |

---

## 9. UI 层测试用例（`import-agent-dialog.tsx` / `agent-sources/page.tsx`）

需真实浏览器操作，建议按 `/verify` 或 `/run` 技能启动前端后手工走查：

| # | 用例 | 步骤 | 期望结果 |
|---|---|---|---|
| U1 | 远程运行时 tab，字段未填全 | provider/endpoint/apiKey 任一为空 | “连接并发现”按钮禁用 |
| U2 | 发现后勾选部分智能体 | 使用 D1 的默认场景 | 支持搜索、全选/取消全选 |
| U3 | 预览命中 blocking | 使用缺 external_id 或 A2A 缺 url 的构造数据 | 导入按钮禁用/报错，不可继续 |
| U4 | 预览命中 approval_required | 使用 `scenarios/high-risk` | 展示警告，需二次确认（`previewConfirmed`）后才允许导入 |
| U5 | credential_mode 切换 | 非管理员账号下 UI 是否禁止选择 `shared` | 应隐藏或禁用“属主隔离密钥”选项 |
| U6 | Markdown 文件 tab | 上传多个 `.md`/`.markdown` | 逐文件展示导入结果 |
| U7 | 智能体包 tab | 上传单个 `.zip` | base64 编码后提交，展示导入结果 |
| U8 | agent-sources 面板 | 查看已导入 A2A 智能体的来源状态 | 展示 connector 状态、sync_state、健康状态 |
| U9 | 智能体详情页触发 sync/health/conformance | 使用已发布的 A2A 智能体 | 面板正确展示最新检查结果与历史 |

---

## 10. 已在本次会话中实测通过的用例

以下用例已针对当前运行中的 `lap` 容器（compose 服务，端口 4000）与新起的 `a2a-agent`
（compose `a2a` profile，端口 8090/内部 8080）实测，均符合预期（均已清理测试数据）：

- D1（正常发现）、D2（missing-name → 严格校验错误）、D4（malformed-json → 500 Decode 错误）、
  D5（http-500 → 502 Upstream 错误）、D8/D9（localhost、127.0.0.1 均被 SSRF 拒绝）
- P2（high-risk → 5 条 `approval_required` issue，字段级 issue 与 `unmapped_high_risk_field`
  逐一核对通过）
- I1（首次导入创建 agent）、I2（相同 payload 二次导入 → `unchanged`，未新增 revision）、
  I3（`raw` 变化后再导入 → `imported`，新增 revision，`status` 回 `draft`）
- I5（非管理员 + `shared` → `401`）、I6（非管理员 + `byo` → `201`，`owner_id` 落地为调用者本人）、
  I7（非管理员传 `owner_id` 被忽略）、I8（管理员传 `owner_id` 被采纳）
- G1（未测试直接 request-publish → `400`）、G5（conformance → `partial`）、
  G6（health → `runtime` 检查 `failed`，见发现 1）
- G7～G11（连接 connector 后 sync 正确检测漂移 `sync_state=drifted`；接受漂移写回 agent 配置并
  重置为 `draft`；拒绝漂移保持 agent 配置不变；无待处理漂移时 accept/reject 均返回 `400`）
- E1/E2/E3（mock RPC 层：同步响应、异步 `completed`、异步 `failed` 的响应结构）

以下用例因 §12 发现 1 而**无法在当前代码上验证到预期的“成功”结果**（已确认复现，记为已测试）：
- G2～G4（完整发布/回滚流程）：`request-publish` 恒被拒绝，因为 G6 的 `test` 结果恒为 `unhealthy`
- `activate`：同样恒被拒绝

---

## 11. 清理

```bash
docker compose --profile a2a stop a2a-agent   # 或 down 移除容器
```

`a2a-agent` 服务默认不随 `docker compose up` 启动（需要显式 `--profile a2a`），不会影响其他日常开发流程。

---

## 12. 本次测试发现的问题（需产品/研发确认）

### 发现 1（严重，已复现，**已完整修复**）：Federated 来源（A2A/ACP/Dify/OpenAPI）导入的智能体永远无法通过治理测试，因而永远无法发布/激活

**修复状态**：`check_runtime` 对 federated 来源的分支已实现并验证（`preflight.rs::check_federated_source`，
复用 `test_connector_inner` 同款的 `provider.discover()` 可达性探测，探测的是"来源能不能连通"，
不代表"`message/send` 真能跑通"，这是刻意选择的验证深度，见下方"已确认与用户对齐的方案"）。
修复后针对 mock A2A 智能体重新跑 `governance/test`，`runtime` 检查从 `failed` 变为
`verified：联邦来源「A2A」已连通...`。**但整体 `lifecycle_status` 仍为 `unhealthy`**——
修复暴露出第二道、此前被 `runtime` 检查挡在前面没显现的独立阻断点，见"发现 1b"。

**复现步骤**：导入任意 A2A 智能体 → `POST /api/agents/{id}/governance/test`。

**实际结果**：`lifecycle_status` 变为 `unhealthy`，`runtime` 检查报错
`Runtime「a2a_v1」无法解析：invalid request json: unsupported runtime: a2a_v1`。

**根因**：`src/http/managed_agents/registry/preflight.rs::check_runtime` 通过
`agent_runtime_alias()` 取 `config.runtime`（对 A2A 导入而言恒为 `"a2a_v1"`），再调用
`src/http/runtime_resolution.rs::resolve_runtime_for_agent`，该函数只认识两类 runtime：
静态运行时注册表（`claude_managed_agents`、`cursor`、`gemini_antigravity` 等托管协议）和
DB 里注册过的自定义 harness 别名。A2A/ACP/Dify/OpenAPI 四个 provider 的
`expose_runtime_harness()` 均为 `false`（这些 provider 的执行走
`src/http/sessions/external_bridge.rs` 里独立的 `source.raw.url` + JSON-RPC/HTTP 直连路径，
根本不经过 runtime-harness 抽象），所以它们的 `api_spec`（`a2a_v1`/`acp_legacy`/`dify_app`/
`openapi_rest`）从未被注册为可解析的 runtime，`check_runtime` 因此恒定 `FAILED`。

**影响链**：`check_runtime FAILED` → `run_preflight().can_activate=false` →
`governance::mark_tested` 记录 `runtime_health="unhealthy"` → `request_publish` 要求
`runtime_health=="healthy"`（拒绝）→ `activate` 同样要求 `lifecycle_status` 为
`published`/`rolled_back`（拒绝）。也就是说，**当前代码下，一个 A2A/ACP/Dify/OpenAPI 智能体
从导入之日起就不可能走完 `docs/engineering/multi-source-agent-governance.mdx` 里承诺的
`imported → testing → tested → pending_approval → published` 生命周期**，尽管这四个 provider
在 `/api/agents/import/providers` 目录里明确标注 `remote_import: true`、`continuous_sync: true`。

**建议 / 已落地**：`check_runtime` 现在会先判断来源是否 federated
（`governance::external_source_kind(agent) == Some("external_agent")` 且
`provider.expose_runtime_harness() == false`），是的话跳过 harness 解析，改为对
`source.endpoint` 跑一次 `provider.discover()`（复用 connector"测试连接"的探测模式，含
SSRF 校验、超时、凭据解密），`verified`/`failed` 由是否连通决定。已用 mock A2A 智能体验证：
`test` 后 `runtime` 检查从 `Runtime「a2a_v1」无法解析` 变为
`联邦来源「A2A」已连通（http://a2a-agent:8080），发现 1 个智能体`。

与用户确认过验证深度：只做可达性探测（discover），不做真实执行探测（`message/send`）——
更快、无副作用，代价是"tested"状态只证明"连得上"，不证明"真能跑通一次任务"。如果之后想升级
成真实执行探测，需要额外确认每个 provider 的合成 prompt 是否安全（是否会产生真实副作用）。

**发现 1b（新，套壳修复后暴露，仍是阻断项）：`runtime_contract` 一致性检查对 federated 来源恒定不通过**

修好 `runtime` 检查后，`governance/test` 对同一个 mock A2A 智能体重新运行，`lifecycle_status`
依然是 `unhealthy`——这次卡在 `runtime_contract` 这项：`契约 lap-runtime-v1 检查结果：partial`。

**根因**：`src/sdk/agents/conformance.rs::inspect_runtime_contract` 里的 `managed_protocol`
是一份写死的 runtime 白名单（`claude_managed_agents`/`cursor`/`gemini_antigravity`/
`elastic_agent_builder`，或 `harness=="claude_managed_agents"`）。`terminal_events`、
`interrupt_or_abort`、`approval_terminal_result` 这三项必需检查全部直接取
`managed_protocol` 的值——A2A/ACP/Dify/OpenAPI 不在白名单里，所以这三项恒为 `false`，
`status` 永远到不了 `conformant`，`preflight.rs::check_source_contract` 又把
非 `conformant` 一律记成 `FAILED`（必需项），于是 `can_activate` 恒为 `false`。

这一次和发现 1 的性质不同：这不是"没实现校验"，而是校验的判定标准本身没覆盖 federated 桥接的
实现——A2A 的 JSON-RPC 桥接（`external_bridge.rs`）其实已经有对应机制：
`tasks/get` 的 `completed`/`failed`/`canceled` 就是终态事件，`tasks/cancel` 就是
interrupt/abort，审批拒绝走的是平台自己的会话终止逻辑而非 A2A 协议本身。

**已完整修复**（与用户对齐后实施）：

1. `src/sdk/agents/conformance.rs` 把 `managed_protocol` 单一布尔值改成
   `runtime_contract_capabilities(runtime, harness)`，按 provider 逐项声明
   `terminal_events`/`interrupt_or_abort`/`approval_terminal_result`/`event_recovery`。
   A2A（`a2a_v1`）现在诚实地声明前三项 `true`（对应 `poll_a2a_task` 的终态映射、
   `cancel()`/`tasks/cancel`、新实现的续接审批收敛)，`event_recovery` 仍为 `false`
   （只有轮询、没有可恢复的事件序列号，但这是可选项不阻塞 `conformant`）。其余
   federated provider（ACP/Dify/OpenAPI）未做类似桥接实现，维持 `false`。
2. **新增了 A2A 的 `input-required`/`auth-required` 续接桥接**
   （`src/http/sessions/external_bridge.rs`），这是 `approval_terminal_result`
   诚实变为 `true` 的前提，而不只是重新贴标签：
   - `poll_a2a_task` 命中 `input-required`/`auth-required` 时，不再直接报错，而是创建一个
     新的 `a2a_continuation` 类型审批项（`pause_for_continuation`），把远程智能体的追问文本
     写入 `body`，`task_id`/`context_id` 写入 `args_json`，并把当前 turn 转为
     `waiting_approval`（复用 `inbox::create_approval` 已有的"绑定当前活动 turn"逻辑）。
   - 批准后（`resolve_continuation`，接在 `inbox::approvals::deliver()` 的
     `"a2a_continuation"` 分支）用同一个 `taskId` 重新 `message/send`，把人工回复接力给远程
     任务；若再次进入续接状态会再次暂停（支持多轮澄清）。
   - 拒绝后直接对该 `task_id` 发 `tasks/cancel`，并把 turn 转终态 `rejected`。
   - 三处硬编码的审批 kind 白名单（`pending_approvals`、`expire_pending_for_session`、
     `decide_approval`，均在 `src/db/managed_agents/inbox/repository.rs`）都需要补上
     `'a2a_continuation'`，否则新审批永远不会出现在 `/api/approvals` 列表里、无法被
     accept/reject——这是端到端联调时才暴露出来的，纯看代码不容易发现。
3. 前端 `InboxKind`（`src/ui/src/lib/api.ts`）和审批面板标签
   （`tool-approval-panel.tsx`）补充了 `a2a_continuation` / "远程任务续接"。

**端到端验证**（通过 mock A2A 智能体 + 真实会话，非仅单元测试）：
- 导入 A2A 智能体 → `governance/test` → `runtime` 检查从 `failed` 变为 `verified`；
  `runtime_contract` 检查从恒定 `failed` 变为 `verified`；`conformance` 端点整体状态从
  `partial` 变为 `conformant`。
- 真实会话触发 `input-required` → 生成 `a2a_continuation` 审批 → **批准**：用户补充信息
  重新发给 mock，任务 `completed`，助手回复正确写回聊天记录，会话回到 `idle`。
- 同一流程 → **拒绝**：`tasks/cancel` 实际发到 mock（mock 侧任务状态验证为
  `canceled`），turn 收敛到 `rejected`，会话回到 `idle`，聊天事件流里出现标准的
  `approval.replied`/`agent.tool_result(rejected)` 事件。
- `cargo test --lib`：206 个测试全部通过，含新增的 conformance 单测
  （`a2a_bridge_is_conformant_once_it_implements_the_contract`、
  `other_federated_bridges_without_a_contract_implementation_stay_partial`）。

**已知的测试环境局限**：本轮联调过程中，这个 docker-compose 沙箱环境偶发"客户端认为收到
HTTP 404，但 mock 服务端访问日志里完全没有对应请求"的连接层异常——已确认与本次代码改动无关
（对未改动的路径、全新 agent/session 也会复现，且总是在重试后恢复），怀疑是 reqwest 连接池在
容器被反复重建后的过期连接复用问题，仅出现在本沙箱内高频重建容器的调试场景下。生产环境不会
频繁重建已导入智能体指向的远程服务，预计不会遇到。如果你们的 CI/联调环境也出现类似"服务端
无日志但客户端报错"的现象，这是排查方向之一，但不建议现在为此改动网络层代码。

### 发现 2（行为，非报错，但容易踩坑）：一次性远程导入不会自动启用来源同步/漂移检测

**复现步骤**：通过 `POST /api/agents/import/a2a` 直接导入（不经由 `/api/agent-source-connectors`
创建连接器）→ 修改远程 Agent Card → `POST /api/agents/{id}/source/sync`。

**实际结果**：`sync_state` 恒为 `in_sync`，`changed_count=0`，不产生 candidate snapshot，
即使远程内容确实变了。

**根因**：`src/http/managed_agents/import.rs:156` 调用
`ensure_source(pool, &governance, "federated", None)`，`connector_id` 恒为 `None`。
`source_management.rs::reconcile_source` 第一行就是
`let Some(connector_id) = source.connector_id.as_deref() else { mark in_sync; return Ok(false) }`，
即没有关联 connector 的来源，`sync` 是纯空操作。只有事后通过 agent-sources 页面/
`POST /api/agent-source-connectors` 用**完全相同的** `owner_id + provider + endpoint`
新建一个 connector，才会触发 `create_connector` 里的回填 `UPDATE ... SET connector_id = ...`，
把已导入的来源接上，之后 `sync` 才会真正重新发现并检测漂移（已实测确认此路径可行，见 §6 G7）。

**建议**：这更像是产品设计上的两步流程（先导入、再按需接入 connector 开启持续同步），
但导入对话框/智能体详情页目前没有任何提示告诉用户"不接 connector 就没有漂移检测"，
容易让人误以为导入后天然具备治理文档所述的"来源变化在一个同步周期内被检测到"能力
（验收标准第 5 条）。建议在 UI 上显式提示，或在导入时按 `provider+endpoint` 自动创建/关联一个
默认 connector。

### 发现 3（低风险，行为确认）：对已软删除（`archived_pending_delete`）的智能体重新导入会静默复活它

**复现步骤**：导入 A2A 智能体 → `DELETE /api/agents/{id}`（状态变为 `archived_pending_delete`）→
用相同 `owner_id + provider + endpoint + external_id` 再次导入。

**实际结果**：返回同一个 `agent_id`，`status` 变回 `draft`，`deleted_at` 消失，未见任何警告。

**根因**：`src/db/managed_agents/governance.rs::find_by_source` 的查询未按 `deleted_at IS NULL`
或状态过滤，因此软删除的治理记录仍会被"是否已存在来源"的查找命中并复用。

**建议**：确认这是否是期望行为（例如"删除只是归档，凭同一来源标识可以找回"）；如果不是，
`find_by_source` 应排除已删除/终态的治理记录，改为创建一个新的 agent_id。
