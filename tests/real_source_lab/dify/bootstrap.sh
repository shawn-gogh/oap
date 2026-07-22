#!/usr/bin/env bash
# Drives Dify's real console API to get from a freshly booted stack to the two
# values the LAP import dialog needs: a published app's `/v1` endpoint and its
# service API key.
#
# These are console endpoints, not Dify's documented service API — they are
# what the web UI itself calls, and they can change between Dify releases. That
# is the trade-off for skipping the click-through; if this script breaks after
# a version bump, doing the same four steps in the UI at http://127.0.0.1:8088
# is always the fallback, and is what tells you the contract moved.
set -euo pipefail

console="${DIFY_CONSOLE_URL:-http://127.0.0.1:8088}/console/api"
email="${DIFY_ADMIN_EMAIL:-lab@lap.test}"
password="${DIFY_ADMIN_PASSWORD:-LapLab123456}"
app_name="${DIFY_APP_NAME:-Evidence Assistant}"
app_mode="${DIFY_APP_MODE:-chat}"

api() {
  local method="$1" path="$2" payload="${3:-}" token="${4:-}"
  local args=(-sS -X "$method" "$console$path" -H 'content-type: application/json')
  [[ -n "$token" ]] && args+=(-H "authorization: Bearer $token")
  [[ -n "$payload" ]] && args+=(-d "$payload")
  curl "${args[@]}"
}

printf 'Waiting for Dify console at %s …\n' "$console"
for attempt in $(seq 1 60); do
  if curl -sS -o /dev/null --max-time 3 "$console/setup"; then
    break
  fi
  if [[ "$attempt" == 60 ]]; then
    printf 'Dify console never became reachable.\n' >&2
    exit 1
  fi
  sleep 5
done

if [[ "$(api GET /setup | jq -r '.step')" == "not_started" ]]; then
  printf 'Creating admin account %s …\n' "$email"
  api POST /setup "$(jq -cn --arg email "$email" --arg password "$password" \
    '{email: $email, name: "LAP Lab", password: $password, language: "en-US"}')" >/dev/null
fi

# Dify wraps the login result as {result, data:{access_token}}; older builds
# returned the token at the top level, so accept either rather than depending
# on a shape this script cannot pin.
token="$(api POST /login "$(jq -cn --arg email "$email" --arg password "$password" \
  '{email: $email, password: $password, remember_me: true}')" \
  | jq -er '.data.access_token // .access_token')"

app="$(api POST /apps "$(jq -cn --arg name "$app_name" --arg mode "$app_mode" \
  '{name: $name, mode: $mode, description: "Real Dify app for LAP import testing",
    icon_type: "emoji", icon: "🔍", icon_background: "#FFEAD5"}')" "$token")"
app_id="$(jq -er '.id' <<<"$app")"

key="$(api POST "/apps/$app_id/api-keys" '{}' "$token" | jq -er '.token')"

cat <<EOF

Dify app ready.

  app id      $app_id
  app mode    $app_mode

Import dialog values:

  endpoint    http://dify-native/v1
  api key     $key

Host-side inspection (rejected by LAP's SSRF validation, so not for the dialog):

  curl -H "authorization: Bearer $key" http://127.0.0.1:8088/v1/info

Discovery works as-is. Execution (POST /v1/chat-messages, which LAP's
invoke_dify calls) additionally needs a model provider configured for this
workspace — add one under Settings → Model Provider in the console UI.
EOF
