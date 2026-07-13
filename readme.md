# LiteLLM Agent Control Plane

1 place to call all your agents - OpenCode, Hermes, Claude
Managed Agents, Cursor Agents API, Deep Agents.

[![Discord](https://img.shields.io/badge/Discord-Chat-5865F2?logo=discord&logoColor=white)](https://discord.gg/Nkxw3rm3EE)

![LiteLLM Agent Platform dashboard](https://github.com/user-attachments/assets/04333758-829c-4b19-bde3-23ade37bb9f1)

LiteLLM Agent Control Plane sits on top of any runtime. Pick a runtime, create an
agent, give your team one UI.

It manages:

- **Unified API across runtimes** - one API to create and run agents,
  regardless of the runtime underneath
- **Access** - developers create and run agents here, no Bedrock or Anthropic
  console access required
- **Session management** - persistent agent sessions across runs
- **CRON schedules** - run agents on a schedule
- **Memory** - agents remember context across sessions

## Quick Start

Prerequisite: Docker Desktop.

```bash
docker compose --profile opencode up
```

Open [http://localhost:4000](http://localhost:4000) and sign in with the
master key (`sk-local` by default). Compose starts the LiteLLM Agent Platform
web/API service, a Postgres database, the OpenCode template runtime, and
registers `local-opencode` in the UI automatically.

To start only the base LAP stack:

```bash
docker compose up
```

To start other template runtime profiles and add them to the UI automatically:

```bash
docker compose --profile deepagents up
docker compose --profile hermes up
docker compose --profile openclaw up
docker compose --profile opencode --profile deepagents up
```

Profiles register `local-opencode`, `local-deepagents`, `local-hermes`, and
`local-openclaw`
through the LAP API after the services are healthy. Add provider credentials in
Settings before running agents against a hosted model provider.

## Usage: Create an Agent

### 1. Make an agent in the UI

![Create agent screen](https://github.com/user-attachments/assets/d2083454-b7c1-4337-b2c2-4c4ba99991b6)

### 2. Select tools and skills to connect to your agent

![Select tools and skills](https://github.com/user-attachments/assets/efd59a4e-dcc7-487a-923b-005ac44b44b0)

### 3. Use your agent

Select your agent and the runtime you want to run it on.

![Run agent on a runtime](https://github.com/user-attachments/assets/be9cfd8c-4475-4309-bed0-4edcd7dd1de1)

## Supported Agent Runtimes

- Claude Managed Agents
- Cursor Agents API
- OpenCode Agents
- OpenClaw Agents
- Deep Agents
- Hermes Agent

## 部署验证（fix/agent-creation-honesty 分支）

本分支包含两组改动，部署后按以下清单逐项验证。改进计划与阶段说明见
[docs/agent-creation-improvement-plan.md](docs/agent-creation-improvement-plan.md)。

### 1. 重建并启动

```bash
git checkout fix/agent-creation-honesty
docker compose build lap
docker compose --profile opencode up -d
curl -s http://localhost:4000/health   # 期望 200
```

注意：`src/ui/package-lock.json` 必须保持与仓库一致（Docker 内使用 `npm ci`）。
如果本地跑过 `npm install` 导致 lock 变更，先 `git checkout -- src/ui/package-lock.json`。

### 2. 权限与组授权（bug 修复验证）

| 步骤 | 期望结果 |
|---|---|
| 用普通用户（如 bob）登录 | 侧边栏**看不到** AI Gateway 分区（密钥/用户/提供方/日志等），只有 Agent Platform |
| 管理员登录 | AI Gateway 分区正常显示，含用户管理/用户组 |
| bob 加入某组，组被授予某智能体访问权限，bob 访问该智能体 | 正常打开，**不再出现** `syntax error at or near "grant"` 的 HTTP 500 |

### 3. 创建流程：最小授权与诚实标注

进入「智能体 → 新建」：

| 步骤 | 期望结果 |
|---|---|
| 选择空白模板，进入设计步骤看工具列表 | 默认只勾选 `read`/`glob`/`grep`；`bash`/`write`/`edit`/`web_fetch` 未勾选，且每个高风险工具下方有琥珀色风险说明 |
| 进入复核步骤，看治理检查 | 审批项标题为「写操作审批（提示性）」，说明文字明确"当前运行时的原生工具平台无法强制拦截" |
| 运行创建前试跑，用例通过 | 结果为中性色（非绿色），文案为「提示词测试通过（未验证工具与真实数据）」 |
| 查看步骤条第 4 步 | 显示 `POST /api/agents`（不是 `/v1/agents`） |
| 在首页选好模型后用描述生成配置 | 若 AI 推荐了不同模型，配置页出现蓝色横幅 [使用建议]/[保留当前]；不再静默覆盖 |

### 4. draft/active 生命周期与预检

| 步骤 | 期望结果 |
|---|---|
| 创建任意智能体 | toast 提示"已创建为草稿"，跳转详情页；顶部有琥珀色「草稿状态」面板，自动显示预检报告 |
| 查看智能体列表 | 新建的智能体名称旁有「草稿」徽章 |
| 预检报告 | 每项带四态徽章：已验证 / 仅存在性 / 未验证 / 失败。Runtime（local-opencode）应为"已连通"；模型在网关列表中应为"已验证" |
| 对 draft 智能体调 `POST /api/agents/{id}/runs` | 返回 400，提示"处于草稿状态，请先激活" |
| 给 draft 智能体配置 routine 定时任务并等待触发 | 不执行（日志有 draft 拒绝记录） |
| 故意声明一个不存在的 vault key，重新预检 | 该项显示「失败」，「激活」按钮不可点；激活接口返回失败原因 |
| 修复失败项后点「激活」 | 状态变为 active，可正常运行 |
| 对 draft 智能体调 `POST /api/agents/{id}/resume` | 返回 400（不能绕过预检激活） |

预检连通性验证（可选，需挂 MCP）：给智能体挂一个 MCP 服务器后重新预检——
可达时该项为"已验证（N 个工具可用）"；停掉该 MCP 服务再预检应为"失败：网络不可达"；
凭证错误应为"失败：认证失败（HTTP 401/403）"。

### 5. 存量行为确认（回归）

- 分支部署**之前**创建的智能体状态不变（active/paused），运行、调度不受影响。
- tools 为空的存量智能体：下次创建 session 时不再自动获得全套默认工具
  （预期中的行为收窄，如某存量智能体依赖这一隐式注入需显式补勾工具）。
- Slack/Teams 渠道消息：active 智能体正常响应；draft 智能体静默忽略（仅日志）。

## Contributing

PRs welcome. See [docs/engineering/contributing.mdx](docs/engineering/contributing.mdx).
