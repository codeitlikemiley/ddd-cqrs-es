---
name: leptos-wasi-cqrs
description: Guidance for building full-stack Event Sourced (CQRS) applications using ddd_cqrs_es and Leptos WASI (via leptos_wasi) on Fermyon Spin or generic Wasmtime runtimes.
---

# Leptos WASI + CQRS/ES Integration Skill

This skill provides step-by-step instructions and reference patterns for an AI agent to build, modify, debug, and expand full-stack Event Sourced (CQRS) applications integrating the `ddd_cqrs_es` framework with the `leptos_wasi` (WASIp2 Component Model) microservice templates on both **Fermyon Spin (SQLite)** and **Generic Wasmtime (Flat-File / Sandboxed FS)** runtimes.

---

## 🗺️ High-Level Architectural Flow

In this architecture, incoming write operations (Commands) are dispatched via **Leptos Server Functions**, validated by the pure business domain (Aggregate), and persisted as immutable history (Events) in the Event Store. Read models are instantly updated by the **PersistedProjectionRunner** using an sequential Catch-Up Projection pattern:

```mermaid
sequenceDiagram
    autonumber
    actor User as 🌐 Browser Client
    participant Client as 🖥️ Leptos Client (WASM)
    participant Server as ⚙️ Leptos Server (WASI)
    participant Domain as 🧠 Aggregate (Domain)
    database EventDB as 🗄️ Event Store
    database ReadDB as 📊 Read Model
    
    User->>Client: Triggers UI Action Form / Server Action
    Client->>Server: HTTP POST /api/command_name (Server Function)
    Server->>EventDB: Fetch historical Event Stream for Aggregate ID
    EventDB-->>Server: [Historical Events...]
    Server->>Domain: Replay events to reconstruct current aggregate state
    Server->>Domain: Validate Command against current state invariants
    Domain-->>Server: Ok([New Committed Events])
    Server->>EventDB: Append new events (Optimistic Concurrency Control)
    Server->>Server: Trigger PersistedProjectionRunner
    Server->>ReadDB: Apply events to read-model tables / files
    Server->>ReadDB: Persist projection sequence checkpoint
    Server-->>Client: HTTP 200 OK
    Client->>Server: Query updated read-model state (Signal reload)
    Server-->>Client: Returns fresh state (UI re-renders)
```

---

## 🧠 1. Pure Domain Design Pattern (`domain.rs`)

The business domain must remain completely decoupled from databases, networking, or frameworks. Define pure state-machine commands, events, and validations using the framework's `Aggregate` trait:

```rust
use serde::{Deserialize, Serialize};
use ddd_cqrs_es::{Aggregate, DomainEvent};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityId(pub String);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EntityCommand {
    DoAction { arg: i32 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EntityEvent {
    ActionDone { arg: i32 },
}

impl DomainEvent for EntityEvent {
    fn event_type(&self) -> &'static str {
        match self {
            EntityEvent::ActionDone { .. } => "action_done",
        }
    }
}

pub struct EntityAggregate {
    pub id: EntityId,
    pub state_val: i32,
    pub revision: u64,
}

impl Aggregate for EntityAggregate {
    type Id = EntityId;
    type Command = EntityCommand;
    type Event = EntityEvent;
    type Error = String;

    fn aggregate_type() -> &'static str { "entity" }
    fn id(&self) -> Option<&Self::Id> { Some(&self.id) }
    fn revision(&self) -> u64 { self.revision }
    fn new() -> Self {
        Self {
            id: EntityId(String::new()),
            state_val: 0,
            revision: 0,
        }
    }

    fn apply(&mut self, event: &Self::Event) {
        match event {
            EntityEvent::ActionDone { arg } => {
                self.state_val += arg;
            }
        }
        self.revision += 1;
    }

    fn handle(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            EntityCommand::DoAction { arg } => {
                if arg <= 0 {
                    return Err("Argument must be positive".to_string());
                }
                Ok(vec![EntityEvent::ActionDone { arg }])
            }
        }
    }
}
```

---

## 💾 2. Multi-Runtime WASI Persistence (`store.rs`)

To ensure standard compilation under target `wasm32-wasip2` and avoid linker integration conflicts across runtimes:
* Compile Spin's native SQLite connections behind `#[cfg(runtime_spin)]`.
* Compile standard WASI flat-file system files inside `/data/` behind `#[cfg(runtime_wasmtime)]`.

### 2.1 Event Store Implementation Pattern

```rust
pub struct SpinSqliteEventStore<A> {
    db_name: String,
    _phantom: std::marker::PhantomData<A>,
}

#[cfg(runtime_spin)]
impl<A: Aggregate> ddd_cqrs_es::EventStore<A> for SpinSqliteEventStore<A> {
    // Open connection via: spin_sdk::sqlite::Connection::open(&self.db_name)
    // Execute SQL to load and append event envelopes
}

#[cfg(runtime_wasmtime)]
impl<A: Aggregate> ddd_cqrs_es::EventStore<A> for SpinSqliteEventStore<A> {
    // Read and write serialization events JSON log array in `/data/events.json`
}
```

Ensure schemas and directories are initialized automatically on server boot:
* Spin: Execute `CREATE TABLE IF NOT EXISTS events ...`
* Wasmtime: Execute `std::fs::create_dir_all("/data")` and write initial file seeds if missing.

---

## ⚡ 3. Server-Side Integration & Command Runner (`app.rs`)

Handle write commands atomically using a unified server-side handler. Use `AsyncRepository` to load, execute, and append events. To avoid unnecessary database queries, apply returned events directly in-memory to project the state when the sequence is contiguous, falling back to sequential catch-up projections only if a gap is detected.

In Leptos, protect server-only SSR imports behind `#[cfg(feature = "ssr")]`:

```rust
#[cfg(feature = "ssr")]
async fn run_cqrs_command(command: crate::domain::CounterCommand) -> Result<(), ServerFnError> {
    use crate::store::{MultiBackendEventStore, MultiBackendCheckpointStore, MultiBackendCounterProjection};
    use crate::domain::{Counter, CounterId};
    use ddd_cqrs_es::AsyncRepository;

    let event_store = MultiBackendEventStore::<Counter>::new();
    let repository = AsyncRepository::new(event_store.clone());
    let aggregate_id = CounterId("global".to_string());

    // Load stream, handle command, and append events
    let committed_events = repository.execute(
        &aggregate_id,
        command,
        ddd_cqrs_es::Metadata::default(),
    ).await.map_err(|e| ServerFnError::new(e.to_string()))?;

    let checkpoint_store = MultiBackendCheckpointStore::new();
    let mut projection = MultiBackendCounterProjection::new();

    // Direct in-memory projection optimization for contiguous stream
    let last_sequence = checkpoint_store.load_checkpoint_async("counter_projection").await
        .unwrap_or(None)
        .unwrap_or(0);

    let mut contiguous = true;
    let mut expected_seq = last_sequence + 1;
    for env in &committed_events {
        if let Some(seq) = env.sequence {
            if seq == expected_seq {
                expected_seq = seq + 1;
            } else {
                contiguous = false;
                break;
            }
        } else {
            contiguous = false;
            break;
        }
    }

    if contiguous && !committed_events.is_empty() {
        for env in &committed_events {
            projection.apply_async(env).await
                .map_err(|e| ServerFnError::new(e.to_string()))?;
        }
        let last_committed_seq = committed_events.last().and_then(|env| env.sequence).unwrap();
        checkpoint_store.save_checkpoint_async("counter_projection", last_committed_seq).await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
    } else {
        // Fall back to standard catch-up runner on sequence gap
        crate::store::run_projections_async(&event_store, &checkpoint_store, &mut projection).await
            .map_err(|e| ServerFnError::new(e))?;
    }

    Ok(())
}
```

Consolidate separate read calls into a single server function returning a unified view state to minimize browser network round-trips:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CounterViewDto {
    pub count: i32,
    pub latest_events: Vec<EventLogDto>,
}

#[server(prefix = "/api")]
pub async fn get_counter_view() -> Result<CounterViewDto, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use crate::store::{get_count_db, get_latest_events_db};
        let count = get_count_db().await.map_err(|e| ServerFnError::new(e))?;
        let latest_events = get_latest_events_db().await.map_err(|e| ServerFnError::new(e))?;
        Ok(CounterViewDto { count, latest_events })
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}
```

---

## 🌐 4. WASI Server Handler & Boot Migrations (`server.rs`)

Database schema creation statements (`CREATE TABLE IF NOT EXISTS`) are highly blocking and prone to transaction conflicts under concurrency. They should be entirely removed from hot paths. Instead, perform migrations **exactly once asynchronously at server boot** (during handling of the first incoming request) using a shared async initialization guard. Do not use an `AtomicBool` `load()` followed by a later `store(true)` around an `.await`; concurrent first requests can all observe `false` and run migrations.

```rust
use leptos_wasi::prelude::Handler;
use wasip3::http::types::{Request, Response, ErrorCode};
use crate::app::{App, shell, GetCounterView, IncrementCount, DecrementCount, ResetCount};

struct LeptosServer;

impl wasip3::exports::http::handler::Guest for LeptosServer {
    async fn handle(request: Request) -> Result<Response, ErrorCode> {
        let _ = init_wasip3_spawner();

        if let Err(e) = crate::store::initialize_schema_async().await {
            eprintln!("Error executing boot schema migrations: {:?}", e);
            return Err(ErrorCode::InternalError(None));
        }

        let conf = get_configuration(None).unwrap();
        let leptos_options = conf.leptos_options;
        let req = wasip3::http_compat::http_from_wasi_request(request)?;

        let wasi_res = Handler::build(req).await
            .map_err(|e| ErrorCode::InternalError(None))?
            .static_files_handler("/pkg", serve_static_files)
            .with_server_fn::<GetCounterView>()
            .with_server_fn::<IncrementCount>()
            .with_server_fn::<DecrementCount>()
            .with_server_fn::<ResetCount>()
            .generate_routes(App)
            .handle_with_context(move || shell(leptos_options.clone()), || {})
            .await
            .map_err(|e| ErrorCode::InternalError(None))?;

        Ok(wasi_res)
    }
}

wasip3::http::service::export!(LeptosServer);
```

---

## 🛠️ 5. Build, Test & Execute Commands

Execute the following commands from the project root to compile, build, and run the microservices:

### 5.1 Fermyon Spin Runtime (SQLite)
Builds the CSS/WASM and runs local native Spin host:
```bash
make db=sqlite spin
```

### 5.2 Generic Wasmtime Runtime
Builds CSS/WASM and serves using generic wasmtime sandbox capabilities:
```bash
make db=neon wasmtime
make db=turso wasmtime
```

### 5.3 Reset Without Serving
Reset the selected backend schema and return without launching the app:
```bash
make db=neon fresh
make db=turso fresh
```
