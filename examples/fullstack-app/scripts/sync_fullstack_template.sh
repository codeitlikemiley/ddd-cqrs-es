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
  # Cargo.toml is mirrored as Cargo.toml.template (see note below).
  build.rs
  compose.yaml
  src
  migrations
  proto
  input.css
  package.json
  package-lock.json
  spin.toml
  spin.production.toml.example
  Makefile
  README.md
  DESIGN.md
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
    echo "error: missing canonical source $SRC/$path" >&2
    exit 1
  fi
  if [[ -d "$SRC/$path" ]]; then
    # Product-domain aggregates (src/domain) are app-specific CLI output and
    # must not dual-sync into the embedded template or wipe user domains.
    if [[ "$path" == "src" ]]; then
      out="$(rsync "${rsync_flags[@]}" \
        --exclude 'target/' \
        --exclude 'node_modules/' \
        --exclude '.DS_Store' \
        --exclude 'domain/' \
        --exclude 'domain_app/' \
        --exclude 'domain_rest.rs' \
        "$SRC/$path/" "$DST/$path/" 2>&1)"
    else
      out="$(rsync "${rsync_flags[@]}" \
        --exclude 'target/' \
        --exclude 'node_modules/' \
        --exclude '.DS_Store' \
        "$SRC/$path/" "$DST/$path/" 2>&1)"
    fi
  elif [[ "$MODE" == "check" ]]; then
    if [[ -f "$DST/$path" ]] && cmp -s "$SRC/$path" "$DST/$path"; then
      out=""
    else
      out="changed file: $path"
    fi
  else
    out="$(rsync "${rsync_flags[@]}" "$SRC/$path" "$DST/$path" 2>&1)"
  fi
  if [[ -n "$out" ]]; then
    echo "$out"
    changed=1
  fi
done

# Nested Cargo.toml is excluded from `cargo package` (treated as another crate).
# Ship it as Cargo.toml.template; the CLI rewrites it to Cargo.toml on init.
if [[ "$MODE" == "check" ]]; then
  if [[ -f "$DST/Cargo.toml.template" ]] && cmp -s "$SRC/Cargo.toml" "$DST/Cargo.toml.template"; then
    :
  else
    echo "changed file: Cargo.toml -> Cargo.toml.template"
    changed=1
  fi
  # stale nested package name must not reappear
  if [[ -f "$DST/Cargo.toml" ]]; then
    echo "error: template still has Cargo.toml (must be Cargo.toml.template only)" >&2
    changed=1
  fi
else
  cp "$SRC/Cargo.toml" "$DST/Cargo.toml.template"
  rm -f "$DST/Cargo.toml"
  echo "mirrored Cargo.toml → Cargo.toml.template"
fi

if [[ "$MODE" == "check" ]]; then
  if [[ "$changed" -eq 1 ]]; then
    echo "error: template drift detected (run scripts/sync_fullstack_template.sh)" >&2
    exit 1
  fi
  echo "template in sync with example (allowlist)"
else
  echo "synced allowlist → $DST"
fi
