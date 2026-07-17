# fullstack-app

Production-shaped **SaaS starter** on Fermyon **Spin** + **Leptos islands** + **wasi-auth**.

Use this example as a **boilerplate for multi-tenant B2B products**: auth, organizations, RBAC, workspace settings, account security, dashboard board, REST/gRPC — deployable from zero on Spin and operable at scale with SpinKube.

| | |
|--|--|
| Runtime | Spin (`wasm32-wasip2` / WASI HTTP) |
| Database | PostgreSQL |
| UI | Leptos **islands** (+ soft-nav chrome) |
| Auth | wasi-auth (email/password default; OAuth/passkeys optional) |
| Default listen | `http://127.0.0.1:3008` (use **localhost** for passkeys) |
| Scaffold | `ddd init --preset fullstack` or copy/fork this folder |

**Live monorepo path:**  
https://github.com/codeitlikemiley/ddd-cqrs-es/tree/main/examples/fullstack-app

---

## What you get

- **Multi-tenant workspaces** — orgs, URL slugs, membership, owner/admin/member roles  
- **Auth** — sessions, email verification, password reset, MFA / passkeys-ready, sessions list  
- **RBAC** — capabilities, step-up (AAL2) for sensitive mutations  
- **Workspace settings** — general, members, invitations, roles, audit log, danger zone  
- **Account** — profile, password, MFA, passkeys, sessions, providers  
- **Dashboard** — 12-column board, widgets, resources/queries, org vault  
- **APIs** — Leptos server functions + REST + Spin gRPC  
- **Mail outbox** — Spin enqueues; native `wasi-auth-outbox-worker` delivers (capture or Resend)  
- **UI chrome** — persistent sidebar org switcher / account / theme across soft navigations  

Deeper design notes: [DESIGN.md](./DESIGN.md) · [Persistent chrome (islands)](../../docs/tutorial/leptos-islands-persistent-chrome.md)

---

## Prerequisites

| Tool | Notes |
|------|--------|
| **Rust** | 1.93.0+ |
| **wasm32-wasip2** | `rustup target add wasm32-wasip2` |
| **cargo-leptos** | `>= 0.3.7` (`cargo install cargo-leptos`) |
| **wasm-tools** | Component inspection / toolchain gate |
| **Spin** | Fermyon Spin CLI |
| **Docker** | Local Postgres via `compose.yaml` |
| **Node** (optional) | Playwright smokes / agent login scripts |

Toolchain is checked by `make` targets (`validate-toolchain`).

---

## Run it (fast path)

### Inside this monorepo (recommended for contributors)

```bash
# from repo root
cp examples/fullstack-app/.env.example examples/fullstack-app/.env   # optional
make -C examples/fullstack-app db-up
make -C examples/fullstack-app dev transport=both
# open http://localhost:3008  (prefer localhost over 127.0.0.1 for WebAuthn)
```

Or from this directory:

```bash
cd examples/fullstack-app
cp .env.example .env   # optional
make db-up
make dev transport=both
```

**`make dev` starts two processes:** Spin (UI/API) **and** the native outbox worker (mail).  
`make spin` alone serves the app but **does not deliver** verification/reset/invite email.

| Target | Purpose |
|--------|---------|
| `make db-up` | Start Postgres |
| `make db-migrate` | Apply wasi-auth migrations |
| `make dev transport=both` | **Spin + outbox worker** (normal local) |
| `make spin transport=both` | Spin only (no mail delivery) |
| `make outbox-worker` | Worker only (second terminal) |
| `make help` | All targets + resolved public origin |
| `make smoke` | HTTP smoke against `BASE_URL` |
| `make browser-smoke` | Playwright auth flows |
| `make fresh` | Wipe Postgres + re-migrate |

Change port once for the whole stack:

```bash
make dev transport=both listen=127.0.0.1:3000
# open http://localhost:3000
```

---

## Use only this example (fork / extract)

You do **not** need the whole monorepo long-term. Three options:

### Option A — Generate a standalone app (cleanest)

Published CLI (includes this template):

```bash
cargo install ddd-cqrs-es-cli --version 0.3.0-rc.5
ddd init my-saas --preset fullstack
cd my-saas
# follow generated README: db-up, dev, …
```

That copies the **fullstack template** into a new project that depends on crates.io  
(`ddd_cqrs_es`, `wasi-auth`, `leptos-wasi-runtime`) — no monorepo required.

### Option B — Sparse-checkout just this folder

```bash
git clone --filter=blob:none --sparse https://github.com/codeitlikemiley/ddd-cqrs-es.git
cd ddd-cqrs-es
git sparse-checkout set examples/fullstack-app
cd examples/fullstack-app
# still a monorepo path-layout; for a real product prefer Option A
make db-up && make dev transport=both
```

### Option C — Copy the directory into your own repo

1. Copy `examples/fullstack-app/` into your repository.  
2. Ensure `Cargo.toml` uses **crates.io versions** (not monorepo `path =` patches), e.g.:
   - `ddd_cqrs_es = "=0.3.0-rc.5"`
   - `wasi-auth = "=0.1.0-rc.2"`
3. Remove any `[patch.crates-io]` entries that point at sibling checkouts unless you keep those crates locally.  
4. Run `make db-up && make dev transport=both`.

Local monorepo development may patch `wasi-auth` to a sibling path in `Cargo.toml` until the next published rc — that is intentional for contributors, not for a forked product.

---

## First-time product walkthrough

1. Open the public origin (`http://localhost:3008` by default).  
2. **Create workspace** / register → check capture mail (or Resend if configured).  
3. Verify email if required → land on **dashboard** board.  
4. Create or select an **organization** → open **Workspace settings** (members, roles, audit).  
5. Account flyout → profile, MFA, passkeys, sessions.

**Agent / automated login** (Playwright, etc.):

```bash
export BASE_URL=http://127.0.0.1:3008
# existing user
export BROWSER_SMOKE_EMAILS=you@example.test
export BROWSER_SMOKE_PASSWORD='your-password'
node scripts/agent_dev_login.mjs
# or local capture-mail register:
# node scripts/agent_dev_login.mjs --register --storage-state=/tmp/agent.json
```

---

## Spin vs outbox worker (two processes)

```text
Browser / REST / gRPC
        │
        ▼
┌──────────────────────────────┐
│ Spin (spin.toml)             │  UI, session, board, APIs
│ Encrypts mail → Postgres     │  Never holds Resend API key
└──────────────┬───────────────┘
               │ outbox rows
               ▼
┌──────────────────────────────┐
│ wasi-auth-outbox-worker      │  Native poller
│ capture | resend | SpiceDB   │  Secrets live here only
└──────────────────────────────┘
```

| Goal | Command |
|------|---------|
| Local UI + working verify email | `make dev` |
| App only (debug Spin logs) | `make spin` + optional `make outbox-worker` |
| Real email | `.env`: `AUTH_MAIL_TRANSPORT=resend`, `AUTH_RESEND_API_KEY`, verified `AUTH_RESEND_FROM` + worker |

Registering the same email twice **does not** re-send verification. Use `/verify-email/resend`.  
Details: [Email verification: register vs resend](#email-verification-register-vs-resend).

---

## Public origin (set once)

JWT issuer, OAuth redirects, passkey origin, mail links, and smokes share **one** origin:

| Variable | Default |
|----------|---------|
| `listen` | `127.0.0.1:3008` |
| `AUTH_PUBLIC_BASE_URL` | derived from `listen` (localhost-canonical for WebAuthn) |
| `BASE_URL` (smoke) | same as public base |

Prefer opening **`http://localhost:PORT`**, not raw `127.0.0.1`, when passkeys are enabled.

---

## Project layout (orientation)

```text
examples/fullstack-app/
  src/app/           # Leptos routes, islands, server_fns
  src/application/   # Auth/session/org application services
  src/auth_product/  # wasi-auth product wiring
  src/store/         # Postgres / board / vault adapters
  proto/             # gRPC contracts
  migrations/        # App-side SQL (auth schema owned by wasi-auth migrate)
  scripts/           # smoke, browser, agent_dev_login, benchmarks
  spin.toml          # Spin components + build
  Makefile           # db / dev / spin / smoke
  DESIGN.md          # UI tokens + chrome notes
```

CLI template mirror (contributors):

```text
examples/fullstack-app  ↔  crates/ddd-cli/templates/fullstack
bash scripts/sync_fullstack_template.sh        # or: … check
```

Manifest name for shipping: `Cargo.toml.template` in the template (Cargo cannot nest a real `Cargo.toml` inside a published crate).

---

## Feature flags & transports

Default Make profile uses **`transport=both`** (HTTP UI + gRPC).  
Common env (see `.env.example`):

- `AUTH_MAIL_TRANSPORT=capture|resend`
- `AUTH_ENABLE_OAUTH` / provider credentials  
- `AUTH_ENABLE_PASSKEYS`  
- `AUTH_VAULT_KEY_BASE64` (secrets vault)  
- Dashboard connectors: `AUTH_DASHBOARD_HTTP_*` (see below)

---

## Operator guide (deeper)

### Tokenized auth links

| Route | Guest-only when authenticated? |
|-------|--------------------------------|
| `/login`, `/register`, `/forgot-password` | Yes → dashboard |
| `/reset-password?token=…` | **No** — always show form |
| `/verify-email?token=…` | **No** — always run verify |
| `/invitations/accept?token=…` | Protected; preserve token in `next=` |

### Account settings

`/account/profile` · `/account/password` · `/account/mfa` · `/account/passkeys` · `/account/sessions` · `/account/providers` · `/u/:handle`

### Dashboard board (`/dashboard`)

Per-user **12-column board** (Spin KV), org-scoped layout/resources/queries/secrets:

| Capability | How |
|------------|-----|
| Layout | Tiles + row/stack containers |
| Edit | Drag reorder, width chips, remove |
| Catalog | Builtins + query metric/list/table |
| Data | Resources & queries (REST, Postgres SELECT, gRPC gateway) |
| Vault | `/org/{slug}/vault` — AES-GCM secrets |
| Onboarding | `/onboarding/workspace` until an org exists |

**Org KV keys:** `app_dashboard:{layout,resources,queries,secrets}:org:{organization_id}`  
Legacy user keys migrate opportunistically on first access.

| Variable | Default | Meaning |
|----------|---------|---------|
| `AUTH_DASHBOARD_HTTP_ENABLED` | on | REST/gateway master switch |
| `AUTH_DASHBOARD_HTTP_ALLOW_PRIVATE` | off | Allow localhost/RFC1918 |
| `AUTH_VAULT_REVEAL_REQUIRE_STEP_UP` | production=on | Gate reveal behind AAL2 |

Tighten `spin.toml` outbound hosts in production (`https://*:*` is for local convenience).

### Email verification: register vs resend

| Action | Result |
|--------|--------|
| 1st register | User + outbox mail |
| 2nd register (same email) | Conflict → **no** new mail |
| `/verify-email/resend` | New token + mail while `pending_verification` |

| Limit | Value |
|-------|--------|
| Resends / register attempts | 5 per email per hour |
| Token lifetime | 24 hours, one-time |

### UI chrome (contributors)

Soft navigation **must not remount** org switcher / account / theme:

- `HydrationScripts { islands_router: true }` — `src/app/router.rs`  
- `initWorkspaceChromePersist()` — content/nav/title swap only  
- Settings islands take **`slug` from the route** (never empty soft-nav loads)

Guide: [Persistent chrome on Leptos islands](../../docs/tutorial/leptos-islands-persistent-chrome.md)

### Smoke and checks

```bash
make check
make smoke
make browser-smoke
# optional: node scripts/verify_workspace_vault.mjs
# optional: node scripts/agent_dev_login.mjs --register
```

---

## Production checklist

Start from `spin.production.toml.example`:

1. HTTPS `AUTH_PUBLIC_BASE_URL` (no loopback)  
2. Real JWT issuer/audience/keys — not sample HS256  
3. Distinct ingress, vault, outbox, recovery, CSRF secrets  
4. Trusted ingress (`AUTH_REQUIRE_TRUSTED_INGRESS`) when fronting with `wasi-auth-ingress`  
5. Migrations as a deploy step (`make db-migrate` / `wasi-auth-migrate apply`)  
6. Outbox worker always running; production rejects capture + dev outbox key  
7. `AUTH_COOKIE_SECURE=true`  
8. Exact OAuth callbacks + passkey rpId/origin  
9. Smoke against real `BASE_URL`  

Local ingress proof:

```bash
export AUTH_TRUSTED_INGRESS_KEY_BASE64="$(openssl rand -base64 32)"
export WASI_AUTH_INGRESS_BIN=/path/to/wasi-auth-ingress
make spin-backend
# second terminal
make trusted-ingress
```

---

## Dependencies (crates.io)

Typical pins (see this tree’s `Cargo.toml` for exact versions):

- [`ddd_cqrs_es`](https://crates.io/crates/ddd_cqrs_es)  
- [`wasi-auth`](https://crates.io/crates/wasi-auth)  
- [`leptos-wasi-runtime`](https://crates.io/crates/leptos-wasi-runtime)  
- CLI: [`ddd-cqrs-es-cli`](https://crates.io/crates/ddd-cqrs-es-cli)  

---

## License

Same as the parent repository ([ddd-cqrs-es](https://github.com/codeitlikemiley/ddd-cqrs-es)).
