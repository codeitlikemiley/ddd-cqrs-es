# fullstack-app

Generated with `ddd init --preset fullstack`.

This project is a Spin fullstack authentication and authorization service with Leptos pages, REST endpoints, and gRPC service contracts.

- Runtime: `spin`
- DB: `postgres`
- Transport: `both`
- UI: `leptos`
- Auth: email/password enabled by default
- OAuth and passkeys: feature-flagged until credentials are configured

## Quick start

You can run targets from the **example directory** or from the **monorepo root**
with `make -C examples/fullstack-app …` (same variables and defaults either way).

> **Mail requires two processes.** `spin.toml` is only the Spin/WASM app
> (UI + APIs). Verification, password-reset, and invitation mail are delivered
> by a **native** `wasi-auth-outbox-worker`, not by a Spin component.
> `AUTH_MAIL_TRANSPORT=resend` in `.env` does **not** make `make spin` send
> email — Spin only enqueues encrypted intents in Postgres. Use
> `make dev` (Spin + worker) or run `make outbox-worker` beside `make spin`.
> See [Spin vs outbox worker](#spin-vs-outbox-worker-why-two-processes).

### From the monorepo root

```bash
# optional once: copy env into the example dir
cp examples/fullstack-app/.env.example examples/fullstack-app/.env

make -C examples/fullstack-app db-up
# preferred: Spin + mail/SpiceDB worker (needed for register/verify email)
make -C examples/fullstack-app dev transport=both
# open the printed public origin (default http://127.0.0.1:3008)
```

App only (no mail delivery — verification emails stay `pending`):

```bash
make -C examples/fullstack-app spin transport=both
# if you need email with spin alone, start the worker in another terminal:
make -C examples/fullstack-app outbox-worker
```

### From this directory

```bash
cd examples/fullstack-app   # if you are still at the monorepo root

cp .env.example .env        # optional; prefer Make-derived origin when possible
make db-up
make dev transport=both     # Spin + wasi-auth-outbox-worker (mail works)
# open the printed public origin (default http://127.0.0.1:3008)

# app only — does NOT send/capture mail until outbox-worker runs
make spin transport=both
```

### Common targets

| Target | What it does |
|--------|----------------|
| `db-up` | Start local PostgreSQL |
| `db-migrate` | Apply wasi-auth migrations |
| `dev transport=both` | **Spin + outbox worker** (use this for register/verify mail) |
| `spin transport=both` | Spin only — **no mail delivery** |
| `outbox-worker` | Worker alone (second terminal next to `spin`) |
| `smoke` | REST/web smoke against `BASE_URL` |
| `browser-smoke` | Playwright page + middleware checks |
| `fresh` | Erase Postgres data and re-migrate |
| `help` | Full target list + resolved public origin |

Examples:

```bash
# monorepo root
make -C examples/fullstack-app help
make -C examples/fullstack-app dev transport=both          # mail works
make -C examples/fullstack-app spin transport=both listen=127.0.0.1:3000
make -C examples/fullstack-app smoke

# example directory
make help
make dev transport=both
make spin transport=both listen=127.0.0.1:3000
make smoke
```

`transport=both` is the default for this app (HTTP/Leptos + gRPC). Passing it
explicitly matches shell history and docs that pin the dual-transport profile.

## Operator guide

### Public origin (set once)

Browser mutations, JWT issuer, OAuth callbacks, passkey origin, mail links, and smoke tests share **one** public origin.

| Variable | Default when unset |
|----------|--------------------|
| `listen` | `127.0.0.1:3008` |
| `AUTH_PUBLIC_BASE_URL` | `http://$(listen)` |
| `AUTH_JWT_ISSUER` | same as public base |
| OAuth redirect URIs | `$(AUTH_PUBLIC_BASE_URL)/api/auth/oauth/.../callback` |
| `AUTH_PASSKEY_ORIGIN` | public base |
| `AUTH_PASSKEY_RP_ID` | host from `listen` (no port) |
| `BASE_URL` (smoke) | public base |

```bash
make spin listen=127.0.0.1:3000
make smoke listen=127.0.0.1:3000
# open http://127.0.0.1:3000
```

Explicit `.env` values win over derivation. If `.env` pins `AUTH_PUBLIC_BASE_URL` to another host/port, keep it aligned with `listen` or remove it.

Loopback host aliases (`localhost` ↔ `127.0.0.1`) match for same-scheme same-port browser `Origin` checks. Passkeys require the **browser hostname** to equal `AUTH_PASSKEY_RP_ID` (ports are not allowed in rpId).

### Tokenized auth links (password reset / email verify)

| Route | Guest-only when authenticated? |
|-------|--------------------------------|
| `/login`, `/register`, `/forgot-password` | Yes → redirect to dashboard |
| `/reset-password` without `token` | Yes → redirect to dashboard |
| `/reset-password?token=…` | **No** — form always renders |
| `/verify-email?token=…` | **No** — verification always runs |
| `/invitations/accept?token=…` | Protected; unauth → `/auth/required?next=…` (token preserved) |

If a user is already signed in and opens a reset email, they must still see **Choose a new password**. Completing reset rotates sessions in the kernel.

### Account settings

Authenticated shell: left sidebar + topbar. Account destinations live in the account flyout:

`/account/profile` · `/account/password` · `/account/mfa` · `/account/passkeys` · `/account/sessions` · `/account/providers` · `/u/:handle` (public)

### Dashboard board (`/dashboard`)

Home is a **per-user board** (Spin KV), not a link farm:

| Capability | How |
|------------|-----|
| Layout | **12-column** grid; tiles + **row/stack containers** |
| Edit | **Edit board** → drag reorder, width chips (¼ · ⅓ · ½ · full), remove |
| Catalog | **Add widget** — builtins + **Query metric/list/table** |
| Data | Builtin snapshot + **Resources & queries** (REST, Postgres, gRPC gateway) — **workspace-scoped** |
| Bind | Edit tile → pick query + field paths |
| Vault | **`/org/{slug}/vault`** — workspace-scoped secrets (AES-GCM); create/delete modals; eye reveal |
| Board | Layout, resources, queries, and secrets key off the **selected organization** |
| Onboarding | **`/onboarding/workspace`** — focused, navigation-free first workspace setup; other protected pages stay gated until an org exists |
| Permissions | `vault.view` (all members), `vault.manage` / `vault.reveal` (owner + admin) |
| Slug | Stored in Postgres (`auth_organizations.slug`) + dual-written to Spin KV for resolve |

**Legacy KV keys** (pre-org): `app_dashboard:{layout,resources,queries,secrets}:{user_id}`  
**Org keys**: `app_dashboard:{layout,resources,queries,secrets}:org:{organization_id}`  

Opportunistic migrate runs on first board/vault access. Owners/admins can also call server fn `migrate_workspace_legacy_data` (`dry_run` supported). Ciphertext-only legacy secrets cannot be re-keyed automatically — re-enter them in the org vault.

Template dual-sync: `scripts/sync_fullstack_template.sh` (or `… check` for drift).
| Demos | **Load demo connectors** seeds REST queries and bound widgets |
| Notes | Personal sticky note (multi-instance) |

**Resources & queries (Retool-style, server-proxied)**

1. Open **Resources** on the dashboard (Catalog · Resource · Query — no secrets tab).
2. Store API keys in the **org Secret vault** (`/org/{slug}/vault`); pick them from resource auth / headers.
3. **Catalog** → REST / PostgreSQL / gRPC resource (auth + headers on REST/gRPC).
4. **Query** → REST method+path, Postgres **SELECT-only** SQL, or gRPC service/method + ProtoJSON (via **gateway_base_url**).
5. **Test** → Raw / Transformed / Meta; then bind a **Query metric/list/table** widget.
6. Optional: **Load demos** for JSONPlaceholder + app Postgres bound widgets.

Vault values are encrypted at rest with `AUTH_VAULT_KEY_BASE64` (same family as MFA). Set `AUTH_VAULT_REVEAL_REQUIRE_STEP_UP=true` to require AAL2 before the eye-reveal API returns plaintext (default on in production).

| Variable | Default | Meaning |
|----------|---------|---------|
| `AUTH_DASHBOARD_HTTP_ENABLED` | on (unless `false`/`0`) | Master switch for HTTP/REST + gateways |
| `AUTH_DASHBOARD_HTTP_ALLOW_PRIVATE` | off | Allow `localhost` / RFC1918 when `true` |
| `AUTH_DASHBOARD_GRPC_ENABLED` | off | Reserved for native Spin HTTP/2 gRPC client |
| `AUTH_VAULT_REVEAL_REQUIRE_STEP_UP` | production=on | Gate vault reveal behind AAL2 |

**PostgreSQL:** host/port/db/user + password secret. The application/auth database is reserved for internal storage and cannot be selected as a dashboard connector. Only `SELECT` / `WITH … SELECT`. Rows capped (~500). Requires `postgres://*:*` (or tighter hosts) in `spin.toml` outbound.

**gRPC:** preferred path is a **JSON HTTP gateway** (`gateway_base_url`) — unary POST to `{gateway}/{service}/{method}`. Native wasi-grpc needs Spin HTTP/2 outbound; without a gateway the runtime returns a clear error (no fake success).

`spin.toml` allows broad `https://*:*` / `http://*:*` / `postgres://*:*` for local dev. **Tighten hosts in production.**

Layout is versioned: old flat `widgets[]` boards migrate to `nodes[]` (v2) on load.

### Spin vs outbox worker (why two processes)

This is one **product**, but **two OS processes**. Only the request path lives in
`spin.toml`.

```text
Browser / REST / gRPC
        │
        ▼
┌───────────────────────────────────┐
│  Spin (spin.toml / WASM guest)    │
│  UI, REST, gRPC, session, board   │
│  Encrypts mail intent → Postgres  │
│  Returns 200 without calling      │
│  Resend / SMTP                    │
│  Never receives AUTH_RESEND_*     │
└─────────────────┬─────────────────┘
                  │ pending outbox rows
                  ▼
┌───────────────────────────────────┐
│  wasi-auth-outbox-worker (native) │
│  Polls/leases intents             │
│  capture: store for local UI      │
│  resend: AUTH_RESEND_API_KEY      │
│  Marks delivered / retry / DLQ    │
└───────────────────────────────────┘
```

| Expectation | Reality |
|-------------|---------|
| “It’s all in one Spin project” | **App** is Spin; **mail delivery** is a native side-car |
| `AUTH_MAIL_TRANSPORT=resend` in `.env` | Tells the **worker** which transport to use; Spin still only enqueues |
| `make spin` alone | Serves pages/APIs; mail stays `pending` forever |
| `make dev` | Starts **both** processes (still two runtimes under one Make target) |
| Put the worker in `spin.toml`? | No — secrets + durable poller stay native by design |

Why not embed delivery in Spin?

1. **Secrets** — browser-facing WASM must not hold Resend / SpiceDB write keys.
2. **Outbox reliability** — register succeeds even if the provider is down; the
   worker retries later.
3. **Process model** — Spin components are request-driven; delivery is a
   long-lived poller with leases and dead-letter handling.

### Transactional email

Outbox delivery uses productized **plain text + HTML** for:

- email verification
- password reset
- organization invitation

Each message includes a greeting, primary CTA button, plain URL fallback, and ignore-if-not-you footer.

| Mode | Config | Use |
|------|--------|-----|
| Capture (default) | `AUTH_MAIL_TRANSPORT=capture` + **worker** | Local demos; UI uses capture `action_url` |
| Resend | `AUTH_MAIL_TRANSPORT=resend` + **worker** + API key | Real delivery |

Provider secrets (`AUTH_RESEND_API_KEY`, `AUTH_RESEND_FROM`) belong on the **outbox worker only**, not in the Spin guest. The Makefile passes them into `outbox-worker` / `dev`, never into `spin up` variables.

```bash
# works for capture or resend (worker required either way)
make -C examples/fullstack-app dev transport=both

# or two terminals
make -C examples/fullstack-app spin transport=both
make -C examples/fullstack-app outbox-worker
```

If you only run `make spin` with `AUTH_MAIL_TRANSPORT=resend`, account creation
still succeeds and an encrypted intent is stored — but **nothing calls Resend**
until a worker is running against the same Postgres and `AUTH_OUTBOX_KEY_*`.

Do **not** expect clean Gmail reputation for loopback `http://127.0.0.1` From addresses. Use capture mode locally; production needs a verified domain + SPF/DKIM/DMARC.

### Template mirror rule

When editing this example, keep the CLI scaffold in sync:

`examples/fullstack-app/**` ↔ `crates/ddd-cli/templates/fullstack/**`

Local development may path-patch `wasi-auth` in `Cargo.toml` until the next published rc.

### Agent / automated browser login

Authenticated pages (account settings, vault, board) need a real session cookie.
Agents (Playwright, computer-use, etc.) should not scrape the login UI when a
dev API is available.

**Option A — existing password account**

```bash
export BASE_URL=http://127.0.0.1:3008
export BROWSER_SMOKE_EMAILS=you@example.test
export BROWSER_SMOKE_PASSWORD='your-password'
node scripts/agent_dev_login.mjs
# → JSON with session_id + cookie_header
```

**Option B — register + captured verification link (local only)**

Requires `AUTH_MAIL_TRANSPORT=capture`, the `mail-capture` feature, and a running
outbox worker (`make dev` / `make outbox-worker`).

```bash
export BASE_URL=http://127.0.0.1:3008
node scripts/agent_dev_login.mjs --register
# registers agent-login-*@example.test, polls
# GET /api/auth/dev/mail/latest?recipient=…&kind=email-verification,
# verifies the token, returns session_id
```

Or fetch the link yourself:

```bash
curl -fsS -G "$BASE_URL/api/auth/dev/mail/latest" \
  --data-urlencode "recipient=$EMAIL" \
  --data-urlencode "kind=email-verification" | jq .
# use .action_url or the token in .body_text
```

**Playwright storage state**

```bash
node scripts/agent_dev_login.mjs --register \
  --storage-state=/tmp/wasi-auth-agent.json
# then: browser.newContext({ storageState: '/tmp/wasi-auth-agent.json' })
```

Cookie name in local dev is typically `wasi_auth_dev_session` (value = `session_id`).
UI pages also expose **Open captured verification link** when capture transport is on.

If your `.env` uses Resend (`AUTH_MAIL_TRANSPORT=resend`), Option B will not see
captured mail — switch to `capture` for agent loops, or use Option A with a known user.

### Smoke and checks

```bash
make check                 # Spin SSR compile
make smoke                 # REST/web route smoke (uses BASE_URL)
make browser-smoke         # Playwright auth pages
make passkey-browser-smoke # requires hostname matching AUTH_PASSKEY_RP_ID
# optional authenticated vault path (needs BROWSER_SMOKE_EMAILS):
#   node scripts/verify_workspace_vault.mjs
# agent session helper:
#   node scripts/agent_dev_login.mjs --register
```

## Production checklist

Use `spin.production.toml.example` as the base. Before exposing traffic:

1. **HTTPS public origin** — `AUTH_PUBLIC_BASE_URL=https://auth.example.com` (no loopback).
2. **JWT** — production issuer, audience, key ring / secret rotation; never ship the sample HS256 secret.
3. **Distinct secrets** — ingress key, vault key, outbox key, recovery-code pepper, CSRF secret must not share development defaults.
4. **Trusted ingress** — terminate at `wasi-auth-ingress`; Spin listens on loopback only; set `AUTH_REQUIRE_TRUSTED_INGRESS=true` and matching `AUTH_TRUSTED_INGRESS_KEY_BASE64`.
5. **Migrations** — run `wasi-auth-migrate apply` / `make db-migrate` as a deployment step; WASM request path never mutates schema.
6. **Outbox worker** — always running beside Spin; share `AUTH_OUTBOX_KEY_*` and Postgres; production rejects capture transport and the documented development outbox key.
7. **Mail** — Resend (or webhook) with verified From; no provider secrets in Spin variables.
8. **Cookies** — `AUTH_COOKIE_SECURE=true` behind HTTPS; prefer `__Host-session` production cookie naming.
9. **OAuth / passkeys** — register exact callback URLs and rpId/origin against the public host.
10. **Smoke** — `make smoke` / browser smoke against the real `BASE_URL`; optional soak and ingress benchmarks under `scripts/`.

Local two-terminal ingress proof:

```bash
export AUTH_TRUSTED_INGRESS_KEY_BASE64="$(openssl rand -base64 32)"
export WASI_AUTH_INGRESS_BIN=/path/to/wasi-auth-ingress
make spin-backend
# second terminal, same environment
make trusted-ingress
```

## What the outbox worker does

`wasi-auth-outbox-worker` is not an email server and it does not replace Resend.
It is a native background process that leases encrypted mail and optional
SpiceDB jobs from PostgreSQL, calls the selected provider, and records delivery
or retry status. The Spin application commits the user change and mail intent
together, then returns without waiting for the provider.

It is **not** declared in `spin.toml`. Spin only encrypts and inserts outbox
rows. Without a worker process, intents stay `pending` whether transport is
`capture` or `resend`.

If the worker is stopped, requests can still commit but mail remains `pending`
until a worker starts again. `make dev` starts Spin and the worker together.
Use `make spin` and `make outbox-worker` separately only when you need
independent logs. Production runs the worker beside Spin, sharing PostgreSQL
and the outbox key; provider credentials stay only in the worker environment.

See [Spin vs outbox worker](#spin-vs-outbox-worker-why-two-processes) above.

The toolchain gate requires Rust 1.93.0+, `cargo-leptos >= 0.3.7`, `wasm32-wasip2`, and `wasm-tools`. The distributed P2 Rust target supplies `std`; the generated component is inspected to prove it exports `wasi:http/handler@0.3.0` and has no Preview 1 imports. The unstable `wasm32-wasip3` Rust target remains a canary.

`wasi-auth` owns the only PostgreSQL auth schema. `make db-migrate` uses its native, advisory-lock-protected migration runner before Spin starts; the WASM request component never mutates schema. `make fresh` resets PostgreSQL and reapplies the immutable migration catalog. The app and worker must share `AUTH_OUTBOX_KEY_BASE64` and `AUTH_OUTBOX_KEY_VERSION`; production rejects capture mail and the documented development key, and requires distinct ingress, vault, outbox, and recovery-code secrets.

For production, start from `spin.production.toml.example`, replace the example auth domain and database hosts with exact deployment hosts, and run the same migration binary as an explicit deployment step. Keep mail and SpiceDB write credentials only in the native worker environment. The Spin guest receives a check-only SpiceDB credential.

Production traffic terminates at the signed native ingress; Spin listens only on loopback. Obtain the signed `wasi-auth-ingress` release artifact, generate a 32-byte `AUTH_TRUSTED_INGRESS_KEY_BASE64`, and supply that same secret to both processes. For a local two-terminal ingress proof:

```bash
export AUTH_TRUSTED_INGRESS_KEY_BASE64="$(openssl rand -base64 32)"
export WASI_AUTH_INGRESS_BIN=/path/to/wasi-auth-ingress
make spin-backend
# second terminal, with the same environment
make trusted-ingress
```

Migrations `0009_context_invalidation` and `0010_typed_relationship_outbox` are mandatory. The native processes refuse unsafe configuration, and the Spin backend must not be exposed outside the pod or host. The ingress proxies Leptos, REST, and every gRPC streaming mode with backpressure, and evaluates the active REST Cedar bundle locally to avoid a second network hop. Run `scripts/benchmark_ingress_overhead.sh` for five protected-path pairs, `scripts/benchmark_fullstack.sh` for the absolute concurrency-100 SLO, and `scripts/soak_fullstack.sh` for ten-minute status, transport, revocation, memory, and sensitive-log gates.

After OAuth provider credentials and callback URLs are configured, run `make oauth-preflight` before the browser callback smoke. Use `make oauth-browser-smoke` to complete the provider login in a browser, or `make oauth-callback` with an issued session cookie to capture final callback evidence manually.

Use `ddd enable oauth-provider google`, `apple`, or `facebook` to record provider placeholders in `ddd.toml` without writing secrets.
