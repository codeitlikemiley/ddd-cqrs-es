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

BASE_URL="${BASE_URL:-${AUTH_PUBLIC_BASE_URL:-http://127.0.0.1:3008}}"
OAUTH_PROVIDERS="${OAUTH_PROVIDERS:-google facebook apple}"
AUTH_SYSTEM_ACCESS_TOKEN="${AUTH_SYSTEM_ACCESS_TOKEN:-}"
OAUTH_EVIDENCE_MODE="${OAUTH_EVIDENCE_MODE:-callback}"
OAUTH_EVIDENCE_STRICT="${OAUTH_EVIDENCE_STRICT:-true}"

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Error: $1 is required." >&2
    exit 1
  fi
}

truthy() {
  case "${1:-}" in
    true|TRUE|1|yes|YES) return 0 ;;
    *) return 1 ;;
  esac
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

event_count() {
  local event_type="$1"
  jq -r --arg event_type "$event_type" '
    (.event_types[]? | select(.event_type == $event_type) | .count) // 0
  ' <<<"$storage_response"
}

checkpoint_sequence() {
  local projection_name="$1"
  jq -r --arg projection_name "$projection_name" '
    (.checkpoints[]? | select(.projection_name == $projection_name) | .last_sequence) // 0
  ' <<<"$storage_response"
}

require_event() {
  local event_type="$1"
  local minimum="$2"
  local count
  count="$(event_count "$event_type")"
  printf 'event %-36s count=%s required>=%s\n' "$event_type" "$count" "$minimum"
  if truthy "$OAUTH_EVIDENCE_STRICT" && (( count < minimum )); then
    echo "Error: missing OAuth evidence event $event_type; count=$count required=$minimum" >&2
    failures=$((failures + 1))
  fi
}

require_command curl
require_command jq

if [[ -z "$AUTH_SYSTEM_ACCESS_TOKEN" ]]; then
  echo "Error: AUTH_SYSTEM_ACCESS_TOKEN is required for OAuth evidence." >&2
  exit 1
fi

case "$OAUTH_EVIDENCE_MODE" in
  preflight|callback) ;;
  *)
    echo "Error: OAUTH_EVIDENCE_MODE must be preflight or callback." >&2
    exit 2
    ;;
esac

provider_total="$(provider_count)"
storage_response="$(curl -sS -f "$BASE_URL/api/auth/storage/status" \
  -H "Authorization: Bearer $AUTH_SYSTEM_ACCESS_TOKEN")"

event_count_total="$(jq -r '.event_count' <<<"$storage_response")"
latest_sequence="$(jq -r '.latest_sequence' <<<"$storage_response")"
auth_checkpoint="$(checkpoint_sequence auth.storage.read_models)"
authz_checkpoint="$(checkpoint_sequence authz.storage.read_models)"

echo "fullstack-app OAuth evidence report"
echo "base_url=$BASE_URL"
echo "providers=$OAUTH_PROVIDERS"
echo "mode=$OAUTH_EVIDENCE_MODE strict=$OAUTH_EVIDENCE_STRICT"
echo "event_count=$event_count_total latest_sequence=$latest_sequence"
echo "checkpoint auth.storage.read_models=$auth_checkpoint"
echo "checkpoint authz.storage.read_models=$authz_checkpoint"

failures=0
require_event auth_oauth_state_created "$provider_total"
if [[ "$OAUTH_EVIDENCE_MODE" == "callback" ]]; then
  require_event auth_oauth_state_consumed "$provider_total"
  require_event auth_external_identity_linked "$provider_total"
  require_event auth_session_issued "$provider_total"
fi

if truthy "$OAUTH_EVIDENCE_STRICT" && (( auth_checkpoint <= 0 )); then
  echo "Error: auth.storage.read_models checkpoint is missing or zero." >&2
  failures=$((failures + 1))
fi

if (( failures > 0 )); then
  echo "fullstack-app OAuth evidence report: failed with $failures issue(s)" >&2
  exit 1
fi

echo "fullstack-app OAuth evidence report: passed"
