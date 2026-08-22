# ADR-0001: Connection pooling for SQL-backed event stores

- Status: Accepted (phase 1 implemented for Postgres and MySQL)
- Date: 2026-08-22
- Scope: `src/postgres.rs`, `src/mysql.rs`, `src/sqlite.rs`

## Context

Every SQL-backed store holds one connection behind `Arc<Mutex<Connection>>`.
All database work in a process serializes on that mutex:

- Native tokio deployments hit a hard throughput ceiling regardless of core
  count; `spawn_blocking` offload moves the wait, it does not remove it.
- Under Spin/WASI each request handler contends on the same lock even when the
  host could service concurrent calls.
- Postgres and MySQL are network databases designed for many concurrent
  connections. SQLite allows only a single writer (WAL permits concurrent
  readers), so it benefits least.

## Decision (implemented)

Replace the single mutex-guarded connection with a small internal connection
pool for Postgres and MySQL:

1. Pool size resolves from an explicit constructor argument, then the
   `DDD_CQRS_ES_POOL_SIZE` environment variable, then the CPU count clamped
   to `[2, 8]` (overrides clamped to `[1, 128]`).
2. Acquisition is mutex + condvar based; leases are held across transactions
   and return to the pool on drop unless marked broken.
3. Reads retry once on a fresh connection after transport-level failures;
   writes run exactly once and only discard connections on transport errors —
   domain outcomes such as concurrency conflicts recycle healthy connections.
   Legacy single-connection constructors retain their connection even when an
   operation fails (no reconnect factory exists to replace it).
4. SQLite keeps the current single-connection mutex deliberately (single
   writer; WAL reader pooling can be a later, separate decision).

Public API stays source-compatible: existing constructors keep accepting a
single `Connection`; `connect_pooled(_with_table_name)` constructors are
additive.

## Alternatives considered

- **Status quo** - predictable, but caps throughput at one in-flight query.
- **Connection per operation** - removes serialization but pays TCP +
  auth handshake per query; unacceptable for Postgres over TLS.
- **Bigger spawn_blocking worker pool** - already in place; concurrency still
  collapses onto the one connection.

## Consequences

- Concurrent loads/appends across different streams no longer queue.
- Pool health logic (discard-on-error, optional pre-use `PING`) becomes
  shared infrastructure used by both backends.
- More failure modes to test: exhausted pool under load, poisoned mutex
  replacement, half-open connections after DB restart.

## Open questions

1. Idle eviction: close surplus connections after N seconds of disuse?
2. Health check strategy: pre-use validation vs. blind reuse + retry?
3. Should the WASI/Spin feature path expose pooling at all, or keep a single
   connection there until host-side pooling exists?

## Verification plan

Extend `crate::testing` contract tests with a concurrency test: N parallel
appends to distinct aggregates must all succeed with expected revisions, and
parallel loads must never interleave transaction fragments.
