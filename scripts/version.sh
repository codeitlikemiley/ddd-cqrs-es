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

LIB_MANIFEST="Cargo.toml"
CLI_MANIFEST="crates/ddd-cli/Cargo.toml"

read_manifest_version() {
  local manifest="$1"
  awk -F '"' '/^version = / {print $2; exit}' "$manifest"
}

update_manifest_version() {
  local manifest="$1"
  local current="$2"
  local target="$3"
  CURRENT_VERSION="$current" TARGET_VERSION="$target" \
    perl -0pi -e 's/version = "\Q$ENV{CURRENT_VERSION}\E"/version = "$ENV{TARGET_VERSION}"/' "$manifest"
}

update_lock_version() {
  local package="$1"
  local current="$2"
  local target="$3"

  if [ ! -f Cargo.lock ]; then
    return
  fi

  PACKAGE_NAME="$package" CURRENT_VERSION="$current" TARGET_VERSION="$target" \
    perl -0pi -e 'my $name = $ENV{PACKAGE_NAME}; my $current = $ENV{CURRENT_VERSION}; my $target = $ENV{TARGET_VERSION}; s/(\[\[package\]\]\nname = "\Q$name\E"\nversion = ")\Q$current\E(")/$1$target$2/g' Cargo.lock
}

if [ ! -f "$CLI_MANIFEST" ]; then
  echo "Error: missing CLI manifest at $CLI_MANIFEST" >&2
  exit 1
fi

CURRENT_VERSION=$(read_manifest_version "$LIB_MANIFEST")
CLI_CURRENT_VERSION=$(read_manifest_version "$CLI_MANIFEST")
if [ -z "$CURRENT_VERSION" ]; then
  echo "Error: could not read version from $LIB_MANIFEST" >&2
  exit 1
fi
if [ -z "$CLI_CURRENT_VERSION" ]; then
  echo "Error: could not read version from $CLI_MANIFEST" >&2
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

update_manifest_version "$LIB_MANIFEST" "$CURRENT_VERSION" "$target_version"
update_manifest_version "$CLI_MANIFEST" "$CLI_CURRENT_VERSION" "$target_version"
update_lock_version "ddd_cqrs_es" "$CURRENT_VERSION" "$target_version"
update_lock_version "ddd-cqrs-es-cli" "$CLI_CURRENT_VERSION" "$target_version"

echo "Version updated:"
echo "  ddd_cqrs_es: ${CURRENT_VERSION} -> ${target_version}"
echo "  ddd-cqrs-es-cli: ${CLI_CURRENT_VERSION} -> ${target_version}"
