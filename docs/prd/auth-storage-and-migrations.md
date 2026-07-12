---
title: Auth Storage and Migrations PRD
description: Plan durable storage, projections, migrations, and reset flows for ddd-auth and ddd-authz.
---

# Auth Storage and Migrations PRD

## Status

implemented

## Goal

Define how authentication and authorization data is stored, projected, migrated,
and reset for the auth stack's verified production backend family: SQLite,
PostgreSQL, and MySQL. Record Redis, Neon, Supabase, and Turso as future
backend-expansion work until dedicated Spin/WASI compatibility checks prove
they preserve durable event replay semantics for auth data.

## Non-Goals

- Do not query event payload JSON for product screens.
- Do not run schema migrations from every command handler.
- Do not claim crash-atomic idempotency for stores that cannot provide it.
- Do not require CREATE/DROP DATABASE permissions for tests.
- Do not mark Redis, Neon, Supabase, or Turso stable for the auth stack without
  live backend contract evidence.

## Success Criteria

- Auth and authz aggregates persist through the existing event-store patterns.
- SQL adapters use atomic idempotency where the backend supports it.
- Read models are explicit projections with checkpoint tracking.
- Boot schema initialization is guarded and skipped for static file requests.
- `make db=<backend> fresh` resets selected auth/authz tables and exits without
  starting the app.

## Interfaces

### Event Streams

Auth aggregate stream types:

- `auth_user`
- `auth_password_credential`
- `auth_external_identity`
- `auth_passkey_credential`
- `auth_session`
- `auth_signing_key_set`
- `auth_provider_config`

Authz aggregate stream types:

- `authz_model`
- `authz_tuple_set`

### Read Models

Required read models:

- `auth_users_by_email`
- `auth_external_identities`
- `auth_passkey_credentials`
- `auth_sessions`
- `auth_refresh_token_hashes`
- `auth_token_grants`
- `auth_jwks`
- `auth_provider_configs`
- `authz_active_model`
- `authz_relationship_tuples`
- `authz_tuple_index_by_subject`
- `authz_tuple_index_by_object`

### Schema and Migration Rules

- Use `CREATE TABLE IF NOT EXISTS` style bootstrap for example apps.
- Keep production migration strategy documented separately when schema changes
  need destructive migration or backfill.
- Validate table names through the framework schema helpers where possible.
- Save projection checkpoints monotonically.
- Use bounded projection catch-up with explicit batch sizes.

### Backend Rules

- SQLite, PostgreSQL, and MySQL are the stable SQL storage targets.
- Redis can be used for experimental persistence or wake transport, but durable
  replay remains the source of truth when paired with SQL.
- Neon, Supabase, and Turso follow their existing WASI HTTP helper constraints.
- MySQL tests use one configured database with unique per-test tables.

## Implementation Milestones

1. Define schema configs, event stream names, and read model tables.
   - Status: done for auth/authz contract constants and the
     `examples/fullstack-app` SQLite schema.
2. Add schema bootstrap and reset support for SQLite, PostgreSQL, and MySQL.
   - Status: done for Spin SQLite reset and host-side PostgreSQL/MySQL reset
     DDL in `examples/fullstack-app/scripts/reset_db.sh`. Live PostgreSQL and
     MySQL reset plus storage smoke passed against local SQL services.
3. Add projections for sessions, JWKS, provider configs, passkeys, active authz
   model, and tuple indexes.
   - Status: done. Provider configs, passkey challenges, JWKS rows,
     password reset grants, active authz model, relationship tuples, and tuple
     indexes have SQLite storage helpers. Sessions, password credentials,
     passkey credentials, external identity links, signing-key lifecycle state,
     and hashed refresh-token rows are storage-backed in the Spin example.
     Major auth/authz write paths append durable rows to the shared `events`
     table, and the admin-only storage status endpoint reports event type
     counts plus monotonic `auth.storage.read_models` and
     `authz.storage.read_models` checkpoints for smoke verification. The Spin
     SQLite example now has an admin-only bounded catch-up route that replays
     durable event payloads into auth/authz read models before checkpoint
     advancement. Application services also run best-effort bounded catch-up
     after auth/authz writes when `AUTH_STORAGE_AUTO_CATCH_UP=true`, which is
     the default. Runtime PostgreSQL/MySQL feature gates, Spin environment
     wiring, and SQL dialect routing are compile-verified. Live PostgreSQL and
     MySQL storage smoke passed with event-log and projection-checkpoint
     evidence. The production
     migration/backfill playbook is documented in
     [Auth Storage Rollout](../production/auth-storage-rollout).
4. Document future backend-expansion boundaries for Redis, Neon, Supabase, and
   Turso after SQL behavior is verified.
   - Status: done. SQLite, PostgreSQL, and MySQL are the production storage
     targets for this PRD. Redis, Neon, Supabase, and Turso remain future
     backend-expansion work until their Spin/WASI helper constraints can be
     verified without weakening durable event replay semantics. The production
     rollout guide labels those backends as not stable for auth-stack
     deployments yet.
5. Add `fresh` support to the auth stack Makefile using the existing public
   backend naming style.
   - Status: done for `db=sqlite`, `db=postgres`, and `db=mysql`. PostgreSQL
     uses `POSTGRES_URL`; MySQL uses `MYSQL_URL`; both are passed to the reset
     script as `DATABASE_URL`.

## Verification

- Contract tests for event store, idempotency store, checkpoints, snapshots when
  used, and projection catch-up.
- SQL tests prove refresh token hashes are unique, session revocation is
  queryable, active signing keys are queryable, and authz tuple indexes serve
  subject/object lookups.
- Reset tests prove `make db=sqlite fresh`, `make db=postgres fresh`, and
  `make db=mysql fresh` reset and return without serving.
- Local reset verification covers SQLite reset plus live PostgreSQL and MySQL
  reset/smoke against disposable integration databases.
- Runtime build checks use `WASI_RUNTIME=spin` to match the Spin app shape.
- Live Spin smoke with `CHECK_STORAGE_EVENTS=1` proves password, reset,
  refresh, logout, authz tuple write/delete events, admin-token enforcement for
  projection catch-up, automatic best-effort projection catch-up after writes,
  and a recovery projection run at
  `POST /api/auth/storage/projections/run?limit=128` that scans zero events
  once checkpoints are current.
