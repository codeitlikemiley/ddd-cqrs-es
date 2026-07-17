#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXAMPLE_DIR="$ROOT_DIR/examples/fullstack-app"
MODE="${1:---check}"

if [[ "$MODE" != "--check" ]]; then
  echo "Usage: $0 [--check]" >&2
  exit 2
fi

STAGING_DIR="$(mktemp -d)"
trap 'rm -rf "$STAGING_DIR"' EXIT

cargo run --quiet \
  --manifest-path "$ROOT_DIR/Cargo.toml" \
  --package ddd-cqrs-es-cli \
  --bin ddd -- \
  --cwd "$STAGING_DIR" \
  init fullstack-app \
  --preset fullstack

GENERATED_DIR="$STAGING_DIR/fullstack-app"

# Compare against a normalized copy of the example:
# - drop monorepo-only docs/artifacts that are not part of the CLI template
# - strip the local wasi-auth path patch (CLI publish path removes it on init)
COMPARE_DIR="$STAGING_DIR/example-normalized"
mkdir -p "$COMPARE_DIR"
rsync -a \
  --exclude='.DS_Store' \
  --exclude='.env' \
  --exclude='.spin' \
  --exclude='Cargo.lock' \
  --exclude='node_modules' \
  --exclude='target' \
  --exclude='.audit-shots' \
  --exclude='HANDOVER.md' \
  --exclude='REFACTOR_GOAL.md' \
  --exclude='TAILWIND_MIGRATION.md' \
  --exclude='public/favicon.svg' \
  "$EXAMPLE_DIR/" "$COMPARE_DIR/"

# Match render_fullstack: strip monorepo-only wasi-auth path override.
perl -0pi -e 's/# Local wasi-auth for HTML mail templates until the next published rc\.\nwasi-auth = \{ path = "[^"]+" \}\n//' \
  "$COMPARE_DIR/Cargo.toml"

DIFF_EXCLUDES=(
  --exclude=.DS_Store
  --exclude=.env
  --exclude=.spin
  --exclude=Cargo.lock
  --exclude=node_modules
  --exclude=target
)

diff -ru "${DIFF_EXCLUDES[@]}" "$GENERATED_DIR" "$COMPARE_DIR"
echo "fullstack example matches the embedded CLI template"
