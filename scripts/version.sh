#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/version.sh [version]

If version is omitted, patch version is auto-incremented from Cargo.toml.
Examples:
  scripts/version.sh
  scripts/version.sh 0.2.1
USAGE
}

if ! command -v perl >/dev/null 2>&1; then
  echo "Error: perl is required." >&2
  exit 1
fi

CURRENT_VERSION=$(awk -F '"' '/^version = / {print $2; exit}' Cargo.toml)
if [ -z "$CURRENT_VERSION" ]; then
  echo "Error: could not read version from Cargo.toml" >&2
  exit 1
fi

target_version="${1:-}"

if [ -z "$target_version" ]; then
  IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"
  PATCH=$((PATCH + 1))
  target_version="${MAJOR}.${MINOR}.${PATCH}"
fi

if ! [[ "$target_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  usage
  echo "Error: version must follow semver pattern MAJOR.MINOR.PATCH" >&2
  exit 1
fi

perl -0pi -e "s/version = \"${CURRENT_VERSION}\"/version = \"${target_version}\"/" Cargo.toml

echo "Version updated: ${CURRENT_VERSION} -> ${target_version}"
