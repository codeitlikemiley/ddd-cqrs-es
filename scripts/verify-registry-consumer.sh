#!/usr/bin/env bash
set -euo pipefail

# Verify the exact registry-only consumer contract after a release. This runs
# outside every repository workspace so local path dependencies and Cargo
# patches cannot mask a missing crates.io publication.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

read_manifest_version() {
    sed -n 's/^version = "\([^"]*\)"/\1/p' "$1" | head -n 1
}

DDD_VERSION="$(read_manifest_version "$REPO_ROOT/Cargo.toml")"
CLI_VERSION="$(read_manifest_version "$REPO_ROOT/crates/ddd-cli/Cargo.toml")"
WASI_AUTH_VERSION="${WASI_AUTH_VERSION:-0.1.0-rc.2}"
LEPTOS_WASI_VERSION="${LEPTOS_WASI_VERSION:-0.4.2-rc.1}"

if [[ -z "$DDD_VERSION" || "$DDD_VERSION" != "$CLI_VERSION" ]]; then
    echo "error: DDD library and CLI versions must be present and equal" >&2
    exit 1
fi

registry_has() {
    local package="$1"
    local version="$2"
    local attempt

    for attempt in 1 2 3 4 5; do
        if (
            cd "$WORK_DIR"
            CARGO_TERM_COLOR=never cargo info --registry crates-io "$package@$version" >/dev/null 2>&1
        ); then
            return 0
        fi
        sleep 2
    done
    return 1
}

require_registry_package() {
    local package="$1"
    local version="$2"
    if ! registry_has "$package" "$version"; then
        echo "error: crates.io does not expose $package@$version" >&2
        exit 1
    fi
    echo "registry package available: $package@$version"
}

require_registry_package leptos-wasi-runtime "$LEPTOS_WASI_VERSION"
require_registry_package wasi-auth "$WASI_AUTH_VERSION"
require_registry_package ddd_cqrs_es "$DDD_VERSION"
require_registry_package ddd-cqrs-es-cli "$CLI_VERSION"

INSTALL_ROOT="$WORK_DIR/cli"
GENERATED_ROOT="$WORK_DIR/generated"
mkdir -p "$INSTALL_ROOT" "$GENERATED_ROOT"

echo "installing ddd-cqrs-es-cli@$CLI_VERSION from crates.io"
cargo install \
    --locked \
    --version "$CLI_VERSION" \
    --root "$INSTALL_ROOT" \
    ddd-cqrs-es-cli

"$INSTALL_ROOT/bin/ddd" \
    --cwd "$GENERATED_ROOT" \
    init fullstack-app \
    --preset fullstack

APP_ROOT="$GENERATED_ROOT/fullstack-app"
if rg -n '^[[:space:]]*path[[:space:]]*=' "$APP_ROOT/Cargo.toml"; then
    echo "error: generated registry consumer contains a local path dependency" >&2
    exit 1
fi

if rg -n 'wasi-authz|ddd-auth|ddd-authz' "$APP_ROOT/Cargo.toml"; then
    echo "error: generated registry consumer contains retired auth packages" >&2
    exit 1
fi

(cd "$APP_ROOT" && cargo generate-lockfile)
cargo check \
    --locked \
    --manifest-path "$APP_ROOT/Cargo.toml" \
    --no-default-features \
    --features migrate \
    --bin wasi-auth-migrate

echo "registry-only generated fullstack consumer passed for DDD $DDD_VERSION and wasi-auth $WASI_AUTH_VERSION"
