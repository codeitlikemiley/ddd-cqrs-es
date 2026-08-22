# ADR-0001: Connection pooling for SQL-backed event stores

- Status: Proposed
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

## Decision (proposed)

Replace the single mutex-guarded connection with a small internal connection
pool for Postgres and MySQL first:

1. Pool size defaults to `std::thread::available_parallelism` clamped to
   `[2, 8]`; overridable via constructor (`with_pool_size`) and env var.
2. Acquisition is semaphore-based FIFO. Leases are held across transactions:
   the append path checks out on `BEGIN` and returns the connection after
   `COMMIT`/`ROLLBACK`, never mid-transaction.
3. Stale-connection handling reuses the existing retry-once pattern from the
   raw-TCP adapter: a failed checkout is discarded and replaced, one retry,
   then surface the error.
4. SQLite keeps the current single-connection mutex deliberately (single
   writer; WAL reader pooling can be a later, separate decision).

Public API stays source-compatible: existing constructors keep accepting a
single `Connection`; pool-aware constructors are additive.

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
