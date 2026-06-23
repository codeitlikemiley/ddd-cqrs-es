# Architecture

The framework is split into domain, application, and infrastructure-facing
modules while remaining a single crate for now.

## Domain

`Aggregate` is the consistency boundary. It validates commands, returns events,
and rebuilds state by applying past events. Aggregate code should not know about
databases, queues, HTTP, JSON, or runtimes.

## Application

`Repository` coordinates the write path:

1. Load committed envelopes from `EventStore`.
2. Replay events into an aggregate.
3. Call `Aggregate::handle`.
4. Append returned events with `ExpectedRevision::Exact`.
5. Return committed envelopes or updated aggregate state.

Domain errors are returned as `RepositoryError::Domain`. Store failures are
returned separately, and standard concurrency failures can be represented as
`RepositoryError::Concurrency`.

## Infrastructure

`EventStore<A>` is the persistence boundary. The included `InMemoryEventStore`
is useful for tests and local development. Durable stores should preserve the
same behavior:

- Append-only events.
- Per-stream revision checks.
- Deterministic stream order.
- Optional global sequence.
- Metadata preservation.

## Read Side

`Projection<E, Id>` builds read models from committed envelopes. The in-memory
runner stores an optional global sequence checkpoint and only advances the
checkpoint after a successful projection apply.

## Policies

`ProcessManager<E, C>` listens to events and returns commands. It is deliberately
separate from aggregate state mutation.

## Snapshots

`SnapshotStore<A>` is optional. Snapshots speed up replay but do not replace the
event log.
