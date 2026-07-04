---
title: ddd-auth PRD
description: Plan the reusable identity, session, password, JWT, OAuth, and passkey crate for the ddd_cqrs_es auth stack.
---

# ddd-auth PRD

## Status

implemented

## Goal

Create a reusable `ddd-auth` crate that provides production-grade
authentication primitives for Spin/WASI applications while keeping protocol
transport, UI, and host runtime concerns outside the core domain.

The crate must model identity through event-sourced aggregates and expose a
shared application service that Spin REST, Spin gRPC, Leptos server functions,
and other HTTP triggers can call without duplicating authentication logic.

## Non-Goals

- Do not build a general web framework or middleware layer in the root
  `ddd_cqrs_es` crate.
- Do not hand-roll cryptographic algorithms.
- Do not require OpenFGA, Keycloak, Auth0, Cognito, or a non-WASI service as a
  runtime dependency.
- Do not implement tenant billing, user profile management, or organization
  administration beyond the identity fields needed for auth.

## Success Criteria

- `ddd-auth` compiles as a standalone workspace crate and under
  `wasm32-wasip2` with the selected feature set.
- Authentication flows produce audit metadata with actor, tenant, request,
  correlation, and provider context.
- Access tokens are short-lived JWTs signed by active keys from a JWKS-capable
  key set.
- Refresh tokens are opaque, hashed at rest, revocable, and rotation-aware.
- Email/password credentials are feature-enabled by configuration, hashed at
  rest with a WASI-compatible password hashing dependency, and validated on
  both client and server surfaces.
- OAuth/OIDC providers are configured, not hard-coded, with first-class
  profiles for Google, Apple, and Facebook.
- Passkey registration and login are implemented through challenge, verify, and
  credential storage APIs after a WASI-compatible WebAuthn dependency spike.

## Interfaces

### Crate and Features

- Crate name: `ddd-auth`.
- Default feature: `std`.
- Required feature gates:
  - `serde`: DTO and event serialization.
  - `json`: JSON request/response support.
  - `jwt`: JWT signing, verification, and JWKS publication.
  - `oauth`: OAuth/OIDC provider flow support.
  - `passkeys`: WebAuthn/passkey challenge and credential support.
  - `wasi`: WASI-safe time, randomness, and HTTP compatibility.
  - `tracing`: structured logs without changing domain behavior.

### Domain Aggregates

- `User`: owns identity lifecycle events such as registered, disabled,
  re-enabled, primary email changed, and tenant membership metadata changed.
- `PasswordCredential`: stores the password hash metadata, revocation state,
  and last-authenticated timestamp for email/password sign-in.
- `ExternalIdentity`: links provider subject IDs to internal users.
- `PasskeyCredential`: stores credential public key material, sign count,
  transports, user handle, and attestation policy result.
- `Session`: records login, refresh, rotation, revocation, expiry, and logout.
- `SigningKeySet`: manages active, next, retired, and revoked signing keys.
- `AuthProviderConfig`: stores enabled providers, issuer URLs, client IDs,
  secret references, allowed scopes, redirect URIs, and claim mapping rules.

### Application Service Commands

The service layer must expose typed commands with idempotency keys where a
browser, provider callback, or client retry can repeat the same operation:

- `StartPasswordlessLogin`
- `RegisterEmailPassword`
- `LoginEmailPassword`
- `StartPasswordReset`
- `CompletePasswordReset`
- `StartPasskeyRegistration`
- `VerifyPasskeyRegistration`
- `StartPasskeyLogin`
- `VerifyPasskeyLogin`
- `StartOAuthLogin`
- `CompleteOAuthCallback`
- `RefreshSession`
- `RevokeSession`
- `RotateSigningKey`
- `ConfigureAuthProvider`

Password login and local password reset are the first production-flow
implementation targets for the Spin example. Email delivery, reset-token replay
auditing, lockout, breach-check policy, and account takeover mitigations remain
follow-up hardening tasks and must be tracked before production rollout beyond
local/example use.

### Token Contracts

- Access token format: JWT.
- Required claims: `iss`, `sub`, `aud`, `exp`, `iat`, `jti`.
- Project claims: `tenant_id`, `session_id`, `roles`, `scope`, and
  `auth_time`.
- Signing keys are selected by `kid`; public keys are exposed through JWKS.
- Refresh tokens are random opaque values. Only hashes are persisted.
- Token verification returns an `AuthenticatedPrincipal` containing user ID,
  tenant ID, session ID, provider, scopes, roles, and raw claim map.

### OAuth/OIDC Provider Contracts

- Provider configs must include `provider_id`, `issuer`, authorization endpoint,
  token endpoint, JWKS URI when applicable, userinfo endpoint when applicable,
  client ID, secret reference, scopes, redirect URI allowlist, and claim map.
- Built-in provider profiles: `google`, `apple`, `facebook`.
- Provider callbacks must validate state, nonce when provided, issuer, audience,
  expiry, and allowed redirect URI.
- Secrets are referenced by name and resolved by the runtime app. The crate must
  not own a secret manager.

### Dependency Spike

Before implementation, create a small compile matrix proving the selected JWT,
JWK, OAuth/OIDC, WebAuthn/passkey, CBOR, COSE, randomness, and time crates
compile under native tests and `wasm32-wasip2`. Record failures in this PRD and
choose a fallback before coding the public interfaces.

Initial spike result: passed on July 3, 2026 in `/tmp/ddd-auth-wasi-spike`.
Each dependency family compiled natively and with
`--target wasm32-wasip2 --no-default-features`.

| Family | Candidate crates | Version | Result |
| --- | --- | --- | --- |
| JWT/JWK | `jsonwebtoken` with `rust_crypto` | `10.4.0` | passed |
| OAuth2 | `oauth2` without default HTTP client | `5.0.0` | passed |
| OIDC | `openidconnect` without default HTTP client | `4.0.1` | passed |
| CBOR/COSE | `ciborium`, `coset` | `0.2.2`, `0.4.2` | passed |
| Passkey core | `passkey` | `0.5.0` | passed |
| Passkey server | `passkey-auth` | `0.1.3` | passed |
| WebAuthn server | `webauthn-rs` without default features | `0.6.1-dev` | passed |

Implementation default: keep protocol dependencies behind feature flags and do
not enable provider HTTP clients in the reusable crate. Spin applications own
outbound HTTP and secret resolution.

## Implementation Milestones

1. Add the crate skeleton, feature flags, DTOs, domain events, and aggregate
   tests with an in-memory event store.
   - Status: done. `ddd-auth` exports reusable `User`,
     `PasswordCredential`, `ExternalIdentity`, `PasskeyCredential`, `Session`,
     `SigningKeySet`, and `AuthProviderConfigAggregate` domain models with
     command/event state machines. Aggregate tests cover password revocation,
     external identity unlinking, passkey counter monotonicity, revoked-session
     refresh rejection, signing-key activation/retirement/revocation, and
     provider config validation plus redirect allowlist idempotency.
2. Add session and token lifecycle services, JWT/JWKS support, and revocation
   projection contracts.
   - Status: done for reusable helpers and Spin runtime integration.
     `ddd-auth` includes reusable JWT encode/decode
     helpers, issuer/audience validation, expiry mapping, JWKS `kid` lookup,
     and revoked-session rejection helpers. The auth-stack runtime now issues
     signed access tokens, stores opaque refresh token hashes, rotates refresh
     tokens, rejects replay, and verifies JWT `kid`, signature, issuer,
     audience, expiry, and backing session state. Session, access-token, and
     refresh-token TTLs are runtime configurable. The Spin auth stack supports
     HS256 for local development and RS256 private-key signing with public JWKS
     publication for production-style deployments. The runtime now supports a
     pre-provisioned `AUTH_JWT_KEY_RING_JSON`, guarded signing-key admin
     listing, active-key rotation, previous-key retirement, and continued
     verification of retired keys until normal token/session expiry. Setting
     `AUTH_PRODUCTION_MODE=true` rejects runtime fallback keys, requires
     `AUTH_JWT_KEY_RING_JSON`, requires `AUTH_ADMIN_TOKEN`, and permits only
     RS256 signing keys with private key material.
3. Add OAuth/OIDC provider configuration and callback validation.
   - Status: done for local deterministic provider logic. The Spin auth stack
     now persists OAuth state grants,
     gates providers behind `AUTH_ENABLE_OAUTH`, provider enable flags/admin
     config, and credential env vars, and validates callback state replay,
     provider mismatch, invalid state, and stored safe redirects through the
     explicit `AUTH_OAUTH_DEVELOPMENT_CALLBACK_BYPASS` smoke path. The runtime
     can build provider authorization URLs, exchange authorization codes at the
     configured token endpoint, resolve provider JWKS from Spin variables or
     outbound HTTP, validate ID-token issuer/audience/nonce/signature, and link
     verified external identities to local users. Facebook access-token
     userinfo profile mapping is implemented for providers that do not return
     OIDC ID tokens. Apple client-secret generation now builds an ES256 JWT from
     `AUTH_APPLE_TEAM_ID`, `AUTH_APPLE_KEY_ID`, `AUTH_APPLE_PRIVATE_KEY`, and a
     clamped `AUTH_APPLE_CLIENT_SECRET_TTL_SECONDS`; a pre-generated
     `AUTH_APPLE_GENERATED_CLIENT_SECRET` remains supported. Real Google,
     Apple, and Facebook credential smoke tests are tracked as the separate
     external OAuth rollout gate because they require provider app credentials;
     [Auth OAuth Rollout](../production/auth-oauth-rollout) defines the exact
     callback URLs, env vars, `make oauth-preflight`, and acceptance checks.
4. Add passkey challenge and verification services after the dependency spike.
   - Status: done for local deterministic passkey logic. The Spin auth stack
     now uses the `passkey-auth`
     verifier for WebAuthn registration and authentication state, stores
     passkey credentials in `auth_passkey_credentials`, updates authenticator
     counters after login, exposes configurable RP ID/name/origin and challenge
     TTL settings, and publishes the verifier types through `ddd-auth` behind
     the `passkeys` feature. Automated smoke covers option JSON creation,
     malformed assertion rejection, replay rejection, short-TTL expiry, the
     unsupported-browser page, and successful browser registration/login
     through a Playwright virtual authenticator.
5. Add transport-neutral error types that can map to REST status codes,
   Leptos server function errors, and gRPC status codes at app boundaries.
   - Status: done. `AuthErrorClass` and `AuthTransportMapping` provide
     dependency-free HTTP status, gRPC code, and server-function code mapping
     data at the reusable crate boundary.
6. Add docs examples showing how to pass `ddd_cqrs_es::Metadata` with actor,
   tenant, request, and provider context.
   - Status: done. `AuthenticatedPrincipal::to_metadata` and
     `to_metadata_with_request` build `ddd_cqrs_es::Metadata` with actor,
     tenant, session, provider, scope, role, request, and correlation context;
     the crate-level doc example is covered by doctest.

## Verification

- `cargo test -p ddd-auth --all-features`.
- `cargo check -p ddd-auth --target wasm32-wasip2 --no-default-features --features serde,json,jwt,oauth,passkeys,wasi`.
- `cargo test -p ddd-auth --doc`.
- Unit tests for aggregate command handling, JWT round trip, token expiry,
  issuer mismatch, audience mismatch, wrong key, JWKS key lookup failure,
  session revocation, key rotation, refresh rotation, OAuth callback replay,
  provider mismatch, redirect mismatch, OIDC ID-token issuer/audience/nonce
  validation, and passkey challenge replay.
- Contract tests proving the application service can be called from REST, gRPC,
  and Leptos adapters without transport-specific dependencies in the domain.
