# OAP · 开放智能体平台 (Open Agent Platform)

一处即可调度你所有的开源智能体运行时 —— OpenCode、Hermes、OpenClaw、Deep Agents。

[![Discord](https://img.shields.io/badge/Discord-Chat-5865F2?logo=discord&logoColor=white)](https://discord.gg/Nkxw3rm3EE)

![OAP 控制台](https://github.com/user-attachments/assets/04333758-829c-4b19-bde3-23ade37bb9f1)

OAP 构建在任意开放、可自托管的运行时之上。选一个运行时、创建一个智能体，给你的团队一套统一的界面。

它负责管理：

- **跨运行时的统一 API** —— 一套 API 即可创建和运行智能体，无论底层是哪种运行时
- **访问** —— 开发者在这里创建和运行智能体，无需 Bedrock 或 Anthropic 控制台的访问权限
- **会话管理** —— 跨多次运行的持久化智能体会话
- **CRON 定时** —— 按计划定时运行智能体
- **记忆** —— 智能体跨会话记住上下文

## 快速开始

前置条件：Docker Desktop。

```bash
docker compose --profile opencode up
```

打开 [http://localhost:4000](http://localhost:4000)，用主密钥登录（默认为 `sk-local`）。Compose 会启动 OAP 的 Web/API 服务、一个 Postgres 数据库、OpenCode 模板运行时，并自动在界面中注册 `local-opencode`。

只启动基础 LAP 栈：

```bash
docker compose up
```

启动其他模板运行时 profile，并自动加入界面：

```bash
docker compose --profile deepagents up
docker compose --profile hermes up
docker compose --profile openclaw up
docker compose --profile opencode --profile deepagents up
```

各 profile 会在服务健康后，通过 LAP API 注册 `local-opencode`、`local-deepagents`、`local-hermes` 和 `local-openclaw`。若要让智能体对接托管的模型提供方，请先在「设置」中添加提供方凭据。

## 用法：创建一个智能体

### 1. 在界面中创建智能体

![创建智能体界面](https://github.com/user-attachments/assets/d2083454-b7c1-4337-b2c2-4c4ba99991b6)

### 2. 选择要接入智能体的工具与技能

![选择工具与技能](https://github.com/user-attachments/assets/efd59a4e-dcc7-487a-923b-005ac44b44b0)

### 3. 使用你的智能体

选择你的智能体，以及要运行它的运行时。

![在运行时上运行智能体](https://github.com/user-attachments/assets/be9cfd8c-4475-4309-bed0-4edcd7dd1de1)

## 支持的智能体运行时

仅限开放、可自托管的运行时 —— 不绑定任何闭源厂商：

- OpenCode Agents
- OpenClaw Agents
- Deep Agents
- Hermes Agent

每个运行时的模型调用都经由 OAP 自带的、兼容 LiteLLM 的网关，因此你可以把它指向任何自行托管的开放权重模型。

## 参与贡献

欢迎提交 PR。详见 [docs/engineering/contributing.mdx](docs/engineering/contributing.mdx)。
