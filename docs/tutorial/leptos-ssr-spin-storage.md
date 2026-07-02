---
title: WASM and Spin Storage
description: Implement custom SQLite event, checkpoint, and projection storage for WASM and Fermyon Spin host calls.
---

# WASM and Spin Storage

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

