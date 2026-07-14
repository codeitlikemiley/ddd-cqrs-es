#!/usr/bin/env bash
# Dual-sync product files: examples/fullstack-app → crates/ddd-cli/templates/fullstack
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
SRC="$ROOT/examples/fullstack-app"
DST="$ROOT/crates/ddd-cli/templates/fullstack"

if [[ ! -d "$SRC" || ! -d "$DST" ]]; then
  echo "error: expected $SRC and $DST" >&2
  exit 1
fi

SYNC_PATHS=(
  src
  input.css
  package.json
  spin.toml
  Makefile
  README.md
  .env.example
  scripts
)

MODE="${1:-sync}" # sync | check

rsync_flags=(-a --delete)
if [[ "$MODE" == "check" ]]; then
  rsync_flags+=(--dry-run --itemize-changes)
fi

changed=0
for path in "${SYNC_PATHS[@]}"; do
  if [[ ! -e "$SRC/$path" ]]; then
    echo "warn: missing source $path" >&2
    continue
  fi
  if [[ -d "$SRC/$path" ]]; then
    out="$(rsync "${rsync_flags[@]}" \
      --exclude 'target/' \
      --exclude 'node_modules/' \
      --exclude '.DS_Store' \
      "$SRC/$path/" "$DST/$path/" 2>&1 || true)"
  else
    out="$(rsync "${rsync_flags[@]}" "$SRC/$path" "$DST/$path" 2>&1 || true)"
  fi
  if [[ -n "$out" ]]; then
    echo "$out"
    changed=1
  fi
done

if [[ "$MODE" == "check" ]]; then
  if [[ "$changed" -eq 1 ]]; then
    echo "error: template drift detected (run scripts/sync_fullstack_template.sh)" >&2
    exit 1
  fi
  echo "template in sync with example (allowlist)"
else
  echo "synced allowlist → $DST"
fi
