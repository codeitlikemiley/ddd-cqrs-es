---
title: 5.0. Production Guarantees
description: Understand which ddd_cqrs_es APIs are portable, which are transaction-aware, and where applications must provide their own delivery guarantees.
---

`ddd_cqrs_es` keeps the domain model portable, but production systems need to be explicit about where durability, idempotency, projection consistency, and realtime delivery guarantees come from.

## Stable Production Surface

Use the native SQL adapters when you need production persistence guarantees:

| Capability | SQLite | PostgreSQL | MySQL |
| :--- | :--- | :--- | :--- |
| Event append with optimistic concurrency | Yes | Yes | Yes |
| Atomic idempotent append | Yes | Yes | Yes |
| Durable checkpoints | Yes | Yes | Yes |
| Durable snapshots | Yes | Yes | Yes |
| Schema table-name validation | Yes | Yes | Yes |

WASI, Spin, Neon, Turso, Supabase, Redis, and JSON-file helpers remain useful for demos, edge experiments, and runtime-specific integration work. Treat those helpers as experimental until your application has a live contract matrix for the runtime and backend combination you deploy.

## Command Execution APIs

The generic repository APIs work with any event store implementation:

```rust
let committed = repo.execute(
    &account_id,
    command,
    Metadata::default(),
)?;
```

For idempotency, the portable path uses a separate `IdempotencyStore`:

```rust
let committed = repo.execute_idempotent(
    &account_id,
    command,
    Metadata::default(),
    IdempotencyKey::new(request_id),
    &idempotency_store,
)?;
```

This path is bounded and portable, but it is not crash-atomic across the idempotency store and event store. If a process dies between appending events and saving the completed idempotency result, a retry may need application-level reconciliation.

For production SQL command handlers, use the atomic SQL path:

```rust
let committed = repo.execute_idempotent_atomic(
    &account_id,
    command,
    Metadata::default(),
    IdempotencyKey::new(request_id),
)?;
```

`execute_idempotent_atomic` requires an event store implementing `AtomicIdempotentEventStore`. The SQLite, PostgreSQL, and MySQL adapters reserve the idempotency key, append events, and save the completed result inside one database transaction. Retrying the same key returns the original committed event stream without appending duplicates.

Async applications can use the matching `AsyncRepository::execute_idempotent_atomic` path with stores that implement `AsyncAtomicIdempotentEventStore`.

## Durable Snapshots

Snapshots are optional accelerators for long streams. They must never replace the event log.

```rust
let snapshot_store = SqliteSnapshotStore::<Account>::new(snapshot_connection)?;

let loaded = repo.load(&account_id)?;
snapshot_store.save_snapshot(Snapshot::new(
    account_id.clone(),
    loaded.revision,
    loaded.state,
    Metadata::default(),
))?;

let loaded = repo.load_with_snapshot(&account_id, &snapshot_store)?;
```

`SqliteSnapshotStore`, `PostgresSnapshotStore`, and `MySqlSnapshotStore` store aggregate state and metadata as JSON. Saving an older revision will not overwrite a newer snapshot for the same aggregate stream.

## Projection Consistency

The standard checkpointed projection runner keeps checkpoint state separate from the read model. That is fine for many replayable read models, but a crash between read-model write and checkpoint save can cause duplicate projection work on restart.

For read models that need the read-model update and checkpoint update to commit together, implement `TransactionalCheckpointedProjection`:

```rust
impl TransactionalCheckpointedProjection<AccountEvent, String> for AccountReadModel {
    type Error = ReadModelError;

    fn name(&self) -> &'static str {
        "account_read_model"
    }

    fn load_checkpoint(&self) -> Result<Option<u64>, Self::Error> {
        self.load_checkpoint_from_db()
    }

    fn apply_and_checkpoint_transactionally(
        &mut self,
        event: &EventEnvelope<AccountEvent, String>,
    ) -> Result<(), Self::Error> {
        let tx = self.connection.transaction()?;
        self.apply_event_with_tx(&tx, event)?;
        self.save_checkpoint_with_tx(&tx, event.sequence)?;
        tx.commit()?;
        Ok(())
    }
}
```

Then run it with `TransactionalCheckpointedProjectionRunner`. The library provides the runner pattern; your projection owns the database transaction because read-model schemas are application-specific.

## Realtime Is Notification, Not Truth

Redis pub/sub, SSE, WebSocket, and polling are delivery mechanisms. They are not the source of truth for event delivery.

The durable event store and checkpoint tables are the source of truth. Realtime notifications should wake clients or workers, then those clients or workers should replay durable events or reload read models from the last known sequence. See [Database Query Patterns](./db-query-patterns) for the query and indexing rules behind that replay model.

The counter app follows this rule:

- `realtime=polling` wakes from SSE polling and replays durable events.
- `realtime=redis` uses Redis wake messages, then still replays durable events by sequence.
- `db=redis` means Redis is also the experimental durable event/checkpoint/read-model store.

## Error Sources and Contracts

`EventStoreError` keeps stable display messages while exposing sources for backend, connection, serialization, and deserialization failures when the adapter has a concrete cause.

For application and transport mapping, see [Error Handling and Transport Mapping](./error-handling). That guide covers preserving `RepositoryError` and `EventStoreError` classifications, then adapting them to REST, Leptos server functions, Spin gRPC, and tracing.

Adapter authors should use the contract helpers in `ddd_cqrs_es::testing`:

- `assert_event_store_contract`
- `assert_event_store_global_replay_contract`
- `assert_checkpoint_store_contract`
- `assert_idempotency_store_contract`
- `assert_snapshot_store_contract`

These helpers are intentionally focused so adapters can validate event append, global replay, idempotency, checkpoint, snapshot, and upcaster behavior without assuming one exact sequence-number policy for every backend.
