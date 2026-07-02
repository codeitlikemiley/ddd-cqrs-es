# Leptos WASM SSR + Spin SQLite CQRS: A Full-Stack Masterclass

In this advanced tutorial, we will design, build, and deploy a complete, production-ready, full-stack reactive application using **Leptos** (WebAssembly Server-Side Rendering) and **Fermyon Spin** (WASI SQLite) powered by our extensible `ddd_cqrs_es` framework.

By the end of this guide, you will understand how to model a domain using Event Sourcing, implement highly optimized read-model projections, overcome the compilation limits of WebAssembly inside sandboxed microservices, and deliver a zero-latency, reactive UI using optimistic updates and server actions.

---

## 🗺️ Architectural Blueprint

Before we dive into the code, let's look at the flow of a modern, full-stack CQRS and Event Sourced system. Here is how commands flow from the interactive Leptos UI on the client, get validated and processed on the server, persist in our event store, update projections sequentially, and hydrate the reactive client-side interface:

```mermaid
sequenceDiagram
    autonumber
    actor User as 🌐 User (Browser)
    participant Client as 🖥️ Leptos Client (WASM)
    participant Server as ⚙️ Leptos Server (WASI)
    participant Domain as 🧠 Aggregate (Counter)
    participant EventDB as 🗄️ Event Store (SQLite)
    participant ReadDB as 📊 Read Model (SQLite)

    %% 1. Command Dispatch
    User->>Client: Clicks "+1" button
    Note over Client: Optimistic Update:<br/>Increment display instantly (e.g. from 5 to 6)
    Client->>Server: HTTP POST /api/increment_count (Server Function)

    %% 2. Rehydration & Handling
    Server->>EventDB: Fetch historical events for Counter ID
    EventDB-->>Server: [Incremented { amount: 1 }]
    Server->>Domain: Replay events to rebuild current state (Value = 5)
    Server->>Domain: Handle Command: Increment { amount: 1 }
    Note over Domain: Check Invariants:<br/>1. Is amount > 0?<br/>2. Will it overflow i32?
    Domain-->>Server: Ok([Incremented { amount: 1 }])

    %% 3. Persistence & Projection
    Server->>EventDB: Append new event with revision tracking
    EventDB-->>Server: Commited sequence #43
    Server->>Server: Trigger projection runner (On-the-fly Checkpoint)
    Server->>ReadDB: Update flat read model: UPDATE counter_read_model SET value = 6
    Server->>ReadDB: Save Checkpoint sequence #43

    %% 4. Response & Hydration
    Server-->>Client: HTTP 200 OK (Sync complete)
    Client->>Server: Fetch current count (or read model)
    Server-->>Client: Returns count = 6
    Note over Client: Finalizes UI state.<br/>Sync indicator glows green!
```

---

## 1. Conceptualization & Domain Modeling

Let's model the **Counter** domain. A counter seems simple, but in an enterprise environment, every state change requires complete auditability, precise validation rules, and high scalability.

### Mapping the Domain Requirements
To model this domain, we map the requirements to core DDD and Event Sourcing patterns:

*   **Aggregate Root (`Counter`)**: The primary consistency boundary. It maintains the current counter value, tracks the stream revision (for optimistic concurrency), and ensures that all state mutations are applied sequentially.
*   **Value Object (`CounterId`)**: A type-safe newtype wrapper around a `String` representing the unique ID of our counter stream.
*   **Commands (`CounterCommand`)**: Intentions to change state. These represent the *write* operations:
    *   `Increment { amount: i32 }`: Requests to add a positive amount.
    *   `Decrement { amount: i32 }`: Requests to subtract a positive amount.
    *   `Reset`: Requests to reset the counter to zero.
*   **Events (`CounterEvent`)**: Historical, immutable facts that have occurred. These represent our *historical log*:
    *   `Incremented { amount: i32 }`
    *   `Decremented { amount: i32 }`
    *   `ResetPerformed { value: i32 }`

### Why This Matters
By separating commands (intentions) from events (facts), we separate validation from execution. 

> [!IMPORTANT]
> **Command Handling is Validative**: Commands can be rejected if they violate invariants.
> **Event Application is Infallible**: Events represent the past. Once an event is committed, it cannot be rejected or fail to apply; it must mutate the aggregate state without further checks.

---

## 2. Implementing the Pure Domain

Let's examine the full, pure domain implementation located inside `examples/counter-app/src/domain.rs`. Notice that this file has absolutely **no infrastructure dependencies** (no databases, no network frameworks). It is pure, highly testable Rust logic that implements our framework's `Aggregate` trait.

```rust
use std::fmt;
use serde::{Deserialize, Serialize};
use ddd_cqrs_es::{Aggregate, DomainEvent};

/// Type-safe newtype wrapper for the Counter Aggregate ID.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CounterId(pub String);

impl fmt::Display for CounterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for CounterId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for CounterId {
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}

/// Commands accepted by the Counter Aggregate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CounterCommand {
    Increment { amount: i32 },
    Decrement { amount: i32 },
    Reset,
}

/// Domain events emitted by the Counter Aggregate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CounterEvent {
    Incremented { amount: i32 },
    Decremented { amount: i32 },
    ResetPerformed { value: i32 },
}

impl DomainEvent for CounterEvent {
    fn event_type(&self) -> &'static str {
        match self {
            CounterEvent::Incremented { .. } => "incremented",
            CounterEvent::Decremented { .. } => "decremented",
            CounterEvent::ResetPerformed { .. } => "reset_performed",
        }
    }
}

/// The Counter Aggregate state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Counter {
    pub id: CounterId,
    pub value: i32,
    pub revision: u64,
}

impl Aggregate for Counter {
    type Id = CounterId;
    type Command = CounterCommand;
    type Event = CounterEvent;
    type Error = String;

    fn aggregate_type() -> &'static str {
        "counter"
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn new() -> Self {
        Self {
            id: CounterId(String::new()),
            value: 0,
            revision: 0,
        }
    }

    /// Infallibly mutates state based on a committed event fact.
    fn apply(&mut self, event: &Self::Event) {
        match event {
            CounterEvent::Incremented { amount } => {
                self.value = self.value.saturating_add(*amount);
            }
            CounterEvent::Decremented { amount } => {
                self.value = self.value.saturating_sub(*amount);
            }
            CounterEvent::ResetPerformed { value } => {
                self.value = *value;
            }
        }
        self.revision += 1;
    }

    /// Validates a command against current state. Returns a vector of events if successful.
    fn handle(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            CounterCommand::Increment { amount } => {
                if amount <= 0 {
                    return Err("amount to increment must be positive".to_string());
                }
                if self.value.checked_add(amount).is_none() {
                    return Err("increment would overflow integer boundary".to_string());
                }
                Ok(vec![CounterEvent::Incremented { amount }])
            }
            CounterCommand::Decrement { amount } => {
                if_dbg!(amount <= 0);
                if amount <= 0 {
                    return Err("amount to decrement must be positive".to_string());
                }
                if self.value.checked_sub(amount).is_none() {
                    return Err("decrement would underflow integer boundary".to_string());
                }
                Ok(vec![CounterEvent::Decremented { amount }])
            }
            CounterCommand::Reset => {
                Ok(vec![CounterEvent::ResetPerformed { value: 0 }])
            }
        }
    }

    /// Rebuilds aggregate state by replaying envelopes sequentially.
    fn replay(events: &[ddd_cqrs_es::EventEnvelope<Self::Event, Self::Id>]) -> ddd_cqrs_es::LoadedAggregate<Self> {
        let mut state = Self::new();
        let mut revision = ddd_cqrs_es::INITIAL_REVISION;

        if let Some(first) = events.first() {
            state.id = first.aggregate_id.clone();
        }

        for envelope in events {
            state.apply(&envelope.payload);
            revision = envelope.revision;
        }

        ddd_cqrs_es::LoadedAggregate { state, revision }
    }
}
```

> [!TIP]
> Notice the usage of `checked_add` and `checked_sub` in `handle`. This ensures the aggregate defends its invariants *before* accepting changes, while `apply` uses `saturating_add` as a secondary safety mechanism when applying historical facts.

---

## 3. Custom SQLite & Checkpoint Storage on WASM/Spin

### The WASM Sandboxing & Native C Compilation Problem
When compiling standard Rust applications to the WebAssembly target `wasm32-wasip2`, you will quickly run into compile-time or runtime walls if you pull in traditional database engines like `rusqlite` or `diesel`. Why?

Standard native driver crates:
1.  **Rely on Native C libraries**: They expect to link dynamically to a local system C-library (`libsqlite3.so` or `libpq.dylib`), which is impossible inside a sandboxed WebAssembly container.
2.  **Require Raw POSIX Syscalls**: Traditional native drivers spawn threads and perform raw, blocking socket connections or open custom files descriptors—operations strictly blocked by the standard WASI sandbox.

### How We Solve This: Extensible Traits & Spin Host-Calls
To circumvent these limits, our framework provides clean, pluggable `EventStore` and `CheckpointStore` traits. 

In Fermyon Spin, the host runtime manages a native, high-performance SQLite database engine. WebAssembly components communicate with this host engine using highly optimized **WASM host-calls** defined via WIT. Spin's host SDK exposes this capability via `spin_sdk::sqlite::Connection`.

By creating a custom adapter inside `src/store.rs`, we can bridge Spin's host-supplied SQLite connection to our framework's traits. Let's see how this is implemented:

```rust
//! Custom Spin-compliant SQLite Store Adapters.
//! This bridges the Spin SDK host database interface with the ddd_cqrs_es traits.

use std::marker::PhantomData;
use serde::{Serialize, de::DeserializeOwned};
use spin_sdk::sqlite::{Connection, Value};

use ddd_cqrs_es::{
    Aggregate, EventStore, EventStream, EventEnvelope, EventId, NewEvent,
    ExpectedRevision, CheckpointStore, ConcurrencyError, EventStoreError, Metadata
};

// =========================================================================
// 1. Spin SQLite Event Store Adapter
// =========================================================================

pub struct SpinSqliteEventStore<A>
where
    A: Aggregate,
{
    connection_name: String,
    table_name: String,
    _marker: PhantomData<fn() -> A>,
}

impl<A> Clone for SpinSqliteEventStore<A>
where
    A: Aggregate,
{
    fn clone(&self) -> Self {
        Self {
            connection_name: self.connection_name.clone(),
            table_name: self.table_name.clone(),
            _marker: PhantomData,
        }
    }
}

impl<A> SpinSqliteEventStore<A>
where
    A: Aggregate,
{
    pub fn new(connection_name: impl Into<String>) -> Self {
        Self {
            connection_name: connection_name.into(),
            table_name: "events".to_string(),
            _marker: PhantomData,
        }
    }

    fn get_connection(&self) -> Connection {
        Connection::open(&self.connection_name)
            .expect("Failed to open Spin Host SQLite database connection")
    }

    /// Prepares database tables if they do not exist yet.
    pub fn initialize_schema(&self) {
        let conn = self.get_connection();
        let query = format!(
            r#"
            CREATE TABLE IF NOT EXISTS {table} (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT NOT NULL UNIQUE,
                aggregate_id TEXT NOT NULL,
                aggregate_type TEXT NOT NULL,
                revision INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                payload TEXT NOT NULL,
                metadata TEXT NOT NULL,
                recorded_at_ms INTEGER NOT NULL,
                UNIQUE (aggregate_type, aggregate_id, revision)
            );
            CREATE INDEX IF NOT EXISTS {table}_global_replay_idx
                ON {table} (aggregate_type, sequence);
            "#,
            table = self.table_name
        );
        conn.execute(&query, &[]).expect("Failed to initialize aggregate events schema");
    }
}

impl<A> EventStore<A> for SpinSqliteEventStore<A>
where
    A: Aggregate + 'static,
    A::Event: Serialize + DeserializeOwned,
    A::Id: Serialize + DeserializeOwned,
{
    type Error = String;

    fn load(&self, aggregate_id: &A::Id) -> Result<EventStream<A>, Self::Error> {
        let conn = self.get_connection();
        let id_str = serde_json::to_string(aggregate_id)
            .map_err(|e| format!("Failed to serialize aggregate ID: {}", e))?;

        let query = format!(
            "SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, \
             payload, metadata, recorded_at_ms FROM {table} \
             WHERE aggregate_type = ? AND aggregate_id = ? ORDER BY revision ASC",
            table = self.table_name
        );

        let row_set = conn.execute(
            &query,
            &[Value::Text(A::aggregate_type().to_string()), Value::Text(id_str)]
        ).map_err(|e| format!("Database read error: {:?}", e))?;

        let mut stream = Vec::new();
        for row in row_set.rows() {
            let event_id: String = row.get("event_id").ok_or("Missing event_id")?;
            let revision: i64 = row.get("revision").ok_or("Missing revision")?;
            let seq: i64 = row.get("sequence").ok_or("Missing sequence")?;
            let payload_str: String = row.get("payload").ok_or("Missing payload")?;
            let metadata_str: String = row.get("metadata").ok_or("Missing metadata")?;
            let recorded_at_ms: i64 = row.get("recorded_at_ms").ok_or("Missing recorded_at_ms")?;

            let payload: A::Event = serde_json::from_str(&payload_str)
                .map_err(|e| format!("Failed to deserialize event: {}", e))?;
            let metadata: Metadata = serde_json::from_str(&metadata_str)
                .map_err(|e| format!("Failed to deserialize metadata: {}", e))?;

            stream.push(EventEnvelope {
                event_id: EventId::from(event_id),
                aggregate_id: aggregate_id.clone(),
                aggregate_type: A::aggregate_type(),
                revision: revision as u64,
                sequence: Some(seq as u64),
                event_type: row.get("event_type").unwrap_or_default(),
                event_version: 1,
                payload,
                metadata,
                recorded_at: std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(recorded_at_ms as u64),
            });
        }

        Ok(stream)
    }

    fn append(
        &self,
        aggregate_id: &A::Id,
        expected_revision: ExpectedRevision,
        events: Vec<NewEvent<A::Event>>,
    ) -> Result<EventStream<A>, Self::Error> {
        let conn = self.get_connection();
        let id_str = serde_json::to_string(aggregate_id)
            .map_err(|e| format!("Failed to serialize aggregate ID: {}", e))?;

        // 1. Concurrency Check: Load current revision
        let count_query = format!(
            "SELECT COALESCE(MAX(revision), 0) as current FROM {table} WHERE aggregate_type = ? AND aggregate_id = ?",
            table = self.table_name
        );
        let res = conn.execute(
            &count_query,
            &[Value::Text(A::aggregate_type().to_string()), Value::Text(id_str.clone())]
        ).map_err(|e| format!("Query error: {:?}", e))?;
        
        let current_revision = if let Some(row) = res.rows().next() {
            let val: i64 = row.get("current").unwrap_or(0);
            val as u64
        } else {
            0
        };

        // Validate expectations (Optimistic Concurrency Control)
        match expected_revision {
            ExpectedRevision::NoStream if current_revision > 0 => {
                return Err(format!("Concurrency Error: Expected NoStream, found revision {}", current_revision));
            }
            ExpectedRevision::Exact(expected) if current_revision != expected => {
                return Err(format!("Concurrency Error: Expected revision {}, found {}", expected, current_revision));
            }
            ExpectedRevision::Any => {}
            _ => {}
        }

        let mut committed = Vec::new();
        let insert_query = format!(
            "INSERT INTO {table} (event_id, aggregate_id, aggregate_type, revision, event_type, payload, metadata, recorded_at_ms) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            table = self.table_name
        );

        // 2. Persist events sequentially
        for (idx, new_event) in events.into_iter().enumerate() {
            let next_rev = current_revision + 1 + idx as u64;
            let event_id = EventId::new();
            let payload_str = serde_json::to_string(&new_event.payload).unwrap();
            let metadata_str = serde_json::to_string(&new_event.metadata).unwrap();
            let timestamp_ms = std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap().as_millis() as i64;

            conn.execute(
                &insert_query,
                &[
                    Value::Text(event_id.to_string()),
                    Value::Text(id_str.clone()),
                    Value::Text(A::aggregate_type().to_string()),
                    Value::Integer(next_rev as i64),
                    Value::Text(new_event.payload.event_type().to_string()),
                    Value::Text(payload_str),
                    Value::Text(metadata_str),
                    Value::Integer(timestamp_ms),
                ]
            ).map_err(|e| format!("Commit append failed: {:?}", e))?;

            // Fetch sequence of appended row to correctly return complete EventEnvelope
            let seq_query = "SELECT last_insert_rowid() as seq";
            let seq_res = conn.execute(seq_query, &[]).unwrap();
            let sequence_val = seq_res.rows().next().unwrap().get::<i64>("seq").unwrap() as u64;

            committed.push(EventEnvelope {
                event_id,
                aggregate_id: aggregate_id.clone(),
                aggregate_type: A::aggregate_type(),
                revision: next_rev,
                sequence: Some(sequence_val),
                event_type: new_event.payload.event_type().to_string(),
                event_version: 1,
                payload: new_event.payload,
                metadata: new_event.metadata,
                recorded_at: std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(timestamp_ms as u64),
            });
        }

        Ok(committed)
    }

    fn load_global_after(&self, sequence: Option<u64>) -> Result<EventStream<A>, Self::Error> {
        let conn = self.get_connection();
        let seq_val = sequence.unwrap_or(0) as i64;

        let query = format!(
            "SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, \
             payload, metadata, recorded_at_ms FROM {table} \
             WHERE aggregate_type = ? AND sequence > ? ORDER BY sequence ASC",
            table = self.table_name
        );

        let row_set = conn.execute(
            &query,
            &[Value::Text(A::aggregate_type().to_string()), Value::Integer(seq_val)]
        ).map_err(|e| format!("Database load global error: {:?}", e))?;

        let mut stream = Vec::new();
        for row in row_set.rows() {
            let event_id: String = row.get("event_id").unwrap();
            let aggregate_id_str: String = row.get("aggregate_id").unwrap();
            let revision: i64 = row.get("revision").unwrap();
            let seq: i64 = row.get("sequence").unwrap();
            let payload_str: String = row.get("payload").unwrap();
            let metadata_str: String = row.get("metadata").unwrap();
            let recorded_at_ms: i64 = row.get("recorded_at_ms").unwrap();

            let aggregate_id: A::Id = serde_json::from_str(&aggregate_id_str).unwrap();
            let payload: A::Event = serde_json::from_str(&payload_str).unwrap();
            let metadata: Metadata = serde_json::from_str(&metadata_str).unwrap();

            stream.push(EventEnvelope {
                event_id: EventId::from(event_id),
                aggregate_id,
                aggregate_type: A::aggregate_type(),
                revision: revision as u64,
                sequence: Some(seq as u64),
                event_type: row.get("event_type").unwrap_or_default(),
                event_version: 1,
                payload,
                metadata,
                recorded_at: std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(recorded_at_ms as u64),
            });
        }

        Ok(stream)
    }
}

// =========================================================================
// 2. Spin SQLite Checkpoint Store Adapter
// =========================================================================

pub struct SpinSqliteCheckpointStore {
    connection_name: String,
    table_name: String,
}

impl SpinSqliteCheckpointStore {
    pub fn new(connection_name: impl Into<String>) -> Self {
        Self {
            connection_name: connection_name.into(),
            table_name: "projection_checkpoints".to_string(),
        }
    }

    fn get_connection(&self) -> Connection {
        Connection::open(&self.connection_name).expect("Failed to open SQLite")
    }

    pub fn initialize_schema(&self) {
        let conn = self.get_connection();
        let query = format!(
            "CREATE TABLE IF NOT EXISTS {table} (projection_name TEXT PRIMARY KEY, sequence INTEGER NOT NULL)",
            table = self.table_name
        );
        conn.execute(&query, &[]).expect("Failed to initialize checkpoint schema");
    }
}

impl CheckpointStore for SpinSqliteCheckpointStore {
    type Error = String;

    fn load_checkpoint(&self, projection_name: &str) -> Result<Option<u64>, Self::Error> {
        let conn = self.get_connection();
        let query = format!(
            "SELECT sequence FROM {table} WHERE projection_name = ?",
            table = self.table_name
        );
        let res = conn.execute(&query, &[Value::Text(projection_name.to_string())])
            .map_err(|e| format!("{:?}", e))?;

        if let Some(row) = res.rows().next() {
            let seq: i64 = row.get("sequence").unwrap_or(0);
            Ok(Some(seq as u64))
        } else {
            Ok(None)
        }
    }

    fn save_checkpoint(&self, projection_name: &str, sequence: u64) -> Result<(), Self::Error> {
        let conn = self.get_connection();
        let query = format!(
            "INSERT INTO {table} (projection_name, sequence) VALUES (?, ?) \
             ON CONFLICT(projection_name) DO UPDATE SET sequence = excluded.sequence",
            table = self.table_name
        );
        conn.execute(&query, &[Value::Text(projection_name.to_string()), Value::Integer(sequence as i64)])
            .map_err(|e| format!("Save checkpoint failed: {:?}", e))?;
        Ok(())
    }
}
```

---

## 4. Asynchronous CQRS Projections & Checkpointing

A primary tenet of the CQRS pattern is the complete separation of your **Write Model** (optimized for committing atomic business facts) and **Read Model** (optimized for blazingly fast querying). 

Our Aggregate is the write model; it doesn't support list queries or range filter aggregates efficiently. To solve this, we stream committed events into a flat, denormalized read model table using a **Projection**.

### The Read Model Database Schema
For the counter UI, we need a flat table containing each counter's latest computed value:

```sql
CREATE TABLE IF NOT EXISTS counter_read_model (
    counter_id TEXT PRIMARY KEY,
    current_value INTEGER NOT NULL
);
```

### Implementing `CounterProjection`
The `CounterProjection` consumes the domain event envelopes and maintains this read model table:

```rust
use ddd_cqrs_es::{Projection, EventEnvelope};
use spin_sdk::sqlite::{Connection, Value};
use crate::domain::{CounterEvent, CounterId};

pub struct CounterProjection {
    connection_name: String,
}

impl CounterProjection {
    pub fn new(connection_name: impl Into<String>) -> Self {
        Self {
            connection_name: connection_name.into(),
        }
    }

    fn get_connection(&self) -> Connection {
        Connection::open(&self.connection_name).unwrap()
    }

    pub fn initialize_schema(&self) {
        let conn = self.get_connection();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS counter_read_model (counter_id TEXT PRIMARY KEY, current_value INTEGER NOT NULL)",
            &[]
        ).expect("Failed to initialize counter read model table");
    }
}

impl Projection<CounterEvent, CounterId> for CounterProjection {
    type Error = String;

    fn name(&self) -> &'static str {
        "counter_projection"
    }

    fn apply(&mut self, event: &EventEnvelope<CounterEvent, CounterId>) -> Result<(), Self::Error> {
        let conn = self.get_connection();
        let id_str = serde_json::to_string(&event.aggregate_id).unwrap();

        match &event.payload {
            CounterEvent::Incremented { amount } => {
                let query = "INSERT INTO counter_read_model (counter_id, current_value) VALUES (?, ?) \
                             ON CONFLICT(counter_id) DO UPDATE SET current_value = current_value + ?";
                conn.execute(query, &[
                    Value::Text(id_str),
                    Value::Integer(*amount as i64),
                    Value::Integer(*amount as i64),
                ]).map_err(|e| format!("{:?}", e))?;
            }
            CounterEvent::Decremented { amount } => {
                let query = "INSERT INTO counter_read_model (counter_id, current_value) VALUES (?, ?) \
                             ON CONFLICT(counter_id) DO UPDATE SET current_value = current_value - ?";
                conn.execute(query, &[
                    Value::Text(id_str),
                    Value::Integer(-(*amount) as i64),
                    Value::Integer(*amount as i64),
                ]).map_err(|e| format!("{:?}", e))?;
            }
            CounterEvent::ResetPerformed { value } => {
                let query = "INSERT INTO counter_read_model (counter_id, current_value) VALUES (?, ?) \
                             ON CONFLICT(counter_id) DO UPDATE SET current_value = ?";
                conn.execute(query, &[
                    Value::Text(id_str),
                    Value::Integer(*value as i64),
                ]).map_err(|e| format!("{:?}", e))?;
            }
        }
        Ok(())
    }
}
```

### Driving Projections with `PersistedProjectionRunner`
To keep this read model updated sequentially, we use `PersistedProjectionRunner`. 

When a command is executed, new events are appended. We load our last processed projection sequence from `SpinSqliteCheckpointStore`, fetch globally newer events from `SpinSqliteEventStore`, apply them sequentially to our projection, and update the checkpoint after each successful event. The projection write and checkpoint write are not one transaction, so projection updates must be idempotent:

```rust
use ddd_cqrs_es::PersistedProjectionRunner;

pub fn sync_read_model(
    store: &SpinSqliteEventStore<Counter>,
    checkpoint_store: &SpinSqliteCheckpointStore,
    projection: &mut CounterProjection,
) -> Result<usize, String> {
    // 1. Wrap the projection and checkpoint tracker
    let mut runner = PersistedProjectionRunner::new(projection, checkpoint_store);
    
    // 2. Fetch checkpoint, pull pending events from the store, apply, and save progress!
    runner.run(store).map_err(|e| format!("Projection runner failed: {:?}", e))
}
```

---

## 5. Leptos SSR, REST, and gRPC Integration

Leptos server functions (`#[server]`) bridge the browser UI with backend
commands. The counter app also exposes explicit JSON REST endpoints and, on
Spin, a gRPC service through the same WASI HTTP trigger. All three command
surfaces call the same application service so persistence, projection catch-up,
and Redis realtime publishing stay consistent.

### 5.1 Server Function Definitions (`src/app.rs`)

Here is how our server functions are constructed inside `/Users/uriah/Code/ddd/examples/counter-app/src/app.rs`. Notice how they isolate SSR execution from client-side WASM hydration compilation:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventLogDto {
    pub sequence: u64,
    pub event_type: String,
    pub revision: u64,
    pub payload: String,
    pub recorded_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CounterViewDto {
    pub count: i32,
    pub latest_events: Vec<EventLogDto>,
    pub last_sequence: u64,
    pub realtime_enabled: bool,
}

#[server(prefix = "/api")]
pub async fn get_counter_view() -> Result<CounterViewDto, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        get_counter_view_db().await
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}

#[server(prefix = "/api")]
pub async fn increment_count(amount: i32) -> Result<CounterViewDto, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        if amount <= 0 {
            return Err(server_fn_error(crate::error::CounterAppError::validation(
                "amount must be positive",
            )));
        }
        run_cqrs_command(crate::domain::CounterCommand::Increment { amount }).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = amount;
        unreachable!()
    }
}

#[server(prefix = "/api")]
pub async fn decrement_count(amount: i32) -> Result<CounterViewDto, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        if amount <= 0 {
            return Err(server_fn_error(crate::error::CounterAppError::validation(
                "amount must be positive",
            )));
        }
        run_cqrs_command(crate::domain::CounterCommand::Decrement { amount }).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = amount;
        unreachable!()
    }
}

#[server(prefix = "/api")]
pub async fn reset_count() -> Result<CounterViewDto, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        run_cqrs_command(crate::domain::CounterCommand::Reset).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}
```

### 5.2 Unified Server-Side Command Execution (`src/application.rs`)

Behind the scenes on the server, the application layer initializes the event
store, executes the command within aggregate consistency boundaries through the
repository, publishes realtime notifications, and advances the projection
runner. The Leptos server functions, REST routes, and gRPC service all call this
same function:

```rust
#[cfg(feature = "ssr")]
pub async fn execute_counter_command(
    command: crate::domain::CounterCommand,
) -> crate::error::CounterAppResult<CounterViewDto> {
    use crate::domain::{Counter, CounterId};
    use crate::error::CounterAppError;
    use crate::store::MultiBackendEventStore;
    use ddd_cqrs_es::AsyncRepository;

    let event_store = MultiBackendEventStore::<Counter>::new();
    let repository = AsyncRepository::new(event_store);
    let aggregate_id = CounterId("global".to_string());

    let (loaded, committed_events) = repository
        .execute_returning_state(
            &aggregate_id,
            command,
            ddd_cqrs_es::Metadata::default(),
        )
        .await
        .map_err(CounterAppError::from_repository_error)?;

    let mut view = get_counter_view_db().await?;
    view.count = loaded.state.value;
    if let Some(last_sequence) = committed_events.last().and_then(|event| event.sequence) {
        view.last_sequence = last_sequence;
    }
    if let Err(error) = crate::store::publish_counter_realtime(&view).await {
        tracing::error!(error = %error, error_code = error.public_code());
    }
    if let Err(error) = crate::store::catch_up_counter_projection().await {
        tracing::error!(error = %error, error_code = error.public_code());
    }

    Ok(view)
}
```

The counter app keeps typed errors until the transport boundary. REST serializes
`{"error":{"code":"...","message":"..."}}`, gRPC maps the same error to a
`tonic::Code`, and server functions convert it to `ServerFnError` only after
logging through `tracing`. See [Error Handling and Transport Mapping](../production/error-handling.md) for the complete production guide.

### 5.3 Curlable REST and Spin gRPC APIs

The UI server functions are framework-owned endpoints. For integration checks,
the app exposes these explicit JSON REST routes:

```bash
curl -sS http://127.0.0.1:3000/api/counter/view
curl -sS -X POST -H 'content-type: application/json' \
  -d '{"amount":1}' \
  http://127.0.0.1:3000/api/counter/increment
curl -sS -X POST 'http://127.0.0.1:3000/api/counter/decrement?amount=1'
curl -sS -X POST http://127.0.0.1:3000/api/counter/reset
```

Spin gRPC uses `proto/counter.proto` and is served through the Spin HTTP
trigger. Run with `transport=both` to keep the browser UI, REST APIs, SSE, and
gRPC active together:

```bash
RUST_LOG=info,counter_app=debug make spin db=sqlite transport=both realtime=redis
grpcurl -plaintext \
  -import-path proto \
  -proto counter.proto \
  -d '{"amount":1}' \
  localhost:3000 \
  counter.v1.CounterService/Increment
```

`transport=grpc` serves only gRPC endpoints. `transport=both` serves HTTP UI,
REST, SSE, and gRPC. Wasmtime currently fails fast for `transport=grpc` and
`transport=both`.

### 5.4 WASI HTTP Routing (`src/server.rs`)

The WASI HTTP router handles transport-specific routes before handing normal UI
and server-function traffic to `leptos_wasi::Handler`. Keep this order:

1. Spin gRPC route detection.
2. `transport=grpc` HTTP guard.
3. Explicit JSON REST counter routes.
4. `/api/counter/stream` SSE realtime route.
5. Leptos static-file, UI, and server-function handler.

```rust
use leptos_wasi::prelude::Handler;
use wasip3::http::types::{Request, Response, ErrorCode};
use crate::app::{shell, App, GetCounterView, IncrementCount, DecrementCount, ResetCount};

struct LeptosServer;

impl wasip3::exports::http::handler::Guest for LeptosServer {
    async fn handle(request: Request) -> Result<Response, ErrorCode> {
        let _ = init_wasip3_spawner();
        let req = wasip3::http_compat::http_from_wasi_request(request)?;
        let request_path = req.uri().path().to_string();

        #[cfg(all(feature = "spin-grpc", runtime_spin))]
        if crate::grpc::is_grpc_request(&req) {
            return crate::grpc::serve(req).await;
        }

        if transport_mode() == "grpc" {
            return plain_text_response(
                http::StatusCode::NOT_FOUND,
                "This component is running with transport=grpc. Use the gRPC service endpoint.",
            );
        }

        if crate::rest::is_rest_request(&req) {
            let response = crate::rest::serve(req)
                .await
                .map_err(|_| ErrorCode::InternalError(None))?;
            return wasip3::http_compat::http_into_wasi_response(response);
        }

        if request_path == "/api/counter/stream" {
            let response = crate::store::counter_stream_response(&req)
                .await
                .map_err(|_| ErrorCode::InternalError(None))?;
            return wasip3::http_compat::http_into_wasi_response(response);
        }

        let conf = get_configuration(None).unwrap();
        let leptos_options = conf.leptos_options;

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
```

---

## 6. Polished Premium UI Walkthrough

On the frontend, Leptos uses reactive signals, server actions, and direct button dispatch to provide a snappy, fluid user interface. The current counter app keeps a local optimistic count, tracks the sequence it expects the server to reach, and ignores older action/SSE snapshots so rapid clicks do not visibly rewind the number:

```rust
#[component]
fn HomePage() -> impl IntoView {
    let increment_action = ServerAction::<IncrementCount>::new();
    let counter_view = Resource::new(|| (), |_| get_counter_view());
    let (current_view, set_current_view) = signal(None::<CounterViewDto>);
    let (optimistic_count, set_optimistic_count) = signal(None::<i32>);
    let (last_seen_sequence, set_last_seen_sequence) = signal(0_u64);
    let (pending_until_sequence, set_pending_until_sequence) = signal(None::<u64>);

    Effect::new(move |_| {
        if let Some(Ok(view_data)) = counter_view.get() {
            if current_view
                .get_untracked()
                .is_some_and(|current| view_data.last_sequence < current.last_sequence)
            {
                return;
            }

            let caught_up = pending_until_sequence
                .get_untracked()
                .is_none_or(|sequence| view_data.last_sequence >= sequence);

            if caught_up {
                set_optimistic_count.set(Some(view_data.count));
                set_pending_until_sequence.set(None);
            }
            set_last_seen_sequence.set(view_data.last_sequence);
            set_current_view.set(Some(view_data));
        }
    });

    let apply_optimistic_increment = move |_| {
        let base_count = optimistic_count
            .get_untracked()
            .or_else(|| current_view.get_untracked().map(|view| view.count))
            .unwrap_or_default();
        set_optimistic_count.set(Some(base_count.saturating_add(1)));

        let base_sequence = pending_until_sequence
            .get_untracked()
            .or_else(|| current_view.get_untracked().map(|view| view.last_sequence))
            .unwrap_or_else(|| last_seen_sequence.get_untracked());
        set_pending_until_sequence.set(Some(base_sequence.saturating_add(1)));

        increment_action.dispatch(IncrementCount { amount: 1 });
    };

    view! {
        <button on:click=apply_optimistic_increment>
            "+1"
        </button>
        <div>{move || optimistic_count.get().unwrap_or_default()}</div>
    }
}
```

### Hydration Mechanics & UI States
1.  **Server-Side Rendering (SSR)**: When the page is loaded, the server triggers `get_counter_view()`, renders the HTML layout with the true count and latest ledger entries, and sends down static markup. The user sees a fully rendered page instantly.
2.  **Hydration**: The compiled Client WebAssembly binary is loaded by the browser, intercepts the static page, attaches event listeners, and initializes signals. The transition is completely invisible and painless.
3.  **Optimistic State Updates**: On button click, the UI applies the local count change immediately and dispatches the server action in the background. The client only lets authoritative responses replace the optimistic count once the returned sequence catches up to the expected sequence, so older SSE/action responses cannot move the visible value backward during bursty clicks.

---

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

## 💎 The Pure DDD & CQRS Advantage

Take a moment to step back and realize what we have accomplished.

By separating **Domain Logic** (commands, aggregate invariants, and events) from **Infrastructure Concerns** (SQLite, Postgres, HTTP API protocols, network sandboxing, and runtime-specific environments), we have made our application completely robust, future-proof, and flexible.

*   Want to run your microservice as a lightweight, zero-dependency serverless edge component? **Set `DATABASE_BACKEND=sqlite`**.
*   Need to scale to enterprise workloads on AWS with thousands of events per second? **Enable `DATABASE_BACKEND=postgres`**.
*   Want to run globally distributed edge containers with serverless SQL backends? **Set `DATABASE_BACKEND=neon` or `DATABASE_BACKEND=turso`**.
*   Want MySQL for a self-hosted or managed relational backend? **Set `DATABASE_BACKEND=mysql`**.
*   Want Redis-backed event persistence and faster cross-client UI wakeups? **Set `DATABASE_BACKEND=redis` and `REALTIME_BACKEND=redis`**.
*   Want MySQL persistence with Redis wake notifications? **Set `DATABASE_BACKEND=mysql` and `REALTIME_BACKEND=redis`**.

Your domain logic does not change by a single letter. That is the outstanding power of building enterprise-grade systems with clean, decoupled **Domain-Driven Design** and **Event Sourcing**!
