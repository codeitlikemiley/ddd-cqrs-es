---
title: Execution and Testing Playbook
description: Run the counter app under Wasmtime and Fermyon Spin across supported database and realtime backends.
---

# Execution and Testing Playbook

## 🛠️ Execution & Testing Playbook

We have provided a unified `Makefile` inside `examples/counter-app` to compile, package, reset, and launch our Leptos WASM application using simple target flags. This shields you from compiling custom target configurations manually.

Run these commands from `examples/counter-app`:

```bash
make help
make help topic=db
make help topic=realtime
make help-matrix
```

The Makefile is the canonical setup path. It derives runtime env vars from the
public backend variables and selects the correct Cargo features, Spin manifest,
and Wasmtime host permissions. If you wire the runtime manually, preserve these
boundaries:

| Backend | Public variable | Runtime env passed to component |
| :--- | :--- | :--- |
| `postgres` | `POSTGRES_URL` | `DATABASE_URL` |
| `neon` | `NEON_DB_URL` | `DATABASE_URL` |
| `supabase` | `SUPABASE_URL`, `SUPABASE_SECRET_KEY` | `DATABASE_URL`, `DATABASE_AUTH_TOKEN` |
| `turso` | `TURSO_URL`, `TURSO_AUTH_TOKEN` | `DATABASE_URL`, `DATABASE_AUTH_TOKEN` |
| `mysql` | `MYSQL_URL` | `DATABASE_URL` |
| `redis` | `REDIS_URL` | `REDIS_URL` |

`DATABASE_URL` and `DATABASE_AUTH_TOKEN` are internal runtime env values. Set
the public backend-specific variables in `.env`; pass the internal values
yourself only when bypassing the Makefile.

Spin manifests must allow outbound hosts for every backend family you plan to
demonstrate:

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

Use `spin.redis.toml` when `realtime=redis`; it adds the Redis trigger sidecar
and exposes `redis_url` / `redis_channel` Spin variables. Wasmtime does not use
that sidecar; it needs Preview 3, HTTP, TCP, inherited networking, DNS lookup,
`./target/site/pkg` mounted at `/`, and `./data` mounted at `/data`.

### 1. Build and Run under Wasmtime (Bare Component Runtime)

Running under Wasmtime is incredibly useful for standard system deployment, local orchestration, and target compatibility checks.

```bash
# Compile and run with the default local JSON Flat-File engine
# (Creates and writes to examples/counter-app/data/ folder automatically!)
make wasmtime

# Compile and run connected to PostgreSQL over TCP
make wasmtime db=postgres

# Compile and run connected to PostgreSQL with Redis wake notifications
make wasmtime db=postgres realtime=redis

# Compile and run connected to Neon serverless Postgres via WASIp3 Outbound HTTP
make wasmtime db=neon

# Compile and run connected to Neon with Redis wake notifications
make wasmtime db=neon realtime=redis

# Compile and run connected to Supabase REST database via WASIp3 Outbound HTTP
make wasmtime db=supabase

# Compile and run connected to Supabase with Redis wake notifications
make wasmtime db=supabase realtime=redis

# Compile and run connected to Turso/LibSQL DB over Hrana HTTP
make wasmtime db=turso

# Compile and run connected to Turso with Redis wake notifications
make wasmtime db=turso realtime=redis

# Compile and run connected to MySQL over raw TCP
make wasmtime db=mysql

# Compile and run connected to MySQL with Redis wake notifications
make wasmtime db=mysql realtime=redis

# Compile and run with the experimental Redis event store and SSE notifications
make wasmtime db=redis realtime=redis
```

### 2. Build and Run under Fermyon Spin (Microservices Runtime)

Running under Fermyon Spin leverages Spin-specific host integrations for SQLite, Postgres, MySQL, and Redis.

```bash
# Compile and run with native Spin SQLite database host-calls
make spin

# Compile and run with native Spin PostgreSQL database connector
make spin db=postgres

# Compile and run with native Spin PostgreSQL and Redis wake notifications
make spin db=postgres realtime=redis

# Compile and run through Spin connected to Neon with Redis wake notifications
make spin db=neon realtime=redis

# Compile and run through Spin connected to Supabase REST database
make spin db=supabase

# Compile and run through Spin connected to Supabase with Redis wake notifications
make spin db=supabase realtime=redis

# Compile and run through Spin connected to Turso with Redis wake notifications
make spin db=turso realtime=redis

# Compile and run with Spin SDK MySQL and SSE polling
make spin db=mysql realtime=polling

# Compile and run with Spin SDK MySQL and Redis wake notifications
make spin db=mysql realtime=redis

# Compile and run with Spin Redis persistence and SSE notifications
make spin db=redis realtime=redis

# Compile and run Spin with browser UI, REST, SSE, gRPC, and Redis wake notifications
make spin db=sqlite transport=both realtime=redis
```

`realtime=redis` is a Redis wake transport, not a request to use Redis as the
event store. It is supported with every supported `db` backend, including
`db=mysql`. Use `db=redis` only when Redis should also be the durable event,
checkpoint, and read-model store.

Spin gRPC support is controlled by `transport=<mode>`:

```bash
# HTTP UI, REST APIs, and SSE only
make spin db=sqlite transport=http

# gRPC only, served through the Spin HTTP trigger
make spin db=sqlite transport=grpc

# HTTP UI, REST APIs, SSE, and gRPC together
make spin db=sqlite transport=both realtime=redis
```

To prove Redis realtime from a terminal command to the browser, start Redis and
the app, then open `http://localhost:3000/`:

```bash
redis-cli ping
RUST_LOG=info,counter_app=debug make spin db=sqlite transport=both realtime=redis
```

Read the current view:

```bash
curl -sS http://127.0.0.1:3000/api/counter/view
```

Trigger a REST command and watch the browser count update without refresh:

```bash
curl -sS -X POST -H 'content-type: application/json' \
  -d '{"amount":1}' \
  http://127.0.0.1:3000/api/counter/increment
```

Trigger the same proof through gRPC:

```bash
grpcurl -plaintext \
  -import-path proto \
  -proto counter.proto \
  -d '{"amount":1}' \
  localhost:3000 \
  counter.v1.CounterService/Increment
```

To see the raw SSE frame, keep this running in a second terminal before running
either command:

```bash
curl -N 'http://127.0.0.1:3000/api/counter/stream?last_sequence=0'
```

Expected SSE frames include:

```text
event: counter
data: {"view":...,"last_sequence":...}
```

### 3. Reset a Backend without Serving

The `fresh` target resets the selected backend schema, tables, or files and
then exits. It does not build or start the application.

```bash
make db=sqlite fresh
make db=postgres fresh
make db=neon fresh
make db=supabase fresh
make db=turso fresh
make db=mysql fresh
make db=redis fresh
```

Once launched, open your web browser to `http://127.0.0.1:3000` to interact with your secure, full-stack, optimistic-updating, Event-Sourced Leptos application!

---

