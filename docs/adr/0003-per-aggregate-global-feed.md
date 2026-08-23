# ADR-0003: Global replay feeds are scoped per aggregate type

- Status: Accepted (documented limitation; untyped feed deferred)
- Date: 2026-08-23
- Scope: `src/event_store.rs`, `src/projection.rs`, all store backends

## Context

`EventStore<A>` is generic over one aggregate type, and every backend's
`load_global_after` / `load_global_after_limited` filters the global sequence
by that aggregate type (`WHERE aggregate_type = $1` in the SQL stores; a
per-store key prefix in Redis). "Global" order is therefore global **within
one aggregate type**, not across the whole event log.

Consequences for read models:

- A projection consumes exactly one aggregate's events. A read model that
  spans aggregates (for example orders joined with payments) needs one runner
  and one checkpoint per aggregate type.
- There is no ordering guarantee **across** those feeds. Two runners each see
  their own stream in order, but the read model observes an interleaving that
  depends on polling cadence.
- The typed API is the reason: envelopes deserialize into `A::Event`, so a
  feed mixing aggregate types cannot exist behind `EventStore<A>`.

## Decision

Keep the per-aggregate-type feed as the only replay surface for now, and
document the limitation prominently instead of shipping a cross-aggregate
feed speculatively.

Rationale:

- Most read models in current deployments project one aggregate type; the
  limitation costs nothing there.
- Cross-aggregate read models are still buildable today: run one
  checkpointed projection per aggregate type into the same read-model store,
  and design the read model to be commutative across feeds (idempotent
  upserts keyed by aggregate id, no cross-stream ordering assumptions).
- A proper fix is additive and deserves its own design pass rather than a
  bolt-on: an untyped feed (for example
  `EventEnvelope<serde_json::Value, String>` without the type filter), a
  projection API over raw envelopes, checkpoint semantics for the combined
  sequence, and upcaster integration all need decisions.

## Revisit criteria

Design the untyped feed (working name: `RawEventFeed`) when any of these
appear:

- A read model that genuinely requires cross-aggregate ordering (not just
  co-location) — e.g. a ledger that must observe `OrderPlaced` before the
  matching `PaymentCaptured` even across streams.
- A subscriber that must consume *every* event in commit order (audit trail,
  outbox relay, external bus bridge).
- More than one deployment hand-rolling SQL against the events table to get
  around the filter.

The SQL schema already supports it: the global `sequence` column is assigned
across aggregate types, so an untyped feed is a query change plus new API,
not a migration.
