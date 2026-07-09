# Auth/Authz Production Hardening Plan (Integrated)

## Status

`in_progress` — partial WIP exists on the local `auth` branch working tree.
Tracker milestones A0–A36 remain `done`. This document defines A37–A48.

## Audience

Hand this document to an implementation agent (Codex or otherwise) as the sole
execution contract for closing production-security gaps in:

- `examples/auth-stack`
- `crates/ddd-cli/templates/auth-stack`
- related docs/PRD/smoke updates

Library crates (`crates/ddd-auth`, `crates/ddd-authz`) are mostly transport
primitives. Prefer fixing enforcement in the auth-stack application layer unless
a change clearly belongs in a reusable crate.

## Goal

Make `examples/auth-stack` a production-grade Spin auth/authz **reference
boundary**, not only a feature demo.

Keep:

- email/password as the primary local path
- OAuth and passkeys behind existing feature/operator gates
- REST, gRPC, and Leptos server functions enforcing the **same** authorization rules

Do not redesign OAuth provider UX or require live Google/Facebook/Apple
credentials for this pass (A11 stays `operator_pending`).

## Sources Merged Into This Plan

1. Original Codex production-hardening plan (surface auth, least privilege,
   CSPRNG tokens, reset hygiene, CSRF, KDF, PRD parity, docs/templates).
2. Independent production-grade audit of the `auth` branch (gRPC/Leptos holes,
   query credentials, event-log secrets, OAuth PKCE/nonce, tenant isolation,
   rate limiting, register enumeration).
3. Current working-tree WIP status (partially landed items listed below).

## Current WIP Snapshot (do not re-implement blindly)

Inspect the tree first. As of the audit/WIP merge, the following are **already
partially present** in uncommitted/local changes:

| Item | Status | Notes |
| --- | --- | --- |
| `RequestAuth` + `require_permission_for` | Landed | REST, gRPC authz, and protected Leptos server functions use it |
| Least-privilege default session permissions | Landed | `auth:session:read`, `auth:token:refresh`, `auth:logout`, `authz:check` |
| `AUTH_BOOTSTRAP_ADMIN_EMAILS` | Landed | Admin scopes added at session issue |
| CSPRNG `secure_storage_id` / `getrandom` | Landed | Fail closed on native randomness failure |
| Argon2id default + PBKDF2 verify/rehash | Landed | Env-configurable params |
| REST authz permission gates | Landed | Check/write routes call `require_permission_for` |
| REST CSRF hooks | Partial | REST cookie-authenticated refresh/logout/authz writes require token; Leptos CSRF remains open |
| `model_ref` required on authz checks | Landed | REST, gRPC, and Leptos authz check paths include/require explicit selector |
| Idempotency key presence validation | Partial | REST/gRPC/Leptos writes require a key; no durable replay store yet |
| Dev-only reset URL via `AUTH_DEV_TOOLS` | Landed | Off under `AUTH_PRODUCTION_MODE` |
| Query `?session_id=` / `?admin_token=` | Landed | REST helpers no longer read query credentials |
| gRPC authz authn/authz | Landed | Authz calls require metadata auth/admin token |
| Leptos server-fn authz gates + CSRF | Partial | Permission gates landed; CSRF token form enforcement remains open |
| Reset token hashing | **Still open** | Raw `grant_id` stored |
| Password hashes in events | **Still open** | Must stop |

Rule for the implementer: **diff first, complete missing pieces, do not wipe
working progress**.

---

## Primary Success Criteria

When this plan is complete:

1. No authz/admin mutation can run without an authenticated authorized principal
   or operational admin header on **REST, gRPC, and Leptos server functions**.
2. Normal users do not receive authz/admin write permissions by default.
3. Session IDs, OAuth state, reset tokens, passkey challenge IDs, refresh material,
   and JWT `jti` are CSPRNG-backed opaque values.
4. Password reset never leaks reset URLs/tokens or account existence in normal
   API/UI responses; production mode never echoes tokens.
5. Cookie-authenticated unsafe requests require CSRF protection.
6. Authz write APIs require durable idempotency; check/list/expand require explicit
   `model_ref`.
7. Password hashes and raw reset tokens are never stored in event payloads.
8. Query credentials (`?session_id=`, `?admin_token=`) are rejected.
9. OAuth authorization requests use PKCE S256 and independent `nonce`.
10. Authz evaluation fails closed on tenant mismatch when a tenant is in context.
11. Docs, CLI templates, Spin env wiring, and smoke tests (`CHECK_AUTH_SECURITY=1`)
    match the new behavior.

---

## Non-Goals

- Live external OAuth credential smoke (A11 remains operator-pending).
- Full email delivery provider (dev evidence path only until mail is added).
- Redesign of ReBAC model language beyond tenant fail-closed and PRD parity.
- Multi-region HA, WAF product selection, or external IdP federation beyond
  current Google/Facebook/Apple scaffolding.
- Publishing crates.io releases as part of this pass.

---

## Architecture Decisions

### Shared authorization layer

Introduce one request-auth path used by all transports:

```text
RequestAuth {
  session_id: Option<String>,      // cookie or x-auth-session only
  access_token: Option<String>,    // Authorization: Bearer
  admin_token: Option<String>,     // x-auth-admin-token only
}
```

Permission resolution order:

1. Valid `x-auth-admin-token` → operator principal for operational endpoints only
   (or explicit break-glass for configured admin routes).
2. Valid access JWT with required scope/permission.
3. Valid session cookie/header with required permission list.
4. Else `401 AuthRequired` or `403 Forbidden`.

Transport mapping:

| Transport | Credentials allowed | CSRF | Idempotency |
| --- | --- | --- | --- |
| REST | Bearer, `x-auth-session`, cookie, `x-auth-admin-token` | Required for cookie-auth unsafe methods | `Idempotency-Key` / `idempotency-key` header or body field on authz writes |
| gRPC | metadata `authorization`, `x-auth-admin-token`, `idempotency-key` | Not required (no ambient cookie) | metadata `idempotency-key` required on writes |
| Leptos server fn | cookie session only | Required on unsafe actions | hidden `idempotency_key` field on authz admin forms |

**Remove** `?admin_token=` and `?session_id=` completely. No compatibility shim.

### Permissions

Default user session:

- `auth:session:read`
- `auth:token:refresh`
- `auth:logout`
- `authz:check`

Bootstrap admin (`AUTH_BOOTSTRAP_ADMIN_EMAILS` comma-separated, normalized email match)
additionally receives:

- `auth:provider:write`
- `auth:redirect:write`
- `auth:signing-key:admin`
- `auth:storage:admin`
- `authz:model:write`
- `authz:tuple:write`

`AUTH_ADMIN_TOKEN` remains an operational/service credential, **header-only**.

### Token generation

One helper:

```text
secure_token(purpose) -> "{purpose}_{base64url(32 CSPRNG bytes)}"
```

Used for: session IDs, password reset tokens, OAuth state, passkey challenge IDs,
refresh token material, JWT `jti`.

Randomness:

- WASI/Spin: `wasip3` random host
- Native/tests: OS CSPRNG (`getrandom`)
- Fail closed if unavailable

Never embed email, provider, tenant, or wall-clock seed into security tokens.

### Password reset

- `StartPasswordReset` always returns generic accepted response (`reset_url: null`).
- Store **only hash** of reset token + expiry + consumed/replay status + audit metadata.
- Dev evidence path only when `AUTH_DEV_TOOLS=true` **and**
  `AUTH_PRODUCTION_MODE=false`.
- UI copy: “If an account exists, we sent reset instructions.”
- Production mode must reject any token-echo behavior.

### CSRF

- Prefer random per-session CSRF secret stored with the session (or hashed), not a
  pure deterministic hash of `session_id` alone if avoidable.
- `GET /api/auth/csrf` for same-origin browser clients (authenticated session).
- Require `x-csrf-token` (REST) or hidden `csrf_token` (server functions) for unsafe
  cookie-authenticated requests.
- Bearer and admin-header requests do not need CSRF.

### Password hashing

- Default `AUTH_PASSWORD_KDF=argon2id`
- Default Argon2id: memory `19456` KiB, iterations `2`, parallelism `1` (env-overridable)
- Verify legacy PBKDF2; rehash on successful login when policy upgrades
- Production mode rejects weak PBKDF2 policy (below documented minimum) and rejects
  using weak default secret material where already gated

### Event payloads must not contain secrets

Never put in durable events:

- password hashes
- raw reset tokens
- raw refresh tokens
- OAuth client secrets
- CSRF secrets

Events may record algorithm version, user id, grant id **hash**, session id, and
timestamps.

### Authz PRD parity

- Writes require idempotency key and durable replay table/lookup.
- `Check` / `BatchCheck` / `ListObjects` / `Expand` require:

```json
"model_ref": { "kind": "active" }
```

or

```json
"model_ref": { "kind": "id", "model_id": "..." }
```

- Model write/activate is tenant-aware; default tenant remains migration fallback.

### Authz tenant fail-closed

In `ddd-authz` evaluator:

- When `AuthzContext.tenant_id = Some(T)`, only tuples with `tenant_id = Some(T)` match.
- Untagged tuples (`tenant_id = None`) must **not** match tenant-scoped checks.
- Optionally reject untagged tuple writes in auth-stack production mode.

### OAuth protocol hardening (in-scope minimal)

Even though live OAuth remains operator-pending:

- Generate independent `state` and `nonce`; store both on the grant.
- Add PKCE S256 (`code_challenge`, `code_challenge_method=S256`) and verify
  `code_verifier` at token exchange.
- Keep development callback bypass gated and production-rejected.

### Abuse resistance (minimum)

- Per-email and/or per-IP failed login counter with backoff or temporary lockout.
- Uniform timing as much as practical for login failure paths.
- Registration: either keep conflict but rate-limit, or move to generic response +
  verification email later. For this pass: rate-limit register/login/reset start and
  document remaining enumeration tradeoff if conflict is kept.

---

## Public Interface Changes

### Removed

- `?admin_token=...`
- `?session_id=...`
- password reset `reset_url` in normal API/UI responses (dev-tools only)

### Added env vars

- `AUTH_BOOTSTRAP_ADMIN_EMAILS`
- `AUTH_DEV_TOOLS`
- `AUTH_PASSWORD_KDF`
- `AUTH_PASSWORD_ARGON2_MEMORY_KIB`
- `AUTH_PASSWORD_ARGON2_ITERATIONS`
- `AUTH_PASSWORD_ARGON2_PARALLELISM`
- `AUTH_PASSWORD_PBKDF2_ITERATIONS`
- `AUTH_CSRF_SECRET` (optional; prefer per-session secret if implemented)
- `AUTH_LOGIN_MAX_FAILURES` / `AUTH_LOGIN_LOCKOUT_SECONDS` (or equivalent)

### Added headers / fields

- REST `Idempotency-Key` / `idempotency-key` for authz writes
- gRPC metadata `idempotency-key`
- REST `x-csrf-token`; Leptos `csrf_token`
- Authz request `model_ref`
- Durable idempotency store table (schema + reset scripts)

### Changed behavior

- Authz/admin writes: `401` unauthenticated, `403` authenticated without permission
- gRPC: `Unauthenticated` / `PermissionDenied`
- Password reset start responses indistinguishable for existing/missing/disabled users
- Event projections no longer require password hash material

---

## Milestone Breakdown (A37–A48)

Add these rows to `docs/prd/auth-implementation-tracker.md` when starting work.
Statuses start as `not_started` unless WIP already covers them.

### Phase 0 — Inventory and stabilize WIP (A37)

**ID:** A37  
**Status start:** `in_progress` if dirty tree exists  
**Goal:** Reconcile local WIP with this plan; prevent regressions while finishing holes.

Tasks:

1. Review dirty files under `examples/auth-stack/src/{application,rest,store,app,grpc,contracts}.rs`.
2. Compile matrix:
   - `rtk env WASI_RUNTIME=spin cargo test --manifest-path examples/auth-stack/Cargo.toml --no-default-features --features ssr,sqlite`
   - `rtk env WASI_RUNTIME=spin cargo check --manifest-path examples/auth-stack/Cargo.toml --target wasm32-wasip2 --no-default-features --features ssr,sqlite,spin-grpc`
3. Produce a short “done vs remaining” checklist against this plan’s tables.
4. Do not commit secrets; keep working tree coherent before feature PRs.

**Exit:** Clean compile; known remaining gaps listed.

### Phase 1 — Close unauthenticated mutation paths (A38) — CRITICAL

**ID:** A38  
**Goal:** Every authz/admin mutation requires auth on REST, gRPC, and server functions.

Tasks:

1. **gRPC**
   - Extract metadata into `RequestAuth`.
   - Gate:
     - check/list/expand → `authz:check`
     - model write/activate → `authz:model:write`
     - tuple write/delete → `authz:tuple:write`
     - signing-key/storage admin → existing admin permission / admin token
   - Pass `idempotency-key` metadata into write requests.
   - Map errors to `Unauthenticated` / `PermissionDenied`.

2. **Leptos server functions**
   - Gate all authz admin server functions with cookie session + required permission.
   - Gate provider/redirect/signing-key/storage admin actions similarly.
   - Add CSRF validation for unsafe server functions.
   - Add hidden `csrf_token` + `idempotency_key` fields to admin forms.

3. **REST**
   - Confirm all mutation routes use `require_permission_for`.
   - Confirm CSRF on cookie-authenticated unsafe routes.
   - Remove query credential fallbacks.

**Exit smoke:**

- Unauthenticated REST/gRPC/server-fn authz writes fail.
- Normal registered user cannot write models/tuples.
- Bootstrap admin email can.
- Header admin token still works for operational endpoints.
- Query admin/session credentials rejected.

### Phase 2 — Credential transport hygiene (A39)

**ID:** A39  
**Goal:** No ambient credential leakage via query strings; CSRF complete.

Tasks:

1. Delete `?session_id=` and `?admin_token=` support from REST helpers and tests.
2. Update smoke scripts that used query credentials to headers/cookies.
3. Finish CSRF:
   - random per-session CSRF secret preferred
   - `GET /api/auth/csrf`
   - enforce on all cookie-authenticated unsafe REST + server-fn paths
4. Keep Bearer/admin-header CSRF-exempt.

**Exit:** Cookie mutation without CSRF fails; with CSRF succeeds.

### Phase 3 — Token and reset hardening (A40)

**ID:** A40  
**Goal:** Opaque CSPRNG tokens; hashed reset tokens; generic reset responses.

Tasks:

1. Confirm all security IDs use `secure_token`/`secure_storage_id`.
2. Password reset:
   - store hash only
   - complete path looks up by hash
   - replay rejected
   - no `reset_url` unless dev tools + non-production
3. Events for reset start/complete carry grant hash or opaque id only, never raw token.
4. UI generic copy; remove production UI reliance on `reset_url`.

**Exit:** Reset response generic; DB cannot yield usable raw reset token; replay fails.

### Phase 4 — Event-log secret hygiene (A41)

**ID:** A41  
**Goal:** Password hashes never enter durable events.

Tasks:

1. Remove `password_hash` from register/rehash/reset-complete event payloads.
2. Update projection handlers to source password hashes only from credential tables
   (or stop projecting hashes from events entirely).
3. Add regression test that event payload JSON for password flows contains no hash
   material.
4. Document retention implication: old events may still contain hashes until DB reset
   or stream redaction tooling (out of scope to rewrite history automatically).

**Exit:** New password flows never emit hash material in `events` table payloads.

### Phase 5 — Real idempotency + model_ref parity (A42)

**ID:** A42  
**Goal:** PRD-compliant authz write/read contracts.

Tasks:

1. Schema: idempotency records table keyed by tenant + key (+ optional scope).
2. On authz write:
   - require key
   - if same key + same body fingerprint → return stored response
   - if same key + different body → conflict
3. Ensure `model_ref` required on check/batch/list/expand across REST, gRPC, contracts,
   and UI forms.
4. Tenant-aware model write/activate if not already.

**Exit:** Missing key fails; replay returns same result; checks without `model_ref` fail.

### Phase 6 — Password KDF production policy finalize (A43)

**ID:** A43  
**Goal:** Argon2id is default; production rejects weak policies.

Tasks:

1. Confirm env knobs wired in `.env.example`, `spin.toml`, production example,
   Makefile, CLI template.
2. Production mode rejects PBKDF2 below minimum and rejects insecure defaults.
3. Unit tests: legacy PBKDF2 verify + rehash; Argon2id verify; production policy fail.

**Exit:** Unit tests pass; production config fails closed on weak KDF.

### Phase 7 — OAuth protocol minimum (A44)

**ID:** A44  
**Goal:** PKCE S256 + independent nonce without requiring live providers.

Tasks:

1. Store `state`, `nonce`, `pkce_verifier` (hashed if stored long-lived) with grant.
2. Authorization URL includes `state`, `nonce`, `code_challenge`,
   `code_challenge_method=S256`.
3. Token exchange sends `code_verifier`.
4. ID token nonce validation uses stored nonce, not state.
5. Development bypass remains gated; production rejects bypass.
6. Extend unit/smoke (`CHECK_OAUTH_STATE=1` or new flag) for PKCE/nonce persistence
   shape without live IdP if possible.

**Exit:** Local OAuth grant records show distinct state/nonce; PKCE fields present;
bypass still gated.

### Phase 8 — Authz tenant fail-closed (A45)

**ID:** A45  
**Goal:** Multi-tenant checks cannot be satisfied by untagged tuples.

Tasks:

1. Change `tuple_tenant_matches` in `crates/ddd-authz` to fail closed when context
   has a tenant.
2. Add unit tests for:
   - matching tenant allow
   - wrong tenant deny
   - untagged tuple denied under tenant context
3. Update auth-stack writes to always set tenant on new tuples.
4. Smoke confirms deny-by-default still holds.

**Exit:** `cargo test -p ddd-authz --all-features` covers tenant isolation; live check
denies untagged foreign grants.

### Phase 9 — Abuse resistance minimum (A46)

**ID:** A46  
**Goal:** Online password brute force is slowed.

Tasks:

1. Track failed login attempts (per email and/or IP if available).
2. After N failures, return generic invalid credentials with lockout window.
3. Rate-limit register and reset-start similarly if cheap.
4. Audit events for lockouts (no secrets).
5. Document remaining register enumeration tradeoff if conflict response kept.

**Exit:** Repeated bad passwords trip lockout; legitimate login still works after expiry.

### Phase 10 — Docs, templates, smoke, tracker (A47)

**ID:** A47  
**Goal:** Generated projects and docs match hardened behavior.

Tasks:

1. Update:
   - `docs/prd/auth-implementation-tracker.md` (A37–A48 rows + verification)
   - `docs/prd/ddd-auth.md`, `ddd-authz.md`, `auth-surface-contracts.md`,
     `spin-auth-stack.md`, `leptos-auth-ui.md`, `auth-verification-rollout.md`
   - `docs/production/auth-oauth-rollout.md`, `auth-storage-rollout.md` as needed
   - `examples/auth-stack/.env.example`, `spin.toml`, `spin.production.toml.example`,
     `Makefile`
   - `crates/ddd-cli` auth-stack template render + tests
2. Extend `examples/auth-stack/scripts/verify_auth_stack.sh` with
   `CHECK_AUTH_SECURITY=1` covering the smoke scenarios below.
3. Keep A11 live OAuth operator-pending.

**Exit:** `rtk bash scripts/verify-docs.sh`; CLI template tests pass; security smoke flag
documented.

### Phase 11 — Final verification gate (A48)

**ID:** A48  
**Goal:** Production-readiness evidence for this hardening pass.

Required commands:

```bash
rtk cargo test -p ddd-auth --all-features
rtk cargo test -p ddd-authz --all-features
rtk cargo test -p ddd-cqrs-es-cli --all-targets
rtk env WASI_RUNTIME=spin cargo test --manifest-path examples/auth-stack/Cargo.toml --no-default-features --features ssr,sqlite
rtk env WASI_RUNTIME=spin cargo check --manifest-path examples/auth-stack/Cargo.toml --target wasm32-wasip2 --no-default-features --features ssr,sqlite,spin-grpc
rtk env CHECK_AUTH_SECURITY=1 BASE_URL=http://127.0.0.1:3008 bash examples/auth-stack/scripts/verify_auth_stack.sh
rtk make -C examples/auth-stack smoke
rtk make -C examples/auth-stack browser-smoke
rtk cargo fmt --all -- --check
rtk bash scripts/verify-docs.sh
rtk git diff --check
```

Optional when available:

```bash
RUN_GRPC=1 CHECK_AUTH_SECURITY=1 rtk bash examples/auth-stack/scripts/verify_auth_stack.sh
```

**Exit:** All required commands pass; tracker A37–A48 marked done with evidence.

---

## Required Smoke Scenarios (`CHECK_AUTH_SECURITY=1`)

Implement as scripted checks against a live Spin server:

1. Unauthenticated REST authz model/tuple write → 401.
2. Unauthenticated gRPC authz write → Unauthenticated (when gRPC enabled).
3. Unauthenticated/unauthorized server-function authz write fails.
4. Normal registered user session cannot access authz model/tuple admin writes → 403.
5. Bootstrap admin email session can access admin UI and authz writes.
6. `?admin_token=` and `?session_id=` rejected (ignored or 400/401; not accepted).
7. Header `x-auth-admin-token` still works for operational endpoints.
8. Cookie-authenticated unsafe REST request fails without CSRF and passes with CSRF.
9. Password reset start response is generic and does not expose reset URL in
   production mode / default mode without dev tools.
10. Reset token replay fails.
11. Authz write fails without idempotency key.
12. Authz write with same key + same body is idempotent.
13. Authz check/list/expand fail without `model_ref`.
14. New password events contain no password hash field.
15. OAuth grant creation stores distinct nonce and PKCE challenge material
    (local/dev-bypass acceptable).

---

## Suggested PR / Commit Slices

Keep reviewable vertical slices; avoid one mega-commit:

| Slice | Milestones | Title idea |
| --- | --- | --- |
| 1 | A37–A38 | auth-stack: gate authz mutations on REST/gRPC/server-fn |
| 2 | A39 | auth-stack: remove query credentials and finish CSRF |
| 3 | A40–A41 | auth-stack: hash reset tokens; strip secrets from events |
| 4 | A42–A43 | auth-stack: durable idempotency + KDF production policy |
| 5 | A44–A45 | oauth PKCE/nonce + authz tenant fail-closed |
| 6 | A46–A48 | abuse limits, docs/templates, security smoke gate |

Each slice must include tests or smoke evidence in the commit message body.

---

## File Ownership Map

| Area | Primary files |
| --- | --- |
| Shared auth layer | `examples/auth-stack/src/application.rs`, `contracts.rs` |
| REST | `examples/auth-stack/src/rest.rs` |
| gRPC | `examples/auth-stack/src/grpc.rs`, `examples/auth-stack/proto/*.proto` |
| Leptos | `examples/auth-stack/src/app.rs` |
| Storage / tokens / KDF | `examples/auth-stack/src/store.rs`, reset scripts |
| OAuth | `examples/auth-stack/src/oauth.rs`, `store.rs` |
| Authz evaluator | `crates/ddd-authz/src/evaluator.rs` (+ tests) |
| CLI templates | `crates/ddd-cli/src/render.rs`, `templates/auth-stack/**`, `tests/cli.rs` |
| Smoke | `examples/auth-stack/scripts/verify_auth_stack.sh`, Makefile |
| Docs | `docs/prd/*`, `docs/production/*`, `docs/docs.json`, `docs/prd/README.md` |

---

## Assumptions

1. This is security hardening, not a product redesign of OAuth/passkeys.
2. Email delivery remains out of scope; use safe dev-only evidence until a mail
   provider lands.
3. Existing local/demo data may contain legacy PBKDF2 hashes and predictable IDs;
   login rehash handles passwords; reset scripts/expiry invalidate old grants/sessions.
4. Backward compatibility for insecure query credentials is intentionally not
   preserved.
5. Production mode must fail closed on insecure KDF, reset echo, missing admin-token
   policy (existing), weak randomness, and development OAuth bypass.
6. WIP on the working tree should be completed, not discarded.

---

## Production-Ready Definition (after A48)

The auth stack may be described as **production-grade for password + session +
JWT + authz reference deployments** when A48 passes, with these remaining
explicit external gates:

- A11 live OAuth provider credentials (operator-pending)
- Operator-managed secrets (RS256 key ring, admin token, CSRF secret if used)
- Edge rate limiting / WAF still recommended in front of Spin
- Historical event streams may still need redaction/reset if they previously
  stored password hashes

Until A48 passes, do **not** market the example as production-grade.

---

## Agent Execution Rules

When implementing this plan:

1. Read this document end-to-end before coding.
2. Diff the current tree; continue WIP rather than reimplementing completed pieces.
3. Prefer thin transport adapters over duplicating auth checks in REST/gRPC/Leptos.
4. Use `rtk` for shell/build commands in this repository when available.
5. After each phase, run the phase exit checks before moving on.
6. Update `docs/prd/auth-implementation-tracker.md` milestone status and evidence
   as phases complete.
7. Do not print secrets in logs, smoke output, or docs.
8. Do not force-push or commit unless the user asked for a commit.

## Handoff Prompt (copy/paste for Codex)

```text
Implement docs/prd/auth-production-hardening-plan.md for the ddd auth stack.

Context:
- Branch: auth (may have uncommitted WIP partially implementing this plan)
- Tracker A0–A36 are done; execute A37–A48 from the integrated plan
- Primary app: examples/auth-stack; also keep CLI template parity

Rules:
1. Read the plan fully, then git status/diff before coding
2. Complete missing pieces; do not discard existing WIP that already matches the plan
3. Priority order: A38 mutation gates → A39 query-cred/CSRF → A40/A41 token+event secrets → A42/A43 idempotency+KDF → A44/A45 OAuth PKCE/nonce + tenant fail-closed → A46 rate limits → A47 docs/templates/smoke → A48 full verification
4. REST, gRPC, and Leptos must enforce the same permissions
5. Remove ?session_id= and ?admin_token=
6. Never put password hashes or raw reset tokens in events
7. Add CHECK_AUTH_SECURITY=1 smoke coverage
8. Update docs/prd/auth-implementation-tracker.md as milestones complete
9. Run the plan’s verification commands before claiming done
10. Do not treat live external OAuth (A11) as in scope

When finished, report:
- milestones completed
- remaining gaps
- commands run and results
```
