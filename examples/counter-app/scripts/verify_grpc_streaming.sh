#!/usr/bin/env bash
set -euo pipefail

endpoint="${COUNTER_GRPC_ENDPOINT:-127.0.0.1:3000}"
proto_dir="${COUNTER_PROTO_DIR:-proto}"
headers=()

if ! command -v grpcurl >/dev/null 2>&1; then
  echo "grpcurl is required" >&2
  exit 2
fi

if [[ -n "${COUNTER_AUTH_BEARER_TOKEN:-}" ]]; then
  headers+=( -H "authorization: Bearer ${COUNTER_AUTH_BEARER_TOKEN}" )
fi

grpc() {
  local method="$1"
  shift
  if ((${#headers[@]} > 0)); then
    grpcurl -plaintext -import-path "$proto_dir" -proto counter.proto "${headers[@]}" "$@" "$endpoint" "$method"
  else
    grpcurl -plaintext -import-path "$proto_dir" -proto counter.proto "$@" "$endpoint" "$method"
  fi
}

unary="$(grpc counter.v1.CounterService/Increment -d '{"amount":1}')"
grep -q '"count"' <<<"$unary"

watch="$(grpc counter.v1.CounterService/WatchCounter -max-time 5 -d '{"lastSequence":"0"}' 2>/dev/null || true)"
grep -q '"view"' <<<"$watch"

applied="$({
  printf '%s\n' '{"operation":"CHANGE_OPERATION_INCREMENT","amount":2}'
  printf '%s\n' '{"operation":"CHANGE_OPERATION_DECREMENT","amount":1}'
} | grpc counter.v1.CounterService/ApplyChanges -d @)"
grep -q '"count"' <<<"$applied"

interaction_file="$(mktemp)"
limit_file=""
trap 'rm -f "$interaction_file" "$limit_file"' EXIT
set +e
{
  printf '%s\n' '{"watch":{"lastSequence":"0"}}'
  printf '%s\n' '{"change":{"operation":"CHANGE_OPERATION_INCREMENT","amount":1}}'
  printf '%s\n' '{"change":{"operation":"CHANGE_OPERATION_UNSPECIFIED","amount":1}}'
} | grpc counter.v1.CounterService/Interact -d @ >"$interaction_file" 2>&1
interaction_status=$?
set -e
test "$interaction_status" -ne 0
grep -q '"view"' "$interaction_file"
grep -qi 'InvalidArgument\|invalid argument\|operation is required' "$interaction_file"

limit_file="$(mktemp)"
set +e
for _ in $(seq 1 101); do
  printf '%s\n' '{"operation":"CHANGE_OPERATION_INCREMENT","amount":1}'
done | grpc counter.v1.CounterService/ApplyChanges -d @ >"$limit_file" 2>&1
limit_status=$?
set -e
test "$limit_status" -ne 0
grep -qi 'ResourceExhausted\|resource exhausted\|100-message' "$limit_file"

echo "counter unary, server-streaming, client-streaming, and bidi-streaming checks passed"
