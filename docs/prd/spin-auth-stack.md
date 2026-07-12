---
title: Spin Auth Stack PRD
description: Plan the Spin microservice stack that exposes ddd-auth and ddd-authz over REST, gRPC, and Leptos boundaries.
---

# Spin Auth Stack PRD

> Historical pre-consolidation record. `ddd-auth`, `ddd-authz`, automatic event
> projection catch-up, SQLite/MySQL auth storage, and request-driven delivery
> are no longer the production architecture. The canonical implementation is
> PostgreSQL-backed `wasi-auth`, native trusted ingress, and the separately
> deployed `wasi-auth-outbox-worker`; see
> [Production fullstack](../production/wasi-auth-fullstack.md) and the generated
> [fullstack example](../../examples/fullstack-app/README.md).

## Status

superseded

## Goal

Build a Spin-native auth stack that deploys authentication and authorization as
reusable microservices. The stack must expose REST and gRPC APIs, support a
Leptos UI component, and keep shared auth logic in application services rather
than transport handlers.

## Non-Goals

- Do not add auth middleware directly to unrelated generated HTTP apps in v1.
- Do not require a sidecar OpenFGA server, Node service, or non-WASI runtime.
- Do not combine all auth behavior into one giant handler when separate Spin
  components make ownership clearer.
- Do not use wildcard production outbound permissions in generated manifests.

## Success Criteria

- `examples/fullstack-app` can run as a Spin app with REST and gRPC enabled.
- `examples/fullstack-app` is a fullstack Leptos WASI project that owns the
  browser-facing authentication pages and auth error pages.
- The default fullstack target serves web pages, REST APIs, and gRPC from the
  same Spin application with `transport=both`.
- Internal components can call auth and authz through stable local service
  names.
- REST, gRPC, and Leptos server functions call the same `application.rs`
  services for each command/check.
- Static assets are served separately from dynamic auth routes.
- Startup schema initialization is guarded and does not run in hot paths for
  static files.

## Interfaces

### Spin Components

- `auth-api`: authentication REST and gRPC endpoints.
- `authz-api`: authorization REST and gRPC endpoints.
- `auth-ui`: Leptos UI and server functions for browser flows.
- `auth-static`: static file serving for Leptos assets.

The default route layout:

- `/api/auth/...` routes to `auth-api`.
- `/api/authz/...` routes to `authz-api`.
- `/pkg/...` routes to `auth-static`.
- `/...` routes to `auth-ui`.

Internal service names:

- `auth.spin.internal`
- `authz.spin.internal`

### Fullstack Project Shape

`examples/fullstack-app` must be generated as a Leptos Spin project, not only an
API service. The project owns these browser and API boundaries:

- `src/app.rs`: Leptos routes, pages, forms, and client-side browser flow state.
- `src/server.rs`: Spin/WASI HTTP router, static file routing, REST/gRPC
  detection, and Leptos server-function registration.
- `src/application.rs`: shared auth and authz application service calls.
- `src/rest.rs`: REST adapters only.
- `src/grpc.rs`: gRPC adapters only.
- `proto/auth.proto` and `proto/authz.proto`: gRPC service contracts.
- `spin.toml`: `auth-api`, `authz-api`, `auth-ui`, and `auth-static`
  component wiring.

The Leptos UI must be deployed in the same Spin application so the generated
project is a fullstack reference implementation. The UI can call `auth-api` and
`authz-api` through server functions or internal Spin service names, but it must
not duplicate provider callback, token, session, or policy decision logic.

### Surface Support Matrix

The canonical surface definitions live in
[Auth Surface Contracts](./auth-surface-contracts.md). This stack must implement
those separate contracts without merging page, form, route, REST, and gRPC
concerns.

Every user-facing capability must be available through the surfaces listed
below. Web pages are for browser users, REST is for HTTP clients and curlable
integration checks, and gRPC is for service-to-service calls.

| Capability | Web page or server function | REST API | gRPC API |
| --- | --- | --- | --- |
| Capabilities | `get_auth_capabilities` | `GET /api/auth/capabilities` | `AuthService/GetCapabilities` |
| Login page | `GET /login` | `GET /api/auth/capabilities`, `GET /api/auth/providers` | `AuthService/GetCapabilities`, `AuthService/ListProviders` |
| Register page | `GET /register` | `GET /api/auth/capabilities`, `GET /api/auth/providers` | `AuthService/GetCapabilities`, `AuthService/ListProviders` |
| Email/password registration | `register_email_password` from `/register` or `/login` | `POST /api/auth/password/register` | `AuthService/RegisterPassword` |
| Email/password login | `login_email_password` from `/login` | `POST /api/auth/password/login` | `AuthService/LoginPassword` |
| Forgot password | `start_password_reset` from `/forgot-password` | `POST /api/auth/password/reset/start` | `AuthService/StartPasswordReset` |
| Reset password | `complete_password_reset` from `/reset-password` | `POST /api/auth/password/reset/complete` | `AuthService/CompletePasswordReset` |
| OAuth start | `start_oauth_login` from `/login` | `GET /api/auth/oauth/{provider}/start` | `AuthService/StartOAuthLogin` |
| OAuth callback | `GET /auth/callback/{provider}` | `GET /api/auth/oauth/{provider}/callback` | `AuthService/CompleteOAuthCallback` |
| OAuth callback error | `GET /auth/callback/{provider}/error` | JSON error body from callback route | `AuthService/CompleteOAuthCallback` error status |
| Passkey registration | `/account/security` server action | `POST /api/auth/passkeys/register/options`, `POST /api/auth/passkeys/register/verify` | `AuthService/StartPasskeyRegistration`, `AuthService/VerifyPasskeyRegistration` |
| Passkey login | `/login` server action | `POST /api/auth/passkeys/login/options`, `POST /api/auth/passkeys/login/verify` | `AuthService/StartPasskeyLogin`, `AuthService/VerifyPasskeyLogin` |
| Logout | `POST /logout` or `logout_current_session` | `POST /api/auth/logout` | `AuthService/Logout` |
| Session inspect | guarded route loader | `GET /api/auth/session` | `AuthService/GetSession` |
| Token verify | no page; used by clients | `POST /api/auth/token/verify` | `AuthService/VerifyToken` |
| Dashboard | `GET /dashboard` | `GET /api/auth/session` | `AuthService/GetSession` |
| Login required | `GET /auth/required` | `401` JSON error with login URL | `Unauthenticated` status |
| Forbidden | `GET /auth/forbidden` | `403` JSON error without protected internals | `PermissionDenied` status |
| Session expired | `GET /auth/session-expired` | `401` JSON error with `session_expired` code | `Unauthenticated` status |
| JWKS | no page; used by clients | `GET /api/auth/.well-known/jwks.json` | `AuthService/GetJwks` |
| Signing key admin | `/admin/auth/signing-keys` | `GET /api/auth/signing-keys`, `POST /api/auth/signing-keys/rotate` | `AuthService/ListSigningKeys`, `AuthService/RotateSigningKey` |
| Authz check | `/admin/authz/check` debugger | `POST /api/authz/check` | `AuthzService/Check` |
| Authz model admin | `/admin/authz/models` | `POST /api/authz/models`, `POST /api/authz/models/{model_id}/activate` | `AuthzService/WriteAuthorizationModel`, `AuthzService/ActivateAuthorizationModel` |
| Authz tuple admin | `/admin/authz/tuples` | `POST /api/authz/tuples/write`, `POST /api/authz/tuples/delete` | `AuthzService/WriteRelationshipTuples`, `AuthzService/DeleteRelationshipTuples` |

### Server Routing Order

The Spin auth stack must follow the same transport boundary discipline as the
counter app:

1. Detect and serve Spin gRPC requests when `transport=grpc` or
   `transport=both`.
2. If `transport=grpc`, return a stable HTTP `404` or equivalent plain response
   for browser and REST paths.
3. Serve explicit REST auth and authz routes.
4. Enforce browser-route auth middleware: authenticated users are redirected
   away from guest-only auth pages, and unauthenticated users are redirected
   away from protected app/admin pages.
5. Serve Leptos server functions for browser actions.
6. Serve static assets under `/pkg/...`.
7. Render Leptos pages and fallback pages for all remaining browser routes.

`transport=http` serves web pages and REST. `transport=grpc` serves only gRPC.
`transport=both` serves web pages, REST, and gRPC, and is the default for the
fullstack auth-stack example.

### REST Boundaries

Authentication REST routes:

- `GET /api/auth/capabilities`
- `GET /api/auth/providers`
- `POST /api/auth/password/register`
- `POST /api/auth/password/login`
- `POST /api/auth/password/reset/start`
- `POST /api/auth/password/reset/complete`
- `POST /api/auth/passkeys/register/options`
- `POST /api/auth/passkeys/register/verify`
- `POST /api/auth/passkeys/login/options`
- `POST /api/auth/passkeys/login/verify`
- `GET /api/auth/oauth/{provider}/start`
- `GET /api/auth/oauth/{provider}/callback`
- `GET /api/auth/session`
- `POST /api/auth/token/refresh`
- `POST /api/auth/token/verify`
- `POST /api/auth/logout`
- `GET /api/auth/.well-known/jwks.json`
- `GET /api/auth/signing-keys`
- `POST /api/auth/signing-keys/rotate`

Admin rollout diagnostics:

- `GET /api/auth/storage/status` returns event counts and projection checkpoint
  positions for smoke verification. This route is guarded by `AUTH_ADMIN_TOKEN`
  and is not a browser-facing auth flow.
- `POST /api/auth/storage/projections/run?limit={n}` runs a bounded auth/authz
  projection catch-up batch and advances checkpoints only after replaying
  events. This route is guarded by `AUTH_ADMIN_TOKEN`.

Authorization REST routes:

- `POST /api/authz/check`
- `POST /api/authz/batch-check`
- `POST /api/authz/list-objects`
- `POST /api/authz/expand`
- `POST /api/authz/models`
- `POST /api/authz/models/{model_id}/activate`
- `POST /api/authz/tuples/write`
- `POST /api/authz/tuples/delete`

### gRPC Boundaries

- `auth.v1.AuthService` owns capabilities, email/password, password reset,
  passkey, OAuth, token refresh, token verification, logout, JWKS,
  signing-key lifecycle, and session inspection methods.
- `authz.v1.AuthzService` owns model, tuple, check, list, and expand methods.
- gRPC status mapping must preserve validation, authentication, authorization,
  concurrency, configuration, and infrastructure failures. Use
  `Unauthenticated` for missing or expired identity and `PermissionDenied` for
  authenticated users who fail authz checks.

### Runtime Configuration

- Public environment names should be simple and documented in the Makefile.
- Provider secrets are referenced by name and resolved by Spin runtime config or
  local `.env`, not embedded in PRDs or generated code.
- `AUTH_ENABLE_PASSWORD_LOGIN=true` is the default. `AUTH_ENABLE_OAUTH=false`
  and `AUTH_ENABLE_PASSKEYS=false` are the default until credentials and
  implementation gates are explicitly configured.
- OAuth providers require `AUTH_ENABLE_OAUTH=true`, a provider enable source
  such as `AUTH_GOOGLE_ENABLED=true` or admin provider config, and
  provider-specific credential variables before they are reported by
  capabilities or rendered by the UI.
- OAuth provider runtime configuration also includes `AUTH_PUBLIC_BASE_URL`,
  `AUTH_<PROVIDER>_ISSUER`, `AUTH_<PROVIDER>_AUTHORIZATION_URL`,
  `AUTH_<PROVIDER>_TOKEN_URL`, `AUTH_<PROVIDER>_JWKS_URL`,
  `AUTH_<PROVIDER>_JWKS_JSON`, `AUTH_<PROVIDER>_SCOPES`, and
  `AUTH_<PROVIDER>_REDIRECT_URI`. Provider endpoint and JWKS values are
  resolved through Spin variables first and local environment fallback second.
  Production and generated Spin manifests must pass these declared variables
  through to the dynamic auth components; declaring them only under
  `[variables]` is not sufficient.
- `AUTH_OAUTH_DEVELOPMENT_CALLBACK_BYPASS=true` enables the local callback
  bypass used by smoke tests. It validates stored state, provider mismatch,
  state replay, and safe redirect behavior without contacting Google,
  Facebook, or Apple.
- JWT runtime variables are `AUTH_PRODUCTION_MODE`, `AUTH_JWT_ISSUER`,
  `AUTH_JWT_AUDIENCE`,
  `AUTH_JWT_KID`, `AUTH_JWT_SECRET`, `AUTH_JWT_ALGORITHM`,
  `AUTH_JWT_PRIVATE_KEY_DER_BASE64`, `AUTH_JWT_PUBLIC_JWKS_JSON`,
  `AUTH_JWT_KEY_RING_JSON`, and `AUTH_ADMIN_TOKEN`. Local HS256 defaults are
  development-only; production deployments can use RS256 private DER key
  material and publish public JWKS through Spin variables or environment
  configuration. Key rotation activates pre-provisioned keys from
  `AUTH_JWT_KEY_RING_JSON`; the admin API never accepts new private key
  material directly. When `AUTH_PRODUCTION_MODE=true`, the runtime rejects
  fallback runtime keys, requires `AUTH_JWT_KEY_RING_JSON`, requires
  `AUTH_ADMIN_TOKEN`, and permits only RS256 signing keys with private key
  material.
- Token and session lifetimes are configurable through
  `AUTH_SESSION_TTL_SECONDS`, `AUTH_ACCESS_TOKEN_TTL_SECONDS`, and
  `AUTH_REFRESH_TOKEN_TTL_SECONDS`.
- Browser session cookies use `wasi_auth_dev_session` with `HttpOnly` and
  `SameSite=Lax`. `AUTH_COOKIE_SECURE=true` adds the `Secure` attribute for
  HTTPS deployments; the local development default is `false`, while the
  production manifest example defaults it to `true`.
- `AUTH_STORAGE_AUTO_CATCH_UP=true` runs bounded auth/authz projection catch-up
  after write operations. Set it to `false` only when an external projection
  worker is responsible for checkpoint advancement.
- `transport=http|grpc|both` controls which auth transports are enabled and
  should match the existing counter-app public parameter style.
- `db=sqlite|postgres|mysql` follows the same public backend naming style as
  the counter app for the auth stack's stable SQL targets. Make passes this to
  the runtime as `DATABASE_BACKEND`; PostgreSQL uses `POSTGRES_URL`, and MySQL
  uses `MYSQL_URL`.

## Implementation Milestones

1. Add `examples/fullstack-app` as a documentation-backed Spin app skeleton after
   the reusable crates compile, including the Leptos route tree and auth error
   pages.
2. Add REST handlers that parse DTOs, call shared services, and map typed errors.
3. Add gRPC protobuf files and generated services for auth and authz.
4. Add Leptos server functions and pages that call the same shared services for
   email/password login, optional OAuth, optional passkeys, logout,
   unauthorized, forbidden, and admin flows.
   - Status: done for local deterministic runtime behavior. Email/password,
     reset, route guards, OAuth state, signing-key rotation, and real WebAuthn
     passkey challenge/verification service paths are implemented; Playwright
     browser smoke covers
     permission-aware route guards and Playwright virtual-authenticator browser
     smoke covers passkey registration/login success. Live external OAuth
     provider smoke remains a credential-dependent rollout gate; use
     [Auth OAuth Rollout](../production/auth-oauth-rollout) for provider setup,
     `make oauth-preflight`, and acceptance checks.
     Authz checks now load the active stored authorization model and
     tenant-scoped tuple set, and REST/gRPC list, expand, read-model, and
     read-tuples paths are no longer placeholders.
5. Add Makefile targets for `make spin`, `make db=<backend> fresh`, and
   `make spin transport=both`.
   - Status: done for SQLite, PostgreSQL, and MySQL compile/reset command
     surfaces. Runtime feature/env wiring and SQL dialect routing are
     compile-verified for Spin SQLite, Spin PostgreSQL, and Spin MySQL. Live
     PostgreSQL and MySQL reset/storage smoke passed against local SQL
     services.
6. Add manifest examples with least-privilege outbound hosts for local and
   production profiles.
   - Status: done. `examples/fullstack-app/spin.toml` remains the local
     development manifest, and `examples/fullstack-app/spin.production.toml.example`
     provides a production hardening starting point with exact OAuth and
     database outbound hosts. Generated `auth-stack` projects also include
     `spin.production.toml.example`, and CLI regression tests reject wildcard,
     localhost, and loopback outbound patterns in that production example.

## Verification

- `make spin db=sqlite transport=both` starts the stack locally.
- `curl` smoke tests prove capabilities, email/password registration,
  email/password login, password reset, guest-only/protected route middleware,
  session inspection, token refresh, logout, JWKS response, passkey
  challenge/replay/expiry checks, and authz deny-by-default checks. The
  current repeatable command is
  `rtk bash examples/fullstack-app/scripts/verify_fullstack.sh` against a running
  Spin server.
- OAuth helper tests prove provider authorization URL generation, token
  exchange boundaries, JWKS parsing, and ID-token validation helpers. Live
  Google, Apple, and Facebook credential smoke is tracked as the external
  provider rollout gate in
  [Auth OAuth Rollout](../production/auth-oauth-rollout), starting with
  `make oauth-preflight` against credentialed providers.
- `grpcurl` smoke tests prove `AuthService` session methods and `AuthzService`
  checks respond through the same Spin HTTP trigger. Spin gRPC compile coverage
  includes authz check, batch-check, list, expand, model write/activate/read,
  and tuple write/delete/read methods.
- Manifest checks confirm production examples do not use wildcard, localhost,
  or loopback outbound hosts.
