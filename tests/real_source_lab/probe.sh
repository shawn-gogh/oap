#!/usr/bin/env bash
set -euo pipefail

lap_url="${LAP_URL:-http://127.0.0.1:4000}"
lap_key="${LAP_MASTER_KEY:-sk-local}"

probe() {
  local name="$1"
  local expected_status="$2"
  local filter="$3"
  shift 3
  local body
  body="$(mktemp)"
  local status
  status="$(curl -sS -o "$body" -w '%{http_code}' "$@")"
  printf '\n[%s] HTTP %s\n' "$name" "$status"
  jq "$filter" "$body" 2>/dev/null || head -c 1200 "$body"
  printf '\n'
  if [[ "$status" != "$expected_status" ]]; then
    printf 'Expected HTTP %s, got %s\n' "$expected_status" "$status" >&2
    rm -f "$body"
    return 1
  fi
  rm -f "$body"
}

probe "OpenCode native health" 200 '{healthy, version}' \
  -u opencode:native-opencode \
  http://127.0.0.1:14096/global/health

probe "OpenCode native agents" 200 '[.[] | {name, hidden, native}]' \
  -u opencode:native-opencode \
  http://127.0.0.1:14096/agent

probe "LAP imports native OpenCode" 200 \
  '{agents: [.agents[] | {id, name, has_prompt: (.raw.prompt != null)}]}' \
  -X POST "$lap_url/api/agents/import/opencode/discover" \
  -H "Authorization: Bearer $lap_key" \
  -H "Content-Type: application/json" \
  --data '{"endpoint":"http://opencode-native:4096","api_key":"native-opencode"}'

probe "Generic OpenAPI native document" 200 \
  '{openapi, title: .info.title, post_paths: [.paths | to_entries[] | select(.value.post) | .key]}' \
  http://127.0.0.1:18080/openapi.json

probe "LAP imports generic OpenAPI" 200 \
  '{agents: [.agents[] | {id, name, provider}]}' \
  -X POST "$lap_url/api/agents/import/openapi/discover" \
  -H "Authorization: Bearer $lap_key" \
  -H "Content-Type: application/json" \
  --data '{"endpoint":"http://openapi-native:8080","api_key":""}'

probe "LangGraph native assistants" 200 \
  '[.[] | {assistant_id, graph_id, name}]' \
  -X POST http://127.0.0.1:18123/assistants/search \
  -H "Content-Type: application/json" \
  --data '{"limit":100,"offset":0}'

probe "LAP imports native LangGraph" 200 \
  '{agents: [.agents[] | {id, name, provider}]}' \
  -X POST "$lap_url/api/agents/import/langgraph/discover" \
  -H "Authorization: Bearer $lap_key" \
  -H "Content-Type: application/json" \
  --data '{"endpoint":"http://langgraph-native:8123","api_key":""}'

probe "CrewAI native health" 200 . \
  http://127.0.0.1:18081/health

probe "LAP routes self-hosted CrewAI to OpenAPI" 400 \
  '{message: .error.message}' \
  -X POST "$lap_url/api/agents/import/crewai/discover" \
  -H "Authorization: Bearer $lap_key" \
  -H "Content-Type: application/json" \
  --data '{"endpoint":"http://crewai-native:8080","api_key":""}'
