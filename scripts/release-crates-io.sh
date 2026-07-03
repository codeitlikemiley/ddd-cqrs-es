#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/release-crates-io.sh <dry-run|publish>

Modes:
  dry-run   Run all checks and validate publishability without uploading.
  publish   Run all checks and publish to crates.io (requires CARGO_REGISTRY_TOKEN).
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

if [[ "$MODE" == "publish" && -z "${CARGO_REGISTRY_TOKEN:-}" ]]; then
  echo "Error: publish mode requires CARGO_REGISTRY_TOKEN environment variable." >&2
  exit 1
fi

PACKAGES=("ddd_cqrs_es" "ddd-cli")

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
  echo "Error: ddd_cqrs_es ($LIB_VERSION) and ddd-cli ($CLI_VERSION) versions must match." >&2
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
cargo test --all-targets -p ddd-cli

if [[ "$MODE" == "dry-run" ]]; then
  echo "Running crates.io dry-run publish checks..."
  for package in "${PACKAGES[@]}"; do
    echo "Dry-run publishing $package..."
    cargo publish -p "$package" --dry-run --allow-dirty
  done
else
  echo "Publishing packages to crates.io..."
  for package in "${PACKAGES[@]}"; do
    echo "Publishing $package..."
    cargo publish -p "$package" --allow-dirty --token "$CARGO_REGISTRY_TOKEN"
  done
fi

echo "Release mode '$MODE' completed successfully."
