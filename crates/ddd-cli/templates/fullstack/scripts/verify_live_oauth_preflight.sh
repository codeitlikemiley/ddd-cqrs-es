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
REDIRECT_PATH="${REDIRECT_PATH:-/dashboard}"
AUTH_SYSTEM_ACCESS_TOKEN="${AUTH_SYSTEM_ACCESS_TOKEN:-}"

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Error: $1 is required." >&2
    exit 1
  fi
}

provider_env_prefix() {
  case "$1" in
    google) echo "GOOGLE" ;;
    facebook) echo "FACEBOOK" ;;
    apple) echo "APPLE" ;;
    *)
      echo "Error: unsupported OAuth provider '$1'. Use google, facebook, or apple." >&2
      exit 2
      ;;
  esac
}

expected_authorization_url() {
  local provider="$1"
  local prefix
  prefix="$(provider_env_prefix "$provider")"
  local var_name="AUTH_${prefix}_AUTHORIZATION_URL"
  local configured="${!var_name:-}"
  if [[ -n "$configured" ]]; then
    printf '%s\n' "$configured"
    return 0
  fi

  case "$provider" in
    google) echo "https://accounts.google.com/o/oauth2/v2/auth" ;;
    facebook) echo "https://www.facebook.com/v20.0/dialog/oauth" ;;
    apple) echo "https://appleid.apple.com/auth/authorize" ;;
  esac
}

expected_redirect_uri() {
  local provider="$1"
  local prefix
  prefix="$(provider_env_prefix "$provider")"
  local var_name="AUTH_${prefix}_REDIRECT_URI"
  local configured="${!var_name:-}"
  if [[ -n "$configured" ]]; then
    printf '%s\n' "$configured"
    return 0
  fi

  local public_base="${AUTH_PUBLIC_BASE_URL:-${AUTH_JWT_ISSUER:-$BASE_URL}}"
  printf '%s/api/auth/oauth/%s/callback\n' "${public_base%/}" "$provider"
}

validate_authorization_url() {
  local provider="$1"
  local response_json="$2"
  local expected_auth_url="$3"
  local expected_redirect="$4"

  python3 - "$provider" "$response_json" "$expected_auth_url" "$expected_redirect" <<'PY'
import json
import sys
from urllib.parse import parse_qs, urlparse

provider, response_json, expected_auth_url, expected_redirect = sys.argv[1:5]
response = json.loads(response_json)
authorization_url = response.get("authorization_url", "")
state = response.get("state", "")

if response.get("provider_id") != provider:
    raise SystemExit(f"{provider}: provider_id mismatch in OAuth start response")
if not state:
    raise SystemExit(f"{provider}: OAuth start did not return state")
if "development-oauth-code" in authorization_url or authorization_url.startswith("/api/auth/oauth/"):
    raise SystemExit(f"{provider}: live preflight received development callback bypass URL")

parsed = urlparse(authorization_url)
expected = urlparse(expected_auth_url)
if parsed.scheme != expected.scheme or parsed.netloc != expected.netloc or parsed.path != expected.path:
    raise SystemExit(
        f"{provider}: authorization endpoint mismatch; expected {expected_auth_url}, got "
        f"{parsed.scheme}://{parsed.netloc}{parsed.path}"
    )
if parsed.scheme != "https":
    raise SystemExit(f"{provider}: live authorization URL must use https")

params = parse_qs(parsed.query)
required = ["response_type", "client_id", "redirect_uri", "scope", "state", "nonce"]
missing = [key for key in required if not params.get(key)]
if missing:
    raise SystemExit(f"{provider}: authorization URL is missing parameters: {', '.join(missing)}")
if params["response_type"][0] != "code":
    raise SystemExit(f"{provider}: response_type must be code")
if params["state"][0] != state:
    raise SystemExit(f"{provider}: URL state does not match response state")
if params["nonce"][0] != state:
    raise SystemExit(f"{provider}: nonce must match stored state")
if params["redirect_uri"][0] != expected_redirect:
    raise SystemExit(
        f"{provider}: redirect_uri mismatch; expected {expected_redirect}, got "
        f"{params['redirect_uri'][0]}"
    )
PY
}

require_command curl
require_command jq
require_command python3

"$SCRIPT_DIR/verify_oauth_credentials.sh"

echo "fullstack-app live OAuth preflight: checking $BASE_URL"

capabilities="$(curl -sS -f "$BASE_URL/api/auth/capabilities")"
jq -e '.oauth_enabled == true' <<<"$capabilities" >/dev/null || {
  echo "Error: OAuth is not enabled. Set AUTH_ENABLE_OAUTH=true and restart the Spin app." >&2
  exit 1
}

for provider in $OAUTH_PROVIDERS; do
  provider="$(printf '%s' "$provider" | tr '[:upper:]' '[:lower:]')"
  provider_env_prefix "$provider" >/dev/null

  if ! jq -e --arg provider "$provider" \
    'any(.providers[]; .provider_id == $provider and .enabled == true)' \
    <<<"$capabilities" >/dev/null; then
    echo "Error: OAuth provider '$provider' is not enabled and credentialed in /api/auth/capabilities." >&2
    exit 1
  fi

  response="$(curl -sS -f "$BASE_URL/api/auth/oauth/$provider/start?next=$REDIRECT_PATH")"
  validate_authorization_url \
    "$provider" \
    "$response" \
    "$(expected_authorization_url "$provider")" \
    "$(expected_redirect_uri "$provider")"

  echo "fullstack-app live OAuth preflight: $provider authorization URL passed"
done

if [[ -n "$AUTH_SYSTEM_ACCESS_TOKEN" ]]; then
  storage_response="$(curl -sS -f "$BASE_URL/api/auth/storage/status" \
    -H "Authorization: Bearer $AUTH_SYSTEM_ACCESS_TOKEN")"
  jq -e '
    any(.event_types[]; .event_type == "auth_oauth_state_created" and .count >= 1)
    and any(.checkpoints[]; .projection_name == "auth.storage.read_models" and .last_sequence > 0)
  ' <<<"$storage_response" >/dev/null || {
    echo "Error: storage status did not show OAuth state creation/projection evidence." >&2
    echo "$storage_response" >&2
    exit 1
  }
  echo "fullstack-app live OAuth preflight: storage evidence passed"
fi

echo "fullstack-app live OAuth preflight: passed"
