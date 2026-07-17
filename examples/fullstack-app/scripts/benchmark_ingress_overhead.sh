#!/usr/bin/env bash

set -Eeuo pipefail

INGRESS_URL="${INGRESS_URL:-${BASE_URL:-${AUTH_PUBLIC_BASE_URL:-http://127.0.0.1:3008}}}"
DIRECT_URL="${DIRECT_URL:-http://127.0.0.1:3010}"
RESULTS_DIR="${RESULTS_DIR:-target/benchmarks/native-ingress-overhead}"
REPETITIONS="${REPETITIONS:-5}"
DURATION="${DURATION:-20s}"
CONCURRENCY="${CONCURRENCY:-100}"
MAX_REGRESSION_PERCENT="${MAX_REGRESSION_PERCENT:-10}"

for command in bash curl python3; do
  command -v "${command}" >/dev/null 2>&1 || {
    echo "error: ${command} is required" >&2
    exit 2
  }
done

if ! [[ "${REPETITIONS}" =~ ^[1-9][0-9]*$ ]] \
  || ! [[ "${CONCURRENCY}" =~ ^[1-9][0-9]*$ ]]; then
  echo "error: repetitions and concurrency must be positive integers" >&2
  exit 2
fi

for target in "${INGRESS_URL}" "${DIRECT_URL}"; do
  status="$(curl -sS -o /dev/null -w '%{http_code}' "${target}/")"
  [[ "${status}" == "200" ]] || {
    echo "error: ${target} returned HTTP ${status}" >&2
    exit 1
  }
done

mkdir -p "${RESULTS_DIR}"

run_profile() {
  local name="$1"
  local target="$2"
  local repetition="$3"
  BASE_URL="${INGRESS_URL}" \
  TARGET_URL_OVERRIDE="${target}/api/authorization/check" \
  RESULTS_DIR="${RESULTS_DIR}/${name}-${repetition}" \
  REPETITIONS=1 \
  DURATION="${DURATION}" \
  CONCURRENCY="${CONCURRENCY}" \
  WARMUP_REQUESTS=100 \
  P99_LIMIT_MS=1000 \
  PROFILE_NAME="${name}-authorization-baseline" \
  bash scripts/benchmark_fullstack.sh >/dev/null
}

for repetition in $(seq 1 "${REPETITIONS}"); do
  if (( repetition % 2 == 1 )); then
    run_profile direct "${DIRECT_URL}" "${repetition}"
    run_profile ingress "${INGRESS_URL}" "${repetition}"
  else
    run_profile ingress "${INGRESS_URL}" "${repetition}"
    run_profile direct "${DIRECT_URL}" "${repetition}"
  fi
done

python3 - "${RESULTS_DIR}" "${REPETITIONS}" "${CONCURRENCY}" \
  "${DURATION}" "${MAX_REGRESSION_PERCENT}" <<'PY'
import json
import pathlib
import statistics
import sys

root = pathlib.Path(sys.argv[1])
repetitions = int(sys.argv[2])
concurrency = int(sys.argv[3])
duration = sys.argv[4]
limit = float(sys.argv[5])
pairs = []

def metrics(name, repetition):
    report = json.loads(
        (root / f"{name}-{repetition}" / "summary.json").read_text(encoding="utf-8")
    )
    sample = report["samples"][0]
    return {
        "requests_per_second": float(sample["requests_per_second"]),
        "p99_ms": float(sample["p99_ms"]),
        "status_failures": int(sample["status_failures"]),
        "transport_failures": int(sample["transport_failures"]),
    }

for repetition in range(1, repetitions + 1):
    direct = metrics("direct", repetition)
    ingress = metrics("ingress", repetition)
    throughput_regression = (
        (direct["requests_per_second"] - ingress["requests_per_second"])
        / direct["requests_per_second"]
        * 100.0
    )
    p99_regression = (
        (ingress["p99_ms"] - direct["p99_ms"]) / direct["p99_ms"] * 100.0
    )
    passed = (
        direct["status_failures"] == 0
        and direct["transport_failures"] == 0
        and ingress["status_failures"] == 0
        and ingress["transport_failures"] == 0
        and throughput_regression <= limit
        and p99_regression <= limit
    )
    pairs.append(
        {
            "repetition": repetition,
            "direct_requests_per_second": round(direct["requests_per_second"], 3),
            "ingress_requests_per_second": round(ingress["requests_per_second"], 3),
            "throughput_regression_percent": round(throughput_regression, 3),
            "direct_p99_ms": round(direct["p99_ms"], 3),
            "ingress_p99_ms": round(ingress["p99_ms"], 3),
            "p99_regression_percent": round(p99_regression, 3),
            "status_failures": direct["status_failures"] + ingress["status_failures"],
            "transport_failures": direct["transport_failures"]
            + ingress["transport_failures"],
            "passed": passed,
        }
    )

summary = {
    "profile": "native-trusted-ingress-paired-protected-authorization",
    "repetitions": repetitions,
    "concurrency": concurrency,
    "duration": duration,
    "max_regression_percent": limit,
    "median_throughput_regression_percent": round(
        statistics.median(pair["throughput_regression_percent"] for pair in pairs), 3
    ),
    "median_p99_regression_percent": round(
        statistics.median(pair["p99_regression_percent"] for pair in pairs), 3
    ),
    "pairs": pairs,
    "promotion_passed": len(pairs) == repetitions and all(pair["passed"] for pair in pairs),
}
(root / "summary.json").write_text(
    json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
print(json.dumps(summary, indent=2, sort_keys=True))
if not summary["promotion_passed"]:
    raise SystemExit(1)
PY
