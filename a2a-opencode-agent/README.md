# a2a-opencode-agent

一个**真实的 A2A 智能体**,用于测试平台接入外部 A2A 智能体的能力。

- **协议层**:官方 [`a2a-sdk`](https://pypi.org/project/a2a-sdk/)(JSON-RPC `message/send` / `message/stream`、SSE、任务生命周期、`/.well-known/agent-card.json`)。
- **运行时**:真实的 [opencode](https://opencode.ai) CLI —— 每条消息通过 `opencode run` 执行,可以写代码、跑 shell 命令。
- **多轮会话**:同一个 A2A `contextId` 映射到独立的 workspace 目录,后续消息用 `opencode run --continue` 续接同一个 opencode 会话,是真实的有状态智能体。
- **模型出口**:通过 env 生成 opencode provider 配置,直连控制面网关(`/v1/messages`,anthropic 兼容;也支持 openai 兼容)。

## 构建与运行

```sh
docker build -t a2a-opencode-agent .

docker run --rm -p 9200:9200 \
  --add-host host.docker.internal:host-gateway \
  -e GATEWAY_BASE_URL=http://host.docker.internal:4000 \
  -e GATEWAY_API_KEY=sk-... \
  -e OPENCODE_MODEL=claude-sonnet-5 \
  a2a-opencode-agent
```

或 `docker compose up --build`(见本目录 compose.yaml)。

| 环境变量 | 默认 | 说明 |
|---|---|---|
| `GATEWAY_KIND` | `anthropic` | `anthropic`(`/v1/messages`)或 `openai`(openai 兼容) |
| `GATEWAY_BASE_URL` | — | 网关地址;不设则 opencode 用自身默认鉴权 |
| `GATEWAY_API_KEY` | `dummy` | 网关 key |
| `OPENCODE_MODEL` | — | 模型 id,如 `claude-sonnet-5` |
| `A2A_PORT` | `9200` | 监听端口 |
| `A2A_PUBLIC_URL` | `http://localhost:9200/` | agent card 里对外声明的 URL |
| `OPENCODE_RUN_TIMEOUT` | `300` | 单次运行超时(秒) |

## 快速验证

```sh
# Agent card
curl -s http://localhost:9200/.well-known/agent-card.json | jq .

# 发一条消息(JSON-RPC)
curl -s http://localhost:9200/ -H 'Content-Type: application/json' -d '{
  "jsonrpc": "2.0", "id": 1, "method": "message/send",
  "params": {"message": {"role": "user", "kind": "message",
    "messageId": "m1",
    "parts": [{"kind": "text", "text": "用一句话介绍你自己"}]}}
}' | jq .

# 流式(SSE)
curl -N http://localhost:9200/ -H 'Content-Type: application/json' -d '{
  "jsonrpc": "2.0", "id": 2, "method": "message/stream",
  "params": {"message": {"role": "user", "kind": "message",
    "messageId": "m2",
    "parts": [{"kind": "text", "text": "写一个 python hello world"}]}}
}'
```

多轮:把第一轮响应里的 `contextId` 带回后续消息,即可续接同一个 opencode 会话。
