---
title: Auth Verification and Rollout PRD
description: Plan the compile, test, smoke, UI, and rollout gates for the Spin auth stack.
---

# Auth Verification and Rollout PRD

## Status

implemented

## Goal

Define the verification gates required before `ddd-auth`, `ddd-authz`, and the
Spin auth stack can be considered production-grade for this repository.

## Non-Goals

- Do not treat a successful `cargo check` as proof of runtime behavior.
- Do not skip browser checks for passkey and redirect flows.
- Do not publish crates or templates before dependency spikes and smoke tests
  are green.
- Do not mark experimental backends stable without live contract evidence.

## Success Criteria

- Core crates pass native and WASI compile checks.
- Spin auth stack passes web page, REST, gRPC, and Leptos server-function smoke
  tests.
- Security-sensitive flows have replay, expiry, invalid signature, invalid
  redirect, revoked session, and deny-by-default tests.
- Docs navigation remains aligned.
- Rollout docs explain stable, experimental, and unsupported combinations.

## Interfaces

The verification matrix must cover the distinct surface contracts defined in
[Auth Surface Contracts](./auth-surface-contracts.md): web routes, pages, forms,
server functions, REST endpoints, and gRPC services.

### Required Commands

Documentation checks:

```bash
rtk bash scripts/verify-docs.sh
rtk rg -n "docs/prd|Product Roadmaps|ddd-auth|ddd-authz" docs docs/docs.json
rtk jq '.navigation.groups[].pages[]' docs/docs.json
```

Crate checks after implementation:

```bash
rtk cargo fmt --all -- --check
rtk cargo clippy --all-targets --all-features -- -D warnings
rtk cargo test --all-features
rtk cargo test -p ddd-cqrs-es-cli --all-targets
```

WASI checks after implementation:

```bash
rtk cargo check -p ddd-auth --target wasm32-wasip2 --no-default-features --features serde,json,jwt,oauth,passkeys,wasi
rtk cargo check -p ddd-authz --target wasm32-wasip2 --no-default-features --features serde,json,wasi
```

Spin smoke checks after implementation:

```bash
rtk make -C examples/fullstack-app check db=sqlite
rtk make -C examples/fullstack-app check db=postgres
rtk make -C examples/fullstack-app check db=mysql
rtk make -C examples/fullstack-app grpc-check db=sqlite
rtk make -C examples/fullstack-app grpc-check db=postgres
rtk make -C examples/fullstack-app grpc-check db=mysql
rtk make -C examples/fullstack-app spin db=sqlite transport=both listen=127.0.0.1:3008
rtk env BASE_URL=http://127.0.0.1:3008 make -C examples/fullstack-app smoke
rtk env CHECK_STORAGE_EVENTS=1 AUTH_ADMIN_TOKEN=dev-admin-token \
  BASE_URL=http://127.0.0.1:3008 bash examples/fullstack-app/scripts/verify_fullstack.sh
rtk env BASE_URL=http://127.0.0.1:3008 AUTH_ADMIN_TOKEN=dev-admin-token \
  make -C examples/fullstack-app oauth-dev-browser-smoke
rtk env OAUTH_PROVIDERS=google OAUTH_EVIDENCE_MODE=preflight \
  BASE_URL=https://<host> AUTH_ADMIN_TOKEN=<token> \
  make -C examples/fullstack-app oauth-evidence
rtk env OAUTH_PROVIDERS=google BASE_URL=https://<host> \
  AUTH_ADMIN_TOKEN=<token> EXPECTED_EMAIL=user@example.com \
  make -C examples/fullstack-app oauth-browser-smoke
rtk env OAUTH_PROVIDERS=google BASE_URL=https://<host> \
  AUTH_ADMIN_TOKEN=<token> SESSION_COOKIE='wasi_auth_dev_session=<id>' \
  make -C examples/fullstack-app oauth-callback
rtk env BASE_URL=http://127.0.0.1:3008 make -C examples/fullstack-app browser-smoke
rtk curl -sS -I http://127.0.0.1:3008/login
rtk curl -sS -I http://127.0.0.1:3008/register
rtk curl -sS -I http://127.0.0.1:3008/forgot-password
rtk curl -sS -I http://127.0.0.1:3008/reset-password
rtk curl -sS -I http://127.0.0.1:3008/dashboard
rtk curl -sS -I http://127.0.0.1:3008/auth/required
rtk curl -sS -I http://127.0.0.1:3008/auth/forbidden
rtk curl -sS -I http://127.0.0.1:3008/auth/session-expired
rtk curl -sS http://127.0.0.1:3008/api/auth/.well-known/jwks.json
rtk curl -sS -X POST -H 'content-type: application/json' \
  -H 'authorization: Bearer <access_jwt>' \
  -d '{"tenant":"tenant:default","subject":"user:alice","relation":"viewer","object":"project:demo","model_ref":{"kind":"active"}}' \
  http://127.0.0.1:3008/api/authz/check
rtk grpcurl -plaintext -import-path examples/fullstack-app/proto -proto auth.proto \
  localhost:3008 auth.v1.AuthService/GetJwks
rtk grpcurl -plaintext -import-path examples/fullstack-app/proto -proto authz.proto \
  -H 'authorization: Bearer <access_jwt>' \
  -d '{"tenant":"tenant:default","subject":"user:alice","relation":"viewer","object":"project:demo","model_ref_kind":"active"}' \
  localhost:3008 authz.v1.AuthzService/Check
```

### Required Scenarios

- JWT valid, expired, wrong issuer, wrong audience, unknown key ID, and revoked
  session.
- Refresh token first use, rotation, replay after rotation, expiry, and logout.
- Password reset request for existing and unknown users, reset completion,
  reset token replay rejection, expired reset token rejection, and old-password
  rejection after reset.
- OAuth state mismatch, nonce mismatch, provider mismatch, unsafe redirect, and
  successful provider callback.
- Passkey challenge creation, verification, replay rejection, expired challenge,
  and unsupported browser fallback.
- Authz direct allow, inherited allow, role-based allow, contextual allow,
  denied default, cycle detection, and max depth rejection.
- Web route direct loads for login, login-required, forbidden, session-expired,
  callback error, passkey unsupported, register, forgot password, reset
  password, dashboard, and unknown route fallback.
- Web middleware checks proving authenticated users are redirected away from
  guest-only routes and unauthenticated users are redirected away from protected
  routes.
- Surface parity checks proving login, OAuth, passkey, logout, session inspect,
  and authz check behavior are available through the planned web, REST, and gRPC
  surfaces where applicable.

## Implementation Milestones

1. Add dependency spike checks before public interfaces are implemented.
2. Add core crate unit and contract tests.
3. Add Spin web page, REST, and gRPC smoke scripts.
   - Status: done for local deterministic checks.
     `examples/fullstack-app/scripts/verify_fullstack.sh`
     covers current REST and web-route middleware smoke checks, password reset
     unknown-user safety, invalid reset token rejection, reset token replay
     rejection, old-password rejection after reset, disabled OAuth/passkey
     guards, method/route errors, authz deny-by-default, direct authz allow
     after tuple write, JWT valid and negative checks, refresh-token rotation
     and replay rejection, configurable refresh-token expiry, RS256 public
     JWKS publication, OAuth state/provider/replay/unsafe-callback-redirect
     validation through `CHECK_OAUTH_STATE=1`, signing-key admin guard and
     active-key rotation through `CHECK_SIGNING_KEY_ROTATION=1`, passkey
     option JSON, replay rejection, and short-TTL expiry through
     `CHECK_PASSKEYS=1 CHECK_PASSKEY_EXPIRY=1`, logout, and
     refresh/verify-after-logout rejection, stored authz model activation,
     tuple-backed check/list/expand behavior, storage event and bounded
     projection catch-up evidence through `CHECK_STORAGE_EVENTS=1`, including
     automatic catch-up after writes and zero-work manual recovery runs, with
     optional gRPC checks via `RUN_GRPC=1`.
     Reusable auth crate tests now also cover OIDC ID-token validation for
     string and array audiences plus nonce rejection. Auth-stack helper tests
     cover OAuth authorization URL generation, form encoding, JWKS key parsing,
     Facebook userinfo profile mapping, Apple generated client-secret TTL
     clamping, and escaped-newline PEM normalization. Live external-provider
     OAuth smoke is tracked separately because it requires real provider
     credentials.
4. Add Playwright checks for login, callback, logout, auth-required, forbidden,
   session-expired, passkey fallback, and admin guard behavior.
   - Status: done for local deterministic checks.
     `examples/fullstack-app/scripts/verify_auth_pages.mjs`
     covers direct-load auth pages, desktop/mobile overflow checks,
     unauthenticated protected-route redirects, stale-cookie rejection,
     authenticated guest-route redirects, safe `next` handling, forbidden
     auth-admin redirects, logout cookie clearing, OAuth callback/error pages,
     and account/authz protected pages against a live Spin server.
     `examples/fullstack-app/scripts/verify_auth_oauth_dev_browser.mjs` covers
     the actual OAuth provider button redirect, development callback redirect,
     httpOnly session-cookie issuance, dashboard landing, authenticated
     session lookup, callback replay rejection, and OAuth storage event deltas
     against a provider-flagged development-bypass server.
5. Add release notes that classify stable SQL storage separately from
   experimental WASI helper backends.
   - Status: done for the auth-stack storage rollout guide. The auth stack now
     compile-checks Spin SQLite, PostgreSQL, and MySQL through public Make
     targets, and [Auth Storage Rollout](../production/auth-storage-rollout)
     classifies stable, live-verified, and unsupported backend combinations.
     Live PostgreSQL and MySQL reset/storage smoke passed against local SQL
     services.
6. Run live external-provider OAuth credential smoke.
   - Status: operator_pending on real Google, Apple, and Facebook OAuth app
     credentials plus provider-side callback URL registration. The
     [Auth OAuth Rollout](../production/auth-oauth-rollout) runbook now records
     how to create provider apps in Google Cloud Console, Meta for Developers,
     and Apple Developer; the exact callback URLs; env vars; Spin credential
     wiring; `make oauth-credentials`; `make oauth-preflight` authorization URL
     checks; `make oauth-browser-smoke` interactive provider login; `make
     oauth-callback` session and storage evidence checks; manual browser checks;
     and storage event evidence required to close this external gate. The local
     development callback bypass and unit/helper tests cover protocol behavior
     until those credentials are available.

## Verification

- All commands in this PRD run successfully in the final implementation PR.
- Failed dependency spikes are documented in the owning PRD before coding
  continues.
- Smoke tests produce evidence for web pages, REST paths, and gRPC paths.
- Browser evidence confirms the Leptos UI routes render and guarded actions are
  enforced server-side.
- `ddd-auth` unit tests cover reusable aggregate command handling, JWT round
  trip, expiry, issuer mismatch, audience mismatch, wrong key, JWKS `kid`
  lookup failure, and revoked-session rejection.
- Auth-stack REST smoke tests cover runtime JWT issuance/verification, wrong
  issuer, wrong audience, unknown key ID, expired token, revoked-session
  rejection after logout, refresh-token rotation, refresh-token replay, and
  refresh-token expiry with `CHECK_REFRESH_TOKEN_EXPIRY=1`.
- RS256 smoke tests cover signed access-token issuance, `kid`/algorithm header
  shape, token verification, and standard public JWKS publication without
  leaking symmetric `k` material.
- Signing-key rotation smoke tests cover admin-token enforcement,
  pre-provisioned key activation, previous-key retirement, new-token `kid`
  changes, and continued verification of retired-key tokens. Unit tests cover
  production signing-key policy: `AUTH_PRODUCTION_MODE=true` rejects runtime
  defaults, missing admin tokens, and HS256 key rings, while allowing an RS256
  pre-provisioned key ring.
- OAuth smoke tests cover stored state validation, replay rejection, provider
  mismatch rejection, invalid development callback code rejection, and ignoring
  unsafe callback `next` values in favor of the redirect stored with state.
  Unit tests cover ID-token issuer/audience/nonce validation and JWKS key
  selection; helper tests cover provider authorization URL generation and token
  exchange support boundaries. `CHECK_OAUTH_REDIRECT_COOKIE=1` verifies the
  browser callback `Set-Cookie` header, and `EXPECT_COOKIE_SECURE=1` verifies
  `AUTH_COOKIE_SECURE=true` adds the `Secure` attribute.
- OAuth development browser smoke covers the real Leptos provider button,
  callback endpoint redirect mode, `Set-Cookie` issuance, dashboard landing,
  authenticated session lookup, callback replay rejection, and storage event
  deltas without requiring external provider credentials.
- Browser server-function regression tests cover token redaction after session
  issuance, proving hydrated UI calls receive redirect/expiry state but not
  `session_id`, `access_token`, or `refresh_token`.
- Authz smoke tests cover stored model validation/activation, tenant-scoped
  tuple writes, model-aware checks, deterministic list-objects, graph expand,
  and gRPC compile coverage for check/list/expand/read-model/read-tuples.
- Passkey smoke tests cover capability exposure, WebAuthn option JSON,
  malformed assertion rejection, one-time challenge replay rejection,
  short-TTL expiry, unsupported-browser page availability, and successful
  registration/login using a Playwright virtual authenticator against a live
  passkey-enabled Spin server.
- Storage smoke tests cover admin-token enforcement for storage diagnostics and
  projection catch-up, durable event append for password
  registration/login/reset, refresh-token rotation, logout, authz tuple
  write/delete, automatic best-effort catch-up after write operations,
  bounded replay through `POST /api/auth/storage/projections/run?limit=128`,
  zero-work recovery runs once checkpoints are current, and monotonic
  auth/authz projection checkpoints.
- Production manifest checks cover `spin.production.toml.example` in both the
  reference app and generated auth-stack projects, with no wildcard,
  localhost, or loopback outbound hosts, and with production
  `auth_cookie_secure` defaulting to `true`. Manifest checks also cover
  representative JWT, OAuth, passkey, admin-token, cookie, and public-base-url
  variables being passed into component runtime variables rather than only
  being declared globally.
- Generated-template checks cover full auth-stack source/script generation,
  package/crate-name substitution, absence of the unused aggregate scaffold,
  `ddd check` validation against the generated project, local source-checkout
  `[patch.crates-io]` overrides for unpublished auth crates, native SQLite SSR
  compile, and Spin/WASI SQLite plus gRPC compile.
- OAuth credential readiness checks cover missing variable names and invalid
  live URL shapes without printing secret values before running live provider
  preflight or browser callback smoke.
- OAuth callback evidence checks cover the issued session cookie,
  `/api/auth/session`, dashboard access, OAuth state creation and consumption,
  external identity linking, session issuance, and auth projection progress
  after a real provider callback completes.
- OAuth evidence reports cover redacted event-count and projection-checkpoint
  output for preflight and callback modes without printing secrets, tokens,
  session IDs, or profile payloads.
- OAuth browser smoke checks cover the actual provider button redirect,
  interactive provider login, automatic session-cookie capture, authenticated
  dashboard reachability, replay rejection for the exact callback URL, and
  post-callback storage evidence.
- Live Google, Apple, and Facebook OAuth credential smoke is the only
  operator-pending external cryptographic rollout gate and is tracked as
  milestone A11 in the implementation tracker. Use
  [Auth OAuth Rollout](../production/auth-oauth-rollout) for provider setup,
  `make oauth-preflight`, `make oauth-browser-smoke`, `make oauth-callback`,
  and the final browser callback acceptance checks.
