#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/release-crates-io.sh <dry-run|publish>

Modes:
  dry-run   Run all checks and validate publishability without uploading.
  publish   Run all checks and publish to crates.io (requires cargo login or CARGO_REGISTRY_TOKEN).
USAGE
  exit 1
}

MODE="${1:-dry-run}"
if [[ "$MODE" != "dry-run" && "$MODE" != "publish" ]]; then
  usage
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "Error: cargo is required." >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REGISTRY_WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$REGISTRY_WORK_DIR"' EXIT

cd "$REPO_ROOT"

if [[ -n "$(git status --porcelain)" ]]; then
  echo "Error: release requires a clean Git tree." >&2
  git status --short >&2
  exit 1
fi

PACKAGES=("ddd_cqrs_es" "ddd-cqrs-es-cli")

read_manifest_version() {
  local manifest="$1"
  awk -F '"' '/^version = / {print $2; exit}' "$manifest"
}

LIB_VERSION=$(read_manifest_version Cargo.toml)
CLI_VERSION=$(read_manifest_version crates/ddd-cli/Cargo.toml)
if [[ -z "$LIB_VERSION" || -z "$CLI_VERSION" ]]; then
  echo "Error: could not read package versions." >&2
  exit 1
fi
if [[ "$LIB_VERSION" != "$CLI_VERSION" ]]; then
  echo "Error: ddd_cqrs_es ($LIB_VERSION) and ddd-cqrs-es-cli ($CLI_VERSION) versions must match." >&2
  exit 1
fi

echo "Release package version: $LIB_VERSION"

bash scripts/verify-docs-rust.sh

echo "Running formatting check..."
cargo fmt --all -- --check

echo "Running workspace cargo check..."
cargo check --workspace --all-targets

echo "Running library full-feature tests..."
cargo test --all-targets --all-features -p ddd_cqrs_es

echo "Running library doc tests..."
cargo test --doc --all-features -p ddd_cqrs_es

echo "Running CLI tests..."
cargo test --all-targets -p ddd-cqrs-es-cli

crate_version_published() {
  local package="$1"
  local version="$2"

  (
    cd "$REGISTRY_WORK_DIR"
    CARGO_TERM_COLOR=never cargo info --registry crates-io "${package}@${version}" >/dev/null 2>&1
  )
}

wait_for_registry_package() {
  local package="$1"
  local version="$2"
  local attempt

  for attempt in 1 2 3 4 5 6 7 8 9 10; do
    if crate_version_published "$package" "$version"; then
      echo "Verified crates.io package $package@$version"
      return 0
    fi
    sleep 3
  done

  echo "Error: crates.io did not expose $package@$version after publication." >&2
  exit 1
}

require_registry_package() {
  local package="$1"
  local version="$2"
  if ! crate_version_published "$package" "$version"; then
    echo "Error: required crates.io dependency $package@$version is not published." >&2
    exit 1
  fi
  echo "Verified prerequisite $package@$version"
}

echo "Running crates.io dry-run publish checks..."
for package in "${PACKAGES[@]}"; do
  echo "Dry-run publishing $package..."
  cargo publish -p "$package" --locked --dry-run
done

if [[ "$MODE" == "publish" ]]; then
  WASI_AUTH_VERSION="${WASI_AUTH_VERSION:-0.1.0-rc.2}"
  LEPTOS_WASI_VERSION="${LEPTOS_WASI_VERSION:-0.4.2-rc.1}"
  require_registry_package wasi-auth "$WASI_AUTH_VERSION"
  require_registry_package leptos-wasi-runtime "$LEPTOS_WASI_VERSION"

  echo "Publishing packages to crates.io..."
  for package in "${PACKAGES[@]}"; do
    if crate_version_published "$package" "$LIB_VERSION"; then
      echo "Skipping $package v$LIB_VERSION; already published."
      continue
    fi
    echo "Publishing $package..."
    cargo publish -p "$package" --locked
    wait_for_registry_package "$package" "$LIB_VERSION"
  done

  "$SCRIPT_DIR/verify-registry-consumer.sh"
fi

echo "Release mode '$MODE' completed successfully."
