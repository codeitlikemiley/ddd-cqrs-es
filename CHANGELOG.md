# Changelog

## 0.4.0-alpha.3

- **Internal:** projection runner catch-up loops (sync), persisted batch apply, and
  owned-checkpoint sync batch paths now share helpers in `projection.rs`
  (`run_projection_catch_up`, `run_persisted_projection_batch`,
  `run_owned_checkpoint_projection_batch`, and async persisted twins). Async runner
  catch-up and owned-checkpoint `run_batch` loops stay inline because async methods
  return futures borrowing `&mut self` until `.await` completes.

## 0.4.0-alpha.2

- **Additive:** [`EventStoreErrorKind`], [`EventStoreError::kind`],
  [`EventStoreError::is_retryable`], and [`RepositoryError::is_retryable`] provide
  a public retryability taxonomy for command and append retry loops.
- **Additive:** [`EventStoreErrorSource::from_error`] and
  [`EventStoreErrorSource::downcast_ref`] preserve typed adapter sources instead
  of stringifying them.
- **Additive:** [`EventStoreError::public_message`] and
  [`EventStoreError::store_source`] document the scrubbed transport surface vs.
  preserved adapter detail. See [error handling](docs/production/error-handling.md).

## 0.4.0-alpha.1

- **Breaking:** `EventEnvelope::aggregate_type` is now [`AggregateType`], a
  serde-transparent `Cow<'static, str>` newtype matching [`EventType`]. Typed
  feeds can borrow [`Aggregate::aggregate_type`] without allocating per
  envelope. Convert with `as_str()` or `into_string()` at store and protocol
  boundaries.

## 0.3.0-rc.8

- **Deferred to 0.4:** `EventEnvelope::aggregate_type` remains an owned
  `String` in 0.3. A `Cow<'static, str>` newtype (matching [`EventType`]) is
  planned for 0.4 so typed feeds can borrow
  [`Aggregate::aggregate_type`](https://docs.rs/ddd_cqrs_es/latest/ddd_cqrs_es/aggregate/trait.Aggregate.html#tymethod.aggregate_type)
  without allocating per envelope. The field documents this deferral; no API
  change in 0.3.
- **Breaking:** `Aggregate::Command` and `Aggregate::Error` now require `Send`,
  matching the async repository and command-bus APIs that hold commands and
  domain errors across `.await`. Aggregates that used non-`Send` command or
  error types (for example `Rc` or thread-local handles) must switch to
  `Send` alternatives before upgrading.

## 0.3.0-rc.7

- **Fixed (unbounded memory / Redis data loss):** projection runners no longer
  buffer the entire tail after their checkpoint. Every `run(...)` (in-memory,
  persisted, checkpointed, transactional, and the async twins) now repeats
  `run_batch(...)` until a batch reports `caught_up`, so catching up on a large
  backlog costs one batch of memory at a time instead of one allocation the
  size of the backlog. A full batch that fails to move the feed position ends
  the loop rather than replaying forever.
- **Breaking (custom stores):** `load_global_after_limited` is now the required
  replay primitive on `EventStore` / `AsyncEventStore`, and `load_global_after`
  is provided — its default pages through the bounded method instead of the
  bounded method collecting the whole tail and truncating it. Custom stores that
  implemented only `load_global_after` must implement
  `load_global_after_limited` (pushing the limit into the backend query) and may
  drop their unbounded override. `load_global_after` is kept, not deprecated,
  and documented as unbounded: use it for tests, small fixtures, and explicit
  maintenance jobs only.
- **Fixed (Redis, permanently unreadable streams):** the Redis adapter chunks
  and pages every read. Event hashes are fetched at most 256 keys per `EVAL`,
  and sorted-set indexes are read through
  `ZRANGEBYSCORE ... WITHSCORES LIMIT` pages of 500 members with a score cursor
  instead of one unbounded `ZRANGE` / `ZRANGEBYSCORE`. Previously a single
  batched `HGETALL` reply carried 21 RESP elements per event, so any stream or
  replay backlog past roughly 47,600 events exceeded the raw RESP client's
  1,000,000 element array limit and could never be loaded again.

- **New:** `PersistedProjectionRunner::with_aggregate_scoped_checkpoints` (and
  the async twin) keys checkpoints on the projection name **and** the feed being
  replayed, so one projection driven by several aggregate types no longer shares
  a single checkpoint row. Global replay feeds are aggregate-type scoped, so
  under the old name-only keying whichever feed advanced furthest hid the other
  feeds' events permanently. `run_raw_batch` gets its own cross-aggregate scope
  under the new keying instead of sharing a position with a typed feed.
  `PersistedProjectionRunner::new` keeps name-only keying, so no existing
  deployment is rewound; when opting in, seed the new rows from the old one with
  the exposed `aggregate_scoped_checkpoint_key` / `raw_checkpoint_key` helpers
  rather than replaying from zero.
- **Fixed (CLI, path traversal):** `ddd` validates `ddd.toml` before codegen —
  every `[domains]` module key must be a snake_case identifier, aggregates must
  be Rust identifiers, and `project.name` must be a usable crate name — and every
  generated read and write is joined through a containment check that rejects
  absolute paths and `..` components. A crafted manifest in a cloned repository
  (for example a `[domains."../../../outside"]` key) can no longer make
  `ddd add` write outside the project root.
- **Fixed (data loss):** the schema migrator no longer answers a failed
  "does the bookkeeping table have a `table_name` column?" probe with
  `DROP TABLE`. Postgres and MySQL propagate the probe error instead of
  collapsing it to `false`, the Postgres probe is resolved through
  `to_regclass` (so it cannot match a same-named table in another schema) and
  ignores dropped columns, and all three dialects now refuse with a recovery
  message when the migrations table exists in an unexpected shape instead of
  dropping it.
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
