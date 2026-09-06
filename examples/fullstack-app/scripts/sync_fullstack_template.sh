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

RSYNC_EXCLUDES=(
  --exclude 'target/'
  --exclude 'node_modules/'
  --exclude '.DS_Store'
)

TEMPLATE_CARGO_FRAMEWORK_PIN='=0.3.0-rc.7'

normalize_cargo_toml() {
  # The CLI rewrites the framework pin on init; compare semantic content only.
  sed -E 's/ddd_cqrs_es = \{ version = "=[^"]+"/ddd_cqrs_es = { version = "=NORMALIZED"/' "$1"
}

write_cargo_template() {
  sed -E "s/ddd_cqrs_es = \{ version = \"=[^\"]+\"/ddd_cqrs_es = { version = \"${TEMPLATE_CARGO_FRAMEWORK_PIN}\"/" \
    "$SRC/Cargo.toml" > "$DST/Cargo.toml.template"
}

files_differ() {
  local left="$1"
  local right="$2"
  if [[ ! -e "$right" ]]; then
    return 0
  fi
  if [[ -d "$left" ]]; then
    ! diff -qr "${RSYNC_EXCLUDES[@]/#/--exclude=}" "$left" "$right" >/dev/null 2>&1
  else
    ! cmp -s "$left" "$right"
  fi
}

changed=0
for path in "${SYNC_PATHS[@]}"; do
  if [[ ! -e "$SRC/$path" ]]; then
    echo "error: missing canonical source $SRC/$path" >&2
    exit 1
  fi
  if [[ "$MODE" == "check" ]]; then
    if [[ -d "$SRC/$path" ]]; then
      if [[ "$path" == "src" ]]; then
        if ! diff -qr \
          --exclude 'target/' \
          --exclude 'node_modules/' \
          --exclude '.DS_Store' \
          --exclude 'domain/' \
          --exclude 'domain_app/' \
          --exclude 'domain_rest.rs' \
          "$SRC/$path" "$DST/$path" >/dev/null 2>&1; then
          echo "changed tree: $path"
          diff -qr \
            --exclude 'target/' \
            --exclude 'node_modules/' \
            --exclude '.DS_Store' \
            --exclude 'domain/' \
            --exclude 'domain_app/' \
            --exclude 'domain_rest.rs' \
            "$SRC/$path" "$DST/$path" | head -20 || true
          changed=1
        fi
      elif files_differ "$SRC/$path" "$DST/$path"; then
        echo "changed tree: $path"
        diff -qr "${RSYNC_EXCLUDES[@]/#/--exclude=}" "$SRC/$path" "$DST/$path" | head -20 || true
        changed=1
      fi
    elif files_differ "$SRC/$path" "$DST/$path"; then
      echo "changed file: $path"
      changed=1
    fi
  elif [[ -d "$SRC/$path" ]]; then
    if [[ "$path" == "src" ]]; then
      rsync -a --delete \
        --exclude 'target/' \
        --exclude 'node_modules/' \
        --exclude '.DS_Store' \
        --exclude 'domain/' \
        --exclude 'domain_app/' \
        --exclude 'domain_rest.rs' \
        "$SRC/$path/" "$DST/$path/"
    else
      rsync -a --delete \
        --exclude 'target/' \
        --exclude 'node_modules/' \
        --exclude '.DS_Store' \
        "$SRC/$path/" "$DST/$path/"
    fi
    echo "synced tree: $path"
  else
    rsync -a "$SRC/$path" "$DST/$path"
    echo "synced file: $path"
  fi
done

# Nested Cargo.toml is excluded from `cargo package` (treated as another crate).
# Ship it as Cargo.toml.template; the CLI rewrites it to Cargo.toml on init.
if [[ "$MODE" == "check" ]]; then
  if [[ ! -f "$DST/Cargo.toml.template" ]]; then
    echo "changed file: Cargo.toml -> Cargo.toml.template (missing destination)" >&2
    changed=1
  elif ! diff -u \
    <(normalize_cargo_toml "$SRC/Cargo.toml") \
    <(normalize_cargo_toml "$DST/Cargo.toml.template") >/dev/null; then
    echo "changed file: Cargo.toml -> Cargo.toml.template"
    diff -u \
      <(normalize_cargo_toml "$SRC/Cargo.toml") \
      <(normalize_cargo_toml "$DST/Cargo.toml.template") | head -20 || true
    changed=1
  fi
  if [[ -f "$DST/Cargo.toml" ]]; then
    echo "error: template still has Cargo.toml (must be Cargo.toml.template only)" >&2
    changed=1
  fi
else
  write_cargo_template
  rm -f "$DST/Cargo.toml"
  echo "mirrored Cargo.toml → Cargo.toml.template (framework pin ${TEMPLATE_CARGO_FRAMEWORK_PIN})"
fi

if [[ "$MODE" == "check" ]]; then
  if [[ "$changed" -eq 1 ]]; then
    echo "error: template drift detected (run scripts/sync_fullstack_template.sh sync from examples/fullstack-app)" >&2
    exit 1
  fi
  echo "template in sync with example (allowlist)"
else
  echo "synced allowlist → $DST"
fi
