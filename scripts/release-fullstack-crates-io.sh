#!/usr/bin/env bash
set -euo pipefail

# Cross-repository release gate for the registry-only fullstack consumer.
# This is the only command that publishes the four-package RC chain.

MODE="${1:-dry-run}"
if [[ "$MODE" != "dry-run" && "$MODE" != "publish" ]]; then
    echo "Usage: $0 <dry-run|publish>" >&2
    exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DDD_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WASI_AUTH_ROOT="${WASI_AUTH_REPO:-$DDD_ROOT/../wasi-auth}"
LEPTOS_WASI_ROOT="${LEPTOS_WASI_REPO:-$DDD_ROOT/../leptos_wasi}"
REGISTRY_WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$REGISTRY_WORK_DIR"' EXIT

require_repo() {
    local path="$1"
    local name="$2"
    if [[ ! -f "$path/Cargo.toml" ]]; then
        echo "error: $name repository is missing at $path" >&2
        exit 1
    fi
    if [[ -n "$(cd "$path" && git status --porcelain)" ]]; then
        echo "error: $name repository must have a clean Git tree" >&2
        (cd "$path" && git status --short) >&2
        exit 1
    fi
}

read_version() {
    sed -n 's/^version = "\([^"]*\)"/\1/p' "$1" | head -n 1
}

registry_has() {
    local package="$1"
    local version="$2"
    (
        cd "$REGISTRY_WORK_DIR"
        CARGO_TERM_COLOR=never cargo info --registry crates-io "$package@$version" >/dev/null 2>&1
    )
}

wait_for_registry() {
    local package="$1"
    local version="$2"
    local attempt
    for attempt in 1 2 3 4 5 6 7 8 9 10; do
        if registry_has "$package" "$version"; then
            echo "Verified crates.io package $package@$version"
            return 0
        fi
        sleep 3
    done
    echo "error: crates.io did not expose $package@$version after publication" >&2
    exit 1
}

publish_if_missing() {
    local repo="$1"
    local package="$2"
    local version="$3"

    if registry_has "$package" "$version"; then
        echo "Skipping already-published $package@$version"
        return 0
    fi

    echo "Publishing $package@$version"
    (
        cd "$repo"
        cargo publish --package "$package" --locked
    )
    wait_for_registry "$package" "$version"
}

require_repo "$DDD_ROOT" "DDD"
require_repo "$WASI_AUTH_ROOT" "wasi-auth"
require_repo "$LEPTOS_WASI_ROOT" "leptos_wasi"

DDD_VERSION="$(read_version "$DDD_ROOT/Cargo.toml")"
CLI_VERSION="$(read_version "$DDD_ROOT/crates/ddd-cli/Cargo.toml")"
WASI_AUTH_VERSION="$(read_version "$WASI_AUTH_ROOT/Cargo.toml")"
LEPTOS_WASI_VERSION="$(read_version "$LEPTOS_WASI_ROOT/Cargo.toml")"

if [[ -z "$DDD_VERSION" || "$DDD_VERSION" != "$CLI_VERSION" ]]; then
    echo "error: DDD library and CLI versions must be present and equal" >&2
    exit 1
fi

echo "Release chain: leptos-wasi-runtime@$LEPTOS_WASI_VERSION -> wasi-auth@$WASI_AUTH_VERSION -> ddd_cqrs_es@$DDD_VERSION -> ddd-cqrs-es-cli@$CLI_VERSION"

echo "Running wasi-auth release checks..."
(
    cd "$WASI_AUTH_ROOT"
    cargo fmt --all -- --check
    cargo test --workspace --all-features --locked
    cargo clippy -p wasi-auth --features outbox-worker --all-targets --locked -- -D warnings
    cargo publish --package wasi-auth --locked --dry-run
)

echo "Running leptos_wasi release checks..."
(
    cd "$LEPTOS_WASI_ROOT"
    cargo fmt --all -- --check
    cargo check --locked --all-features
    cargo test --locked
    cargo publish --package leptos-wasi-runtime --locked --dry-run
)

echo "Running DDD release checks..."
(
    cd "$DDD_ROOT"
    make fullstack-check
    make publish dry-run
)

if [[ "$MODE" == "dry-run" ]]; then
    echo "Release dry-run passed. No crates.io package was uploaded."
    exit 0
fi

if [[ -z "${CARGO_REGISTRY_TOKEN:-}" && -z "${CARGO_REGISTRIES_CRATES_IO_TOKEN:-}" && ! -f "${CARGO_HOME:-$HOME/.cargo}/credentials.toml" && ! -f "${CARGO_HOME:-$HOME/.cargo}/credentials" ]]; then
    echo "error: publish mode requires Cargo registry credentials" >&2
    exit 1
fi

publish_if_missing "$LEPTOS_WASI_ROOT" leptos-wasi-runtime "$LEPTOS_WASI_VERSION"
publish_if_missing "$WASI_AUTH_ROOT" wasi-auth "$WASI_AUTH_VERSION"

(
    cd "$DDD_ROOT"
    WASI_AUTH_VERSION="$WASI_AUTH_VERSION" \
        LEPTOS_WASI_VERSION="$LEPTOS_WASI_VERSION" \
        bash scripts/release-crates-io.sh publish
)

(
    cd "$DDD_ROOT"
    WASI_AUTH_VERSION="$WASI_AUTH_VERSION" \
        LEPTOS_WASI_VERSION="$LEPTOS_WASI_VERSION" \
        bash scripts/verify-registry-consumer.sh
)

echo "Fullstack crates.io release and registry-only consumer verification passed."
