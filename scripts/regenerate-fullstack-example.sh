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
DIFF_EXCLUDES=(
  --exclude=.DS_Store
  --exclude=.env
  --exclude=.spin
  --exclude=Cargo.lock
  --exclude=node_modules
  --exclude=target
)

diff -ru "${DIFF_EXCLUDES[@]}" "$GENERATED_DIR" "$EXAMPLE_DIR"
echo "fullstack example matches the embedded CLI template"
