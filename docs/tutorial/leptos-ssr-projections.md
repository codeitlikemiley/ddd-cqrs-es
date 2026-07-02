---
title: Projections and Checkpointing
description: Build asynchronous CQRS projections and persisted checkpoint tracking for read models.
---

# Projections and Checkpointing

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

