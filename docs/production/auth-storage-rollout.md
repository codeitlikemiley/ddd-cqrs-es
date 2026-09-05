---
title: 5.11. Auth Storage Rollout
description: Roll out wasi-auth storage, transactional outboxes, projections, and SQL backend checks for the Spin fullstack app.
---

The Spin fullstack app stores authentication and authorization state in the
`wasi-auth` PostgreSQL relational kernel. Users, credentials, sessions,
providers, passkeys, organizations, roles, policies, and audit rows are
authoritative relational records; auth event replay and projection catch-up
are not part of this product. Each mutation is one typed PostgreSQL statement
that also commits idempotency, authorization revision, audit, and encrypted
outbox changes. Embedded Cedar is the default authorizer; mail and optional
SpiceDB relationships share the canonical `auth_outbox`.

This page is the production checklist for `examples/fullstack-app`. It does not
replace database backups, provider credential setup, or cloud-specific network
allowlisting.

## Backend Status

| Backend | Status | Evidence Gate |
| --- | --- | --- |
| Spin PostgreSQL | RC production profile | Migration, HTTP/gRPC, browser, performance, revocation, and soak gates |
| Spin SQLite | Removed from auth product | The former migration-only feature diverged and implemented no product workflows |
| Other adapters | Not part of this template | Keep unrelated adapters in `ddd_cqrs_es`; this identity template intentionally has one PostgreSQL path. |

## Configuration

Use the public Makefile inputs. `DATABASE_URL` is an internal value passed from
the Makefile to the Spin component.

```bash
POSTGRES_URL=postgresql://user:password@host:5432/fullstack_app \
  make -C examples/fullstack-app check db=postgres
```

Runtime values used by storage rollout:

| Setting | Purpose |
| --- | --- |
| `DATABASE_BACKEND=postgres` | The sole identity-product backend |
| `POSTGRES_URL` | PostgreSQL connection URL |
| `SYSTEM_ADMIN_BEARER` | Client-shell variable containing an access token issued by an explicit API-client login for an MFA-authenticated system administrator; it is never server configuration or a shared admin secret. |

## Fresh Schema Reset

The reset target erases only fullstack auth tables for the selected backend. It
does not carry or recreate a second schema copy; `wasi-auth` reapplies its
checksum-verified canonical migration when the app next starts.

```bash
POSTGRES_URL=postgresql://user:password@host:5432/fullstack_app \
  make -C examples/fullstack-app fresh db=postgres
```

Use `fresh` for local development, disposable integration databases, or a
planned destructive reset. Do not run it against production data unless the
event history has been exported or the environment is intentionally being
destroyed.

Do not treat historical MySQL auth-stack evidence as evidence for this
consolidated template. PostgreSQL must be re-verified against the current
`wasi-auth` schema and all REST/gRPC/browser flows before stable promotion.

## Safe Rollout Sequence

1. Back up the SQL database or snapshot the managed database instance.
2. Compile the selected backend:

   ```bash
   make -C examples/fullstack-app check db=postgres
   make -C examples/fullstack-app grpc-check db=postgres
   ```

3. Apply additive schema changes with the generated `wasi-auth-migrate`
   binary before starting Spin. The request component never mutates schema.
   For `wasi-auth` `0.1.0-rc.5` and later, this **must** include migration
   `0014_totp_last_consumed_step` before serving MFA step-up traffic; see
   [TOTP replay protection](#totp-replay-protection-0014_totp_last_consumed_step).
4. Start the Spin app with the selected backend URL.
5. Check storage status:

   ```bash
   curl -sS -f http://127.0.0.1:3008/api/auth/storage/status \
     -H "Authorization: Bearer $SYSTEM_ADMIN_BEARER"
   ```

6. Run the auth smoke suite against the live app:

   ```bash
   SMOKE_EMAIL=storage-smoke@example.test CHECK_STORAGE_EVENTS=1 \
     BASE_URL=http://127.0.0.1:3008 \
     bash examples/fullstack-app/scripts/verify_fullstack.sh
   ```

   Start that test server with
   `AUTH_BOOTSTRAP_ADMIN_EMAILS=storage-smoke@example.test`. The smoke enrolls
   MFA, uses the verified session cookie plus CSRF token for administration,
   and never installs a shared system token.

## Migration Rules

- Never edit an applied migration or overwrite its checksum.
- Apply additive numbered migrations under the PostgreSQL advisory lock.
- Reject checksum drift, gaps, duplicates, and unknown future versions.
- Run `wasi-auth-migrate verify-database` after apply and before serving.
- Back up authoritative relational tables before a destructive transform.

## TOTP replay protection (`0014_totp_last_consumed_step`)

`wasi-auth` `0.1.0-rc.5` closes the TOTP replay window by persisting
`auth_totp_factors.last_consumed_step` and rejecting an RFC 6238 step that was
already consumed (enrollment confirm and step-up verification). **Do not rely
on that protection until migration `0014_totp_last_consumed_step` is applied.**
An older schema with rc.5 application code still accepts the same step more than
once within skew.

The fullstack template ships `wasi-auth-migrate` behind the `migrate` feature.
Operators run it as a deploy step; the Spin request component never mutates
schema.

```bash
# From the fullstack tree (local compose Postgres must be up for db-migrate):
POSTGRES_URL=postgresql://fullstack:fullstack@127.0.0.1:54329/fullstack_app \
  make -C examples/fullstack-app db-migrate db-verify

# Or directly (any consumer that vendors wasi-auth-migrate):
DATABASE_URL='postgresql://user:password@host:5432/fullstack_app' \
  cargo run --quiet --no-default-features --features migrate --bin wasi-auth-migrate -- plan

DATABASE_URL='postgresql://user:password@host:5432/fullstack_app' \
  cargo run --quiet --no-default-features --features migrate --bin wasi-auth-migrate -- apply

DATABASE_URL='postgresql://user:password@host:5432/fullstack_app' \
  cargo run --quiet --no-default-features --features migrate --bin wasi-auth-migrate -- verify-database
```

The apply report must list `0014_totp_last_consumed_step` under `applied` with
`pending: none` and `database verified: true`. Confirm the column exists:

```bash
psql "$DATABASE_URL" -c '\d auth_totp_factors' | grep last_consumed_step
# expect: last_consumed_step | bigint | ...
```

Install a pinned migration binary in production instead of `cargo run` when
the deploy host does not carry the application workspace:

```bash
cargo install wasi-auth --version 0.1.0-rc.5 --bin wasi-auth-migrate
DATABASE_URL='postgresql://...' wasi-auth-migrate apply
DATABASE_URL='postgresql://...' wasi-auth-migrate verify-database
```

## Rollback Rules

- If command writes fail after a deploy, stop writes first.
- Roll back app code only when the older binary supports the already-applied
  additive schema.
- Never roll back by deleting migration rows or restoring old checksums.
- If authoritative auth rows are corrupt or missing, restore from database
  backup; there is no auth event log from which to rebuild them.

## Production Gate

Before a backend is marked stable for fullstack production use, capture:

- Compile evidence for HTTP and gRPC features.
- Reset evidence against a disposable live database.
- `CHECK_STORAGE_EVENTS=1` smoke evidence.
- `wasi-auth-migrate verify-database` after the final migration, including
  `0014_totp_last_consumed_step` when running `wasi-auth` `0.1.0-rc.5` or
  later.
- The dedicated live PostgreSQL kernel contracts, including final-owner
  concurrency, token replay, TOTP step replay rejection, outbox delivery, and
  invalidation notification.
