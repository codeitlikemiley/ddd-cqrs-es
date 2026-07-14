#!/usr/bin/env bash
# Fail if any Rust source under src/ exceeds the LOC budget (pre-Tailwind cleanup guardrail).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MAX_LOC="${MAX_LOC:-1200}"
# Files allowed to exceed temporarily while still being split (comma-separated basenames).
# Basenames still being split under the modularization plan.
ALLOWLIST="${LOC_ALLOWLIST:-mod.rs,store.rs,auth_product.rs,grpc.rs}"

fail=0
while IFS= read -r -d '' file; do
  base="$(basename "$file")"
  lines="$(wc -l <"$file" | tr -d ' ')"
  if [[ "$lines" -le "$MAX_LOC" ]]; then
    continue
  fi
  if [[ ",$ALLOWLIST," == *",$base,"* ]]; then
    echo "warn: $file is ${lines} LOC (allowlisted; still split down)"
    continue
  fi
  echo "error: $file is ${lines} LOC (budget ${MAX_LOC})"
  fail=1
done < <(find "$ROOT/src" -name '*.rs' -print0)

if [[ "$fail" -ne 0 ]]; then
  echo "LOC budget failed. Split modules or raise MAX_LOC / LOC_ALLOWLIST intentionally."
  exit 1
fi
echo "LOC budget OK (max ${MAX_LOC}; allowlist: ${ALLOWLIST})"
