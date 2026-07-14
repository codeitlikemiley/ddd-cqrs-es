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

OAUTH_PROVIDERS="${OAUTH_PROVIDERS:-google facebook apple}"

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

is_true() {
  case "${1:-}" in
    true|TRUE|1|yes|YES) return 0 ;;
    *) return 1 ;;
  esac
}

require_non_empty() {
  local name="$1"
  if [[ -z "${!name:-}" ]]; then
    echo "missing:$name"
    return 1
  fi
}

require_https_url() {
  local name="$1"
  local value="${!name:-}"
  if [[ -z "$value" ]]; then
    echo "missing:$name"
    return 1
  fi
  if [[ "$value" != https://* ]]; then
    echo "invalid:$name must start with https:// for live OAuth"
    return 1
  fi
  if [[ "$value" == *localhost* || "$value" == *127.0.0.1* || "$value" == *"[::1]"* ]]; then
    echo "invalid:$name must use a provider-reachable host"
    return 1
  fi
}

failures=0

if ! is_true "${AUTH_ENABLE_OAUTH:-}"; then
  echo "invalid:AUTH_ENABLE_OAUTH must be true"
  failures=$((failures + 1))
fi

if ! require_https_url AUTH_PUBLIC_BASE_URL; then
  failures=$((failures + 1))
fi

for provider in $OAUTH_PROVIDERS; do
  provider="$(printf '%s' "$provider" | tr '[:upper:]' '[:lower:]')"
  prefix="$(provider_env_prefix "$provider")"
  enabled_var="AUTH_${prefix}_ENABLED"
  redirect_var="AUTH_${prefix}_REDIRECT_URI"

  if ! is_true "${!enabled_var:-}"; then
    echo "invalid:$enabled_var must be true"
    failures=$((failures + 1))
  fi
  if ! require_https_url "$redirect_var"; then
    failures=$((failures + 1))
  fi

  case "$provider" in
    google|facebook)
      for name in "AUTH_${prefix}_CLIENT_ID" "AUTH_${prefix}_CLIENT_SECRET"; do
        if ! require_non_empty "$name"; then
          failures=$((failures + 1))
        fi
      done
      ;;
    apple)
      if ! require_non_empty AUTH_APPLE_CLIENT_ID; then
        failures=$((failures + 1))
      fi
      if [[ -z "${AUTH_APPLE_GENERATED_CLIENT_SECRET:-}" ]]; then
        for name in AUTH_APPLE_TEAM_ID AUTH_APPLE_KEY_ID AUTH_APPLE_PRIVATE_KEY; do
          if ! require_non_empty "$name"; then
            failures=$((failures + 1))
          fi
        done
      fi
      ;;
  esac
done

if [[ "$failures" -gt 0 ]]; then
  echo "fullstack-app OAuth credential readiness failed: $failures issue(s)" >&2
  exit 1
fi

echo "fullstack-app OAuth credential readiness passed for: $OAUTH_PROVIDERS"
