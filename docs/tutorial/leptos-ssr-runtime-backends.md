---
title: Runtime Configuration and Backends
description: Configure local, Spin, WASI, cloud SQL, MySQL, Redis, and outbound HTTP backend options for the counter app.
---

# Runtime Configuration and Backends

## 7. Getting Started & Execution Guide

Follow these steps to build, configure, and execute the application with various database engines across different WASI-compliant WebAssembly runtimes.

### ⚙️ Prerequisites

Ensure you have the following installed on your developer machine:
*   **Rust Toolchain**: Stable release (Rust 1.93.0+ or similar)
*   **WASM Target**: `rustup target add wasm32-wasip2`
*   **Fermyon Spin CLI** (for Spin runtime): `brew install fermyon/tap/spin`
*   **Wasmtime CLI** (for bare WASM runtime): `brew install wasmtime`
*   **cargo-leptos**: `cargo install --locked cargo-leptos`

### 🔑 Environment Setup (`.env`)

Before running the application, configure your databases. We provide a complete template. Copy the example file to initialize your config:

```bash
cp examples/counter-app/.env.example examples/counter-app/.env
```

Open `examples/counter-app/.env` and inspect the configuration variables. The `.env.example` template is tracked by version control as a reference, enabling seamless collaboration and automated testing across local and cloud environments:

```ini
# Supported make backends: sqlite, postgres, neon, supabase, turso, mysql, redis
DATABASE_BACKEND=sqlite

# Realtime transport: off, polling, redis
REALTIME_BACKEND=off
REDIS_CHANNEL=counter-events

# make derives DATABASE_URL/DATABASE_AUTH_TOKEN from the backend-specific
# values below before launching Spin or Wasmtime. Do not set those internal
# runtime values here for normal counter-app workflows.

# =========================================================================
# 1. PostgreSQL Settings (Local PostgreSQL)
# =========================================================================
POSTGRES_URL=postgresql://postgres:postgres@localhost:5432/postgres

# =========================================================================
# 2. Neon Settings
# =========================================================================
NEON_DB_URL=

# =========================================================================
# 3. Supabase Settings
# =========================================================================
SUPABASE_URL=
SUPABASE_SECRET_KEY=

# =========================================================================
# 4. LibSQL / Turso Settings (HTTP API)
# =========================================================================
TURSO_URL=
TURSO_AUTH_TOKEN=

# =========================================================================
# 5. MySQL Settings
# =========================================================================
MYSQL_URL=mysql://user:password@127.0.0.1:3306/counter_app

# =========================================================================
# 6. Redis Settings (experimental event store and realtime notifications)
# =========================================================================
REDIS_URL=redis://127.0.0.1:6379
```

---

## 🔀 Advanced Enterprise Multi-Backend Architecture

Our Leptos application implements a state-of-the-art **Multi-Backend Persistence Engine** inside `src/store.rs`. 

Because our CQRS and Event Sourcing infrastructure depends strictly on framework traits (`EventStore`, `CheckpointStore`, `Projection`), we designed dynamic, runtime-routed wrappers—`MultiBackendEventStore<A>`, `MultiBackendCheckpointStore`, and `MultiBackendCounterProjection`—which inspect the environment at boot-time and execute the correct database operations without modifying a single line of business or component-rendering code.

```mermaid
flowchart TD
    %% Define styles
    classDef client fill:#1e1e38,stroke:#00d4aa,stroke-width:2px,color:#fff;
    classDef server fill:#2d1e38,stroke:#a855f7,stroke-width:2px,color:#fff;
    classDef routing fill:#1e293b,stroke:#f59e0b,stroke-width:2px,color:#fff;
    classDef storage fill:#0f172a,stroke:#64748b,stroke-width:1px,color:#fff;

    Client["🖥️ Leptos Frontend (WASM)"]:::client
    Server["⚙️ Server Functions (WASI)"]:::server
    Router["🔀 Multi-Backend Dispatcher"]:::routing
    
    %% Storage Backends
    SpinSqlite["🗄️ Spin SQLite (Host-calls)"]:::storage
    PostgresNative["🐘 Postgres Native (TCP)"]:::storage
    NeonPostgres["⚡ Neon Serverless (Outbound HTTP)"]:::storage
    SupabaseDB["⚡ Supabase REST (Outbound HTTP)"]:::storage
    TursoLibSql["🌀 Turso/LibSQL (Hrana HTTP)"]:::storage
    MySqlStore["MySQL (TCP or Spin host API)"]:::storage
    RedisStore["Redis Event Store + Notifications"]:::storage
    JsonFile["📂 JSON Flat-File (WASM Directory Mount)"]:::storage

    %% Relationships
    Client -->|"POST /api/increment_count"| Server
    Server -->|"Repository::execute()"| Router
    
    Router -->|"DATABASE_BACKEND = sqlite"| SpinSqlite
    Router -->|"DATABASE_BACKEND = postgres"| PostgresNative
    Router -->|"DATABASE_BACKEND = neon"| NeonPostgres
    Router -->|"DATABASE_BACKEND = supabase"| SupabaseDB
    Router -->|"DATABASE_BACKEND = turso"| TursoLibSql
    Router -->|"DATABASE_BACKEND = mysql"| MySqlStore
    Router -->|"DATABASE_BACKEND = redis"| RedisStore
    Router -->|"DATABASE_BACKEND = sqlite (under Wasmtime)"| JsonFile
```

### Supported Database Backends Matrix

| Backend Key (`db`) | Connection Model | Network Protocol | Target Runtime Compatibility | Use Cases | Realtime Support |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **`sqlite`** | Local Host-Call or JSON files | Spin SQLite host calls; Wasmtime `/data` mount | **Fermyon Spin** & **Wasmtime** | Low-latency local dev, edge microservices, zero-dependency local testing | Yes (SSE polling or Redis wake) |
| **`postgres`** | Direct Socket Pool | TCP Socket stream | **Fermyon Spin** & **Wasmtime** | Classic high-throughput self-hosted PG | Yes (SSE polling or Redis wake) |
| **`neon`** | Stateless HTTP SQL | JSON over HTTP (WASIp3) | **Wasmtime** & **Fermyon Spin** | Serverless cloud databases with cold-start mitigation | Yes (SSE polling or Redis wake) |
| **`supabase`** | Stateless REST | JSON REST over HTTP (WASIp3) | **Wasmtime** & **Fermyon Spin** | Rapid prototyping, managed Supabase database integration | Yes (SSE polling or Redis wake) |
| **`turso`** | Hrana Protocol | Pipeline HTTP (WASIp3) | **Wasmtime** & **Fermyon Spin** | Globally distributed SQL, SQLite-at-the-edge (Turso) | Yes (SSE polling or Redis wake) |
| **`redis`** | Async Redis commands | RESP TCP under Wasmtime, Spin Redis outbound and optional Redis Trigger under Spin | **Wasmtime** & **Fermyon Spin** | Experimental event persistence, checkpoints, and realtime notifications | Yes (via PubSub / SSE) |
| **`mysql`** | Direct Socket | TCP stream on Wasmtime; Spin SDK MySQL host API on Spin; native driver on non-WASI targets | **Wasmtime**, **Fermyon Spin**, and native targets | High-throughput self-hosted or cloud MySQL | Yes (SSE polling or Redis wake) |

---

## ⚡ Overcoming WASM Sandbox Limits: Stateless Outbound HTTP

When compiling applications to WebAssembly targets, traditional blocking socket connection pools (such as those used by `tokio-postgres` or standard native SQLite engines written in C) are strictly incompatible with the isolated, single-threaded sandboxed environment of a WASM component.

To solve this compilation and runtime block, our Multi-Backend Engine employs a stateless, highly optimized **Outbound HTTP Bridge** powered by the WASIp3 HTTP Component Model standards. 

When a database request is sent to Neon, Supabase, or Turso, the store adapter converts the SQL query and its parameterized arguments into a payload format (e.g., the JSON-based Hrana pipeline protocol for Turso/LibSQL or the HTTP SQL Endpoint format for Neon), dispatches it via a single non-blocking HTTP POST, collects the response, and translates the rows back into DDD event envelopes.

### Sequence Flow: Outbound HTTP Database Query

```mermaid
sequenceDiagram
    autonumber
    participant App as ⚙️ WASM Component
    participant SDK as 🔌 WASIp3 Outbound HTTP
    participant Host as 🌐 Runtime Host (Wasmtime/Spin)
    participant Cloud as ☁️ Database Cloud API (Neon/Turso)

    App->>App: Serialize query + params into JSON body
    App->>SDK: wasip3::http::client::send(wasi_req)
    Note over SDK,Host: Native host-call transitions<br/>out of the WASM sandbox
    Host->>Cloud: HTTP POST (TLS connection over internet)
    Cloud-->>Host: HTTP 200 OK (JSON Rows/Result)
    Host-->>SDK: Parse HTTP response stream
    SDK-->>App: Return raw response bytes
    App->>App: Deserialize bytes into EventEnvelope<A>
```

Here is a simplified look at how the `MultiBackendEventStore` leverages WASIp3 Outbound HTTP helpers to communicate with external SQL APIs:

```rust
// A look under the hood of src/store.rs:
pub struct MultiBackendEventStore<A> {
    _phantom: PhantomData<fn() -> A>,
}

impl<A> EventStore<A> for MultiBackendEventStore<A>
where
    A: Aggregate + 'static,
    A::Event: serde::Serialize + serde::de::DeserializeOwned,
    A::Id: serde::Serialize + serde::de::DeserializeOwned,
{
    type Error = EventStoreError;

    fn load(&self, aggregate_id: &A::Id) -> Result<Vec<EventEnvelope<A::Event, A::Id>>, Self::Error> {
        let backend = get_backend();
        
        match backend.as_str() {
            "sqlite" => {
                #[cfg(runtime_spin)] {
                    let store = SpinSqliteEventStore::<A>::new("default");
                    store.load(aggregate_id)
                }
                #[cfg(not(runtime_spin))] {
                    // Under Wasmtime, fallback to JSON Flat-File Store mounted at /data/
                    let store = JsonFileEventStore::<A>::new("/data");
                    store.load(aggregate_id)
                }
            }
            "postgres" => {
                #[cfg(feature = "postgres")] {
                    let store = PostgresEventStore::<A>::new(get_postgres_url());
                    store.load(aggregate_id)
                }
                #[cfg(not(feature = "postgres"))] {
                    Err(EventStoreError::Backend("Postgres feature not enabled".to_string()))
                }
            }
            "neon" => {
                // Execute stateless queries over Outbound HTTP SQL API
                let url = get_postgres_url();
                let sql = "SELECT ... FROM events WHERE aggregate_id = $1";
                let rows = block_on(execute_neon_query(&url, sql, vec![aggregate_id.to_string()]))?;
                deserialize_postgres_rows(rows)
            }
            "turso" | "libsql" => {
                // Public backend name is "turso"; "libsql" remains an internal
                // compatibility branch used by runtime/store internals.
                // Execute Hrana pipeline over Outbound HTTP
                let url = get_turso_url();
                let token = get_turso_auth_token();
                let sql = "SELECT ... FROM events WHERE aggregate_id = ?";
                let result = block_on(execute_hrana_query(&url, token.as_deref(), sql, vec![aggregate_id.to_string()]))?;
                deserialize_sqlite_rows(result.rows)
            }
            _ => Err(EventStoreError::Backend(format!("Unsupported database backend: {}", backend))),
        }
    }
    
    // Similarly dispatched for `append()` and `load_global_after()`...
}
```

---

