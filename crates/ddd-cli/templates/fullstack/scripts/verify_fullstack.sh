#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:3008}"
RUN_GRPC="${RUN_GRPC:-0}"
CHECK_REFRESH_TOKEN_EXPIRY="${CHECK_REFRESH_TOKEN_EXPIRY:-0}"
CHECK_ES256_JWKS="${CHECK_ES256_JWKS:-0}"
CHECK_OAUTH_STATE="${CHECK_OAUTH_STATE:-0}"
CHECK_OAUTH_REDIRECT_COOKIE="${CHECK_OAUTH_REDIRECT_COOKIE:-0}"
EXPECT_COOKIE_SECURE="${EXPECT_COOKIE_SECURE:-0}"
CHECK_SIGNING_KEY_ROTATION="${CHECK_SIGNING_KEY_ROTATION:-0}"
CHECK_PASSKEYS="${CHECK_PASSKEYS:-0}"
CHECK_PASSKEY_EXPIRY="${CHECK_PASSKEY_EXPIRY:-0}"
CHECK_STORAGE_EVENTS="${CHECK_STORAGE_EVENTS:-0}"
CHECK_ATOMIC_ROLLBACK="${CHECK_ATOMIC_ROLLBACK:-0}"
SMOKE_EMAIL="${SMOKE_EMAIL:-smoke-auth-$(date +%s)-$$@example.test}"
SIGNING_KEY_ROTATE_FROM_KID="${SIGNING_KEY_ROTATE_FROM_KID:-fullstack-app-key-a}"
SIGNING_KEY_ROTATE_TO_KID="${SIGNING_KEY_ROTATE_TO_KID:-fullstack-app-key-b}"
REFRESH_EXPIRY_WAIT_SECONDS="${REFRESH_EXPIRY_WAIT_SECONDS:-2}"
PASSKEY_EXPIRY_WAIT_SECONDS="${PASSKEY_EXPIRY_WAIT_SECONDS:-2}"
PROTO_DIR="${PROTO_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/proto}"

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Error: $1 is required." >&2
    exit 1
  fi
}

status_code() {
  local method="$1"
  local url="$2"
  shift 2
  curl -sS -o /tmp/fullstack-app-smoke-body.json -w '%{http_code}' -X "$method" "$url" "$@"
}

header_value() {
  local header_file="$1"
  local header_name="$2"
  local normalized_name
  normalized_name="$(printf '%s' "$header_name" | tr '[:upper:]' '[:lower:]')"
  awk -v name="$normalized_name" '
    BEGIN { FS=": " }
    tolower($1) == name { value=$2 }
    END { gsub(/\r$/, "", value); print value }
  ' "$header_file"
}

assert_status() {
  local expected="$1"
  local method="$2"
  local url="$3"
  shift 3
  local actual
  actual="$(status_code "$method" "$url" "$@")"
  if [[ "$actual" != "$expected" ]]; then
    echo "Expected $method $url to return $expected, got $actual" >&2
    cat /tmp/fullstack-app-smoke-body.json >&2 || true
    exit 1
  fi
}

assert_error() {
  local expected_status="$1"
  local expected_code="$2"
  local method="$3"
  local url="$4"
  shift 4
  local actual
  actual="$(status_code "$method" "$url" "$@")"
  if [[ "$actual" != "$expected_status" ]]; then
    echo "Expected $method $url to return $expected_status, got $actual" >&2
    cat /tmp/fullstack-app-smoke-body.json >&2 || true
    exit 1
  fi
  if ! jq -e --arg code "$expected_code" '.error.code == $code' /tmp/fullstack-app-smoke-body.json >/dev/null; then
    echo "Expected $method $url to return error code $expected_code" >&2
    cat /tmp/fullstack-app-smoke-body.json >&2 || true
    exit 1
  fi
}

assert_redirect() {
  local url="$1"
  local expected_location="$2"
  shift 2
  local headers
  headers="$(mktemp)"
  curl -sS -D "$headers" -o /tmp/fullstack-app-smoke-body.txt "$url" "$@"
  local location
  location="$(header_value "$headers" location)"
  rm -f "$headers"
  if [[ "$location" != "$expected_location" ]]; then
    echo "Expected $url to redirect to $expected_location, got ${location:-<none>}" >&2
    cat /tmp/fullstack-app-smoke-body.txt >&2 || true
    exit 1
  fi
}

json_post() {
  local path="$1"
  local body="$2"
  shift 2
  curl -sS -X POST "$BASE_URL$path" \
    -H 'content-type: application/json' \
    "$@" \
    --data "$body"
}

json_post_bearer() {
  local path="$1"
  local body="$2"
  local token="$3"
  shift 3
  json_post "$path" "$body" -H "authorization: Bearer $token" "$@"
}

totp_code() {
  python3 - "$1" <<'PY'
import base64
import hashlib
import hmac
import struct
import sys
import time

secret = sys.argv[1].strip().upper()
secret += "=" * ((8 - len(secret) % 8) % 8)
key = base64.b32decode(secret)
counter = int(time.time()) // 30
digest = hmac.new(key, struct.pack(">Q", counter), hashlib.sha1).digest()
offset = digest[-1] & 0x0F
binary = struct.unpack(">I", digest[offset:offset + 4])[0] & 0x7FFFFFFF
print(f"{binary % 1_000_000:06d}")
PY
}

run_oauth_state_check() {
  jq -e '
    .oauth_enabled == true
    and any(.providers[]; .provider_id == "google")
    and any(.providers[]; .provider_id == "facebook")
  ' /tmp/fullstack-app-capabilities.json >/dev/null

  local start_response
  start_response="$(curl -sS -f "$BASE_URL/api/auth/oauth/google/start?next=/dashboard")"
  local state
  state="$(jq -r '.state' <<<"$start_response")"
  if [[ -z "$state" || "$state" == "null" ]]; then
    echo "OAuth start did not return state: $start_response" >&2
    exit 1
  fi
  jq -e --arg state "$state" '
    .provider_id == "google"
    and .state == $state
    and (.authorization_url | contains("/api/auth/oauth/google/callback"))
    and (.authorization_url | contains("code=development-oauth-code"))
    and (.authorization_url | contains("next=%2Fdashboard"))
  ' <<<"$start_response" >/dev/null

  if [[ "$CHECK_OAUTH_REDIRECT_COOKIE" == "1" ]]; then
    local cookie_start_response
    cookie_start_response="$(curl -sS -f "$BASE_URL/api/auth/oauth/google/start?next=/dashboard")"
    local cookie_state
    cookie_state="$(jq -r '.state' <<<"$cookie_start_response")"
    local headers
    headers="$(mktemp)"
    curl -sS -D "$headers" -o /tmp/fullstack-app-smoke-body.txt \
      "$BASE_URL/api/auth/oauth/google/callback?code=development-oauth-code&state=$cookie_state"
    local location
    location="$(header_value "$headers" location)"
    local set_cookie
    set_cookie="$(header_value "$headers" set-cookie)"
    rm -f "$headers"
    if [[ "$location" != "/dashboard" ]]; then
      echo "Expected OAuth browser callback to redirect to /dashboard, got ${location:-<none>}" >&2
      cat /tmp/fullstack-app-smoke-body.txt >&2 || true
      exit 1
    fi
    case "$set_cookie" in
      *wasi_auth_dev_session=*HttpOnly*SameSite=Lax*) ;;
      *)
        echo "Expected OAuth browser callback Set-Cookie to include wasi_auth_dev_session, HttpOnly, and SameSite=Lax" >&2
        echo "set-cookie: ${set_cookie:-<none>}" >&2
        exit 1
        ;;
    esac
    if [[ "$EXPECT_COOKIE_SECURE" == "1" && "$set_cookie" != *Secure* ]]; then
      echo "Expected OAuth browser callback Set-Cookie to include Secure" >&2
      echo "set-cookie: $set_cookie" >&2
      exit 1
    fi
  fi

  local callback_response
  callback_response="$(curl -sS -f "$BASE_URL/api/auth/oauth/google/callback?code=development-oauth-code&state=$state&next=https://evil.example&format=json")"
  jq -e '
    .authenticated == true
    and .redirect_url == "/dashboard"
    and (.session_id | type == "string" and length > 0)
    and (.access_token | type == "string" and length > 0)
    and (.refresh_token | type == "string" and length > 0)
  ' <<<"$callback_response" >/dev/null

  assert_error 409 conflict GET "$BASE_URL/api/auth/oauth/google/callback?code=development-oauth-code&state=$state&format=json"

  start_response="$(curl -sS -f "$BASE_URL/api/auth/oauth/google/start?next=/dashboard")"
  state="$(jq -r '.state' <<<"$start_response")"
  assert_error 400 validation GET "$BASE_URL/api/auth/oauth/facebook/callback?code=development-oauth-code&state=$state&format=json"

  start_response="$(curl -sS -f "$BASE_URL/api/auth/oauth/google/start?next=/dashboard")"
  state="$(jq -r '.state' <<<"$start_response")"
  assert_error 400 validation GET "$BASE_URL/api/auth/oauth/google/callback?code=not-the-dev-code&state=$state&format=json"

  assert_error 400 validation GET "$BASE_URL/api/auth/oauth/google/callback?code=development-oauth-code&state=not-a-real-state&format=json"

  echo "fullstack-app smoke: OAuth state validation passed"
}

mint_hs256_jwt() {
  local issuer="$1"
  local audience="$2"
  local key_id="$3"
  local session_id="$4"
  local expires_delta="$5"
  python3 - "$issuer" "$audience" "$key_id" "$session_id" "$expires_delta" <<'PY'
import base64
import hashlib
import hmac
import json
import os
import sys
import time

issuer, audience, key_id, session_id, expires_delta = sys.argv[1:6]
now = int(time.time())
exp = now + int(expires_delta)
secret = os.environ.get("AUTH_JWT_SECRET", "dev-fullstack-app-secret-change-me").encode("utf-8")

def b64(value):
    return base64.urlsafe_b64encode(value).rstrip(b"=").decode("ascii")

header = {"typ": "JWT", "alg": "HS256", "kid": key_id}
payload = {
    "iss": issuer,
    "sub": "user:jwt-smoke",
    "aud": [audience],
    "exp": exp,
    "iat": now,
    "jti": "jwt-smoke",
    "tenant_id": "tenant:default",
    "session_id": session_id,
    "roles": [],
    "scope": ["auth:session:read"],
    "auth_time": now,
    "extra": {},
}
signing_input = ".".join([
    b64(json.dumps(header, separators=(",", ":")).encode()),
    b64(json.dumps(payload, separators=(",", ":")).encode()),
])
signature = hmac.new(secret, signing_input.encode("ascii"), hashlib.sha256).digest()
print(f"{signing_input}.{b64(signature)}")
PY
}

jwt_kid() {
  python3 - "$1" <<'PY'
import base64
import json
import sys

token = sys.argv[1]
header_segment = token.split(".", 1)[0]
padding = "=" * (-len(header_segment) % 4)
header = json.loads(base64.urlsafe_b64decode(header_segment + padding))
print(header.get("kid", ""))
PY
}

run_signing_key_rotation_check() {
  assert_error 401 auth_required GET "$BASE_URL/api/auth/signing-keys"

  local list_response
  list_response="$(curl -sS -f "$BASE_URL/api/auth/signing-keys" \
    -H "$session_cookie")"
  jq -e --arg kid "$SIGNING_KEY_ROTATE_FROM_KID" '
    any(.keys[]; .kid == $kid and .active == true and .status == "active")
  ' <<<"$list_response" >/dev/null

  local initial_kid
  initial_kid="$(jwt_kid "$access_token")"
  if [[ "$initial_kid" != "$SIGNING_KEY_ROTATE_FROM_KID" ]]; then
    echo "Expected initial JWT kid $SIGNING_KEY_ROTATE_FROM_KID, got $initial_kid" >&2
    exit 1
  fi

  local rotate_response
  rotate_response="$(curl -sS -f -X POST "$BASE_URL/api/auth/signing-keys/rotate" \
    -H "$session_cookie" \
    -H "x-csrf-token: $csrf_token" \
    -H 'content-type: application/json' \
    --data "{\"kid\":\"$SIGNING_KEY_ROTATE_TO_KID\",\"retire_previous\":true}")"
  jq -e \
    --arg active "$SIGNING_KEY_ROTATE_TO_KID" \
    --arg previous "$SIGNING_KEY_ROTATE_FROM_KID" '
      .active_kid == $active
      and .previous_kid == $previous
      and .retired_previous == true
      and any(.keys[]; .kid == $active and .active == true and .status == "active")
      and any(.keys[]; .kid == $previous and .active == false and .status == "retired")
    ' <<<"$rotate_response" >/dev/null

  local rotated_refresh_response
  rotated_refresh_response="$(curl -sS -f -X POST "$BASE_URL/api/auth/token/refresh" \
    -H "$session_cookie" \
    -H 'content-type: application/json' \
    --data "{\"refresh_token\":\"$refresh_token\"}")"
  local rotated_access_token
  rotated_access_token="$(jq -r '.access_token' <<<"$rotated_refresh_response")"
  local rotated_kid
  rotated_kid="$(jwt_kid "$rotated_access_token")"
  if [[ "$rotated_kid" != "$SIGNING_KEY_ROTATE_TO_KID" ]]; then
    echo "Expected rotated JWT kid $SIGNING_KEY_ROTATE_TO_KID, got $rotated_kid" >&2
    exit 1
  fi

  json_post /api/auth/token/verify "{\"access_token\":\"$access_token\"}" \
    | jq -e --arg session_id "$session_id" '.active == true and .session_id == $session_id' >/dev/null
  json_post /api/auth/token/verify "{\"access_token\":\"$rotated_access_token\"}" \
    | jq -e --arg session_id "$session_id" '.active == true and .session_id == $session_id' >/dev/null

  echo "fullstack-app smoke: signing key rotation passed"
}

run_passkey_check() {
  jq -e '.passkeys_enabled == true' /tmp/fullstack-app-capabilities.json >/dev/null

  local passkey_email
  passkey_email="passkey-smoke-$(date +%s)-$$@example.test"
  assert_error 401 invalid_credentials POST "$BASE_URL/api/auth/passkeys/login/options" \
    -H 'content-type: application/json' \
    --data "{\"email\":\"$passkey_email\",\"redirect_url\":\"/dashboard\"}"
  assert_error 401 auth_required POST "$BASE_URL/api/auth/passkeys/register/options" \
    -H 'content-type: application/json' \
    --data "{\"email\":\"$passkey_email\",\"redirect_url\":\"/dashboard\"}"

  local register_response verification_mail verification_path verification_token
  register_response="$(json_post /api/auth/password/register \
    "{\"email\":\"$passkey_email\",\"password\":\"passkey-correct-123\",\"redirect_url\":\"/dashboard\"}")"
  jq -e '.authenticated == false and .session_id == null' <<<"$register_response" >/dev/null
  verification_mail="$(curl -sS -f -G "$BASE_URL/api/auth/dev/mail/latest" \
    --data-urlencode "recipient=$passkey_email" \
    --data-urlencode "kind=email-verification")"
  verification_path="$(jq -r '.body_text' <<<"$verification_mail")"
  verification_token="${verification_path##*token=}"
  local verification_response passkey_session passkey_cookie passkey_csrf
  verification_response="$(json_post /api/auth/email/verify \
    "{\"token\":\"$verification_token\",\"redirect_url\":\"/dashboard\"}")"
  passkey_session="$(jq -r '.session_id' <<<"$verification_response")"
  passkey_cookie="Cookie: wasi_auth_dev_session=$passkey_session"
  passkey_csrf="$(curl -sS -f "$BASE_URL/api/auth/csrf" -H "$passkey_cookie" | jq -r '.token')"

  local start_response
  start_response="$(json_post /api/auth/passkeys/register/options \
    "{\"email\":\"$passkey_email\",\"redirect_url\":\"/dashboard\"}" \
    -H "$passkey_cookie" -H "x-csrf-token: $passkey_csrf")"
  local challenge_id
  challenge_id="$(jq -r '.challenge_id' <<<"$start_response")"
  if [[ -z "$challenge_id" || "$challenge_id" == "null" ]]; then
    echo "Passkey registration start did not return challenge_id: $start_response" >&2
    exit 1
  fi
  jq -e '
    .redirect_url == "/dashboard"
    and (.public_key_options_json | fromjson | .challenge | type == "string" and length > 0)
    and (.public_key_options_json | fromjson | .rp.id == "localhost")
    and (.public_key_options_json | fromjson | .user.name | contains("passkey-smoke-"))
    and (.public_key_options_json | fromjson | .pubKeyCredParams | length > 0)
  ' <<<"$start_response" >/dev/null

  local malformed_credential
  malformed_credential='{"id":"not-base64url","attestationObject":"not-base64url","clientDataJSON":"not-base64url"}'
  local verify_body
  verify_body="$(jq -nc \
    --arg challenge_id "$challenge_id" \
    --arg credential_json "$malformed_credential" \
    '{challenge_id:$challenge_id, credential_json:$credential_json, redirect_url:"/dashboard"}')"
  assert_error 401 invalid_credentials POST "$BASE_URL/api/auth/passkeys/register/verify" \
    -H "$passkey_cookie" -H "x-csrf-token: $passkey_csrf" \
    -H 'content-type: application/json' \
    --data "$verify_body"
  assert_error 409 conflict POST "$BASE_URL/api/auth/passkeys/register/verify" \
    -H "$passkey_cookie" -H "x-csrf-token: $passkey_csrf" \
    -H 'content-type: application/json' \
    --data "$verify_body"

  if [[ "$CHECK_PASSKEY_EXPIRY" == "1" ]]; then
    local expiry_response
    expiry_response="$(json_post /api/auth/passkeys/register/options \
      "{\"email\":\"$passkey_email\",\"redirect_url\":\"/dashboard\"}" \
      -H "$passkey_cookie" -H "x-csrf-token: $passkey_csrf")"
    local expiry_challenge_id
    expiry_challenge_id="$(jq -r '.challenge_id' <<<"$expiry_response")"
    sleep "$PASSKEY_EXPIRY_WAIT_SECONDS"
    local expiry_body
    expiry_body="$(jq -nc \
      --arg challenge_id "$expiry_challenge_id" \
      --arg credential_json "$malformed_credential" \
      '{challenge_id:$challenge_id, credential_json:$credential_json, redirect_url:"/dashboard"}')"
    assert_error 401 session_expired POST "$BASE_URL/api/auth/passkeys/register/verify" \
      -H "$passkey_cookie" -H "x-csrf-token: $passkey_csrf" \
      -H 'content-type: application/json' \
      --data "$expiry_body"
  fi

  curl -sS -f "$BASE_URL/auth/passkey-unsupported" | grep -q "Passkey unavailable"

  echo "fullstack-app smoke: passkey challenge validation passed"
}

run_storage_event_check() {
  assert_error 401 auth_required GET "$BASE_URL/api/auth/storage/status"
  assert_error 401 auth_required POST "$BASE_URL/api/auth/storage/projections/run?limit=128"

  local storage_response
  storage_response="$(curl -sS -f "$BASE_URL/api/auth/storage/status" \
    -H "$session_cookie")"
  jq -e '
    . as $root
    | .event_count >= 8
    and .latest_sequence >= .event_count
    and any(.event_types[]; .event_type == "auth_password_user_registered")
    and any(.event_types[]; .event_type == "auth_session_issued")
    and any(.event_types[]; .event_type == "auth_refresh_token_rotated")
    and any(.event_types[]; .event_type == "auth_password_reset_started")
    and any(.event_types[]; .event_type == "auth_password_reset_completed")
    and any(.event_types[]; .event_type == "auth_password_login_succeeded")
    and any(.checkpoints[]; .projection_name == "auth.storage.read_models" and .last_sequence == $root.latest_sequence)
  ' <<<"$storage_response" >/dev/null

  local projection_response
  projection_response="$(curl -sS -f -X POST "$BASE_URL/api/auth/storage/projections/run?limit=128" \
    -H "$session_cookie" \
    -H "x-csrf-token: $csrf_token")"
  jq -e '
    any(.[]; .projection_name == "auth.storage.read_models"
      and .events_scanned == 0
      and .events_applied == 0
      and .last_sequence_after == .last_sequence_before)
  ' <<<"$projection_response" >/dev/null

  echo "fullstack-app smoke: storage event log passed"
}

run_atomic_rollback_check() {
  local response
  response="$(curl -sS -f -X POST "$BASE_URL/api/auth/dev/storage/rollback-probe")"
  jq -e '
    .rolled_back == true
    and (.verified_categories | sort == ["event", "idempotency", "outbox", "projection", "secret"])
  ' <<<"$response" >/dev/null
  echo "fullstack-app smoke: atomic rollback passed"
}

require_command curl
require_command jq
require_command python3

echo "fullstack-app smoke: checking $BASE_URL"

if [[ "$CHECK_ATOMIC_ROLLBACK" == "1" ]]; then
  run_atomic_rollback_check
fi

curl -sS -f "$BASE_URL/api/auth/capabilities" >/tmp/fullstack-app-capabilities.json
jq -e '.password_enabled == true' /tmp/fullstack-app-capabilities.json >/dev/null

assert_error 405 validation GET "$BASE_URL/api/auth/password/login"
assert_error 404 not_found GET "$BASE_URL/api/auth/not-a-real-route"
if [[ "$CHECK_OAUTH_STATE" == "1" ]]; then
  run_oauth_state_check
  exit 0
fi
if [[ "$CHECK_PASSKEYS" == "1" ]]; then
  run_passkey_check
  exit 0
fi

assert_error 503 configuration GET "$BASE_URL/api/auth/oauth/google/start"
assert_error 503 configuration POST "$BASE_URL/api/auth/passkeys/login/options" \
  -H 'content-type: application/json' \
  --data '{"email":"nobody@example.test","redirect_url":"/dashboard"}'

assert_redirect "$BASE_URL/dashboard" "/auth/required?next=/dashboard"
assert_redirect "$BASE_URL/account/security" "/auth/required?next=/account/security"
assert_redirect "$BASE_URL/admin/authorization/policy" "/auth/required?next=/admin/authorization/policy"

email="$SMOKE_EMAIL"
old_password="old-correct-123"
new_password="new-correct-456"

register_response="$(json_post /api/auth/password/register \
  "{\"email\":\"$email\",\"password\":\"$old_password\",\"redirect_url\":\"/dashboard\"}")"
jq -e '.authenticated == false and .redirect_url == "/verify-email" and .session_id == null and .access_token == null and .refresh_token == null' \
  <<<"$register_response" >/dev/null
verification_mail="$(curl -sS -f -G "$BASE_URL/api/auth/dev/mail/latest" \
  --data-urlencode "recipient=$email" \
  --data-urlencode "kind=email-verification")"
verification_path="$(jq -r '.body_text' <<<"$verification_mail")"
verification_token="${verification_path##*token=}"
if [[ -z "$verification_token" || "$verification_token" == "$verification_path" ]]; then
  echo "Mail capture returned an invalid verification path" >&2
  exit 1
fi
verification_response="$(json_post /api/auth/email/verify \
  "{\"token\":\"$verification_token\",\"redirect_url\":\"/dashboard\"}")"
assert_error 400 validation POST "$BASE_URL/api/auth/email/verify" \
  -H 'content-type: application/json' \
  --data "{\"token\":\"$verification_token\",\"redirect_url\":\"/dashboard\"}"
session_id="$(jq -r '.session_id' <<<"$verification_response")"
access_token="$(jq -r '.access_token' <<<"$verification_response")"
refresh_token="$(jq -r '.refresh_token' <<<"$verification_response")"
if [[ -z "$session_id" || "$session_id" == "null" || -z "$access_token" || "$access_token" == "null" || -z "$refresh_token" || "$refresh_token" == "null" ]]; then
  echo "Verification did not return an API session: $verification_response" >&2
  exit 1
fi

session_cookie="Cookie: wasi_auth_dev_session=$session_id"

jq -e '.authenticated == true and .redirect_url == "/dashboard"' <<<"$verification_response" >/dev/null
curl -sS "$BASE_URL/api/auth/session" -H "$session_cookie" \
  | jq -e --arg email "$email" '.authenticated == true and .primary_email == $email' >/dev/null

if [[ "$CHECK_REFRESH_TOKEN_EXPIRY" == "1" ]]; then
  sleep "$REFRESH_EXPIRY_WAIT_SECONDS"
  assert_error 401 session_expired POST "$BASE_URL/api/auth/token/refresh" \
    -H "Authorization: Bearer $access_token" \
    -H 'content-type: application/json' \
    --data "{\"refresh_token\":\"$refresh_token\"}"
  echo "fullstack-app smoke: refresh expiry passed"
  exit 0
fi

json_post /api/auth/token/verify "{\"access_token\":\"$access_token\"}" \
  | jq -e --arg session_id "$session_id" '.active == true and .session_id == $session_id' >/dev/null

csrf_token="$(curl -sS -f "$BASE_URL/api/auth/csrf" -H "$session_cookie" | jq -r '.token')"
if [[ -z "$csrf_token" || "$csrf_token" == "null" ]]; then
  echo "CSRF endpoint did not return a token" >&2
  exit 1
fi
assert_error 400 validation POST "$BASE_URL/api/auth/mfa/totp/enroll/start" \
  -H "$session_cookie" \
  -H 'content-type: application/json' \
  --data '{}'
mfa_start="$(json_post /api/auth/mfa/totp/enroll/start '{}' \
  -H "$session_cookie" -H "x-csrf-token: $csrf_token")"
mfa_secret="$(jq -r '.secret_base32' <<<"$mfa_start")"
if [[ -z "$mfa_secret" || "$mfa_secret" == "null" ]]; then
  echo "TOTP enrollment did not return a secret" >&2
  exit 1
fi
mfa_code="$(totp_code "$mfa_secret")"
mfa_confirm="$(json_post /api/auth/mfa/totp/enroll/confirm \
  "{\"code\":\"$mfa_code\"}" -H "$session_cookie" -H "x-csrf-token: $csrf_token")"
jq -e '.assurance == "aal2" and (.recovery_codes | length) == 10' <<<"$mfa_confirm" >/dev/null
recovery_code="$(jq -r '.recovery_codes[0]' <<<"$mfa_confirm")"
admin_recovery_code="$(jq -r '.recovery_codes[1]' <<<"$mfa_confirm")"
json_post /api/auth/mfa/recovery/verify "{\"code\":\"$recovery_code\"}" \
  -H "$session_cookie" -H "x-csrf-token: $csrf_token" \
  | jq -e '.assurance == "aal2"' >/dev/null
replay_status="$(status_code POST "$BASE_URL/api/auth/mfa/recovery/verify" \
  -H "$session_cookie" -H "x-csrf-token: $csrf_token" \
  -H 'content-type: application/json' --data "{\"code\":\"$recovery_code\"}")"
if [[ "$replay_status" == "200" ]]; then
  echo "Recovery code replay unexpectedly succeeded" >&2
  exit 1
fi

if [[ "$CHECK_SIGNING_KEY_ROTATION" == "1" ]]; then
  run_signing_key_rotation_check
  exit 0
fi

if [[ "$CHECK_ES256_JWKS" == "1" ]]; then
  expected_kid="${AUTH_JWT_KID:-fullstack-app-dev-rs256}"
  curl -sS -f "$BASE_URL/api/auth/.well-known/jwks.json" \
    | jq -e --arg kid "$expected_kid" '
        .keys
        | type == "array"
        and any(.[]; .kid == $kid and .kty == "EC" and .alg == "ES256" and .use == "sig" and .crv == "P-256" and (.x | length > 0) and (.y | length > 0) and (has("d") | not))
      ' >/dev/null
  python3 - "$access_token" "$expected_kid" <<'PY'
import base64
import json
import sys

token, expected_kid = sys.argv[1:3]
header_segment = token.split(".", 1)[0]
padding = "=" * (-len(header_segment) % 4)
header = json.loads(base64.urlsafe_b64decode(header_segment + padding))
if header.get("alg") != "ES256" or header.get("kid") != expected_kid:
    raise SystemExit(f"unexpected JWT header: {header}")
PY
  echo "fullstack-app smoke: ES256 JWKS passed"
  exit 0
fi

bad_issuer_token="$(mint_hs256_jwt "https://wrong-issuer.example" "fullstack-app" "fullstack-app-dev-hs256" "$session_id" 300)"
bad_audience_token="$(mint_hs256_jwt "http://127.0.0.1:3008" "wrong-audience" "fullstack-app-dev-hs256" "$session_id" 300)"
unknown_kid_token="$(mint_hs256_jwt "http://127.0.0.1:3008" "fullstack-app" "unknown-kid" "$session_id" 300)"
expired_token="$(mint_hs256_jwt "http://127.0.0.1:3008" "fullstack-app" "fullstack-app-dev-hs256" "$session_id" -1000000000)"
assert_error 401 invalid_token POST "$BASE_URL/api/auth/token/verify" \
  -H 'content-type: application/json' \
  --data "{\"access_token\":\"$bad_issuer_token\"}"
assert_error 401 invalid_token POST "$BASE_URL/api/auth/token/verify" \
  -H 'content-type: application/json' \
  --data "{\"access_token\":\"$bad_audience_token\"}"
assert_error 401 invalid_token POST "$BASE_URL/api/auth/token/verify" \
  -H 'content-type: application/json' \
  --data "{\"access_token\":\"$unknown_kid_token\"}"
assert_error 401 session_expired POST "$BASE_URL/api/auth/token/verify" \
  -H 'content-type: application/json' \
  --data "{\"access_token\":\"$expired_token\"}"

refresh_response="$(curl -sS -X POST "$BASE_URL/api/auth/token/refresh" \
  -H "Authorization: Bearer $access_token" \
  -H 'content-type: application/json' \
  --data "{\"refresh_token\":\"$refresh_token\"}")"
next_refresh_token="$(jq -r '.refresh_token' <<<"$refresh_response")"
jq -e '.access_token != null and .refresh_token != null and .expires_in_seconds > 0' <<<"$refresh_response" >/dev/null
access_token="$(jq -r '.access_token' <<<"$refresh_response")"
refresh_token="$next_refresh_token"

curl -sS -f "$BASE_URL/" -H "$session_cookie" | grep -q "Production fullstack Rust"
for path in /login /register /forgot-password /reset-password; do
  assert_redirect "$BASE_URL$path" "/dashboard" -H "$session_cookie"
done
assert_redirect "$BASE_URL/login?next=/forgot-password" "/dashboard" -H "$session_cookie"

assert_error 401 invalid_token POST "$BASE_URL/api/auth/token/refresh" \
  -H "Authorization: Bearer $access_token" \
  -H 'content-type: application/json' \
  --data "{\"refresh_token\":\"$(jq -r '.refresh_token' <<<"$verification_response")\"}"
curl -sS "$BASE_URL/api/auth/session" -H "$session_cookie" \
  | jq -e '.authenticated == false' >/dev/null
assert_redirect "$BASE_URL/dashboard" "/auth/required?next=/dashboard" -H "$session_cookie"

unknown_reset_response="$(json_post /api/auth/password/reset/start \
  '{"email":"unknown-smoke-account@example.test","redirect_url":"/dashboard"}')"
jq -e '.accepted == true and (has("reset_url") | not) and .expires_in_seconds > 0' \
  <<<"$unknown_reset_response" >/dev/null

assert_error 400 validation POST "$BASE_URL/api/auth/password/reset/complete" \
  -H 'content-type: application/json' \
  --data '{"token":"missing-reset-token","password":"new-correct-456","redirect_url":"/dashboard"}'

start_reset_response="$(json_post /api/auth/password/reset/start \
  "{\"email\":\"$email\",\"redirect_url\":\"/dashboard\"}")"
jq -e '.accepted == true and (has("reset_url") | not) and .expires_in_seconds > 0' \
  <<<"$start_reset_response" >/dev/null
captured_mail="$(curl -sS -f -G "$BASE_URL/api/auth/dev/mail/latest" \
  --data-urlencode "recipient=$email" \
  --data-urlencode "kind=password-reset" 2>/dev/null || true)"
if [[ -n "$captured_mail" ]]; then
  reset_path="$(jq -r '.body_text' <<<"$captured_mail")"
  reset_token="${reset_path##*token=}"
  if [[ -z "$reset_token" || "$reset_token" == "$reset_path" ]]; then
    echo "Mail capture returned an invalid reset path" >&2
    exit 1
  fi

  complete_reset_response="$(json_post /api/auth/password/reset/complete \
    "{\"token\":\"$reset_token\",\"password\":\"$new_password\",\"redirect_url\":\"/dashboard\"}")"
  jq -e '.authenticated == true and .redirect_url == "/dashboard"' <<<"$complete_reset_response" >/dev/null

  assert_error 400 validation POST "$BASE_URL/api/auth/password/reset/complete" \
    -H 'content-type: application/json' \
    --data "{\"token\":\"$reset_token\",\"password\":\"$new_password\",\"redirect_url\":\"/dashboard\"}"

  assert_status 401 POST "$BASE_URL/api/auth/password/login" \
    -H 'content-type: application/json' \
    --data "{\"email\":\"$email\",\"password\":\"$old_password\",\"redirect_url\":\"/dashboard\"}"
else
  new_password="$old_password"
fi

login_response="$(json_post /api/auth/password/login \
  "{\"email\":\"$email\",\"password\":\"$new_password\",\"redirect_url\":\"/dashboard\"}")"
jq -e '.authenticated == true and .redirect_url == "/dashboard"' <<<"$login_response" >/dev/null
session_id="$(jq -r '.session_id' <<<"$login_response")"
access_token="$(jq -r '.access_token' <<<"$login_response")"
refresh_token="$(jq -r '.refresh_token' <<<"$login_response")"
session_cookie="Cookie: wasi_auth_dev_session=$session_id"
json_post /api/auth/token/verify "{\"access_token\":\"$access_token\"}" \
  | jq -e --arg session_id "$session_id" '.active == true and .session_id == $session_id' >/dev/null

if [[ "$CHECK_STORAGE_EVENTS" == "1" ]]; then
  csrf_token="$(curl -sS -f "$BASE_URL/api/auth/csrf" -H "$session_cookie" | jq -r '.token')"
  json_post /api/auth/mfa/recovery/verify "{\"code\":\"$admin_recovery_code\"}" \
    -H "$session_cookie" -H "x-csrf-token: $csrf_token" \
    | jq -e '.assurance == "aal2"' >/dev/null
fi

curl -sS -f "$BASE_URL/api/auth/.well-known/jwks.json" | jq -e '.keys | type == "array"' >/dev/null

assert_error 401 auth_required POST "$BASE_URL/api/authorization/check" \
  -H 'content-type: application/json' \
  --data '{"action":"authz:check","resource_type":"Organization","resource_id":"tenant:default","organization_id":"tenant:default"}'

json_post_bearer /api/authorization/check \
  '{"action":"authz:check","resource_type":"Organization","resource_id":"tenant:default","organization_id":"tenant:default"}' \
  "$access_token" \
  | jq -e '.allowed == true and .policy_revision == "embedded-v1"' >/dev/null

counter_change_response="$(json_post_bearer /api/authorization/check \
  '{"action":"counter.change","resource_type":"Counter","resource_id":"counter-1","organization_id":"tenant:default"}' \
  "$access_token")"
if [[ "$CHECK_STORAGE_EVENTS" == "1" ]]; then
  jq -e '.allowed == true' <<<"$counter_change_response" >/dev/null
else
  jq -e '.allowed == false' <<<"$counter_change_response" >/dev/null
fi

if [[ "$CHECK_STORAGE_EVENTS" == "1" ]]; then
  run_storage_event_check
fi

logout_response="$(curl -sS -X POST "$BASE_URL/api/auth/logout" -H "Authorization: Bearer $access_token")"
jq -e '.redirect_url == "/login"' <<<"$logout_response" >/dev/null
assert_error 401 auth_required POST "$BASE_URL/api/auth/token/refresh" \
  -H "Authorization: Bearer $access_token" \
  -H 'content-type: application/json' \
  --data "{\"refresh_token\":\"$refresh_token\"}"
assert_error 401 auth_required POST "$BASE_URL/api/auth/token/verify" \
  -H 'content-type: application/json' \
  --data "{\"access_token\":\"$access_token\"}"
curl -sS "$BASE_URL/api/auth/session" -H "$session_cookie" \
  | jq -e '.authenticated == false' >/dev/null

if [[ "$RUN_GRPC" == "1" ]]; then
  require_command grpcurl
  grpc_target="${BASE_URL#http://}"
  grpcurl -plaintext -import-path "$PROTO_DIR" -proto auth.proto \
    "$grpc_target" auth.v1.AuthService/GetCapabilities >/tmp/fullstack-app-grpc-auth.json
  jq -e '.passwordEnabled == true or .password_enabled == true' /tmp/fullstack-app-grpc-auth.json >/dev/null
  grpcurl -plaintext -import-path "$PROTO_DIR" -proto authorization.proto \
    "$grpc_target" authorization.v1.AuthorizationService/GetCapabilities \
    >/tmp/fullstack-app-grpc-authorization.json
  jq -e '.provider == "embedded-cedar"' /tmp/fullstack-app-grpc-authorization.json >/dev/null
fi

echo "fullstack-app smoke: passed"
