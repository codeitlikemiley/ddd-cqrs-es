# Persistence

`EventStore<A>` is the adapter boundary for persistence. Implementations must
store committed event envelopes and preserve optimistic concurrency semantics.

Required behavior:

- Load events by aggregate ID.
- Append events to a stream.
- Enforce `ExpectedRevision`.
- Preserve metadata.
- Assign stream revisions starting at `1`.
- Preserve event order in each stream.
- Return cloned envelopes without exposing mutable store internals.
- Support global reads after a sequence when the backend has global ordering.

The in-memory store uses `Arc<RwLock<...>>`, stores events per aggregate stream,
assigns global sequence numbers, and exposes `clear` for tests.

Durable adapters should map concurrency failures to `ConcurrencyError` and
preserve adapter-specific context in `EventStoreError` or their own associated
error type.

Suggested SQL shape for future PostgreSQL and SQLite adapters:

```sql
CREATE TABLE events (
    sequence BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    aggregate_id TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    revision BIGINT NOT NULL,
    event_type TEXT NOT NULL,
    event_version INT NOT NULL,
    payload JSONB NOT NULL,
    metadata JSONB NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (aggregate_type, aggregate_id, revision)
);
```
