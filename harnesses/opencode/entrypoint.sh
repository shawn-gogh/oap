#!/usr/bin/env bash
# opencode harness entrypoint.
# All common setup (vault, git clone, LAP_FILE injection, phase reporting) is
# handled by the shared script. See harnesses/_shared/entrypoint-common.sh.
set -euo pipefail

. /opt/lap/common.sh

: "${LITELLM_DEFAULT_MODEL:?LITELLM_DEFAULT_MODEL required}"

# Normalize base URL: strip trailing slash, ensure /v1 suffix.
BASE="${LITELLM_API_BASE%/}"
case "$BASE" in
  */v1) ;;
  *) BASE="${BASE}/v1" ;;
esac

cd "$REPO_DIR"

# Belt-and-suspenders: ensure .git/config has clean remote (no embedded creds).
if [ -n "${REPO_URL:-}" ]; then
  git remote set-url origin "$REPO_URL" 2>/dev/null || true
fi

# Wire LiteLLM through opencode's native Anthropic adapter, pointed at the
# gateway's Anthropic Messages endpoint (BASE is already normalized to .../v1,
# and @ai-sdk/anthropic POSTs to {baseURL}/messages → .../v1/messages).
#
# Why not @ai-sdk/openai-compatible: that adapter stalls after tool calls with
# OpenAI-compatible gateways like LiteLLM (opencode#14972) — the agent runs a
# tool then goes silent. The Anthropic path doesn't. We keep the provider id
# "litellm" so UI/CLI/Slack model references (providerID:"litellm") still match.
#
# permission: allow-all so the harness runs bypass-permissions. Without it,
# headless `opencode serve` parks forever on the first "ask" prompt with no UI
# to approve it (opencode#16367).
#
# Thinking config (per Anthropic adaptive-thinking docs): opus-4-7 supports ONLY
# the adaptive format; other Claude models use the legacy enabled+budget format
# (what the bundled @ai-sdk/anthropic can send). Haiku / non-Claude: no thinking.
case "$LITELLM_DEFAULT_MODEL" in
  *opus-4-7*)
    MODEL_OPTS='{ "options": { "thinking": { "type": "adaptive", "display": "summarized" }, "effort": "high" } }' ;;
  *sonnet*|*opus*)
    MODEL_OPTS='{ "options": { "thinking": { "type": "enabled", "budgetTokens": 8000 } } }' ;;
  *)
    MODEL_OPTS='{}' ;;
esac
# Sandbox tools: when E2B is configured, mount the bundled stdio MCP that
# exposes provision/execute (same tool surface as the claude-agent-sdk harness).
# Lives at /opt/lap/opencode-sandbox-mcp with its own node_modules baked in.
MCP_BLOCK=""
if [ -n "${E2B_API_KEY:-}" ]; then
  # Build the env object with node so the API key is JSON-escaped regardless of
  # special characters (a raw " or \ in the key would corrupt opencode.json).
  MCP_ENV=$(node -e 'process.stdout.write(JSON.stringify({E2B_API_KEY:process.env.E2B_API_KEY||"",E2B_TEMPLATE:process.env.E2B_TEMPLATE||"base"}))')
  MCP_BLOCK=$(cat <<EOF
  "mcp": {
    "sandbox": {
      "type": "local",
      "command": ["node", "/opt/lap/opencode-sandbox-mcp/sandbox-mcp.mjs"],
      "enabled": true,
      "environment": ${MCP_ENV}
    }
  },
EOF
)
  echo "[entrypoint] E2B configured — mounting sandbox MCP (provision/execute)"
fi

cat > opencode.json <<EOF
{
  "\$schema": "https://opencode.ai/config.json",
${MCP_BLOCK}
  "provider": {
    "litellm": {
      "npm": "@ai-sdk/anthropic",
      "options": {
        "baseURL": "${BASE}",
        "apiKey": "${LITELLM_API_KEY}"
      },
      "models": {
        "${LITELLM_DEFAULT_MODEL}": ${MODEL_OPTS}
      }
    }
  },
  "model": "litellm/${LITELLM_DEFAULT_MODEL}",
  "permission": {
    "edit": "allow",
    "bash": "allow",
    "webfetch": "allow",
    "doom_loop": "allow",
    "external_directory": "allow"
  }
}
EOF

if [ -n "${AGENT_PROMPT:-}" ]; then
  mkdir -p .opencode/agent
  cat > .opencode/agent/default.md <<EOF2
---
description: sandbox agent
---
${AGENT_PROMPT}
EOF2
fi

echo "[entrypoint] booting opencode serve on 0.0.0.0:${PORT}"
echo "[entrypoint] base=${BASE} model=${LITELLM_DEFAULT_MODEL} repo=${REPO_DIR}"

exec opencode serve --hostname 0.0.0.0 --port "$PORT"
