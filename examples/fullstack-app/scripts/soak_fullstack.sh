#!/usr/bin/env bash

set -Eeuo pipefail

RESULTS_DIR="${RESULTS_DIR:-target/benchmarks/fullstack-soak}"
SOAK_DURATION="${SOAK_DURATION:-10m}"
CONCURRENCY="${CONCURRENCY:-100}"
SAMPLE_INTERVAL_SECONDS="${SAMPLE_INTERVAL_SECONDS:-5}"
INGRESS_PID="${INGRESS_PID:-}"
SPIN_PID="${SPIN_PID:-}"

for command in ps python3 rg; do
  command -v "${command}" >/dev/null 2>&1 || {
    echo "error: ${command} is required" >&2
    exit 2
  }
done
[[ -n "${INGRESS_PID}" && -n "${SPIN_PID}" ]] || {
  echo "error: INGRESS_PID and SPIN_PID are required" >&2
  exit 2
}
for pid in "${INGRESS_PID}" "${SPIN_PID}"; do
  kill -0 "${pid}" 2>/dev/null || {
    echo "error: process ${pid} is not running" >&2
    exit 2
  }
done

mkdir -p "${RESULTS_DIR}"
memory_csv="${RESULTS_DIR}/memory.csv"
printf 'timestamp_seconds,process,rss_kib\n' >"${memory_csv}"

RESULTS_DIR="${RESULTS_DIR}/load" \
REPETITIONS=1 \
DURATION="${SOAK_DURATION}" \
CONCURRENCY="${CONCURRENCY}" \
WARMUP_REQUESTS=500 \
P99_LIMIT_MS=25 \
bash scripts/benchmark_fullstack.sh &
benchmark_pid=$!
trap 'kill "${benchmark_pid}" 2>/dev/null || true' EXIT

while kill -0 "${benchmark_pid}" 2>/dev/null; do
  timestamp="$(python3 -c 'import time; print(time.time())')"
  for entry in "ingress:${INGRESS_PID}" "spin:${SPIN_PID}"; do
    name="${entry%%:*}"
    pid="${entry##*:}"
    rss="$(ps -o rss= -p "${pid}" | tr -d ' ' || true)"
    [[ "${rss}" =~ ^[0-9]+$ ]] || {
      echo "error: ${name} process ${pid} exited during soak" >&2
      exit 1
    }
    printf '%s,%s,%s\n' "${timestamp}" "${name}" "${rss}" >>"${memory_csv}"
  done
  sleep "${SAMPLE_INTERVAL_SECONDS}"
done
wait "${benchmark_pid}"
trap - EXIT

if rg -n -i \
  'authorization:[[:space:]]*bearer|"refresh_token"[[:space:]]*:|"password"[[:space:]]*:|AUTH_[A-Z0-9_]*SECRET=' \
  .spin/logs >"${RESULTS_DIR}/sensitive-log-findings.txt"; then
  echo "error: potential credential material appeared in Spin logs" >&2
  exit 1
fi

python3 - "${memory_csv}" "${RESULTS_DIR}/soak-summary.json" <<'PY'
import csv
import json
import pathlib
import statistics
import sys

rows = list(csv.DictReader(pathlib.Path(sys.argv[1]).open(encoding="utf-8")))
by_process = {}
for row in rows:
    by_process.setdefault(row["process"], []).append(
        (float(row["timestamp_seconds"]), int(row["rss_kib"]))
    )

results = {}
passed = True
for process, samples in sorted(by_process.items()):
    if len(samples) < 20:
        raise SystemExit(f"insufficient memory samples for {process}: {len(samples)}")
    second_half = samples[len(samples) // 2 :]
    quarter = max(3, len(second_half) // 4)
    start_median = statistics.median(value for _, value in second_half[:quarter])
    end_median = statistics.median(value for _, value in second_half[-quarter:])
    times = [timestamp - second_half[0][0] for timestamp, _ in second_half]
    values = [value for _, value in second_half]
    mean_time = statistics.mean(times)
    mean_value = statistics.mean(values)
    denominator = sum((value - mean_time) ** 2 for value in times)
    slope_kib_per_second = (
        sum((time - mean_time) * (value - mean_value) for time, value in zip(times, values))
        / denominator
        if denominator
        else 0.0
    )
    growth_kib = end_median - start_median
    growth_limit_kib = max(32 * 1024, start_median * 0.10)
    process_passed = not (
        growth_kib > growth_limit_kib and slope_kib_per_second > (1024 / 60)
    )
    passed = passed and process_passed
    results[process] = {
        "samples": len(samples),
        "second_half_start_median_kib": round(start_median, 1),
        "second_half_end_median_kib": round(end_median, 1),
        "second_half_growth_kib": round(growth_kib, 1),
        "slope_kib_per_minute": round(slope_kib_per_second * 60, 1),
        "passed": process_passed,
    }

summary = {"memory": results, "sensitive_log_findings": 0, "passed": passed}
pathlib.Path(sys.argv[2]).write_text(
    json.dumps(summary, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
print(json.dumps(summary, indent=2, sort_keys=True))
if not passed:
    raise SystemExit(1)
PY
