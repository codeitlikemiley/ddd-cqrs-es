---
title: 5.11. Auth Storage Rollout
description: Roll out ddd-auth and ddd-authz storage, projection backfills, and SQL backend checks for the Spin auth stack.
---

The Spin auth stack stores authentication and authorization state as durable
events plus explicit read models. The event table is the source of truth.
Read-model tables are rebuildable projections used by login, session, provider,
passkey, signing-key, and authz tuple queries.

This page is the production checklist for `examples/auth-stack`. It does not
replace database backups, provider credential setup, or cloud-specific network
allowlisting.

## Backend Status

| Backend | Status | Evidence Gate |
| --- | --- | --- |
| Spin SQLite | Stable local default | `make check db=sqlite`, `make grpc-check db=sqlite`, and local smoke checks |
| Spin PostgreSQL | Live-verified locally | `make check db=postgres`, `make grpc-check db=postgres`, `make fresh db=postgres`, and storage smoke with `POSTGRES_URL` |
| Spin MySQL | Live-verified locally | `make check db=mysql`, `make grpc-check db=mysql`, `make fresh db=mysql`, and storage smoke with `MYSQL_URL` |
| Redis, Neon, Supabase, Turso | Not stable for auth stack yet | Future backend-expansion work after dedicated Spin/WASI compatibility checks |

## Configuration

Use the public Makefile inputs. `DATABASE_URL` is an internal value passed from
the Makefile to the Spin component.

```bash
# SQLite
make -C examples/auth-stack check db=sqlite

# PostgreSQL
POSTGRES_URL=postgresql://user:password@host:5432/auth_stack \
  make -C examples/auth-stack check db=postgres

# MySQL
MYSQL_URL=mysql://user:password@host:3306/auth_stack \
  make -C examples/auth-stack check db=mysql
```

Runtime values used by storage rollout:

| Setting | Purpose |
| --- | --- |
| `DATABASE_BACKEND=sqlite|postgres|mysql` | Runtime backend selected by `db=<backend>` |
| `POSTGRES_URL` | PostgreSQL connection URL for `db=postgres` |
| `MYSQL_URL` | MySQL connection URL for `db=mysql` |
| `AUTH_STORAGE_AUTO_CATCH_UP=true` | Runs bounded projection catch-up after auth/authz writes |
| `AUTH_ADMIN_TOKEN` | Required for storage status and manual projection recovery routes |

## Fresh Schema Reset

The reset target drops and recreates only auth-stack tables for the selected
backend. It does not start the server.

```bash
make -C examples/auth-stack fresh db=sqlite

POSTGRES_URL=postgresql://user:password@host:5432/auth_stack \
  make -C examples/auth-stack fresh db=postgres

MYSQL_URL=mysql://user:password@host:3306/auth_stack \
  make -C examples/auth-stack fresh db=mysql
```

Use `fresh` for local development, disposable integration databases, or a
planned destructive reset. Do not run it against production data unless the
event history has been exported or the environment is intentionally being
destroyed.

Local PostgreSQL smoke evidence exists for the auth stack using
`postgresql://uriah@127.0.0.1:5432/auth_stack_test`. Local MySQL smoke evidence
exists using a disposable MySQL 9.7.1 instance on `127.0.0.1:33306` with an
`auth_stack_test` database. Both verified paths reset the schema, ran the Spin
server with the selected backend, passed `CHECK_STORAGE_EVENTS=1`, returned 11
stored events, and proved a manual projection recovery run had zero remaining
events to scan.

## Safe Rollout Sequence

1. Back up the SQL database or snapshot the managed database instance.
2. Compile the selected backend:

   ```bash
   make -C examples/auth-stack check db=postgres
   make -C examples/auth-stack grpc-check db=postgres
   ```

3. Apply additive schema changes through the app bootstrap or an external
   migration tool. Avoid destructive changes while writes are active.
4. Start the Spin app with the selected backend URL.
5. Check storage status:

   ```bash
   curl -sS -f http://127.0.0.1:3008/api/auth/storage/status \
     -H "x-auth-admin-token: $AUTH_ADMIN_TOKEN"
   ```

6. Run bounded projection recovery until it reports no more scanned events:

   ```bash
   curl -sS -f -X POST \
     "http://127.0.0.1:3008/api/auth/storage/projections/run?limit=128" \
     -H "x-auth-admin-token: $AUTH_ADMIN_TOKEN"
   ```

7. Run the auth smoke suite against the live app:

   ```bash
   CHECK_STORAGE_EVENTS=1 AUTH_ADMIN_TOKEN="$AUTH_ADMIN_TOKEN" \
     BASE_URL=http://127.0.0.1:3008 \
     bash examples/auth-stack/scripts/verify_auth_stack.sh
   ```

## Backfill Rules

- Backfill from `events`, not from browser-visible tables.
- Keep batches bounded with `limit=128` or another explicit limit.
- Advance checkpoints only after projection writes succeed.
- Projection writes must be idempotent because a crash can replay the last
  batch.
- Use `GET /api/auth/storage/status` to compare `latest_sequence` with
  `auth.storage.read_models` and `authz.storage.read_models`.

## Rollback Rules

- If command writes fail after a deploy, stop writes first.
- Roll back app code before replaying projections with an older schema.
- If only read-model rows are corrupt, keep the event table and rebuild
  projections from a known checkpoint.
- If event rows are corrupt or missing, restore from database backup; do not
  invent replacement event history.

## Production Gate

Before a backend is marked stable for auth-stack production use, capture:

- Compile evidence for HTTP and gRPC features.
- Reset evidence against a disposable live database.
- `CHECK_STORAGE_EVENTS=1` smoke evidence.
- Storage status showing projection checkpoints caught up to `latest_sequence`.
- A second manual projection run that scans zero events.
