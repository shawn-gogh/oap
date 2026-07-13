# External Agent Adapter（外部智能体适配器脚手架）

把任意外部组织的智能体系统纳管进平台的"深度接入"模板：实现了网关所需的
managed-agents 运行时契约（与 `templates/deepagents` 同一套 11 个端点），
**接入方只需要改 `src/hooks.mjs` 里的 4 个函数**，把它们对接到自己的系统
（LangGraph server、Dify、自研 HTTP 服务……都可以）。

默认实现是一个 echo 智能体，克隆下来即可注册、跑通全链路，再逐步替换钩子。

> 只有一个普通 OpenAI 兼容 chat 端点、不需要工具/流式语义的外部系统，
> 不必用本模板——直接在平台"运行时"页注册 `generic_chat` 即可（零部署）。

## 需要实现的 4 个钩子（src/hooks.mjs）

| 钩子 | 作用 | 必须实现？ |
|---|---|---|
| `createRemoteSession(ctx)` | 在你的系统中开会话，返回任意状态（如对方 conversation_id） | 可选 |
| `sendPrompt(ctx)` | 把用户消息发给你的系统，`return` 整段回复或用 `ctx.emit(text)` 分段输出 | **必须** |
| `abortRun(ctx)` | 中断当前运行 | 可选 |
| `healthy()` | 探测你的系统可达性，false 时 /health 报 503 | 可选 |

`sendPrompt` 的 `ctx.history` 携带本地保存的完整对话（`[{role, text}]`），
`ctx.agent` 携带平台侧的 agent 定义（name/system/model），无状态的目标系统
可以直接用它们重建上下文。

## 运行与注册

```bash
# 本地运行
npm install && RUNTIME_API_KEY=my-adapter-key npm start   # :8080

# 或容器
docker build -t my-adapter . && docker run -p 8080:8080 -e RUNTIME_API_KEY=my-adapter-key my-adapter

# 注册到平台（或在 UI 运行时页新建，api_spec 选 Claude Managed Agents）
curl -X POST http://<gateway>/api/runtime-harnesses \
  -H "Authorization: Bearer <master-key>" -H "Content-Type: application/json" \
  -d '{"alias":"my-external","api_spec":"claude_managed_agents",
       "api_base":"http://<adapter-host>:8080","api_key":"my-adapter-key"}'
```

注册后即可在平台上创建使用该运行时的智能体并开会话；权限（owner/授权）、
收件箱、评估等平台能力自动适用。

## 契约端点（本模板已全部实现，无需改动）

```
GET  /health                          GET  /v1/models
POST /v1/agents   GET /v1/agents      GET  /v1/agents/{id}
POST /v1/environments                 POST /v1/sessions
POST /v1/sessions/{id}/events         GET  /v1/sessions/{id}/events
GET  /v1/sessions/{id}/events/stream  POST /v1/sessions/{id}/abort
```

事件语义：收到 `user.message` 后异步执行钩子，产出
`agent.message`（可多条）→ `session.status_idle`；失败时 `session.error`。

## 限制

- 状态存内存：适配器重启后会话历史丢失（对方系统有自己的会话状态时，
  在 `createRemoteSession`/`sendPrompt` 里以对方为准即可）。生产化时把
  `agents`/`sessions` 两个 Map 换成 SQLite（参考 `templates/opencode/src/store.mjs`）。
- 未实现工具调用/审批事件的透传；需要时参考 `templates/opencode` 的完整实现。
