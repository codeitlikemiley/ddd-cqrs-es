#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="$(cd -- "$SCRIPT_DIR/.." && pwd)"

load_dotenv_defaults() {
  local env_file="$1"
  [[ -f "$env_file" ]] || return 0

  local line key value
  while IFS= read -r line || [[ -n "$line" ]]; do
    line="${line%$'\r'}"
    [[ -z "$line" || "$line" == \#* || "$line" != *=* ]] && continue
    key="${line%%=*}"
    value="${line#*=}"
    key="${key#"${key%%[![:space:]]*}"}"
    key="${key%"${key##*[![:space:]]}"}"
    [[ "$key" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || continue
    if [[ -z "${!key+x}" ]]; then
      export "$key=$value"
    fi
  done <"$env_file"
}

load_dotenv_defaults "$APP_DIR/.env"

BASE_URL="${BASE_URL:-http://127.0.0.1:3008}"
OAUTH_PROVIDERS="${OAUTH_PROVIDERS:-google facebook apple}"
AUTH_SYSTEM_ACCESS_TOKEN="${AUTH_SYSTEM_ACCESS_TOKEN:-}"
SESSION_COOKIE="${SESSION_COOKIE:-}"
SESSION_ID="${SESSION_ID:-}"
EXPECTED_EMAIL="${EXPECTED_EMAIL:-}"

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Error: $1 is required." >&2
    exit 1
  fi
}

provider_count() {
  local count=0
  local provider
  for provider in $OAUTH_PROVIDERS; do
    case "$(printf '%s' "$provider" | tr '[:upper:]' '[:lower:]')" in
      google|facebook|apple) count=$((count + 1)) ;;
      *)
        echo "Error: unsupported OAuth provider '$provider'. Use google, facebook, or apple." >&2
        exit 2
        ;;
    esac
  done
  echo "$count"
}

session_cookie_header() {
  if [[ -n "$SESSION_COOKIE" ]]; then
    case "$SESSION_COOKIE" in
      Cookie:*) printf '%s\n' "$SESSION_COOKIE" ;;
      *wasi_auth_dev_session=*) printf 'Cookie: %s\n' "$SESSION_COOKIE" ;;
      *) printf 'Cookie: wasi_auth_dev_session=%s\n' "$SESSION_COOKIE" ;;
    esac
    return 0
  fi

  if [[ -n "$SESSION_ID" ]]; then
    printf 'Cookie: wasi_auth_dev_session=%s\n' "$SESSION_ID"
    return 0
  fi

  echo "Error: SESSION_COOKIE or SESSION_ID is required after completing provider login." >&2
  exit 1
}

require_command curl
require_command jq

"$SCRIPT_DIR/verify_oauth_credentials.sh"

if [[ -z "$AUTH_SYSTEM_ACCESS_TOKEN" ]]; then
  echo "Error: AUTH_SYSTEM_ACCESS_TOKEN is required for OAuth callback storage evidence." >&2
  exit 1
fi

cookie_header="$(session_cookie_header)"
provider_total="$(provider_count)"

echo "fullstack-app live OAuth callback: checking authenticated session at $BASE_URL"

session_response="$(curl -sS -f "$BASE_URL/api/auth/session" -H "$cookie_header")"
if [[ -n "$EXPECTED_EMAIL" ]]; then
  jq -e --arg email "$EXPECTED_EMAIL" '
    .authenticated == true
    and .primary_email == $email
  ' <<<"$session_response" >/dev/null
else
  jq -e '
    .authenticated == true
    and (.primary_email | type == "string" and length > 0)
  ' <<<"$session_response" >/dev/null
fi

curl -sS -f "$BASE_URL/dashboard" -H "$cookie_header" >/dev/null

storage_response="$(curl -sS -f "$BASE_URL/api/auth/storage/status" \
  -H "Authorization: Bearer $AUTH_SYSTEM_ACCESS_TOKEN")"
jq -e --argjson count "$provider_total" '
  .event_count >= $count
  and any(.event_types[]; .event_type == "auth_oauth_state_created" and .count >= $count)
  and any(.event_types[]; .event_type == "auth_oauth_state_consumed" and .count >= $count)
  and any(.event_types[]; .event_type == "auth_external_identity_linked" and .count >= $count)
  and any(.event_types[]; .event_type == "auth_session_issued" and .count >= $count)
  and any(.checkpoints[]; .projection_name == "auth.storage.read_models" and .last_sequence > 0)
' <<<"$storage_response" >/dev/null

echo "fullstack-app live OAuth callback: session and storage evidence passed for $OAUTH_PROVIDERS"
