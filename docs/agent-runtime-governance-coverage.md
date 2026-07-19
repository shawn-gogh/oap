# 智能体运行时治理覆盖矩阵

本文记录各运行时当前可由平台实际强制的治理能力。产品界面、指标和发布门禁不得把
“运行时声明支持”展示成“平台已强制”。

## 覆盖级别

- **强制**：执行请求经过平台控制点，运行时无法自行绕过。
- **部分**：只覆盖特定部署形态、协议或调用路径。
- **建议**：平台把策略传给运行时，但执行依赖运行时配合。
- **不可见**：平台没有足够数据证明行为或成本。

## 当前矩阵

| 运行时 | 执行位置 | 模型成本 | 原生工具审批 | MCP 授权 | 网络出口 | 中断/取消 | 真实运行时 Eval |
|---|---|---|---|---|---|---|---|
| 本地 opencode 自定义 harness | 平台管理的 wrapper | 部分：workspace session 经 LAP 网关的调用可关联 agent/session/当前主 invocation；共享进程调用仍不可可靠归因 | 强制 | 强制 | 部分：workspace session 进程强制使用 CONNECT 代理 | 强制 | 部分：当前 revision 的黄金用例 eval 已纳入发布门禁；eval 仍是网关模型单轮回答，不是完整 workspace session |
| Claude Managed Agents | 外部托管 | 不可见 | 建议 | 部分：仅平台 MCP 调用可强制 | 不可见 | 依赖远端 API | 部分：黄金用例 eval 已纳入发布门禁；尚未通过远端完整会话执行 |
| Cursor | 外部托管 | 不可见 | 建议 | 部分：仅平台 MCP 调用可强制 | 不可见 | 依赖远端 API | 部分：黄金用例 eval 已纳入发布门禁；尚未通过远端完整会话执行 |
| Gemini Antigravity | 外部托管 | 不可见 | 建议 | 部分：仅平台 MCP 调用可强制 | 不可见 | 依赖远端 API | 部分：黄金用例 eval 已纳入发布门禁；尚未通过远端完整会话执行 |
| Elastic Agent Builder | 外部托管 | 不可见 | 建议 | 部分：仅平台 MCP 调用可强制 | 不可见 | 依赖远端 API | 未实现 |
| generic_chat 自定义 harness | LAP 发起单次外部 HTTP 调用 | 不可见：当前未解析或记录 provider usage | 不适用 | 不适用 | 部分：由 LAP 发起，但不经过会话出口代理 | 部分：无法可靠终止已发出的上游请求 | 未实现 |
| A2A / ACP / Dify / OpenAPI 联邦来源 | 外部托管 | 不可见，除非来源显式上报 | 建议 | 部分：仅平台 MCP 调用可强制 | 不可见 | 取决于来源能力协商 | 未实现 |

## 指标口径

后续运行指标必须返回计量覆盖级别，不能把未知成本折算为零：

- `gateway_metered`：模型调用经过 LAP 模型网关，token 和成本由平台计算。
- `provider_reported`：外部运行时返回可验证的 usage/cost。
- `unmetered`：平台只能统计 Turn、Invocation、延迟和终态，不能给出可靠成本。

生产调用、评估调用、Guardian 调用和平台系统调用必须分别标记，避免治理成本混入智能体
生产成本。

当前归因信任边界：调用方仅提供 session 标识，网关在鉴权后从数据库解析 owner、agent 和
当前主 invocation，不接受客户端直接声明 agent/invocation。非管理员只能引用自己拥有的会话。
workspace opencode 的 provider 配置会自动注入 session 标识；共享 opencode 进程没有稳定的
单会话上下文，因此暂不宣称完整归因覆盖。

## 更新要求

新增运行时或改变执行链路时，PR 必须同时更新本矩阵，并为声称“强制”的能力增加集成测试。
预算硬门禁只能用于 `gateway_metered` 或 `provider_reported`；其余运行时只能执行次数、速率和
并发配额。

## 指标 API

`GET /api/agents/{id}/metrics?days=30` 返回 UTC 日粒度的运行次数、模型调用、tokens、
估算成本、成功率和平均延迟。模型调用指标来自 `purpose=production` 的 SpendLog；
运行次数来自主 invocation。存在对应生产 SpendLog 的 invocation 计为
`gateway_metered`，其余计为 `unmetered`。当前尚无可信的外部 provider usage 接入，
因此 `provider_reported` 显式返回 0，而不是把未知成本当作零成本。
