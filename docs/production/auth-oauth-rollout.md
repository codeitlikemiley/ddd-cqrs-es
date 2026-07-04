---
title: 5.12. Auth OAuth Rollout
description: Configure and verify Google, Facebook, and Apple OAuth providers for the Spin auth stack.
---

The auth stack ships with OAuth disabled. Enable providers only after a provider
app exists, callback URLs are registered with that provider, and the matching
environment variables are set.

This page is the operator runbook for milestone A11 in the auth PRD tracker.
It covers live-provider verification only. Local deterministic OAuth state,
replay, provider mismatch, and unsafe redirect checks are already covered by
the development callback bypass smoke tests.

## Callback URLs

Use the same public base URL for `AUTH_PUBLIC_BASE_URL` and every enabled
provider callback. Register the exact callback URL with the provider console.

| Provider | Callback URL |
| --- | --- |
| Google | `https://<host>/api/auth/oauth/google/callback` |
| Facebook | `https://<host>/api/auth/oauth/facebook/callback` |
| Apple | `https://<host>/api/auth/oauth/apple/callback` |

Google and Facebook provider consoles require the redirect URI to match what
the auth request sends. Apple Sign in with Apple for web requires an absolute
return URL with scheme, host, and path; use a real HTTPS domain for Apple live
smoke instead of `localhost` or an IP address.

## Environment

Start from `examples/auth-stack/.env.example`, keep password login enabled, and
enable only the provider being tested.

```bash
AUTH_ENABLE_OAUTH=true
AUTH_PUBLIC_BASE_URL=https://auth.example.com
AUTH_COOKIE_SECURE=true
AUTH_ADMIN_TOKEN=change-me

AUTH_GOOGLE_ENABLED=true
AUTH_GOOGLE_CLIENT_ID=
AUTH_GOOGLE_CLIENT_SECRET=
AUTH_GOOGLE_REDIRECT_URI=https://auth.example.com/api/auth/oauth/google/callback

AUTH_FACEBOOK_ENABLED=true
AUTH_FACEBOOK_CLIENT_ID=
AUTH_FACEBOOK_CLIENT_SECRET=
AUTH_FACEBOOK_REDIRECT_URI=https://auth.example.com/api/auth/oauth/facebook/callback

AUTH_APPLE_ENABLED=true
AUTH_APPLE_CLIENT_ID=
AUTH_APPLE_TEAM_ID=
AUTH_APPLE_KEY_ID=
AUTH_APPLE_PRIVATE_KEY=
AUTH_APPLE_GENERATED_CLIENT_SECRET=
AUTH_APPLE_REDIRECT_URI=https://auth.example.com/api/auth/oauth/apple/callback
```

For Apple, use either a generated client secret from
`AUTH_APPLE_TEAM_ID`, `AUTH_APPLE_KEY_ID`, and `AUTH_APPLE_PRIVATE_KEY`, or set
`AUTH_APPLE_GENERATED_CLIENT_SECRET` to a pre-generated client-secret JWT.
For HTTPS deployments, keep `AUTH_COOKIE_SECURE=true` so browser callback and
password-login sessions are issued with `Secure`, `HttpOnly`, and
`SameSite=Lax` cookie attributes. Local HTTP development can keep it `false`.

## Provider Setup

| Provider | Required app settings | Auth-stack variables |
| --- | --- | --- |
| Google | OAuth web client, authorized redirect URI, email/profile scopes. | `AUTH_GOOGLE_ENABLED`, `AUTH_GOOGLE_CLIENT_ID`, `AUTH_GOOGLE_CLIENT_SECRET`, `AUTH_GOOGLE_REDIRECT_URI` |
| Facebook | Facebook Login product, valid OAuth redirect URI, app ID, app secret, email/public_profile scopes. | `AUTH_FACEBOOK_ENABLED`, `AUTH_FACEBOOK_CLIENT_ID`, `AUTH_FACEBOOK_CLIENT_SECRET`, `AUTH_FACEBOOK_REDIRECT_URI` |
| Apple | Services ID, website domain, return URL, private key or pre-generated client secret. | `AUTH_APPLE_ENABLED`, `AUTH_APPLE_CLIENT_ID`, `AUTH_APPLE_TEAM_ID`, `AUTH_APPLE_KEY_ID`, `AUTH_APPLE_PRIVATE_KEY`, `AUTH_APPLE_GENERATED_CLIENT_SECRET`, `AUTH_APPLE_REDIRECT_URI` |

### Shared App Requirements

Use a real HTTPS host before creating provider apps. Provider consoles validate
redirect URLs against what the auth stack sends at runtime, so these values
must agree exactly:

| Setting | Value |
| --- | --- |
| Public base URL | `https://<host>` |
| Google callback | `https://<host>/api/auth/oauth/google/callback` |
| Facebook callback | `https://<host>/api/auth/oauth/facebook/callback` |
| Apple callback | `https://<host>/api/auth/oauth/apple/callback` |

Do not use `localhost`, `127.0.0.1`, or an IP address for live provider smoke.
For local development, put the Spin app behind a trusted HTTPS tunnel, or
deploy the app to a disposable HTTPS test host.

After the provider app is created, copy values into
`examples/auth-stack/.env`. Keep `.env` local and uncommitted. The Makefile and
the OAuth verification scripts load this file automatically.

### Google OAuth Setup

Use Google Cloud Console for the Google provider.

1. Open the Google Cloud Console and select or create the project that owns the
   auth stack.
2. Configure the OAuth consent screen. For early testing, keep the app in a
   testing state and add the test Google accounts that will complete browser
   smoke.
3. Go to **APIs & Services** > **Credentials**.
4. Create an OAuth client ID with application type **Web application**.
5. Add this authorized redirect URI exactly:

   ```text
   https://<host>/api/auth/oauth/google/callback
   ```

6. Save the client and copy the client ID and client secret.
7. Set these values in `examples/auth-stack/.env`:

   ```bash
   AUTH_ENABLE_OAUTH=true
   AUTH_PUBLIC_BASE_URL=https://<host>
   AUTH_COOKIE_SECURE=true

   AUTH_GOOGLE_ENABLED=true
   AUTH_GOOGLE_CLIENT_ID=<google-web-client-id>
   AUTH_GOOGLE_CLIENT_SECRET=<google-web-client-secret>
   AUTH_GOOGLE_REDIRECT_URI=https://<host>/api/auth/oauth/google/callback
   AUTH_GOOGLE_SCOPES=openid email profile
   ```

The auth stack uses Google's OIDC endpoints from `.env.example` by default.
Override `AUTH_GOOGLE_AUTHORIZATION_URL`, `AUTH_GOOGLE_TOKEN_URL`, or
`AUTH_GOOGLE_JWKS_URL` only when Google changes the endpoint or a test
environment requires it.

### Facebook Login Setup

Use Meta for Developers for the Facebook provider.

1. Open Meta for Developers and create or select the app that owns Facebook
   Login.
2. In the app dashboard, add the **Facebook Login** product if it is not
   already enabled.
3. Open **Facebook Login** > **Settings**.
   If the dashboard uses the newer **Use cases** navigation, open the
   authentication/login use case and continue to its Facebook Login settings.
4. In Client OAuth settings, keep web OAuth enabled and add this value under
   **Valid OAuth Redirect URIs**:

   ```text
   https://<host>/api/auth/oauth/facebook/callback
   ```

5. Save changes.
6. Open **Settings** > **Basic** and copy the App ID and App Secret.
7. Set these values in `examples/auth-stack/.env`:

   ```bash
   AUTH_ENABLE_OAUTH=true
   AUTH_PUBLIC_BASE_URL=https://<host>
   AUTH_COOKIE_SECURE=true

   AUTH_FACEBOOK_ENABLED=true
   AUTH_FACEBOOK_CLIENT_ID=<facebook-app-id>
   AUTH_FACEBOOK_CLIENT_SECRET=<facebook-app-secret>
   AUTH_FACEBOOK_REDIRECT_URI=https://<host>/api/auth/oauth/facebook/callback
   AUTH_FACEBOOK_SCOPES=email public_profile
   ```

For app-review-restricted Facebook apps, test with users that are allowed by
the app's current mode and roles. If Facebook reports a redirect mismatch, copy
the callback URL from this page into the provider console again and verify it
matches `AUTH_FACEBOOK_REDIRECT_URI` byte-for-byte.

### Apple Sign In Setup

Use Apple Developer **Certificates, Identifiers & Profiles** for the Apple
provider. Apple web login requires a Services ID associated with an app that
has Sign in with Apple enabled.

1. Ensure a primary App ID exists for the app or organization and that Sign in
   with Apple is enabled for it.
2. Create or select a Services ID. This Services ID is the OAuth client ID used
   by the auth stack.
3. Configure Sign in with Apple for that Services ID.
4. Select the primary App ID that is related to the website.
5. Under website URLs, add the HTTPS domain and the return URL:

   ```text
   https://<host>
   https://<host>/api/auth/oauth/apple/callback
   ```

6. Create a Sign in with Apple private key, or use an existing one. Record the
   Team ID, Key ID, and downloaded private key contents.
7. Set these values in `examples/auth-stack/.env`:

   ```bash
   AUTH_ENABLE_OAUTH=true
   AUTH_PUBLIC_BASE_URL=https://<host>
   AUTH_COOKIE_SECURE=true

   AUTH_APPLE_ENABLED=true
   AUTH_APPLE_CLIENT_ID=<apple-services-id>
   AUTH_APPLE_TEAM_ID=<apple-team-id>
   AUTH_APPLE_KEY_ID=<apple-key-id>
   AUTH_APPLE_PRIVATE_KEY=<apple-private-key-pem-or-escaped-pem>
   AUTH_APPLE_REDIRECT_URI=https://<host>/api/auth/oauth/apple/callback
   ```

Instead of providing `AUTH_APPLE_TEAM_ID`, `AUTH_APPLE_KEY_ID`, and
`AUTH_APPLE_PRIVATE_KEY`, you may pre-generate the Apple client-secret JWT and
set `AUTH_APPLE_GENERATED_CLIENT_SECRET`. The generated-secret path is useful
when private keys are managed outside the Spin deployment system.

### Hook Credentials Into Spin

For local operator smoke, put provider values in `examples/auth-stack/.env`.
For deployed Spin environments, pass the same values as Spin variables or
runtime secrets that map to the variables in `spin.production.toml.example`.

Minimum shared values:

```bash
AUTH_ENABLE_OAUTH=true
AUTH_PUBLIC_BASE_URL=https://<host>
AUTH_COOKIE_SECURE=true
AUTH_ADMIN_TOKEN=<strong-admin-token>
```

Provider-specific values can be enabled one provider at a time. Start with
Google, then Facebook, then Apple. This keeps redirect and token-exchange
failures isolated.

## Live Smoke

Run one provider at a time first. This keeps callback and credential failures
easy to isolate.

```bash
make -C examples/auth-stack spin db=sqlite transport=both listen=127.0.0.1:3008
```

Use the public URL that the provider can redirect to. For local development,
put the auth stack behind a trusted HTTPS tunnel or deploy the Spin app to a
test host.

Before starting the live preflight, run the offline credential readiness check.
It reports missing variable names and invalid live URL shapes without printing
secret values.

```bash
OAUTH_PROVIDERS=google \
make -C examples/auth-stack oauth-credentials
```

Then run the live OAuth preflight against the running Spin app. The Make target
and the underlying preflight script run the same readiness check first. Run one
provider first, then expand `OAUTH_PROVIDERS` after the first provider passes.

```bash
OAUTH_PROVIDERS=google \
BASE_URL=https://auth.example.com \
AUTH_ADMIN_TOKEN=change-me \
make -C examples/auth-stack oauth-preflight
```

The preflight checks `/api/auth/capabilities`, rejects the development callback
bypass URL, validates the external HTTPS authorization endpoint, verifies
`response_type`, `client_id`, `redirect_uri`, `scope`, `state`, and `nonce`,
and checks OAuth state storage evidence when `AUTH_ADMIN_TOKEN` is set. It does
not replace the browser callback test because provider credentials and user
login are still external dependencies.

You can print a redacted event-count report after preflight. This does not
print provider secrets, tokens, session IDs, or profile payloads.

```bash
OAUTH_PROVIDERS=google \
OAUTH_EVIDENCE_MODE=preflight \
BASE_URL=https://auth.example.com \
AUTH_ADMIN_TOKEN=change-me \
make -C examples/auth-stack oauth-evidence
```

Then verify:

1. `GET /api/auth/capabilities` includes the enabled provider.
2. `/login` shows only credentialed, enabled OAuth providers.
3. Clicking the provider redirects to the provider authorization URL.
4. Completing provider login returns to `/dashboard`.
5. `/api/auth/session` reports an authenticated session.
6. `GET /api/auth/storage/status` with `x-auth-admin-token` shows:
   `auth_oauth_state_created`, `auth_oauth_state_consumed`,
   `auth_external_identity_linked`, and `auth_session_issued`.
7. Reusing the same callback URL fails because OAuth state is single-use.

The fastest way to run the browser portion is the interactive browser smoke.
It opens Chromium, clicks the configured provider, waits for you to complete
the provider login, captures the issued `ddd_auth_session` cookie, then checks
the session and storage evidence automatically. It also replays the exact
callback URL in JSON mode and expects a conflict response, proving OAuth state
is single-use.

```bash
OAUTH_PROVIDERS=google \
BASE_URL=https://auth.example.com \
AUTH_ADMIN_TOKEN=change-me \
EXPECTED_EMAIL=user@example.com \
make -C examples/auth-stack oauth-browser-smoke
```

After completing provider login, capture the issued `ddd_auth_session` cookie
from the browser and run the callback evidence check. `SESSION_COOKIE` may be a
bare session ID, `ddd_auth_session=<id>`, or a full `Cookie:` header value.
`EXPECTED_EMAIL` is optional but should be set when the provider account is
known. This manual path is useful when the interactive browser smoke cannot run
on the operator machine.

```bash
OAUTH_PROVIDERS=google \
BASE_URL=https://auth.example.com \
AUTH_ADMIN_TOKEN=change-me \
SESSION_COOKIE='ddd_auth_session=<issued-session-id>' \
EXPECTED_EMAIL=user@example.com \
make -C examples/auth-stack oauth-callback
```

The callback evidence check verifies `/api/auth/session`, confirms `/dashboard`
is reachable with the issued session cookie, and checks storage status for
OAuth state creation, state consumption, external identity linking, session
issuance, and auth projection progress.

After callback smoke, run the same evidence report in callback mode to confirm
the final event counts and projection checkpoint.

```bash
OAUTH_PROVIDERS=google \
OAUTH_EVIDENCE_MODE=callback \
BASE_URL=https://auth.example.com \
AUTH_ADMIN_TOKEN=change-me \
make -C examples/auth-stack oauth-evidence
```

The implementation tracker marks A11 as `operator_pending` until this live
smoke passes for Google, Facebook, and Apple against real provider apps. Do
not mark A11 `done` without the preflight, browser, callback, and evidence
commands above.

## Failure Checks

- Provider missing from `/api/auth/capabilities`: check
  `AUTH_ENABLE_OAUTH`, `AUTH_<PROVIDER>_ENABLED`, and provider credentials.
- Provider returns redirect mismatch: compare the provider console callback URL
  with `AUTH_<PROVIDER>_REDIRECT_URI`.
- Apple token exchange fails: verify the Services ID, Team ID, Key ID, private
  key, and client-secret TTL.
- User lands on an auth error page: inspect `/api/auth/storage/status` and the
  Spin component logs before retrying.

## References

- [Google OAuth 2.0 for web server applications](https://developers.google.com/identity/protocols/oauth2/web-server)
- [Facebook Login security](https://developers.facebook.com/documentation/facebook-login/security)
- [Facebook Login manual flow](https://developers.facebook.com/documentation/facebook-login/guides/advanced/manual-flow)
- [Configure Sign in with Apple for the web](https://developer.apple.com/help/account/capabilities/configure-sign-in-with-apple-for-the-web/)
- [Configuring your environment for Sign in with Apple](https://developer.apple.com/documentation/signinwithapple/configuring-your-environment-for-sign-in-with-apple)
- [Sign in with Apple REST API token validation](https://developer.apple.com/documentation/signinwithapplerestapi/generate-and-validate-tokens)
