#!/usr/bin/env bash

set -Eeuo pipefail

trap 'echo "error: benchmark command failed at line ${LINENO}" >&2' ERR

BASE_URL="${BASE_URL:-http://127.0.0.1:3008}"
RESULTS_DIR="${RESULTS_DIR:-target/benchmarks/fullstack-authz}"
REPETITIONS="${REPETITIONS:-5}"
DURATION="${DURATION:-60s}"
CONCURRENCY="${CONCURRENCY:-100}"
P99_LIMIT_MS="${P99_LIMIT_MS:-25}"
WARMUP_REQUESTS="${WARMUP_REQUESTS:-100}"
CREDENTIAL_MODE="${CREDENTIAL_MODE:-bearer}"
TARGET_URLS_FILE="${TARGET_URLS_FILE:-}"
TARGET_URL_OVERRIDE="${TARGET_URL_OVERRIDE:-}"
PROFILE_NAME="${PROFILE_NAME:-native-ingress-spin-postgres-embedded-cedar-${CREDENTIAL_MODE}}"

for command in curl jq oha python3; do
  command -v "${command}" >/dev/null 2>&1 || {
    echo "error: ${command} is required" >&2
    exit 2
  }
done

if ! [[ "${REPETITIONS}" =~ ^[1-9][0-9]*$ ]]   || ! [[ "${CONCURRENCY}" =~ ^[1-9][0-9]*$ ]]   || ! [[ "${WARMUP_REQUESTS}" =~ ^[1-9][0-9]*$ ]]; then
  echo "error: repetitions, concurrency, and warmup requests must be positive integers" >&2
  exit 2
fi

mkdir -p "${RESULTS_DIR}"
oha_target="${BASE_URL}/api/authorization/check"
replica_count=1
if [[ -n "${TARGET_URLS_FILE}" && -n "${TARGET_URL_OVERRIDE}" ]]; then
  echo "error: TARGET_URLS_FILE and TARGET_URL_OVERRIDE are mutually exclusive" >&2
  exit 2
elif [[ -n "${TARGET_URL_OVERRIDE}" ]]; then
  oha_target="${TARGET_URL_OVERRIDE}"
elif [[ -n "${TARGET_URLS_FILE}" ]]; then
  [[ -f "${TARGET_URLS_FILE}" ]] || {
    echo "error: TARGET_URLS_FILE does not exist" >&2
    exit 2
  }
  replica_count="$(awk 'NF { count += 1 } END { print count + 0 }' "${TARGET_URLS_FILE}")"
  [[ "${replica_count}" -gt 0 ]] || {
    echo "error: TARGET_URLS_FILE contains no targets" >&2
    exit 2
  }
  oha_target="${TARGET_URLS_FILE}"
fi
email="benchmark-$(date +%s)-$$@example.test"
password="benchmark-correct-horse-123"
session_id=""
access_token=""

cleanup() {
  if [[ -n "${access_token}" ]]; then
    curl -sS -X POST "${BASE_URL}/api/auth/logout"       -H "Authorization: Bearer ${access_token}"       >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

json_post() {
  local path="$1"
  local body="$2"
  shift 2
  curl -fsS -X POST "${BASE_URL}${path}"     -H "content-type: application/json"     "$@"     --data "${body}"
}

registration="$(json_post /api/auth/password/register   "{\"email\":\"${email}\",\"password\":\"${password}\",\"redirect_url\":\"/dashboard\"}")"
jq -e '.authenticated == false and .redirect_url == "/verify-email/pending"'   <<<"${registration}" >/dev/null

verification_mail=""
for _ in $(seq 1 50); do
  verification_mail="$(curl -fsS -G "${BASE_URL}/api/auth/dev/mail/latest"     --data-urlencode "recipient=${email}"     --data-urlencode "kind=email-verification" 2>/dev/null || true)"
  [[ -n "${verification_mail}" ]] && break
  sleep 0.1
done
[[ -n "${verification_mail}" ]] || {
  echo "error: verification mail was not captured" >&2
  exit 1
}

verification_path="$(jq -er '.body_text' <<<"${verification_mail}")"
verification_token="${verification_path##*token=}"
[[ -n "${verification_token}" && "${verification_token}" != "${verification_path}" ]] || {
  echo "error: captured verification token is invalid" >&2
  exit 1
}

verification="$(json_post /api/auth/email/verify   "{\"token\":\"${verification_token}\",\"redirect_url\":\"/dashboard\"}")"
session_id="$(jq -er '.session_id' <<<"${verification}")"
initial_refresh_token="$(jq -er '.refresh_token' <<<"${verification}")"
session_cookie="Cookie: wasi_auth_dev_session=${session_id}"
csrf_token="$(curl -fsS "${BASE_URL}/api/auth/csrf"   -H "${session_cookie}" | jq -er '.token')"

organization="$(json_post /api/organizations   "{\"name\":\"Benchmark Organization\",\"slug\":\"bench-org\"}"   -H "${session_cookie}"   -H "x-csrf-token: ${csrf_token}")"
organization_id="$(jq -er '.organization_id' <<<"${organization}")"

refreshed="$(json_post /api/auth/token/refresh   "{\"refresh_token\":\"${initial_refresh_token}\"}"   -H "${session_cookie}"   -H "x-csrf-token: ${csrf_token}")"
access_token="$(jq -er '.access_token' <<<"${refreshed}")"
verified="$(json_post /api/auth/token/verify   "{\"access_token\":\"${access_token}\"}")"
token_tenant="$(jq -er '.tenant_id' <<<"${verified}")"
[[ "${token_tenant}" == "${organization_id}" ]] || {
  echo "error: refreshed benchmark token did not bind the created organization" >&2
  exit 1
}

case "${CREDENTIAL_MODE}" in
  bearer) credential_header="Authorization: Bearer ${access_token}" ;;
  cookie) credential_header="Cookie: wasi_auth_dev_session=${session_id}" ;;
  *)
    echo "error: CREDENTIAL_MODE must be bearer or cookie" >&2
    exit 2
    ;;
esac

request_body="$(jq -cn   --arg organization_id "${organization_id}"   '{
    action: "organization.view",
    resource_type: "Organization",
    resource_id: $organization_id,
    organization_id: $organization_id
  }')"

probe="$(json_post /api/authorization/check "${request_body}"   -H "${credential_header}")"
if ! jq -e '.allowed == true and (.policy_revision | length > 0)'   <<<"${probe}" >/dev/null; then
  echo "error: authenticated Cedar probe was denied: ${probe}" >&2
  exit 1
fi

warmup_per_target=$(((WARMUP_REQUESTS + replica_count - 1) / replica_count))
warm_target() {
  local target="$1"
  local response
  response="$(curl -fsS -X POST "${target}"     -H "content-type: application/json"     -H "${credential_header}"     --data "${request_body}")"
  jq -e '.allowed == true' <<<"${response}" >/dev/null || {
    echo "error: replica authorization probe was denied" >&2
    exit 1
  }
  for _ in $(seq 1 "${warmup_per_target}"); do
    curl -fsS -X POST "${target}"       -H "content-type: application/json"       -H "${credential_header}"       --data "${request_body}" >/dev/null
  done
}

if [[ -n "${TARGET_URLS_FILE}" ]]; then
  while IFS= read -r target; do
    [[ -n "${target}" ]] && warm_target "${target}"
  done <"${TARGET_URLS_FILE}"
else
  warm_target "${oha_target}"
fi

for repetition in $(seq 1 "${REPETITIONS}"); do
  report="${RESULTS_DIR}/sample-${repetition}.json"
  if [[ -n "${TARGET_URLS_FILE}" ]]; then
    NO_COLOR=true oha     -z "${DURATION}"     -w     -c "${CONCURRENCY}"     -t 5s     --no-tui     --output-format json     --output "${report}"     --method POST     -H "content-type: application/json"     -H "${credential_header}"     -d "${request_body}"     --urls-from-file     "${oha_target}"
  else
    NO_COLOR=true oha     -z "${DURATION}"     -w     -c "${CONCURRENCY}"     -t 5s     --no-tui     --output-format json     --output "${report}"     --method POST     -H "content-type: application/json"     -H "${credential_header}"     -d "${request_body}"     "${oha_target}"
  fi
done

revocation_started_ns="$(python3 -c 'import time; print(time.time_ns())')"
logout_status="$(curl -sS -o /dev/null -w '%{http_code}' -X POST "${BASE_URL}/api/auth/logout" \
  -H "Authorization: Bearer ${access_token}")"
[[ "${logout_status}" == "200" ]] || {
  echo "error: benchmark session revocation returned HTTP ${logout_status}" >&2
  exit 1
}
access_token=""
revoked_status=""
for _ in $(seq 1 50); do
  revoked_status="$(curl -sS -o /dev/null -w '%{http_code}' -X POST \
    "${BASE_URL}/api/authorization/check" \
    -H "content-type: application/json" \
    -H "${credential_header}" \
    --data "${request_body}")"
  [[ "${revoked_status}" == "401" ]] && break
  sleep 0.02
done
[[ "${revoked_status}" == "401" ]] || {
  echo "error: revoked benchmark session remained authorized (HTTP ${revoked_status})" >&2
  exit 1
}
python3 - "${RESULTS_DIR}/revocation.json" "${revocation_started_ns}" <<'PY'
import json
import pathlib
import sys
import time

elapsed_ms = (time.time_ns() - int(sys.argv[2])) / 1_000_000
pathlib.Path(sys.argv[1]).write_text(
    json.dumps({"revoked_status": 401, "propagation_ms": round(elapsed_ms, 3)}, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY

python3 - "${RESULTS_DIR}" "${REPETITIONS}" "${CONCURRENCY}"   "${DURATION}" "${P99_LIMIT_MS}" "${CREDENTIAL_MODE}" "${replica_count}" "${PROFILE_NAME}" <<'PY'
import json
import pathlib
import statistics
import sys

root = pathlib.Path(sys.argv[1])
repetitions = int(sys.argv[2])
concurrency = int(sys.argv[3])
duration = sys.argv[4]
p99_limit_ms = float(sys.argv[5])
credential_mode = sys.argv[6]
replica_count = int(sys.argv[7])
profile_name = sys.argv[8]
samples = []
promotion_passed = True

for repetition in range(1, repetitions + 1):
    path = root / f"sample-{repetition}.json"
    report = json.loads(path.read_text(encoding="utf-8"))
    summary = report["summary"]
    statuses = report.get("statusCodeDistribution", {})
    errors = report.get("errorDistribution", {})
    p99_ms = float(report["latencyPercentiles"]["p99"]) * 1000.0
    requests_per_second = float(summary["requestsPerSec"])
    status_failures = sum(
        int(count) for status, count in statuses.items() if int(status) != 200
    )
    transport_failures = sum(int(count) for count in errors.values())
    passed = (
        float(summary["successRate"]) == 1.0
        and status_failures == 0
        and transport_failures == 0
        and p99_ms <= p99_limit_ms
    )
    promotion_passed = promotion_passed and passed
    samples.append(
        {
            "repetition": repetition,
            "requests_per_second": round(requests_per_second, 3),
            "p99_ms": round(p99_ms, 3),
            "status_failures": status_failures,
            "transport_failures": transport_failures,
            "passed": passed,
        }
    )

summary = {
    "profile": profile_name,
    "replicas": replica_count,
    "repetitions": repetitions,
    "concurrency": concurrency,
    "duration": duration,
    "p99_limit_ms": p99_limit_ms,
    "requests_per_second_median": round(
        statistics.median(sample["requests_per_second"] for sample in samples), 3
    ),
    "p99_ms_max": max(sample["p99_ms"] for sample in samples),
    "samples": samples,
    "promotion_passed": promotion_passed and len(samples) == repetitions,
}
(root / "summary.json").write_text(
    json.dumps(summary, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
print(json.dumps(summary, indent=2, sort_keys=True))
if not summary["promotion_passed"]:
    raise SystemExit(1)
PY
