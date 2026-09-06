#!/usr/bin/env bash
# Fail CI when production Spin examples still allow wildcard or loopback outbound hosts.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

MANIFESTS=(
  "$ROOT/examples/fullstack-app/spin.production.toml.example"
  "$ROOT/crates/ddd-cli/templates/fullstack/spin.production.toml.example"
)

# Patterns that must never appear in production outbound allowlists.
FORBIDDEN_PATTERNS=(
  '*://'
  '://*'
  '*:*'
  'localhost'
  '127.0.0.1'
)

failures=0
for manifest in "${MANIFESTS[@]}"; do
  if [[ ! -f "$manifest" ]]; then
    echo "error: missing production manifest $manifest" >&2
    failures=$((failures + 1))
    continue
  fi
  rel="${manifest#"$ROOT"/}"
  echo "checking $rel"
  for pattern in "${FORBIDDEN_PATTERNS[@]}"; do
    if grep -Fq "$pattern" "$manifest"; then
      echo "error: $rel contains forbidden production outbound pattern: $pattern" >&2
      failures=$((failures + 1))
    fi
  done
  if ! grep -q 'auth_production_mode = { default = "true" }' "$manifest"; then
    echo "error: $rel must default auth_production_mode to true" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q 'auth_require_trusted_ingress = { default = "true" }' "$manifest"; then
    echo "error: $rel must default auth_require_trusted_ingress to true" >&2
    failures=$((failures + 1))
  fi
done

if [[ "$failures" -gt 0 ]]; then
  echo "error: $failures production Spin manifest check(s) failed" >&2
  exit 1
fi

echo "production Spin manifests are wildcard-free and production-hardened"
