#!/usr/bin/env bash
set -euo pipefail

log() {
  echo
  echo "==> $*"
}

command -v cargo >/dev/null 2>&1 || { echo "Error: cargo is required." >&2; exit 1; }

log "Installing Rust WASI target for example check"
rustup target add wasm32-wasip2

log "Running rustfmt"
cargo fmt --all -- --check

log "Compiling library crate"
cargo check --locked --all-targets -p ddd_cqrs_es

log "Running locked clippy with warnings denied"
cargo clippy --locked --all-targets --all-features -p ddd_cqrs_es -- -D warnings

log "Running unit and integration tests"
cargo test --locked --all-targets --all-features -p ddd_cqrs_es

log "Running doc tests"
cargo test --locked --doc --all-features -p ddd_cqrs_es

log "Checking representative feature powerset"
cargo hack check --locked -p ddd_cqrs_es --feature-powerset --depth 2

log "Checking RustSec advisories and dependency policy"
cargo audit --deny warnings
cargo deny check advisories bans licenses sources

log "Inspecting publishable package"
package_archive="$PWD/target/package/ddd_cqrs_es-0.3.0-alpha.1.crate"
rm -f "$package_archive"
cargo package --locked -p ddd_cqrs_es --allow-dirty --no-verify
test -f "$package_archive"
tar -tf "$package_archive" | grep -q '/Cargo.toml$'
if tar -xOf "$package_archive" 'ddd_cqrs_es-0.3.0-alpha.1/Cargo.toml' | grep -Eq 'rustls-rustcrypto|(^|[[:space:]])rsa[[:space:]]*='; then
  echo "Error: quarantined direct-TCP dependencies leaked into the package." >&2
  exit 1
fi

log "Running docs hardening checks"
bash scripts/verify-docs-rust.sh

log "Running CLI tests and generated fullstack drift check"
cargo test --all-targets -p ddd-cqrs-es-cli
bash scripts/regenerate-fullstack-example.sh --check

log "Compiling counter-app example with sqlite"
patch_config="$PWD/target/ci-local-patches.toml"
mkdir -p "$(dirname "$patch_config")"
{
  printf '%s\n' '[patch.crates-io]'
  printf 'ddd_cqrs_es = { path = "%s" }\n' "$PWD"
  if [[ -f ../wasi-auth/crates/wasi-auth/Cargo.toml ]]; then
    printf 'wasi-auth = { path = "%s" }\n' "$PWD/../wasi-auth/crates/wasi-auth"
  fi
  if [[ -f ../leptos_wasi/Cargo.toml ]]; then
    printf 'leptos_wasi = { path = "%s" }\n' "$PWD/../leptos_wasi"
  fi
} >"$patch_config"
local_patch_args="--config $patch_config"
make example-check CARGO_CONFIG_ARGS="$local_patch_args"

log "CI check suite complete"
