#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:3008}"
RUN_GRPC="${RUN_GRPC:-0}"
CHECK_REFRESH_TOKEN_EXPIRY="${CHECK_REFRESH_TOKEN_EXPIRY:-0}"
CHECK_RS256_JWKS="${CHECK_RS256_JWKS:-0}"
CHECK_OAUTH_STATE="${CHECK_OAUTH_STATE:-0}"
CHECK_OAUTH_REDIRECT_COOKIE="${CHECK_OAUTH_REDIRECT_COOKIE:-0}"
EXPECT_COOKIE_SECURE="${EXPECT_COOKIE_SECURE:-0}"
CHECK_SIGNING_KEY_ROTATION="${CHECK_SIGNING_KEY_ROTATION:-0}"
CHECK_PASSKEYS="${CHECK_PASSKEYS:-0}"
CHECK_PASSKEY_EXPIRY="${CHECK_PASSKEY_EXPIRY:-0}"
CHECK_STORAGE_EVENTS="${CHECK_STORAGE_EVENTS:-0}"
SIGNING_KEY_ROTATE_FROM_KID="${SIGNING_KEY_ROTATE_FROM_KID:-auth-stack-key-a}"
SIGNING_KEY_ROTATE_TO_KID="${SIGNING_KEY_ROTATE_TO_KID:-auth-stack-key-b}"
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
  curl -sS -o /tmp/auth-stack-smoke-body.json -w '%{http_code}' -X "$method" "$url" "$@"
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
    cat /tmp/auth-stack-smoke-body.json >&2 || true
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
    cat /tmp/auth-stack-smoke-body.json >&2 || true
    exit 1
  fi
  if ! jq -e --arg code "$expected_code" '.error.code == $code' /tmp/auth-stack-smoke-body.json >/dev/null; then
    echo "Expected $method $url to return error code $expected_code" >&2
    cat /tmp/auth-stack-smoke-body.json >&2 || true
    exit 1
  fi
}

assert_redirect() {
  local url="$1"
  local expected_location="$2"
  shift 2
  local headers
  headers="$(mktemp)"
  curl -sS -D "$headers" -o /tmp/auth-stack-smoke-body.txt "$url" "$@"
  local location
  location="$(header_value "$headers" location)"
  rm -f "$headers"
  if [[ "$location" != "$expected_location" ]]; then
    echo "Expected $url to redirect to $expected_location, got ${location:-<none>}" >&2
    cat /tmp/auth-stack-smoke-body.txt >&2 || true
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

json_post_admin() {
  local path="$1"
  local body="$2"
  shift 2
  json_post "$path" "$body" -H "x-auth-admin-token: ${AUTH_ADMIN_TOKEN:-}" "$@"
}

run_oauth_state_check() {
  jq -e '
    .oauth_enabled == true
    and any(.providers[]; .provider_id == "google")
    and any(.providers[]; .provider_id == "facebook")
  ' /tmp/auth-stack-capabilities.json >/dev/null

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
    curl -sS -D "$headers" -o /tmp/auth-stack-smoke-body.txt \
      "$BASE_URL/api/auth/oauth/google/callback?code=development-oauth-code&state=$cookie_state"
    local location
    location="$(header_value "$headers" location)"
    local set_cookie
    set_cookie="$(header_value "$headers" set-cookie)"
    rm -f "$headers"
    if [[ "$location" != "/dashboard" ]]; then
      echo "Expected OAuth browser callback to redirect to /dashboard, got ${location:-<none>}" >&2
      cat /tmp/auth-stack-smoke-body.txt >&2 || true
      exit 1
    fi
    case "$set_cookie" in
      *ddd_auth_session=*HttpOnly*SameSite=Lax*) ;;
      *)
        echo "Expected OAuth browser callback Set-Cookie to include ddd_auth_session, HttpOnly, and SameSite=Lax" >&2
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

  echo "auth-stack smoke: OAuth state validation passed"
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
secret = os.environ.get("AUTH_JWT_SECRET", "dev-auth-stack-secret-change-me").encode("utf-8")

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
  if [[ -z "${AUTH_ADMIN_TOKEN:-}" ]]; then
    echo "AUTH_ADMIN_TOKEN is required for CHECK_SIGNING_KEY_ROTATION=1" >&2
    exit 1
  fi

  assert_error 401 auth_required GET "$BASE_URL/api/auth/signing-keys"

  local list_response
  list_response="$(curl -sS -f "$BASE_URL/api/auth/signing-keys" \
    -H "x-auth-admin-token: $AUTH_ADMIN_TOKEN")"
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
    -H "x-auth-admin-token: $AUTH_ADMIN_TOKEN" \
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

  echo "auth-stack smoke: signing key rotation passed"
}

run_passkey_check() {
  jq -e '.passkeys_enabled == true' /tmp/auth-stack-capabilities.json >/dev/null

  local passkey_email
  passkey_email="passkey-smoke-$(date +%s)-$$@example.test"
  assert_error 401 invalid_credentials POST "$BASE_URL/api/auth/passkeys/login/options" \
    -H 'content-type: application/json' \
    --data "{\"email\":\"$passkey_email\",\"redirect_url\":\"/dashboard\"}"

  local start_response
  start_response="$(json_post /api/auth/passkeys/register/options \
    "{\"email\":\"$passkey_email\",\"redirect_url\":\"/dashboard\"}")"
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
    -H 'content-type: application/json' \
    --data "$verify_body"
  assert_error 409 conflict POST "$BASE_URL/api/auth/passkeys/register/verify" \
    -H 'content-type: application/json' \
    --data "$verify_body"

  if [[ "$CHECK_PASSKEY_EXPIRY" == "1" ]]; then
    local expiry_response
    expiry_response="$(json_post /api/auth/passkeys/register/options \
      "{\"email\":\"expired-$passkey_email\",\"redirect_url\":\"/dashboard\"}")"
    local expiry_challenge_id
    expiry_challenge_id="$(jq -r '.challenge_id' <<<"$expiry_response")"
    sleep "$PASSKEY_EXPIRY_WAIT_SECONDS"
    local expiry_body
    expiry_body="$(jq -nc \
      --arg challenge_id "$expiry_challenge_id" \
      --arg credential_json "$malformed_credential" \
      '{challenge_id:$challenge_id, credential_json:$credential_json, redirect_url:"/dashboard"}')"
    assert_error 401 session_expired POST "$BASE_URL/api/auth/passkeys/register/verify" \
      -H 'content-type: application/json' \
      --data "$expiry_body"
  fi

  curl -sS -f "$BASE_URL/auth/passkey-unsupported" | grep -q "Passkey unavailable"

  echo "auth-stack smoke: passkey challenge validation passed"
}

run_storage_event_check() {
  if [[ -z "${AUTH_ADMIN_TOKEN:-}" ]]; then
    echo "AUTH_ADMIN_TOKEN is required for CHECK_STORAGE_EVENTS=1" >&2
    exit 1
  fi

  assert_error 401 auth_required GET "$BASE_URL/api/auth/storage/status"
  assert_error 401 auth_required POST "$BASE_URL/api/auth/storage/projections/run?limit=128"

  local storage_response
  storage_response="$(curl -sS -f "$BASE_URL/api/auth/storage/status" \
    -H "x-auth-admin-token: $AUTH_ADMIN_TOKEN")"
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
    and any(.event_types[]; .event_type == "auth_session_revoked")
    and any(.event_types[]; .event_type == "authz_relationship_tuples_written")
    and any(.event_types[]; .event_type == "authz_relationship_tuples_deleted")
    and any(.checkpoints[]; .projection_name == "auth.storage.read_models" and .last_sequence == $root.latest_sequence)
    and any(.checkpoints[]; .projection_name == "authz.storage.read_models" and .last_sequence > 0)
  ' <<<"$storage_response" >/dev/null

  local projection_response
  projection_response="$(curl -sS -f -X POST "$BASE_URL/api/auth/storage/projections/run?limit=128" \
    -H "x-auth-admin-token: $AUTH_ADMIN_TOKEN")"
  jq -e '
    any(.[]; .projection_name == "auth.storage.read_models"
      and .events_scanned == 0
      and .events_applied == 0
      and .last_sequence_after == .last_sequence_before)
    and any(.[]; .projection_name == "authz.storage.read_models"
      and .events_scanned == 0
      and .events_applied == 0
      and .last_sequence_after == .last_sequence_before)
  ' <<<"$projection_response" >/dev/null

  echo "auth-stack smoke: storage event log passed"
}

require_command curl
require_command jq
require_command python3

echo "auth-stack smoke: checking $BASE_URL"

curl -sS -f "$BASE_URL/api/auth/capabilities" >/tmp/auth-stack-capabilities.json
jq -e '.password_enabled == true' /tmp/auth-stack-capabilities.json >/dev/null

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
assert_redirect "$BASE_URL/admin/authz/check" "/auth/required?next=/admin/authz/check"

email="smoke-auth-$(date +%s)-$$@example.test"
old_password="old-correct-123"
new_password="new-correct-456"

register_response="$(json_post /api/auth/password/register \
  "{\"email\":\"$email\",\"password\":\"$old_password\",\"redirect_url\":\"/dashboard\"}")"
session_id="$(jq -r '.session_id' <<<"$register_response")"
if [[ -z "$session_id" || "$session_id" == "null" ]]; then
  echo "Register did not return a session_id: $register_response" >&2
  exit 1
fi
access_token="$(jq -r '.access_token' <<<"$register_response")"
refresh_token="$(jq -r '.refresh_token' <<<"$register_response")"
if [[ -z "$access_token" || "$access_token" == "null" || -z "$refresh_token" || "$refresh_token" == "null" ]]; then
  echo "Register did not return access and refresh tokens: $register_response" >&2
  exit 1
fi

session_cookie="Cookie: ddd_auth_session=$session_id"

jq -e '.authenticated == true and .redirect_url == "/dashboard"' <<<"$register_response" >/dev/null
curl -sS "$BASE_URL/api/auth/session" -H "$session_cookie" \
  | jq -e --arg email "$email" '.authenticated == true and .primary_email == $email' >/dev/null

if [[ "$CHECK_REFRESH_TOKEN_EXPIRY" == "1" ]]; then
  sleep "$REFRESH_EXPIRY_WAIT_SECONDS"
  assert_error 401 session_expired POST "$BASE_URL/api/auth/token/refresh" \
    -H "x-auth-session: $session_id" \
    -H 'content-type: application/json' \
    --data "{\"refresh_token\":\"$refresh_token\"}"
  echo "auth-stack smoke: refresh expiry passed"
  exit 0
fi

json_post /api/auth/token/verify "{\"access_token\":\"$access_token\"}" \
  | jq -e --arg session_id "$session_id" '.active == true and .session_id == $session_id' >/dev/null

if [[ "$CHECK_SIGNING_KEY_ROTATION" == "1" ]]; then
  run_signing_key_rotation_check
  exit 0
fi

if [[ "$CHECK_RS256_JWKS" == "1" ]]; then
  expected_kid="${AUTH_JWT_KID:-auth-stack-dev-rs256}"
  curl -sS -f "$BASE_URL/api/auth/.well-known/jwks.json" \
    | jq -e --arg kid "$expected_kid" '
        .keys
        | type == "array"
        and any(.[]; .kid == $kid and .kty == "RSA" and .alg == "RS256" and .use == "sig" and (.n | length > 0) and (.e | length > 0) and (has("k") | not))
      ' >/dev/null
  python3 - "$access_token" "$expected_kid" <<'PY'
import base64
import json
import sys

token, expected_kid = sys.argv[1:3]
header_segment = token.split(".", 1)[0]
padding = "=" * (-len(header_segment) % 4)
header = json.loads(base64.urlsafe_b64decode(header_segment + padding))
if header.get("alg") != "RS256" or header.get("kid") != expected_kid:
    raise SystemExit(f"unexpected JWT header: {header}")
PY
  echo "auth-stack smoke: RS256 JWKS passed"
  exit 0
fi

bad_issuer_token="$(mint_hs256_jwt "https://wrong-issuer.example" "auth-stack" "auth-stack-dev-hs256" "$session_id" 300)"
bad_audience_token="$(mint_hs256_jwt "http://127.0.0.1:3008" "wrong-audience" "auth-stack-dev-hs256" "$session_id" 300)"
unknown_kid_token="$(mint_hs256_jwt "http://127.0.0.1:3008" "auth-stack" "unknown-kid" "$session_id" 300)"
expired_token="$(mint_hs256_jwt "http://127.0.0.1:3008" "auth-stack" "auth-stack-dev-hs256" "$session_id" -1000000000)"
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
  -H "x-auth-session: $session_id" \
  -H 'content-type: application/json' \
  --data "{\"refresh_token\":\"$refresh_token\"}")"
next_refresh_token="$(jq -r '.refresh_token' <<<"$refresh_response")"
jq -e '.access_token != null and .refresh_token != null and .expires_in_seconds > 0' <<<"$refresh_response" >/dev/null
assert_error 401 invalid_token POST "$BASE_URL/api/auth/token/refresh" \
  -H "x-auth-session: $session_id" \
  -H 'content-type: application/json' \
  --data "{\"refresh_token\":\"$refresh_token\"}"
access_token="$(jq -r '.access_token' <<<"$refresh_response")"
refresh_token="$next_refresh_token"

for path in / /login /register /forgot-password /reset-password; do
  assert_redirect "$BASE_URL$path" "/dashboard" -H "$session_cookie"
done
assert_redirect "$BASE_URL/login?next=/forgot-password" "/dashboard" -H "$session_cookie"

unknown_reset_response="$(json_post /api/auth/password/reset/start \
  '{"email":"unknown-smoke-account@example.test","redirect_url":"/dashboard"}')"
jq -e '.accepted == true and .reset_url == null and .expires_in_seconds > 0' \
  <<<"$unknown_reset_response" >/dev/null

assert_error 400 validation POST "$BASE_URL/api/auth/password/reset/complete" \
  -H 'content-type: application/json' \
  --data '{"token":"missing-reset-token","password":"new-correct-456","redirect_url":"/dashboard"}'

start_reset_response="$(json_post /api/auth/password/reset/start \
  "{\"email\":\"$email\",\"redirect_url\":\"/dashboard\"}")"
reset_url="$(jq -r '.reset_url' <<<"$start_reset_response")"
if [[ "$reset_url" != "null" && -n "$reset_url" ]]; then
  reset_token="${reset_url#/reset-password?token=}"
  if [[ -z "$reset_token" || "$reset_token" == "$reset_url" ]]; then
    echo "Reset start returned an invalid reset_url: $start_reset_response" >&2
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
  jq -e '.accepted == true and .reset_url == null and .expires_in_seconds > 0' \
    <<<"$start_reset_response" >/dev/null
  new_password="$old_password"
fi

login_response="$(json_post /api/auth/password/login \
  "{\"email\":\"$email\",\"password\":\"$new_password\",\"redirect_url\":\"/dashboard\"}")"
jq -e '.authenticated == true and .redirect_url == "/dashboard"' <<<"$login_response" >/dev/null
session_id="$(jq -r '.session_id' <<<"$login_response")"
access_token="$(jq -r '.access_token' <<<"$login_response")"
refresh_token="$(jq -r '.refresh_token' <<<"$login_response")"
session_cookie="Cookie: ddd_auth_session=$session_id"
json_post /api/auth/token/verify "{\"access_token\":\"$access_token\"}" \
  | jq -e --arg session_id "$session_id" '.active == true and .session_id == $session_id' >/dev/null

curl -sS -f "$BASE_URL/api/auth/.well-known/jwks.json" | jq -e '.keys | type == "array"' >/dev/null

assert_error 401 auth_required POST "$BASE_URL/api/authz/check" \
  -H 'content-type: application/json' \
  --data '{"tenant":"tenant:default","subject":"user:alice","relation":"viewer","object":"project:demo","model_ref":{"kind":"active"},"context":{}}'

json_post_bearer /api/authz/check \
  '{"tenant":"tenant:default","subject":"user:alice","relation":"viewer","object":"project:demo","model_ref":{"kind":"active"},"context":{}}' \
  "$access_token" \
  | jq -e '.allowed == false and (.model_id | length > 0)' >/dev/null

if [[ -n "${AUTH_ADMIN_TOKEN:-}" ]]; then
  authz_model_json='{"model_id":"authz-smoke-model","schema_version":"1.0","types":{"project":{"name":"project","relations":{"viewer":{"rewrite":{"type":"direct"}}}}}}'
  authz_model_payload="$(jq -n --arg model_id "authz-smoke-model" --arg schema_json "$authz_model_json" \
    '{model_id:$model_id,schema_json:$schema_json,idempotency_key:"authz-smoke-model-write"}')"
  json_post_admin /api/authz/models "$authz_model_payload" \
    | jq -e '.model_id == "authz-smoke-model" and .active == false' >/dev/null
  json_post_admin /api/authz/models/authz-smoke-model/activate '{}' \
    -H 'idempotency-key: authz-smoke-model-activate' \
    | jq -e '.model_id == "authz-smoke-model" and .active == true' >/dev/null

  tuple_payload='{"tuples_json":"[{\"tenant\":\"tenant:default\",\"subject\":\"user:alice\",\"object\":\"project:demo\",\"relation\":\"viewer\"}]","idempotency_key":"authz-smoke-tuples-write"}'
  json_post_admin /api/authz/tuples/write "$tuple_payload" \
    | jq -e '.accepted == 1' >/dev/null
  json_post_bearer /api/authz/check \
    '{"tenant":"tenant:default","subject":"user:alice","relation":"viewer","object":"project:demo","model_ref":{"kind":"active"},"context":{}}' \
    "$access_token" \
    | jq -e '.allowed == true and .model_id == "authz-smoke-model"' >/dev/null
  json_post_bearer /api/authz/list-objects \
    '{"tenant":"tenant:default","subject":"user:alice","relation":"viewer","object_type":"project","model_ref":{"kind":"active"},"context":{}}' \
    "$access_token" \
    | jq -e '.objects == ["project:demo"]' >/dev/null
  json_post_bearer /api/authz/expand \
    '{"tenant":"tenant:default","relation":"viewer","object":"project:demo","model_ref":{"kind":"active"},"context":{}}' \
    "$access_token" \
    | jq -e '.graph_json | fromjson | .rewrite == "direct" and (.subjects | index("user:alice"))' >/dev/null
  json_post_admin /api/authz/tuples/delete \
    '{"tuples_json":"[{\"tenant\":\"tenant:default\",\"subject\":\"user:alice\",\"object\":\"project:demo\",\"relation\":\"viewer\"}]","idempotency_key":"authz-smoke-tuples-delete"}' \
    | jq -e '.accepted == 1' >/dev/null
else
  assert_error 401 auth_required POST "$BASE_URL/api/authz/models" \
    -H 'content-type: application/json' \
    --data '{"model_id":"authz-smoke-model","schema_json":"{}","idempotency_key":"authz-smoke-model-write"}'
fi

logout_response="$(curl -sS -X POST "$BASE_URL/api/auth/logout" -H "x-auth-session: $session_id")"
jq -e '.redirect_url == "/login"' <<<"$logout_response" >/dev/null
assert_error 401 auth_required POST "$BASE_URL/api/auth/token/refresh" \
  -H "x-auth-session: $session_id" \
  -H 'content-type: application/json' \
  --data "{\"refresh_token\":\"$refresh_token\"}"
assert_error 401 auth_required POST "$BASE_URL/api/auth/token/verify" \
  -H 'content-type: application/json' \
  --data "{\"access_token\":\"$access_token\"}"
curl -sS "$BASE_URL/api/auth/session" -H "$session_cookie" \
  | jq -e '.authenticated == false' >/dev/null

if [[ "$CHECK_STORAGE_EVENTS" == "1" ]]; then
  run_storage_event_check
fi

if [[ "$RUN_GRPC" == "1" ]]; then
  require_command grpcurl
  grpc_target="${BASE_URL#http://}"
  grpcurl -plaintext -import-path "$PROTO_DIR" -proto auth.proto \
    "$grpc_target" auth.v1.AuthService/GetCapabilities >/tmp/auth-stack-grpc-auth.json
  jq -e '.passwordEnabled == true or .password_enabled == true' /tmp/auth-stack-grpc-auth.json >/dev/null
  grpcurl -plaintext -import-path "$PROTO_DIR" -proto authz.proto \
    -d '{"tenant":"tenant:default","subject":"user:alice","relation":"viewer","object":"project:demo"}' \
    "$grpc_target" authz.v1.AuthzService/Check >/tmp/auth-stack-grpc-authz.json
  jq -e '.allowed == false' /tmp/auth-stack-grpc-authz.json >/dev/null
fi

echo "auth-stack smoke: passed"
