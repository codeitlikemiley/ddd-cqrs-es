#!/usr/bin/env bash
set -euo pipefail

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
if (( major < 0 \
   || (major == 0 && minor < 3) \
   || (major == 0 && minor == 3 && patch < 7) )); then
  echo "Error: cargo-leptos $version is too old; >= 0.3.7 is required for wasm-split 0.2.3 artifacts." >&2
  exit 1
fi

if ! rustup target list --installed | grep -qx 'wasm32-wasip2'; then
  echo "Error: install the wasm32-wasip2 Rust target with: rustup target add wasm32-wasip2" >&2
  exit 1
fi
if ! command -v wasm-tools >/dev/null 2>&1; then
  echo "Error: wasm-tools is required for final-WASI artifact inspection." >&2
  exit 1
fi

echo "Verified cargo-leptos $version, wasm32-wasip2, and wasm-tools."
