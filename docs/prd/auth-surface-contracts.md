---
title: Auth Surface Contracts PRD
description: Define the separate web routes, pages, forms, server functions, REST APIs, and gRPC services for the fullstack auth solution.
---

# Auth Surface Contracts PRD

## Status

implemented

## Goal

Define the browser, API, and service contracts for the fullstack auth solution
without conflating UI components, pages, forms, routes, REST endpoints, or gRPC
services. This PRD is the coordination point for `ddd-auth`, `ddd-authz`, the
Spin auth stack, and the Leptos UI.

## Non-Goals

- Do not treat a route, page, form, server function, REST endpoint, and gRPC
  method as interchangeable.
- Do not make browser redirects the REST or gRPC error contract.
- Do not put policy decisions in UI components.
- Do not expose protected object internals in web pages, REST errors, or gRPC
  status messages.

## Success Criteria

- Every user-facing auth capability has an explicit route, page, form, server
  function, REST endpoint, and gRPC method where that surface applies.
- Web routes render pages. Pages own forms. Forms call server functions. Server
  functions call shared application services.
- REST endpoints return JSON and HTTP status codes.
- gRPC methods return protobuf messages and gRPC status codes.
- Auth and authz failures have separate browser pages and transport-native API
  errors.

## Interfaces

### Surface Definitions

| Term | Meaning | Example |
| --- | --- | --- |
| Route | URL path matched by Spin and/or Leptos. | `/login` |
| Page | Route-level Leptos view rendered for a browser route. | `LoginPage` |
| Form | User input and submit surface inside a page. | `EmailPasswordAuthForm` |
| UI component | Reusable view element that is not itself a route. | `ProviderButton` |
| Server function | Leptos browser-to-server action used by pages/forms. | `start_oauth_login` |
| REST endpoint | Public JSON HTTP API for non-browser or integration clients. | `POST /api/authz/check` |
| gRPC method | Protobuf service method for service-to-service clients. | `AuthzService/Check` |
| Application service | Transport-neutral Rust function containing auth/authz behavior. | `check_authorization` |

### Web Routes and Pages

| Route | Page | Auth required | Primary forms/components |
| --- | --- | --- | --- |
| `/` | `LoginPage` | Guest-only | `EmailPasswordAuthForm`, `OptionalLoginMethods`, `OAuthProviderList`, `PasskeyLoginForm` |
| `/login` | `LoginPage` | Guest-only | `EmailPasswordAuthForm`, `OptionalLoginMethods`, `OAuthProviderList`, `PasskeyLoginForm` |
| `/register` | `RegisterPage` | Guest-only | `EmailPasswordAuthForm`, `OptionalLoginMethods` |
| `/forgot-password` | `ForgotPasswordPage` | Guest-only | `ForgotPasswordForm` |
| `/reset-password` | `ResetPasswordPage` | Guest-only | `ResetPasswordForm` |
| `/dashboard` | `DashboardPage` | Yes | `SessionSummary`, `WorkspaceAccessLinks` |
| `/logout` | `LogoutPage` | Yes | `LogoutForm` |
| `/auth/callback/{provider}` | `OAuthCallbackPage` | No | `OAuthCallbackStatus` |
| `/auth/callback/{provider}/error` | `OAuthCallbackErrorPage` | No | `ReturnToLoginLink` |
| `/auth/required` | `AuthRequiredPage` | No | `LoginRedirectLink` |
| `/auth/forbidden` | `ForbiddenPage` | Yes | `AccountSecurityLink`, `LogoutForm` |
| `/auth/session-expired` | `SessionExpiredPage` | No | `LoginRedirectLink` |
| `/auth/passkey-unsupported` | `PasskeyUnsupportedPage` | No | `OAuthProviderList` |
| `/account/security` | `AccountSecurityPage` | Yes | `SessionSummary`, `OptionalPasskeyRegistration`, `LogoutForm` |
| `/admin/auth/signing-keys` | `SigningKeyAdminPage` | Yes plus authz | `SigningKeyRotationForm` |
| `/admin/auth/providers` | `AuthProviderAdminPage` | Yes plus authz | `ProviderConfigForm` |
| `/admin/auth/redirects` | `RedirectAllowlistPage` | Yes plus authz | `RedirectAllowlistForm` |
| `/admin/authz/models` | `AuthzModelAdminPage` | Yes plus authz | `AuthorizationModelForm`, `ActivateModelForm` |
| `/admin/authz/tuples` | `RelationshipTupleAdminPage` | Yes plus authz | `RelationshipTupleForm`, `TupleImportForm` |
| `/admin/authz/check` | `AuthzCheckPage` | Yes plus authz | `ManualAuthzCheckForm` |
| fallback | `NotFoundPage` | No | `ReturnHomeLink` |

Guest-only routes must redirect authenticated browser users to `/dashboard`.
Protected browser routes must redirect unauthenticated users to
`/auth/required?next=<safe-local-path>` before rendering protected content.

### Forms and Server Functions

| Form | Page | Server function | Application service call |
| --- | --- | --- | --- |
| `EmailPasswordAuthForm` | `LoginPage`, `RegisterPage` | `login_email_password`, `register_email_password` | `login_email_password`, `register_email_password` |
| `ForgotPasswordForm` | `ForgotPasswordPage` | `start_password_reset` | `start_password_reset` |
| `ResetPasswordForm` | `ResetPasswordPage` | `complete_password_reset` | `complete_password_reset` |
| `OptionalLoginMethods` | `LoginPage`, `RegisterPage` | `get_auth_capabilities`, `list_auth_providers` | `auth_capabilities`, `list_auth_providers` |
| `ProviderLoginButton` | `LoginPage`, `RegisterPage` | `start_oauth_login` | `start_oauth_login` |
| `PasskeyLoginForm` | `LoginPage`, `RegisterPage` | `start_passkey_login`, `verify_passkey_login` | `start_passkey_login`, `verify_passkey_login` |
| `LogoutForm` | `LogoutPage`, `ForbiddenPage` | `logout_current_session` | `revoke_session` |
| `PasskeyRegistrationForm` | `AccountSecurityPage` | `start_passkey_registration`, `verify_passkey_registration` | `register_passkey` |
| `LogoutOtherSessionsForm` | `AccountSecurityPage` | `revoke_other_sessions` | `revoke_sessions_for_user` |
| `ProviderConfigForm` | `AuthProviderAdminPage` | `save_auth_provider` | `configure_auth_provider` |
| `RedirectAllowlistForm` | `RedirectAllowlistPage` | `save_redirect_allowlist` | `configure_redirect_allowlist` |
| `SigningKeyRotationForm` | `SigningKeyAdminPage` | `list_signing_keys`, `rotate_signing_key` | `list_signing_keys`, `rotate_signing_key` |
| `AuthorizationModelForm` | `AuthzModelAdminPage` | `write_authorization_model` | `write_authorization_model` |
| `ActivateModelForm` | `AuthzModelAdminPage` | `activate_authorization_model` | `activate_authorization_model` |
| `RelationshipTupleForm` | `RelationshipTupleAdminPage` | `write_relationship_tuples`, `delete_relationship_tuples` | `write_relationship_tuples`, `delete_relationship_tuples` |
| `TupleImportForm` | `RelationshipTupleAdminPage` | `import_relationship_tuples` | `write_relationship_tuples` |
| `ManualAuthzCheckForm` | `AuthzCheckPage` | `run_authorization_check` | `check_authorization` |

Forms must submit to server functions only. They must not call REST endpoints
directly from hydrated client code in v1, because server functions are the web
UI boundary where cookies, redirects, and page-level auth errors are handled.
Session-issuing server functions must set the `ddd_auth_session` httpOnly cookie
and return browser-safe completion responses without `session_id`,
`access_token`, or `refresh_token` fields. REST and gRPC retain token-bearing
responses for non-browser clients.

### REST API Contract

REST endpoints are JSON contracts. They must not return HTML pages. OAuth start
and callback routes may return redirects because they are browser protocol
handoffs, but all other REST failures return JSON.

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/api/auth/capabilities` | Return enabled auth methods and credentialed providers. |
| `GET` | `/api/auth/providers` | List enabled login providers. |
| `POST` | `/api/auth/password/register` | Register an email/password account and issue a session. |
| `POST` | `/api/auth/password/login` | Verify email/password credentials and issue a session. |
| `POST` | `/api/auth/password/reset/start` | Create a short-lived password reset grant for an email address. |
| `POST` | `/api/auth/password/reset/complete` | Consume a reset grant, update the password hash, and issue a session. |
| `POST` | `/api/auth/passkeys/register/options` | Create passkey registration challenge. |
| `POST` | `/api/auth/passkeys/register/verify` | Verify passkey registration response. |
| `POST` | `/api/auth/passkeys/login/options` | Create passkey login challenge. |
| `POST` | `/api/auth/passkeys/login/verify` | Verify passkey login response. |
| `GET` | `/api/auth/oauth/{provider}/start` | Start OAuth/OIDC login and redirect to provider. |
| `GET` | `/api/auth/oauth/{provider}/callback` | Complete OAuth/OIDC callback. |
| `GET` | `/api/auth/session` | Return current session. |
| `POST` | `/api/auth/token/refresh` | Rotate refresh token and issue new access token. |
| `POST` | `/api/auth/token/verify` | Verify an access token and return active principal/session details. |
| `POST` | `/api/auth/logout` | Revoke current session. |
| `GET` | `/api/auth/.well-known/jwks.json` | Return active public signing keys. |
| `GET` | `/api/auth/signing-keys` | List configured signing key lifecycle state; requires admin token. |
| `POST` | `/api/auth/signing-keys/rotate` | Activate a pre-provisioned signing key; requires admin token. |
| `POST` | `/api/authz/check` | Evaluate one authorization request. |
| `POST` | `/api/authz/batch-check` | Evaluate multiple authorization requests. |
| `POST` | `/api/authz/list-objects` | List objects visible to a subject/relation. |
| `POST` | `/api/authz/expand` | Return an authorization graph expansion. |
| `POST` | `/api/authz/models` | Write an authorization model. |
| `POST` | `/api/authz/models/{model_id}/activate` | Activate an authorization model. |
| `POST` | `/api/authz/tuples/write` | Write relationship tuples. |
| `POST` | `/api/authz/tuples/delete` | Delete relationship tuples. |

Session context:

- Browser server functions use the `ddd_auth_session` httpOnly cookie.
- Browser server functions must not expose issued session ids, access tokens, or
  refresh tokens to hydrated browser code after issuing that cookie.
- Browser page middleware reads the `ddd_auth_session` httpOnly cookie before
  rendering guest-only or protected routes.
- REST smoke and service-to-service callers may pass `x-auth-session`, the
  `ddd_auth_session` cookie, or `Authorization: Bearer <access_jwt>`.
  Query-string credentials such as `?session_id=...` and `?admin_token=...`
  are not supported.
- Access-token callers use `POST /api/auth/token/verify` with a signed JWT
  access token. The verifier checks `kid`, signature, issuer, audience, expiry,
  and backing session state.
- `GET /api/auth/session` returns an unauthenticated view for missing,
  revoked, expired, or unknown sessions.
- `POST /api/auth/token/refresh` returns `401` for missing, revoked, expired,
  unknown, replayed, or rotated refresh tokens.

REST error mapping:

- `400`: validation, malformed request, invalid callback input.
- `401`: missing identity, expired session, revoked session, invalid token.
- `403`: authenticated but not authorized.
- `404`: unknown REST route or missing resource that is safe to reveal.
- `409`: concurrency or idempotency conflict.
- `503`: configuration missing or dependent backend unavailable.
- `500`: unexpected infrastructure failure.

### gRPC Service Contract

gRPC services are protobuf contracts. They must not encode page routes,
browser-only redirects, or HTML response concepts.

`auth.v1.AuthService` methods:

- `GetCapabilities`
- `ListProviders`
- `RegisterPassword`
- `LoginPassword`
- `StartPasswordReset`
- `CompletePasswordReset`
- `StartOAuthLogin`
- `CompleteOAuthCallback`
- `StartPasskeyRegistration`
- `VerifyPasskeyRegistration`
- `StartPasskeyLogin`
- `VerifyPasskeyLogin`
- `GetSession`
- `RefreshToken`
- `VerifyToken`
- `Logout`
- `GetJwks`
- `ListSigningKeys`
- `RotateSigningKey`

Session-bearing request messages:

- `GetSessionRequest`: `session_id = 1`
- `RefreshTokenRequest`: `session_id = 1`, `refresh_token = 2`
- `TokenVerifyRequest`: `access_token = 1`
- `LogoutRequest`: `session_id = 1`
- `ListSigningKeysRequest`: `admin_token = 1`
- `SigningKeyRotateRequest`: `admin_token = 1`, `kid = 2`,
  `retire_previous = 3`

`authz.v1.AuthzService` methods:

- `Check`
- `BatchCheck`
- `ListObjects`
- `Expand`
- `WriteAuthorizationModel`
- `ActivateAuthorizationModel`
- `ReadAuthorizationModel`
- `WriteRelationshipTuples`
- `DeleteRelationshipTuples`
- `ReadRelationshipTuples`

gRPC status mapping:

- `InvalidArgument`: validation or malformed request.
- `Unauthenticated`: missing identity, expired session, revoked session, invalid
  token.
- `PermissionDenied`: authenticated but not authorized.
- `NotFound`: missing model, tuple, provider, session, or safe-to-reveal
  resource.
- `Aborted`: concurrency or idempotency conflict.
- `FailedPrecondition`: inactive model or invalid auth stack configuration.
- `Unavailable`: dependent backend unavailable.
- `Internal`: unexpected infrastructure failure.

## Implementation Milestones

1. Keep this PRD synchronized with the Spin, Leptos UI, auth, authz, and CLI
   PRDs before code generation starts.
2. Generate Leptos routes and pages from the web route table.
3. Generate form/server-function pairs from the forms table.
4. Generate REST route handlers from the REST contract table.
5. Generate protobuf services from the gRPC contract lists.
6. Add parity tests for capabilities that exist across web, REST, and gRPC.

## Verification

- `rtk bash scripts/verify-docs.sh`.
- Web route tests direct-load every route in the web route table.
- Form tests prove every form submits to the listed server function.
- REST smoke tests cover every REST endpoint and error class.
- `rtk bash examples/auth-stack/scripts/verify_auth_stack.sh` covers
  guest-only route redirects, protected route redirects, register/login,
  session inspection, token refresh, logout, forgot/reset password, JWKS,
  stored authz model activation, tuple-backed check/list/expand, and authz
  deny-by-default behavior against a live Spin server.
- gRPC compile and smoke coverage includes auth session methods plus authz
  check, batch-check, list, expand, model write/activate/read, tuple
  write/delete/read, and mapped status classes.
- Parity tests prove shared application service calls are reused by page/server
  functions, REST handlers, and gRPC handlers.
