---
title: 5.4. Redis Event Store and Realtime
description: Experimental async Redis persistence and notification support.
---

Redis support has two separate roles in this project:

1. **Experimental event persistence:** `RedisEventStore<A, C>` implements the async event-store contract.
2. **Realtime notification:** `RedisPubSubPublisher<C>` publishes wake-up messages after commands commit.

Redis pub/sub is never the source of truth. Clients should use notifications to
wake up, then read durable events, checkpoints, or read models.

For Spin, Redis support also has two separate runtime paths:

* **Outbound Redis:** the HTTP component opens `spin_sdk::redis::Connection`
  for persistence, queries, and publishing.
* **Redis Trigger:** a separate subscriber component is invoked when Redis
  publishes to the configured channel.

---

## Feature Flags

Enable the base async Redis API with `redis`, then choose the runtime client:

| Feature | Runtime | Purpose |
| :--- | :--- | :--- |
| `redis` | Any async Rust target | Enables `RedisEventStore`, `RedisCheckpointStore`, `RedisPubSubPublisher`, and the `RedisCommandExecutor` trait. |
| `wasi-redis` | Generic Wasmtime/WASI | Enables `WasiRedisClient`, a small raw RESP client for plain `redis://` TCP URLs. |
| `spin-redis` | Fermyon Spin | Enables `SpinRedisClient`, backed by `spin_sdk::redis::Connection`. |

`RedisEventStore` is async-only. It intentionally does not implement the sync
`EventStore` trait because the current host APIs used by Spin and the counter
example are async.

---

## Redis Event Store Schema

The adapter stores event data with a small key layout under a configurable
prefix. The default prefix is `ddd_cqrs_es`.

| Key | Purpose |
| :--- | :--- |
| `{prefix}:seq` | Global monotonic sequence counter. |
| `{prefix}:global` | Sorted set of all global sequences. |
| `{prefix}:revision:{aggregate_type_hex}:{aggregate_id_hex}` | Current revision for one aggregate stream. |
| `{prefix}:stream:{aggregate_type_hex}:{aggregate_id_hex}` | Sorted set of sequences for one aggregate stream, scored by stream revision. |
| `{prefix}:event:{sequence}` | Redis hash containing one event envelope. |
| `{prefix}:checkpoint:{projection_name_hex}` | Last processed global sequence for one projection. |

Append is performed by one Lua `EVAL` script. The script validates the expected
revision, allocates global sequence numbers, updates the stream revision, stores
event hashes, and updates stream/global indexes atomically.

---

## Basic Usage

```rust,no_run
use ddd_cqrs_es::{AsyncRepository, RedisEventStore, WasiRedisClient};

# async fn setup() -> Result<(), Box<dyn std::error::Error>> {
let client = WasiRedisClient::new("redis://127.0.0.1:6379");
let store = RedisEventStore::<BankAccount, _>::new(client);
let repo = AsyncRepository::new(store);
# Ok(())
# }
```

Use a custom prefix when multiple apps share one Redis database:

```rust,no_run
use ddd_cqrs_es::{RedisEventStore, WasiRedisClient};

# fn setup() -> Result<(), ddd_cqrs_es::EventStoreError> {
let client = WasiRedisClient::new("redis://127.0.0.1:6379");
let store = RedisEventStore::<BankAccount, _>::with_prefix(client, "my_app:v1")?;
# Ok(())
# }
```

---

## Checkpoints

`RedisCheckpointStore<C>` implements `AsyncCheckpointStore`.

```rust,no_run
use ddd_cqrs_es::{RedisCheckpointStore, WasiRedisClient};

# async fn checkpoint() -> Result<(), ddd_cqrs_es::EventStoreError> {
let client = WasiRedisClient::new("redis://127.0.0.1:6379");
let checkpoints = RedisCheckpointStore::new(client);

checkpoints.save_checkpoint("counter_projection", 42).await?;
let last = checkpoints.load_checkpoint("counter_projection").await?;
assert_eq!(last, Some(42));
# Ok(())
# }
```

Projection writes and checkpoint writes are still separate operations. Projection
handlers must be idempotent so a retry does not corrupt a read model.

---

## Pub/Sub Notifications

`RedisPubSubPublisher<C>` is notification-only. Publish after event append and
projection update succeeds.

```rust,no_run
use ddd_cqrs_es::{RedisPubSubPublisher, WasiRedisClient};
use serde::Serialize;

#[derive(Serialize)]
struct CounterMessage {
    last_sequence: u64,
}

# async fn publish() -> Result<(), ddd_cqrs_es::EventStoreError> {
let client = WasiRedisClient::new("redis://127.0.0.1:6379");
let publisher = RedisPubSubPublisher::new(client, "counter-events");

publisher
    .publish_json(&CounterMessage { last_sequence: 42 })
    .await?;
# Ok(())
# }
```

If notification publishing fails after a command has committed, do not roll back
the command. Log or emit telemetry, then allow clients to recover through
durable replay from their last seen sequence.

---

## Counter App Realtime

The counter example uses SSE/EventSource as the browser transport:

```bash
cd examples/counter-app
make db=redis fresh
make wasmtime db=redis realtime=redis
```

`realtime=redis` can also be used as a wake transport with another durable
backend. It is supported with every counter-app backend:

```bash
make wasmtime db=sqlite realtime=redis
make spin db=sqlite realtime=redis
make wasmtime db=postgres realtime=redis
make spin db=postgres realtime=redis
make wasmtime db=neon realtime=redis
make spin db=neon realtime=redis
make wasmtime db=supabase realtime=redis
make spin db=supabase realtime=redis
make wasmtime db=turso realtime=redis
make spin db=turso realtime=redis
make wasmtime db=mysql realtime=redis
make spin db=mysql realtime=redis
make wasmtime db=redis realtime=redis
make spin db=redis realtime=redis
```

Spin uses the Spin Redis client:

```bash
make spin db=redis realtime=redis
```

When `realtime=redis`, the Spin example uses `spin.redis.toml` and starts a
separate Redis trigger component subscribed to `REDIS_CHANNEL`. The trigger
parses each `CounterRealtimeMessage` and records health markers in Redis:

| Key | Meaning |
| :--- | :--- |
| `counter:redis_trigger:last_sequence` | Last realtime sequence observed by the Spin Redis trigger. |
| `counter:redis_trigger:last_count` | Counter value from the last valid realtime message. |
| `counter:redis_trigger:received_count` | Number of valid realtime messages observed. |

The trigger does not update projections, checkpoints, event-store data, or the
browser SSE response. It is a smoke-testable subscriber that proves Spin Redis
Trigger wiring is active.

Environment variables:

| Variable | Default | Meaning |
| :--- | :--- | :--- |
| `DATABASE_BACKEND` | `sqlite` | Set to `redis` for Redis event persistence in the counter app. |
| `REALTIME_BACKEND` | `off` | `off`, `polling`, or `redis`. Non-`off` enables `/api/counter/stream`. |
| `REDIS_URL` | `redis://127.0.0.1:6379` | Redis connection URL. |
| `REDIS_CHANNEL` | `counter-events` | Channel used for Redis notification publishing. |

Runtime setup checklist:

| Runtime | Required setup |
| :--- | :--- |
| Spin without Redis realtime | Use `spin.toml`, pass `DATABASE_BACKEND`, derived `DATABASE_URL`, derived `DATABASE_AUTH_TOKEN`, and `REALTIME_BACKEND=off` or `polling`. |
| Spin with Redis realtime | Use `spin.redis.toml`, pass the same database env plus `REALTIME_BACKEND=redis`, `REDIS_URL`, and `REDIS_CHANNEL`; set Spin variables `redis_url` and `redis_channel` for the Redis trigger. |
| Wasmtime without Redis realtime | Enable Preview 3, HTTP, TCP, inherited network, DNS lookup, mount `target/site/pkg` at `/`, mount `./data` at `/data`, and pass database env values. |
| Wasmtime with Redis realtime | Use the Wasmtime setup above plus `REALTIME_BACKEND=redis`, `REDIS_URL`, and `REDIS_CHANNEL`; there is no Redis trigger sidecar under Wasmtime. |

Spin outbound permissions must include the protocols and hosts used by the
selected durable backend and by Redis realtime. The counter app manifests allow:

```toml
allowed_outbound_hosts = [
  "*://*.turso.io:*",
  "*://*.neon.tech:*",
  "*://*.supabase.co:*",
  "*://localhost:*",
  "*://127.0.0.1:*",
  "postgres://*:*",
  "postgresql://*:*",
  "mysql://*:*",
  "redis://*:*",
  "rediss://*:*",
]
```

The Makefile derives the internal runtime values from backend-specific public
variables:

| `db` | Public variable | Runtime value |
| :--- | :--- | :--- |
| `postgres` | `POSTGRES_URL` | `DATABASE_URL` |
| `neon` | `NEON_DB_URL` | `DATABASE_URL` |
| `supabase` | `SUPABASE_URL`, `SUPABASE_SECRET_KEY` | `DATABASE_URL`, `DATABASE_AUTH_TOKEN` |
| `turso` | `TURSO_URL`, `TURSO_AUTH_TOKEN` | `DATABASE_URL`, `DATABASE_AUTH_TOKEN` |
| `mysql` | `MYSQL_URL` | `DATABASE_URL` |
| `redis` | `REDIS_URL` | `DATABASE_URL` |

The SSE endpoint is:

```text
/api/counter/stream?last_sequence=0
```

It emits frames like:

```text
event: counter
data: {"view":{"count":1,"latest_events":[...],"last_sequence":1,"realtime_enabled":true},"last_sequence":1}
```

Do not set `Connection: keep-alive` manually on this endpoint. WASIp3 rejects
that hop-by-hop header during response conversion. The stream stays open because
the response body is streaming and the content type is `text/event-stream`.

With `REALTIME_BACKEND=redis`, the counter app SSE route uses Redis as the wake
transport. Each browser request registers a short-TTL Redis list queue. After
commands commit and projections update, the publisher sends a notification to
`REDIS_CHANNEL` and fans one wake message out to every live queue. The SSE
handler treats that wake as notification-only and reads durable events after the
client's `last_sequence` before emitting one `counter` event on the open
response.

Idle clients do not reconnect every few hundred milliseconds. On Spin, the
handler waits inside `BRPOP` for up to 25 seconds, emits one SSE comment
keepalive with a 1 second EventSource retry interval, and continues waiting. On
Wasmtime, the handler uses repeated `RPOP` calls with WASI async sleeps so the
component can continue serving ordinary HTTP requests while it waits for Redis
wake messages.

Redis publishing remains a notification hook. On Spin, the optional Redis
trigger sidecar observes the same pub/sub notifications and records health
markers, but browser delivery uses the per-connection Redis list queues because
the trigger cannot write into an already-open HTTP response owned by the HTTP
component. The HTTP route does not perform a blocking Redis `SUBSCRIBE`; it uses
the existing outbound Redis command path so Spin and Wasmtime share the same
browser delivery model.
Redis wake delivery is not an exactly-once guarantee. Duplicate or missed wake
messages must be harmless because clients recover by replaying durable events
or read models from the last observed sequence.

For the counter app's Redis backend, read-model updates and checkpoint updates
are applied together with one Lua command per event. The generic projection
runner contract remains store-agnostic and still requires idempotent projection
handlers.

---

## Current Limitations

Redis support is marked experimental until broader live contract coverage proves
ordering, recovery, and operational behavior under production traffic.

Known boundaries:

* `WasiRedisClient` supports plain `redis://` TCP URLs. It does not implement TLS, Sentinel, Cluster, or RESP3-specific behavior.
* Redis pub/sub is lossy notification, not durable delivery.
* Counter SSE wake queues are best-effort notification. Durable events remain the source of truth, and clients recover through `last_sequence` replay.
* The event store is async-only.
* Generic projection writes and checkpoint writes are not one transaction unless an adapter or application adds a transaction-aware runner.
* The counter app HTTP SSE route uses per-client Redis list wake queues instead of Redis `SUBSCRIBE`, because Spin outbound Redis exposes command execution while Redis Trigger runs as a separate component. Spin waits with `BRPOP`; Wasmtime polls with `RPOP` and WASI async sleeps. It is not a permanent multi-chunk WebSocket-style stream.
