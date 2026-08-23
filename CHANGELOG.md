# Changelog

## 0.3.0-rc.7

- **New:** `RawEventFeed` / `AsyncRawEventFeed` — a bounded cross-aggregate
  replay feed returning `RawEventEnvelope`
  (`EventEnvelope<serde_json::Value, String>`) in global commit order with no
  aggregate-type filter, implemented for Postgres, MySQL, SQLite, Redis, the
  in-memory store, and the JSON file store. Raw projections are plain
  `Projection<serde_json::Value, String>` driven by
  `PersistedProjectionRunner::run_raw_batch` (sync and async) with the same
  once-per-batch checkpointing as typed runners. See ADR-0003.
- **Dev-file format change:** `JsonFileEventStore` now stores events as JSON
  Lines (one envelope per line) with fsync-before-acknowledge appends;
  legacy whole-array files are migrated in place on first read. Appends are
  O(1) writes instead of whole-file rewrites; `JsonFileCheckpointStore`
  writes are also fsynced.
- `EventEnvelope::builder(...)` replaces the ten-positional-argument
  construction pattern in docs (`new` remains available); `EventType` is
  backed by `Cow<'static, str>` with `EventType::from_static`, so
  `DomainEvent::event_type()` names no longer allocate per event on append.
- `InMemoryEventStore` stores each envelope once (per-stream indices into the
  global log) and serves global replay polls with a binary search instead of
  a full scan.
- Schema migration errors from Postgres/MySQL carry the server's detailed
  message and SQLSTATE/errno code instead of the client's vague Display.
- The sync/async idempotency wait and release policies are shared code, so
  the repository twins can no longer drift.
- Multi-event SQL appends batch into one `INSERT` statement: Postgres uses a
  single `INSERT ... SELECT FROM UNNEST ... RETURNING` round trip; MySQL uses
  one multi-row `INSERT` plus one sequence read-back (two round trips
  regardless of event count).
- Connection pooling for the native Postgres and MySQL event stores
  (`connect_pooled*`, sized via `DDD_CQRS_ES_POOL_SIZE` or CPU count), with
  broken-connection eviction, a bounded 30s acquire wait instead of blocking
  forever on an exhausted pool, and code-aware eviction: statement-level
  backend errors (unique violations, constraint failures) keep their
  connection pooled; only connection-level codes and IO failures evict.
- Checkpoint, idempotency, and snapshot stores can share the event store's
  pool via `checkpoint_store()`, `idempotency_store()`, and
  `snapshot_store()` accessors on the Postgres and MySQL event stores.
- SQL schema migrations serialize behind database-wide advisory locks
  (`pg_advisory_lock` / `GET_LOCK`), fixing a fresh-database race between
  concurrently initializing stores.
- Concurrent `append_idempotent` calls racing on one idempotency key now
  surface `IdempotentAppendError::Pending` (retried by the repository wait
  loop) instead of a fatal backend error.
- Projection runners flush checkpoints once per pass/batch instead of once
  per event, still recording progress through the last successful event
  before reporting a mid-pass projection failure.
- Redis: the WASI and Spin clients reuse their established connections
  (probe-guarded on the raw RESP client) instead of reconnecting per command,
  and the batched hash fetch returns a flat length-prefixed reply that the
  Spin `redis-result` interface can represent.
- SQLite statements go through `prepare_cached`, removing SQL re-parsing
  from every load, append, and checkpoint call.
- `UpcasterRegistry::upcast` rejects non-advancing upcasters (version cycles)
  with an error instead of looping forever.
- `adapters/runtime.rs` split into per-transport modules; the retired
  raw-TCP `wasi-postgres-tcp` / `wasi-mysql` drivers are deleted (recoverable
  from git history) along with their feature-graph allowances and docs rows.
- ADR-0003 documents that global replay feeds are scoped per aggregate type,
  with the cross-aggregate workaround pattern and revisit criteria.
- Dependencies: `wasi-auth` pin moved to `0.1.0-rc.3` after the `rc.2` yank;
  RUSTSEC-2026-0253 (unsound `lru` via `mysql`, unreachable panic-safety
  path) is documented and ignored in `.cargo/audit.toml` until a `mysql`
  release moves past `lru 0.18.2`.
- **Breaking:** `Aggregate` no longer requires `fn revision(&self)`. The
  framework always derived revisions from persisted envelopes
  (`LoadedAggregate::revision`); aggregates no longer need to carry and bump a
  shadow revision field. Delete the method (and the field, if nothing else
  uses it) from your aggregates; read the stream revision from
  `LoadedAggregate` instead. `ddd add aggregate` generates the new shape.
- **Breaking:** `EventStoreError` consolidated: each `X(String)` /
  `XWithSource { .. }` variant pair is now a single struct variant
  `X { message, code, source }`. Construct via `EventStoreError::backend(..)`,
  `backend_with_source(..)`, and the other per-kind constructors; match with
  `EventStoreError::Backend { .. }` patterns. The new
  `code: Option<String>` carries the backend's machine-readable error code
  (SQLSTATE for Postgres, server errno for MySQL, extended result code for
  SQLite), exposed via `EventStoreError::code()`. Display strings are
  unchanged; equality compares kind, message, and code and ignores sources.

## 0.3.0-rc.6

- Fullstack product domain: `ddd add aggregate` wires `src/domain_app` (InMemory
  demo store) and `/api/domain/...` REST hooks beside wasi-auth; refuse unwired
  projection/route stubs.
- `ddd serve` for fullstack uses `make dev`; scaffold UX via
  `make scaffold-fullstack`.
- Docs: promote Fullstack SaaS (Spin + wasi-auth) and islands chrome guide to
  top-level nav.
- **Note:** `wasi-auth` stays at `0.1.0-rc.2` (no auth crate changes in this
  release).

## 0.3.0-rc.5

- Fullstack settings islands take **route `slug` props** so soft-nav no longer
  hits empty-slug 500s after client-side hops.
- CI: read publishable package version dynamically; normalize monorepo-only
  fullstack example artifacts during drift checks.
- CLI ships the dual-synced product README for `ddd init --preset fullstack`.
- **Note:** `wasi-auth` stays at `0.1.0-rc.2` (no auth crate changes in this
  release).

## 0.3.0-rc.4

- Fix CLI packaging: ship fullstack manifest as `Cargo.toml.template` so
  `cargo package` includes the full template tree (nested `Cargo.toml` was
  treated as a separate package and dropped from the crate).
- Yank recommendation: `ddd-cqrs-es-cli 0.3.0-rc.3` cannot scaffold fullstack
  (`fullstack Cargo.toml must be embedded`); use `0.3.0-rc.4` or later.

## 0.3.0-rc.3

- Fullstack Leptos template: **persistent workspace chrome** soft-nav so org
  switcher, account menu, and theme stay mounted across in-app hops
  (`islands_router` + content-only region swap).
- Cache-first chrome snapshot and client-side flyout focus for settings/account
  menus; composable skeleton loaders for page bodies.
- Documented the technique in
  `docs/tutorial/leptos-islands-persistent-chrome.md` for reuse in other islands
  apps.
- Dual-synced the CLI `fullstack` template with the example product tree.
- **Note:** `wasi-auth` stays at `0.1.0-rc.2` (no auth crate changes in this
  release).

## 0.3.0-rc.2

- Added native Resend delivery through `wasi-auth-outbox-worker` with durable
  delivery status, provider idempotency, and secret-isolated worker startup.
- Clarified the outbox worker, capture mail, Resend configuration, and local
  versus production process topology in the fullstack documentation.
- Fixed verification-page navigation, capture-link UX, stale-cookie handling,
  and browser smoke expectations for ordinary users versus system admins.

## 0.3.0-rc.1

- Removed the duplicate `ddd-auth` and `ddd-authz` products in favor of the
  single `wasi-auth` dependency while keeping the DDD core identity-agnostic.
- Replaced the `auth-stack` CLI preset with the canonical `fullstack` preset and
  byte-for-byte generated `examples/fullstack-app`.
- Added final-WASI Leptos islands, REST, and Spin gRPC dispatch to the fullstack
  product surface.
