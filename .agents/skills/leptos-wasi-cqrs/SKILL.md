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

Handle write commands atomically using a unified server-side handler. In Leptos, protect server-only SSR imports behind `#[cfg(feature = "ssr")]`:

```rust
#[cfg(feature = "ssr")]
fn run_cqrs_command(command: crate::domain::EntityCommand) -> Result<(), ServerFnError> {
    use ddd_cqrs_es::{Repository, PersistedProjectionRunner};
    use crate::store::{SpinSqliteEventStore, SpinSqliteCheckpointStore, EntityProjection};
    use crate::domain::{EntityAggregate, EntityId};

    let event_store = SpinSqliteEventStore::<EntityAggregate>::new("default");
    event_store.initialize_schema().map_err(|e| ServerFnError::new(e))?;

    let repo = Repository::new(event_store.clone());
    let aggregate_id = EntityId("global_id".to_string());

    // Execute through consistency boundary
    repo.execute(&aggregate_id, command, ddd_cqrs_es::Metadata::default())
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    // Instantly catch up projections sequentially
    let checkpoint_store = SpinSqliteCheckpointStore::new("default");
    let projection = EntityProjection::new("default");
    let mut runner = PersistedProjectionRunner::new(projection, checkpoint_store);

    runner.run::<EntityAggregate, _>(&event_store)
        .map_err(|e| ServerFnError::new(format!("{:?}", e)))?;

    Ok(())
}
```

Define clean Server Functions using the `#[server]` macro:

```rust
#[server(prefix = "/api")]
pub async fn trigger_action(val: i32) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        run_cqrs_command(crate::domain::EntityCommand::DoAction { arg: val })
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = val;
        unreachable!()
    }
}
```

---

## 🌐 4. WASI Server Handler Registration (`server.rs`)

Leptos server functions must be registered on the WASIp3 `Handler` builder inside `server.rs` using `.with_server_fn::<T>()`:

```rust
use leptos_wasi::prelude::Handler;
use wasip3::http::types::{Request, Response, ErrorCode};
use crate::app::{App, shell, TriggerAction, GetState};

struct LeptosServer;

impl wasip3::exports::http::handler::Guest for LeptosServer {
    async fn handle(request: Request) -> Result<Response, ErrorCode> {
        let _ = init_wasip3_spawner();
        let conf = get_configuration(None).unwrap();
        let leptos_options = conf.leptos_options;

        let req = wasip3::http_compat::http_from_wasi_request(request)?;

        let wasi_res = Handler::build(req).await
            .map_err(|e| ErrorCode::InternalError(None))?
            .static_files_handler("/pkg", serve_static_files)
            .with_server_fn::<TriggerAction>()
            .with_server_fn::<GetState>()
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
WASI_RUNTIME=spin cargo leptos build --release
WASI_RUNTIME=spin LEPTOS_OUTPUT_NAME=counter_app cargo build --lib --target wasm32-wasip2 --release --no-default-features --features ssr
spin up
```
*Or execute `make spin` if available in the template directory.*

### 5.2 Generic Wasmtime Runtime (Flat File)
Builds CSS/WASM and serves using generic wasmtime sandbox capabilities:
```bash
WASI_RUNTIME=wasmtime cargo leptos build --release
WASI_RUNTIME=wasmtime LEPTOS_OUTPUT_NAME=counter_app cargo build --lib --target wasm32-wasip2 --release --no-default-features --features ssr --target-dir target/wasmtime
wasmtime serve \
    -W component-model-async=y \
    -S p3=y \
    -S cli=y \
    -S http=y \
    --dir=./target/site/pkg::/ \
    --dir=./data::/data \
    --env=LEPTOS_OUTPUT_NAME=counter_app \
    --addr 127.0.0.1:3000 \
    target/wasmtime/wasm32-wasip2/release/counter_app.wasm
```
*Or execute `make wasmtime` if available in the template directory.*
