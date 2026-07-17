#!/bin/sh
# Generate opencode provider config from env, then start the A2A server.
#
# Env:
#   GATEWAY_KIND       anthropic | openai        (default: anthropic)
#   GATEWAY_BASE_URL   e.g. http://host.docker.internal:4000  (the control plane gateway)
#   GATEWAY_API_KEY    key the gateway expects
#   OPENCODE_MODEL     model ref opencode should use, e.g. "gateway/claude-sonnet-5"
set -eu

mkdir -p "$HOME/.config/opencode" /workspaces

if [ -n "${GATEWAY_BASE_URL:-}" ]; then
  KIND="${GATEWAY_KIND:-anthropic}"
  if [ "$KIND" = "anthropic" ]; then
    NPM="@ai-sdk/anthropic"
    BASE_URL="${GATEWAY_BASE_URL%/}/v1"
  else
    NPM="@ai-sdk/openai-compatible"
    BASE_URL="${GATEWAY_BASE_URL%/}/v1"
  fi
  MODEL_ID="${OPENCODE_MODEL#gateway/}"
  cat > "$HOME/.config/opencode/opencode.json" <<EOF
{
  "\$schema": "https://opencode.ai/config.json",
  "provider": {
    "gateway": {
      "npm": "$NPM",
      "name": "Control-plane gateway",
      "options": {
        "baseURL": "$BASE_URL",
        "apiKey": "${GATEWAY_API_KEY:-dummy}"
      },
      "models": {
        "$MODEL_ID": { "name": "$MODEL_ID" }
      }
    }
  },
  "model": "gateway/$MODEL_ID",
  "permission": { "edit": "allow", "bash": "allow", "webfetch": "allow" }
}
EOF
  echo "[entrypoint] opencode configured: provider=gateway kind=$KIND baseURL=$BASE_URL model=gateway/$MODEL_ID"
  export OPENCODE_MODEL="gateway/$MODEL_ID"
else
  echo "[entrypoint] GATEWAY_BASE_URL not set — opencode will use its default providers/auth"
fi

exec python /app/server.py
