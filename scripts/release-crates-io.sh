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

bash scripts/verify-docs-rust.sh

echo "Running formatting check..."
cargo fmt --all -- --check

echo "Running cargo check..."
cargo check --all-targets -p ddd_cqrs_es

echo "Running full tests..."
cargo test --all-targets --all-features -p ddd_cqrs_es

echo "Running doc tests..."
cargo test --doc --all-features -p ddd_cqrs_es

if [[ "$MODE" == "dry-run" ]]; then
  echo "Running crates.io dry-run publish check..."
  cargo publish --dry-run --allow-dirty
else
  echo "Publishing to crates.io..."
  cargo publish --allow-dirty --token "$CARGO_REGISTRY_TOKEN"
fi

echo "Release mode '$MODE' completed successfully."
