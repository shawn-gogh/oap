#!/usr/bin/env bash
set -euo pipefail

lap_url="${LAP_URL:-http://127.0.0.1:4000}"
lap_key="${LAP_MASTER_KEY:-sk-local}"
auth=(-H "Authorization: Bearer $lap_key" -H "Content-Type: application/json")

api() {
  local method="$1"
  local path="$2"
  local data="${3:-}"
  if [[ -n "$data" ]]; then
    curl -fsS -X "$method" "$lap_url$path" "${auth[@]}" --data "$data"
  else
    curl -fsS -X "$method" "$lap_url$path" "${auth[@]}"
  fi
}

import_agent() {
  local provider="$1"
  local endpoint="$2"
  local mapping="$3"
  local discovered external payload imported agent_id
  discovered="$(api POST "/api/agents/import/$provider/discover" \
    "$(jq -cn --arg endpoint "$endpoint" '{endpoint: $endpoint, api_key: ""}')")"
  external="$(jq -c --argjson mapping "$mapping" '
    .agents[0]
    | .raw["x-lap-runtime"] = $mapping
    | {
        external_id: .id,
        name,
        description,
        model,
        raw
      }
  ' <<<"$discovered")"
  payload="$(jq -cn \
    --arg endpoint "$endpoint" \
    --argjson external "$external" \
    '{
      endpoint: $endpoint,
      credential_mode: "shared",
      api_key: "unused-by-local-source",
      agents: [$external]
    }')"
  imported="$(api POST "/api/agents/import/$provider" "$payload")"
  agent_id="$(jq -er '.results[0].agent_id' <<<"$imported")"
  api POST "/api/agents/$agent_id/source/runtime-mapping" "$mapping" >/dev/null
  printf '%s' "$agent_id"
}

activate_agent() {
  local agent_id="$1"
  local status approval_id
  status="$(api GET "/api/agents/$agent_id" | jq -r '.status')"
  if [[ "$status" == "active" ]]; then
    return
  fi
  api POST "/api/agents/$agent_id/governance/test" '{}' >/dev/null
  approval_id="$(api POST "/api/agents/$agent_id/governance/request-publish" '{}' \
    | jq -er '.approval.id')"
  api POST "/api/approvals/$approval_id/accept" '{"arguments":null}' >/dev/null
  api POST "/api/agents/$agent_id/activate" >/dev/null
}

run_structured_input() {
  local provider="$1"
  local agent_id="$2"
  local runtime="$3"
  local expected="$4"
  local session session_id
  session="$(api POST /session "$(jq -cn \
    --arg agent "$agent_id" \
    --arg runtime "$runtime" \
    --arg title "real-source $provider run" \
    '{agent: $agent, agent_id: $agent, runtime: $runtime, title: $title}')")"
  session_id="$(jq -er '.id' <<<"$session")"
  api POST "/session/$session_id/message" "$(jq -cn \
    --arg model "$provider-native" \
    --arg text "Review this incident evidence" \
    '{
      model: {modelID: $model},
      input: {messages: [{role: "user", content: $text}]}
    }')" >/dev/null
  for _ in $(seq 1 50); do
    if api GET "/session/$session_id/message" | jq -e --arg expected "$expected" \
      'tostring | contains($expected)' >/dev/null; then
      printf '%s: PASS (agent=%s session=%s)\n' "$provider" "$agent_id" "$session_id"
      return
    fi
    sleep 0.1
  done
  printf '%s: FAIL (no expected assistant output)\n' "$provider" >&2
  return 1
}

settings="$(api GET /api/governance/settings)"
separation="$(jq -r '.separation_of_duties' <<<"$settings")"
api PUT /api/governance/settings '{"separation_of_duties":false}' >/dev/null
trap 'api PUT /api/governance/settings "{\"separation_of_duties\":$separation}" >/dev/null' EXIT

openapi_agent="$(import_agent openapi http://openapi-native:8080 \
  '{"path":"/api/v1/runs","input_field":"messages","output_field":"messages"}')"
activate_agent "$openapi_agent"
run_structured_input openapi "$openapi_agent" openapi_rest \
  'Verify dates, identities, and primary sources.'

langgraph_agent="$(import_agent langgraph http://langgraph-native:8123 \
  '{"input_field":"messages","output_path":"/messages"}')"
activate_agent "$langgraph_agent"
run_structured_input langgraph "$langgraph_agent" langgraph_assistant \
  'Provide timestamps and primary sources'
