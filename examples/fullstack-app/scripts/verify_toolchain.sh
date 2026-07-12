#!/usr/bin/env bash
set -euo pipefail

required_leptos_major=0
required_leptos_minor=3
required_leptos_patch=7

if ! command -v cargo >/dev/null 2>&1; then
  echo "Error: cargo is required." >&2
  exit 1
fi

if ! cargo leptos --version >/dev/null 2>&1; then
  echo "Error: cargo-leptos >= 0.3.7 is required. Install it with:" >&2
  echo "  cargo install --locked cargo-leptos --version '^0.3.7'" >&2
  exit 1
fi

version="$(cargo leptos --version | awk '{print $2}')"
numeric="${version%%-*}"
IFS=. read -r major minor patch <<<"$numeric"
major="${major:-0}"
minor="${minor:-0}"
patch="${patch:-0}"
if (( major < required_leptos_major \
   || (major == required_leptos_major && minor < required_leptos_minor) \
   || (major == required_leptos_major && minor == required_leptos_minor && patch < required_leptos_patch) )); then
  echo "Error: cargo-leptos $version is too old; >= 0.3.7 is required for wasm-split 0.2.3 artifacts." >&2
  exit 1
fi

if ! rustup target list --installed | grep -qx 'wasm32-wasip2'; then
  echo "Error: the wasm32-wasip2 Rust target is required. Install it with:" >&2
  echo "  rustup target add wasm32-wasip2" >&2
  exit 1
fi

if ! command -v wasm-tools >/dev/null 2>&1; then
  echo "Error: wasm-tools is required for final-WASI artifact inspection." >&2
  exit 1
fi

echo "Verified cargo-leptos $version, wasm32-wasip2, and wasm-tools."
