#!/usr/bin/env bash
set -euo pipefail

lap_url="${LAP_URL:-http://127.0.0.1:4000}"
lap_key="${LAP_MASTER_KEY:-sk-local}"

probe() {
  local name="$1"
  shift
  local body
  body="$(mktemp)"
  local status
  status="$(curl -sS -o "$body" -w '%{http_code}' "$@")"
  printf '\n[%s] HTTP %s\n' "$name" "$status"
  jq . "$body" 2>/dev/null || head -c 1200 "$body"
  printf '\n'
  rm -f "$body"
}

probe "OpenCode native health" \
  -u opencode:native-opencode \
  http://127.0.0.1:14096/global/health

probe "OpenCode native agents" \
  -u opencode:native-opencode \
  http://127.0.0.1:14096/agent

probe "LAP imports native OpenCode" \
  -X POST "$lap_url/api/agents/import/opencode/discover" \
  -H "Authorization: Bearer $lap_key" \
  -H "Content-Type: application/json" \
  --data '{"endpoint":"http://opencode-native:4096","api_key":"native-opencode"}'

probe "Generic OpenAPI native document" \
  http://127.0.0.1:18080/openapi.json

probe "LAP imports generic OpenAPI" \
  -X POST "$lap_url/api/agents/import/openapi/discover" \
  -H "Authorization: Bearer $lap_key" \
  -H "Content-Type: application/json" \
  --data '{"endpoint":"http://openapi-native:8080","api_key":""}'

probe "LangGraph native assistants" \
  -X POST http://127.0.0.1:18123/assistants/search \
  -H "Content-Type: application/json" \
  --data '{"limit":100,"offset":0}'

probe "LAP imports native LangGraph" \
  -X POST "$lap_url/api/agents/import/langgraph/discover" \
  -H "Authorization: Bearer $lap_key" \
  -H "Content-Type: application/json" \
  --data '{"endpoint":"http://langgraph-native:8123","api_key":""}'

probe "CrewAI native health" \
  http://127.0.0.1:18081/health

probe "LAP imports self-hosted CrewAI" \
  -X POST "$lap_url/api/agents/import/crewai/discover" \
  -H "Authorization: Bearer $lap_key" \
  -H "Content-Type: application/json" \
  --data '{"endpoint":"http://crewai-native:8080","api_key":""}'
