#!/usr/bin/env bash
set -euo pipefail

artifact="${1:-target/wasm32-wasip2/release/fullstack_app.wasm}"
if [[ ! -f "$artifact" ]]; then
  echo "Error: component artifact not found: $artifact" >&2
  exit 1
fi

world="$(wasm-tools component wit "$artifact")"
if ! grep -q 'export wasi:http/handler@0.3.0;' <<<"$world"; then
  echo "Error: $artifact does not export wasi:http/handler@0.3.0." >&2
  exit 1
fi
if grep -q 'wasi_snapshot_preview1' <<<"$world"; then
  echo "Error: $artifact retains forbidden WASI Preview 1 imports." >&2
  exit 1
fi

echo "Verified final-WASI HTTP export and absence of Preview 1 imports: $artifact"
