---
title: 5.11. Auth Storage Rollout
description: Roll out wasi-auth storage, transactional outboxes, projections, and SQL backend checks for the Spin fullstack app.
---

The Spin fullstack app stores authentication and authorization state as durable
events plus explicit read models. The event table is the source of truth.
Read-model tables are rebuildable projections used by login, session, provider,
passkey, organization, role, signing-key, policy, and audit queries. Embedded
Cedar is the default authorizer; optional SpiceDB relationships use a durable
outbox rather than raw tuple administration routes.

This page is the production checklist for `examples/fullstack-app`. It does not
replace database backups, provider credential setup, or cloud-specific network
allowlisting.

## Backend Status

| Backend | Status | Evidence Gate |
| --- | --- | --- |
| Spin SQLite | Stable local default | `make check db=sqlite`, `make grpc-check db=sqlite`, and local smoke checks |
| Spin PostgreSQL | Live-verified locally | `make check db=postgres`, `make grpc-check db=postgres`, `make fresh db=postgres`, and storage smoke with `POSTGRES_URL` |
| Other adapters | Not part of this template | Keep unrelated adapters in `ddd_cqrs_es`; this production template intentionally supports PostgreSQL plus Spin SQLite development only. |

## Configuration

Use the public Makefile inputs. `DATABASE_URL` is an internal value passed from
the Makefile to the Spin component.

```bash
# SQLite
make -C examples/fullstack-app check db=sqlite

# PostgreSQL
POSTGRES_URL=postgresql://user:password@host:5432/fullstack_app \
  make -C examples/fullstack-app check db=postgres
```

Runtime values used by storage rollout:

| Setting | Purpose |
| --- | --- |
| `DATABASE_BACKEND=sqlite|postgres` | Runtime backend selected by `db=<backend>` |
| `POSTGRES_URL` | PostgreSQL connection URL for `db=postgres` |
| `AUTH_STORAGE_AUTO_CATCH_UP=true` | Runs bounded projection catch-up after auth writes |
| `SYSTEM_ADMIN_BEARER` | Client-shell variable containing an access token issued by an explicit API-client login for an MFA-authenticated system administrator; it is never server configuration or a shared admin secret. |

## Fresh Schema Reset

The reset target erases only fullstack auth tables for the selected backend. It
does not carry or recreate a second schema copy; `wasi-auth` reapplies its
checksum-verified canonical migration when the app next starts.

```bash
make -C examples/fullstack-app fresh db=sqlite

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

3. Apply additive schema changes through the app bootstrap or an external
   migration tool. Avoid destructive changes while writes are active.
4. Start the Spin app with the selected backend URL.
5. Check storage status:

   ```bash
   curl -sS -f http://127.0.0.1:3008/api/auth/storage/status \
     -H "Authorization: Bearer $SYSTEM_ADMIN_BEARER"
   ```

6. Run bounded projection recovery until it reports no more scanned events:

   ```bash
   curl -sS -f -X POST \
     "http://127.0.0.1:3008/api/auth/storage/projections/run?limit=128" \
     -H "Authorization: Bearer $SYSTEM_ADMIN_BEARER"
   ```

7. Run the auth smoke suite against the live app:

   ```bash
   SMOKE_EMAIL=storage-smoke@example.test CHECK_STORAGE_EVENTS=1 \
     BASE_URL=http://127.0.0.1:3008 \
     bash examples/fullstack-app/scripts/verify_fullstack.sh
   ```

   Start that test server with
   `AUTH_BOOTSTRAP_ADMIN_EMAILS=storage-smoke@example.test`. The smoke enrolls
   MFA, uses the verified session cookie plus CSRF token for administration,
   and never installs a shared system token.

## Backfill Rules

- Backfill from `events`, not from browser-visible tables.
- Keep batches bounded with `limit=128` or another explicit limit.
- Advance checkpoints only after projection writes succeed.
- Projection writes must be idempotent because a crash can replay the last
  batch.
- Use `GET /api/auth/storage/status` to compare `latest_sequence` with
  the consolidated auth projection checkpoint.

## Rollback Rules

- If command writes fail after a deploy, stop writes first.
- Roll back app code before replaying projections with an older schema.
- If only read-model rows are corrupt, keep the event table and rebuild
  projections from a known checkpoint.
- If event rows are corrupt or missing, restore from database backup; do not
  invent replacement event history.

## Production Gate

Before a backend is marked stable for fullstack production use, capture:

- Compile evidence for HTTP and gRPC features.
- Reset evidence against a disposable live database.
- `CHECK_STORAGE_EVENTS=1` smoke evidence.
- Storage status showing projection checkpoints caught up to `latest_sequence`.
- A second manual projection run that scans zero events.
