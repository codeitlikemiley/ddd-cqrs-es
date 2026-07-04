---
title: Leptos Auth UI PRD
description: Plan the Leptos WASI authentication and authorization UI for the Spin auth stack.
---

# Leptos Auth UI PRD

## Status

implemented

## Goal

Build a Leptos WASI UI for the Spin auth stack that handles email/password
login and registration first, optional passkeys, optional OAuth redirects,
logout, unauthenticated and unauthorized states, and auth/admin configuration
screens without embedding auth decisions in UI components.

## Non-Goals

- Do not build a marketing landing page.
- Do not store secrets, refresh tokens, or provider credentials in browser
  local storage.
- Do not make UI routes the source of authorization truth.
- Do not show OAuth or passkey UI unless the backend capability endpoint reports
  that those methods are enabled and configured.

## Success Criteria

- Browser users can create an email/password account, login with those
  credentials, receive validation errors, and redirect to a safe `next` target.
- Optional OAuth and passkey entry points are hidden until their feature flags
  and credentials are configured.
- Browser routes are first-class web pages in the Spin app, not static mockups
  or API-only placeholders.
- Redirect handling preserves an allowed `next` URL and rejects unsafe targets.
- Logged-in users can inspect their session, logout, and register passkeys.
- Admin users can configure OAuth providers, redirect allowlists,
  authorization models, and relationship tuples through guarded screens.
- Users who are not authenticated see a login-required page with a safe return
  path.
- Users who are authenticated but not authorized see a forbidden page that does
  not leak protected resource details.
- UI server functions call `ddd-auth` and `ddd-authz` application services.

## Interfaces

The canonical distinction between route, page, form, UI component, server
function, REST endpoint, and gRPC method is defined in
[Auth Surface Contracts](./auth-surface-contracts.md). This PRD owns the Leptos
web portion of that contract.

### Routes

- `/login`: email/password sign in, optional provider/passkey methods, and safe
  redirect handling.
- `/register`: email/password account creation with the registration mode
  selected by default.
- `/forgot-password`: request a short-lived local password reset link.
- `/reset-password`: consume a reset token, set a new password, and receive a
  fresh session.
- `/dashboard`: protected post-login landing route that displays the current
  session and links to account/security and authz tools.
- `/logout`: logout confirmation and POST-backed logout action.
- `/auth/callback/{provider}`: OAuth callback completion surface.
- `/auth/callback/{provider}/error`: safe callback error page for provider,
  state, nonce, redirect, or configuration failures.
- `/auth/required`: unauthenticated page shown when a user must login before
  reaching a protected route.
- `/auth/forbidden`: authenticated-but-not-authorized page shown when authz
  denies access.
- `/auth/session-expired`: expired or revoked session page with login action and
  safe `next` preservation.
- `/auth/passkey-unsupported`: browser fallback page when passkey APIs are not
  available.
- `/account/security`: current sessions and passkey registration.
- `/admin/auth/signing-keys`: signing key inspection and active-key rotation.
- `/admin/auth/providers`: OAuth provider configuration.
- `/admin/auth/redirects`: redirect URI allowlist management.
- `/admin/authz/models`: model editor and active model selector.
- `/admin/authz/tuples`: relationship tuple editor and import surface.
- `/admin/authz/check`: manual check debugger.

`/login`, `/register`, `/forgot-password`, `/reset-password`, `/dashboard`,
`/auth/required`, `/auth/forbidden`, `/auth/session-expired`, and
`/auth/passkey-unsupported` must work as direct page loads and client-side
router navigations. They must not require a previous in-memory UI state.

### Server Functions

- `list_auth_providers`
- `get_auth_capabilities`
- `register_email_password`
- `login_email_password`
- `start_password_reset`
- `complete_password_reset`
- `get_current_session`
- `require_authenticated_route`
- `require_authorized_route`
- `start_passkey_registration`
- `verify_passkey_registration`
- `start_passkey_login`
- `verify_passkey_login`
- `start_oauth_login`
- `complete_oauth_callback`
- `logout_current_session`
- `save_auth_provider`
- `save_redirect_allowlist`
- `list_signing_keys`
- `rotate_signing_key`
- `write_authorization_model`
- `activate_authorization_model`
- `write_relationship_tuples`
- `delete_relationship_tuples`
- `run_authorization_check`

### HTTP Status and Page Mapping

The web UI must render page-level auth failures while REST and gRPC receive
transport-native errors from the same application service decision:

| Condition | Web page | REST status/code | gRPC status |
| --- | --- | --- | --- |
| Missing identity | `/auth/required?next=<safe-path>` | `401` with `auth_required` | `Unauthenticated` |
| Expired or revoked session | `/auth/session-expired?next=<safe-path>` | `401` with `session_expired` | `Unauthenticated` |
| Authenticated but denied | `/auth/forbidden` | `403` with `forbidden` | `PermissionDenied` |
| OAuth callback failed | `/auth/callback/{provider}/error` | `400` or `401` with stable callback code | `InvalidArgument` or `Unauthenticated` |
| Passkeys unsupported | `/auth/passkey-unsupported` | not applicable to REST | not applicable to gRPC |
| Unknown page | Leptos router fallback page | `404` for REST paths | `Unimplemented` for unknown gRPC methods |

### UI State Rules

- Access tokens may be kept in memory or secure cookies depending on runtime
  configuration, but refresh tokens must remain httpOnly when cookie-based.
- Browser session state is carried by the `ddd_auth_session` httpOnly cookie;
  server functions read that cookie and pass the session id into the shared
  application service.
- Session-issuing server functions must return browser-safe completion
  responses that keep `authenticated`, `redirect_url`, and expiry metadata but
  omit `session_id`, `access_token`, and `refresh_token`.
- Redirect targets must be normalized and checked against the server-side
  allowlist before use.
- Guest-only routes (`/`, `/login`, `/register`, `/forgot-password`, and
  `/reset-password`) must redirect authenticated browser users to `/dashboard`
  at the Spin HTTP boundary.
- Protected browser routes (`/dashboard`, `/account/security`, and `/admin/*`)
  must redirect unauthenticated users to `/auth/required?next=<safe-path>` at
  the Spin HTTP boundary before rendering protected content.
- Passkey challenge state is server-owned and expires.
- `auth/required`, `auth/forbidden`, `auth/session-expired`, callback error, and
  passkey unsupported pages must be first-class route targets, not transient
  inline messages hidden inside `/login`.
- Protected UI routes must call `require_authenticated_route` first and
  `require_authorized_route` for permissioned actions. Client-side route guards
  are convenience only; server functions remain the enforcement point.
- Authorization admin screens must call authz checks before rendering mutable
  actions and must still rely on server-side enforcement.
- REST and gRPC errors must not be turned into browser-only redirects. Page
  redirects are only for Leptos browser routes and server functions invoked by
  those routes.

### Page Behavior

- Login page: shows the email/password form by default and a safe hidden `next`
  target. It must not show disabled or misconfigured OAuth providers or passkey
  controls.
- Register page: renders the same email/password form with account creation
  mode selected by default.
- Forgot-password page: accepts an email address and returns a generic accepted
  state. In local development without SMTP, the server may expose a short-lived
  reset URL for verification.
- Reset-password page: requires a reset token, validates the new password, calls
  the server function, and redirects to `/dashboard` after the server issues a
  new session.
- Dashboard page: is the protected default landing route for authenticated
  users and must not render without a valid session.
- Email/password form: validates required email, email shape, required
  password, and registration password length in the browser, then relies on the
  server function for authoritative validation and session issuance.
- OAuth callback page: completes the server-side callback, then redirects to
  the safe `next` target or renders the callback error route.
- Login-required page: explains that authentication is required and links to
  `/login?next=<safe-path>`.
- Forbidden page: explains that the current account cannot access the page and
  links to account security and logout. It must not reveal relation names,
  tuple paths, or protected object internals.
- Session-expired page: clears local session UI state and offers login with the
  safe `next` target preserved.
- Passkey unsupported page: offers OAuth login alternatives and a browser
  compatibility note.

## Implementation Milestones

1. Build the unauthenticated email/password login and registration screen with
   server-function backed actions.
   - Status: done; `/login` and `/register` are implemented with
     server-function backed password login/register actions.
2. Build `auth/required`, `auth/forbidden`, `auth/session-expired`,
   callback error, and passkey unsupported pages.
   - Status: done; direct-load auth error pages and OAuth callback/error pages
     are implemented and covered by browser smoke checks.
3. Build account security screens for session display, logout, and passkey
   registration.
   - Status: done; session display/logout and real WebAuthn passkey
     registration/login browser calls exist behind `AUTH_ENABLE_PASSKEYS`, with
     a Playwright virtual-authenticator browser smoke covering successful
     registration and login.
4. Build guarded admin screens for provider config, redirect allowlists, and
   signing-key lifecycle.
   - Status: done; provider, redirect allowlist, and signing-key rotation forms
     exist behind permission-aware route middleware, so ordinary authenticated
     sessions are redirected to `/auth/forbidden`.
5. Build guarded authz admin screens for models, tuples, and manual checks.
   - Status: done; authz model, tuple, and check screens exist behind
     permission-aware route middleware.
6. Add browser-side error states for expired challenge, invalid callback,
   denied action, and configuration missing.
7. Add forgot-password and reset-password forms backed by server functions.
   - Status: done for local reset-token flow; SMTP/email delivery remains a
     future feature flag.
8. Redact token-bearing login completion data at the browser server-function
   boundary while preserving REST and gRPC token responses.
   - Status: done; Leptos password register/login, password reset completion,
     passkey verification, and OAuth callback server functions issue the
     httpOnly cookie and return a browser-safe login completion response.

## Verification

- Playwright checks for `/login` email/password validation, registration, login,
  callback error state, login-required route, forbidden route, session-expired
  route, passkey unsupported browser state, logout, and guarded admin redirects.
- Direct-load checks for every public auth page route.
- Server-function tests confirm every UI mutation maps to a shared application
  service command.
- Server-function tests confirm session-issuing UI calls do not return
  `session_id`, `access_token`, or `refresh_token` to hydrated browser code.
- UI tests prove unsafe `next` values do not redirect outside the allowlist.
- UI tests prove unauthenticated users are routed to `/auth/required` and
  authenticated users without permission are routed to `/auth/forbidden`.
- `rtk bash examples/auth-stack/scripts/verify_auth_stack.sh` proves current
  guest-only/protected route middleware, password reset, session, logout, JWKS,
  passkey challenge/replay/expiry, and authz deny-by-default behavior against a
  live Spin server.
- `rtk env BASE_URL=http://127.0.0.1:3008 make -C examples/auth-stack browser-smoke`
  proves direct-load auth pages render at desktop and mobile widths without
  horizontal overflow, protected routes redirect guests to `/auth/required`,
  stale cookies are rejected, guest-only routes redirect authenticated users to
  safe destinations, unsafe `next` values are ignored, ordinary authenticated
  users are redirected from auth-admin pages to `/auth/forbidden`, and logout
  clears the browser session cookie.
- `rtk env BASE_URL=http://localhost:3008 make -C examples/auth-stack passkey-browser-smoke`
  proves WebAuthn registration and login succeed in a browser through a
  Playwright virtual authenticator when `AUTH_ENABLE_PASSKEYS=true`,
  `AUTH_PASSKEY_RP_ID=localhost`, and
  `AUTH_PASSKEY_ORIGIN=http://localhost:3008`.
- Manual browser checks confirm text does not overlap at desktop and mobile
  widths.
