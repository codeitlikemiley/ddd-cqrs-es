# ADR-0002: Snapshot-aware `load_after_revision` in every backend

- Status: Proposed
- Date: 2026-08-22
- Scope: `src/postgres.rs`, `src/mysql.rs`, `src/sqlite.rs`, `src/redis.rs`, `src/repository.rs`

## Context

`AsyncEventStore::load_after_revision` has a default implementation that loads
the entire stream and filters in memory. No backend overrides it. When a
snapshot exists, the repository still downloads every event since sequence 0
and discards those at or below the snapshot revision:

- Cost grows linearly with total stream length even when only one event is new.
- The database already maintains `UNIQUE (aggregate_type, aggregate_id,
  revision)` (Postgres/MySQL/SQLite), so revision-bounded range scans are
  index-covered and cheap.

## Decision (proposed)

1. Each SQL backend implements `load_after_revision` with
   `WHERE aggregate_type = ? AND aggregate_id = ? AND revision > ? ORDER BY
   revision ASC`, reusing the existing row-to-envelope mappers so upcasting
   and deserialization stay byte-identical to the shared default path.
2. Redis implements it with `ZRANGEBYSCORE` on the per-aggregate stream key
   using `(revision` as exclusive min, then batch-fetches hashes through
   `FETCH_HASHES_LUA` (introduced with batched loads).
3. `AsyncRepository` snapshot flow calls `load_after_revision(snapshot.revision)`
   after hydrating from the snapshot store; without a snapshot it keeps calling
   plain `load`.
4. Extend `crate::testing` with a snapshot-resume contract: append events,
   snapshot mid-stream, assert `load_after_revision(snap.revision)` returns
   exactly the later events in order - for every backend.

## Alternatives considered

- **Keep default + filter** - zero backend code, but O(stream) IO forever.
- **Snapshot-only reads without replay** - breaks aggregate state that is not
  fully captured by snapshots; rejected.

## Consequences

- O(delta) IO for snapshot-accelerated loads; large streams stop dominating
  read latency.
- Ordering guarantee (`ORDER BY revision ASC`) moves into each backend and
  must be preserved there; the contract test pins it.
- Redis global-feed consumers are unaffected; this concerns per-aggregate
  replay only.

## Open questions

1. Should backends paginate internally (e.g., 1000-row batches) or return the
   full delta? Full delta matches current semantics; pagination can follow.
2. Does any projection code rely on receiving events at or before the snapshot
   revision today? Audit callers before switching repository behavior.
