#!/usr/bin/env bash
# Compile a freshly generated fullstack project the way a consumer gets it.
#
# `ddd init --preset fullstack` writes a manifest whose dependencies all resolve
# from crates.io. That property is easy to lose and was lost once already: the
# template pinned a `wasi-auth` version whose published API did not contain the
# functions the generated source calls, and the gap was invisible here because
# the author's machine patched `wasi-auth` to an unpublished sibling checkout.
# Generating into a temporary directory and compiling with no sibling present is
# what makes that failure reproducible in CI instead of on a user's first build.
#
# The one patch applied is `ddd_cqrs_es` -> this checkout. The template pins the
# exact framework version the CLI ships with, which is unpublished for the whole
# development cycle, so resolving it from crates.io would only ever pass in the
# hours after a release. Patching it to the tree under test is also the stronger
# check: the generated project is compiled against the library as it stands now.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPLATE_MANIFEST="$ROOT_DIR/crates/ddd-cli/templates/fullstack/Cargo.toml.template"

SSR_FEATURES="${SSR_FEATURES:-ssr,postgres,mail-http}"
HYDRATE_FEATURES="${HYDRATE_FEATURES:-hydrate}"

log() {
  echo
  echo "==> $*"
}

# crates.io sparse index path: names of four characters or more live under
# `<first two>/<next two>/<name>`.
index_url_for() {
  local name="$1"
  printf 'https://index.crates.io/%s/%s/%s' "${name:0:2}" "${name:2:2}" "$name"
}

pinned_version_of() {
  local name="$1"
  sed -n "s/^${name} = { version = \"=\([^\"]*\)\".*/\1/p" "$TEMPLATE_MANIFEST" | head -1
}

wasi_auth_version="$(pinned_version_of wasi-auth)"
if [[ -z "$wasi_auth_version" ]]; then
  echo "Error: could not read the pinned wasi-auth version from $TEMPLATE_MANIFEST." >&2
  exit 1
fi

log "Template pins wasi-auth $wasi_auth_version"

# Fetched into a variable first so a network or index failure fails the job.
# Folding it into the version test would turn any transient error into a skip,
# which is the one outcome this job must never reach by accident.
if ! index_entry="$(curl --silent --show-error --fail --location \
  --retry 3 --retry-all-errors "$(index_url_for wasi-auth)")"; then
  echo "Error: could not read wasi-auth from the crates.io index." >&2
  exit 1
fi

if ! sed -n 's/.*"vers":"\([^"]*\)".*/\1/p' <<<"$index_entry" |
  grep -Fxq "$wasi_auth_version"; then
  cat >&2 <<EOF

  SKIPPED: wasi-auth $wasi_auth_version is not on crates.io yet.

  A generated fullstack project cannot resolve its dependencies until that
  version is published, so there is nothing this job can compile. It is not a
  drift failure and not something to fix in this repository: publish
  wasi-auth $wasi_auth_version, and this job starts enforcing on the next run
  with no change here.

EOF
  exit 0
fi

STAGING_DIR="$(mktemp -d)"
trap 'rm -rf "$STAGING_DIR"' EXIT

log "Generating a fullstack project"
cargo run --quiet \
  --manifest-path "$ROOT_DIR/Cargo.toml" \
  --package ddd-cqrs-es-cli \
  --bin ddd -- \
  --cwd "$STAGING_DIR" \
  init fullstack-app \
  --preset fullstack >/dev/null

GENERATED_DIR="$STAGING_DIR/fullstack-app"

if grep -n 'path = ' "$GENERATED_DIR/Cargo.toml"; then
  echo "Error: the generated manifest carries a path dependency; a consumer" \
    "outside this machine cannot resolve it." >&2
  exit 1
fi

PATCH_CONFIG="$STAGING_DIR/framework-patch.toml"
{
  printf '%s\n' '[patch.crates-io]'
  printf 'ddd_cqrs_es = { path = "%s" }\n' "$ROOT_DIR"
} >"$PATCH_CONFIG"

log "Compiling the generated project for the server ($SSR_FEATURES)"
cargo check \
  --manifest-path "$GENERATED_DIR/Cargo.toml" \
  --config "$PATCH_CONFIG" \
  --target wasm32-wasip2 \
  --no-default-features \
  --features "$SSR_FEATURES"

log "Compiling the generated project for the browser ($HYDRATE_FEATURES)"
cargo check \
  --manifest-path "$GENERATED_DIR/Cargo.toml" \
  --config "$PATCH_CONFIG" \
  --target wasm32-unknown-unknown \
  --no-default-features \
  --features "$HYDRATE_FEATURES"

log "Generated fullstack project compiles under ssr and hydrate"
