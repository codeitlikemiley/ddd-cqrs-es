use std::marker::PhantomData;
use ddd_cqrs_es::{Aggregate, EventEnvelope, EventId, ExpectedRevision, NewEvent, EventStore};
use ddd_cqrs_es::error::EventStoreError;
use spin_sdk::sqlite::{Connection, Value};
use futures::executor::block_on;

#[cfg(feature = "postgres")]
pub use ddd_cqrs_es::{PostgresEventStore, PostgresCheckpointStore};

pub struct SpinSqliteEventStore<A> {
    db_name: String,
    _phantom: PhantomData<fn() -> A>,
}

impl<A> Clone for SpinSqliteEventStore<A> {
    fn clone(&self) -> Self {
        Self {
            db_name: self.db_name.clone(),
            _phantom: PhantomData,
        }
    }
}

impl<A> SpinSqliteEventStore<A>
where
    A: Aggregate,
{
    pub fn new(db_name: impl Into<String>) -> Self {
        Self {
            db_name: db_name.into(),
            _phantom: PhantomData,
        }
    }

    pub fn initialize_schema(&self) -> Result<(), String> {
        let connection = block_on(Connection::open(&self.db_name)).map_err(|e| e.to_string())?;
        
        let create_events = r#"
            CREATE TABLE IF NOT EXISTS events (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT NOT NULL UNIQUE,
                aggregate_id TEXT NOT NULL,
                aggregate_type TEXT NOT NULL,
                revision INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                event_version INTEGER NOT NULL,
                payload TEXT NOT NULL,
                metadata TEXT NOT NULL,
                recorded_at_ms INTEGER NOT NULL,
                UNIQUE (aggregate_type, aggregate_id, revision)
            );
        "#;
        block_on(connection.execute(create_events, [])).map_err(|e| e.to_string())?;

        let create_checkpoints = r#"
            CREATE TABLE IF NOT EXISTS checkpoints (
                projection_name TEXT PRIMARY KEY,
                last_sequence INTEGER NOT NULL
            );
        "#;
        block_on(connection.execute(create_checkpoints, [])).map_err(|e| e.to_string())?;

        let create_read_model = r#"
            CREATE TABLE IF NOT EXISTS counter_read_model (
                id TEXT PRIMARY KEY,
                value INTEGER NOT NULL
            );
        "#;
        block_on(connection.execute(create_read_model, [])).map_err(|e| e.to_string())?;

        Ok(())
    }
}

impl<A> EventStore<A> for SpinSqliteEventStore<A>
where
    A: Aggregate + 'static,
    A::Event: serde::Serialize + serde::de::DeserializeOwned,
    A::Id: serde::Serialize + serde::de::DeserializeOwned,
{
    type Error = EventStoreError;

    fn load(&self, aggregate_id: &A::Id) -> Result<Vec<EventEnvelope<A::Event, A::Id>>, Self::Error> {
        let aggregate_id_str = serde_json::to_string(aggregate_id)
            .map_err(|e| EventStoreError::Serialization(e.to_string()))?;
        
        let connection = block_on(Connection::open(&self.db_name))
            .map_err(|e| EventStoreError::Connection(e.to_string()))?;
        
        let query = "SELECT sequence, event_id, aggregate_id, aggregate_type, revision, event_type, event_version, payload, metadata, recorded_at_ms FROM events WHERE aggregate_type = ? AND aggregate_id = ? ORDER BY revision ASC";
        let params = vec![
            Value::Text(A::aggregate_type().to_string()),
            Value::Text(aggregate_id_str),
        ];

        let query_result = block_on(connection.execute(query, params))
            .map_err(|e| EventStoreError::Backend(e.to_string()))?;

        let rows = block_on(query_result.collect())
            .map_err(|e| EventStoreError::Backend(e.to_string()))?;

        let mut envelopes = Vec::new();
        for row in rows {
            let sequence = row.get::<i64>(0)
                .ok_or_else(|| EventStoreError::Deserialization("Missing sequence".to_string()))? as u64;
            let event_id_str = row.get::<&str>(1)
                .ok_or_else(|| EventStoreError::Deserialization("Missing event_id".to_string()))?.to_string();
            let aggregate_id_raw = row.get::<&str>(2)
                .ok_or_else(|| EventStoreError::Deserialization("Missing aggregate_id".to_string()))?.to_string();
            let aggregate_type = row.get::<&str>(3)
                .ok_or_else(|| EventStoreError::Deserialization("Missing aggregate_type".to_string()))?.to_string();
            let revision = row.get::<i64>(4)
                .ok_or_else(|| EventStoreError::Deserialization("Missing revision".to_string()))? as u64;
            let event_type = row.get::<&str>(5)
                .ok_or_else(|| EventStoreError::Deserialization("Missing event_type".to_string()))?.to_string();
            let event_version = row.get::<i64>(6)
                .ok_or_else(|| EventStoreError::Deserialization("Missing event_version".to_string()))? as u32;
            let payload_str = row.get::<&str>(7)
                .ok_or_else(|| EventStoreError::Deserialization("Missing payload".to_string()))?.to_string();
            let metadata_str = row.get::<&str>(8)
                .ok_or_else(|| EventStoreError::Deserialization("Missing metadata".to_string()))?.to_string();
            let recorded_at_ms = row.get::<i64>(9)
                .ok_or_else(|| EventStoreError::Deserialization("Missing recorded_at_ms".to_string()))?;

            let aggregate_id_val: A::Id = serde_json::from_str(&aggregate_id_raw)
                .map_err(|e| EventStoreError::Deserialization(e.to_string()))?;

            let payload: A::Event = serde_json::from_str(&payload_str)
                .map_err(|e| EventStoreError::Deserialization(e.to_string()))?;

            let metadata: ddd_cqrs_es::Metadata = serde_json::from_str(&metadata_str)
                .map_err(|e| EventStoreError::Deserialization(e.to_string()))?;

            let recorded_at = std::time::UNIX_EPOCH + std::time::Duration::from_millis(recorded_at_ms as u64);

            envelopes.push(EventEnvelope::new(
                EventId::from_string(event_id_str),
                aggregate_id_val,
                aggregate_type,
                revision,
                Some(sequence),
                event_type,
                event_version,
                payload,
                metadata,
                recorded_at,
            ));
        }

        Ok(envelopes)
    }

    fn append(
        &self,
        aggregate_id: &A::Id,
        expected_revision: ExpectedRevision,
        events: Vec<NewEvent<A::Event>>,
    ) -> Result<Vec<EventEnvelope<A::Event, A::Id>>, Self::Error> {
        let aggregate_id_str = serde_json::to_string(aggregate_id)
            .map_err(|e| EventStoreError::Serialization(e.to_string()))?;

        let connection = block_on(Connection::open(&self.db_name))
            .map_err(|e| EventStoreError::Connection(e.to_string()))?;

        // 1. Get current max revision inside a connection session
        let current_revision = {
            let query = "SELECT COALESCE(MAX(revision), 0) as max_rev FROM events WHERE aggregate_type = ? AND aggregate_id = ?";
            let params = vec![
                Value::Text(A::aggregate_type().to_string()),
                Value::Text(aggregate_id_str.clone()),
            ];
            let query_result = block_on(connection.execute(query, params))
                .map_err(|e| EventStoreError::Backend(e.to_string()))?;
            let rows = block_on(query_result.collect())
                .map_err(|e| EventStoreError::Backend(e.to_string()))?;
            let mut actual = 0u64;
            if let Some(row) = rows.first() {
                if let Some(rev) = row.get::<i64>(0) {
                    actual = rev as u64;
                }
            }
            actual
        };

        // 2. Optimistic Concurrency Control (OCC) check
        match expected_revision {
            ExpectedRevision::Any => {}
            ExpectedRevision::NoStream if current_revision == 0 => {}
            ExpectedRevision::NoStream => {
                return Err(EventStoreError::Concurrency(
                    ddd_cqrs_es::ConcurrencyError::StreamAlreadyExists,
                ));
            }
            ExpectedRevision::Exact(expected) if expected == current_revision => {}
            ExpectedRevision::Exact(_) => {
                return Err(EventStoreError::Concurrency(
                    ddd_cqrs_es::ConcurrencyError::WrongExpectedRevision {
                        expected: expected_revision,
                        actual: current_revision,
                    },
                ));
            }
        }

        if events.is_empty() {
            return Ok(Vec::new());
        }

        // 3. Append events
        let mut envelopes = Vec::new();
        let insert_query = r#"
            INSERT INTO events (
                event_id, aggregate_id, aggregate_type, revision, event_type, event_version, payload, metadata, recorded_at_ms
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        let now = std::time::SystemTime::now();
        let recorded_at_ms = now.duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        for (i, event) in events.into_iter().enumerate() {
            let revision = current_revision + i as u64 + 1;
            let event_id = EventId::new();

            let payload_str = serde_json::to_string(&event.payload)
                .map_err(|e| EventStoreError::Serialization(e.to_string()))?;
            let metadata_str = serde_json::to_string(&event.metadata)
                .map_err(|e| EventStoreError::Serialization(e.to_string()))?;

            let params = vec![
                Value::Text(event_id.to_string()),
                Value::Text(aggregate_id_str.clone()),
                Value::Text(A::aggregate_type().to_string()),
                Value::Integer(revision as i64),
                Value::Text(event.event_type.clone()),
                Value::Integer(event.event_version as i64),
                Value::Text(payload_str),
                Value::Text(metadata_str),
                Value::Integer(recorded_at_ms),
            ];

            block_on(connection.execute(insert_query, params))
                .map_err(|e| {
                    let err_str = e.to_string();
                    if err_str.contains("constraint") || err_str.contains("UNIQUE") {
                        EventStoreError::Concurrency(ddd_cqrs_es::ConcurrencyError::WrongExpectedRevision {
                            expected: expected_revision,
                            actual: current_revision,
                        })
                    } else {
                        EventStoreError::Backend(err_str)
                    }
                })?;

            // Retrieve the sequence ID of the inserted event
            let sequence = block_on(connection.last_insert_rowid()) as u64;

            envelopes.push(EventEnvelope::new(
                event_id,
                aggregate_id.clone(),
                A::aggregate_type(),
                revision,
                Some(sequence),
                event.event_type,
                event.event_version,
                event.payload,
                event.metadata,
                now,
            ));
        }

        Ok(envelopes)
    }

    fn load_global_after(&self, sequence: Option<u64>) -> Result<Vec<EventEnvelope<A::Event, A::Id>>, Self::Error> {
        let seq = sequence.unwrap_or(0) as i64;
        
        let connection = block_on(Connection::open(&self.db_name))
            .map_err(|e| EventStoreError::Connection(e.to_string()))?;
        
        let query = "SELECT sequence, event_id, aggregate_id, aggregate_type, revision, event_type, event_version, payload, metadata, recorded_at_ms FROM events WHERE aggregate_type = ? AND sequence > ? ORDER BY sequence ASC";
        let params = vec![
            Value::Text(A::aggregate_type().to_string()),
            Value::Integer(seq),
        ];

        let query_result = block_on(connection.execute(query, params))
            .map_err(|e| EventStoreError::Backend(e.to_string()))?;

        let rows = block_on(query_result.collect())
            .map_err(|e| EventStoreError::Backend(e.to_string()))?;

        let mut envelopes = Vec::new();
        for row in rows {
            let sequence = row.get::<i64>(0)
                .ok_or_else(|| EventStoreError::Deserialization("Missing sequence".to_string()))? as u64;
            let event_id_str = row.get::<&str>(1)
                .ok_or_else(|| EventStoreError::Deserialization("Missing event_id".to_string()))?.to_string();
            let aggregate_id_raw = row.get::<&str>(2)
                .ok_or_else(|| EventStoreError::Deserialization("Missing aggregate_id".to_string()))?.to_string();
            let aggregate_type = row.get::<&str>(3)
                .ok_or_else(|| EventStoreError::Deserialization("Missing aggregate_type".to_string()))?.to_string();
            let revision = row.get::<i64>(4)
                .ok_or_else(|| EventStoreError::Deserialization("Missing revision".to_string()))? as u64;
            let event_type = row.get::<&str>(5)
                .ok_or_else(|| EventStoreError::Deserialization("Missing event_type".to_string()))?.to_string();
            let event_version = row.get::<i64>(6)
                .ok_or_else(|| EventStoreError::Deserialization("Missing event_version".to_string()))? as u32;
            let payload_str = row.get::<&str>(7)
                .ok_or_else(|| EventStoreError::Deserialization("Missing payload".to_string()))?.to_string();
            let metadata_str = row.get::<&str>(8)
                .ok_or_else(|| EventStoreError::Deserialization("Missing metadata".to_string()))?.to_string();
            let recorded_at_ms = row.get::<i64>(9)
                .ok_or_else(|| EventStoreError::Deserialization("Missing recorded_at_ms".to_string()))?;

            let aggregate_id_val: A::Id = serde_json::from_str(&aggregate_id_raw)
                .map_err(|e| EventStoreError::Deserialization(e.to_string()))?;

            let payload: A::Event = serde_json::from_str(&payload_str)
                .map_err(|e| EventStoreError::Deserialization(e.to_string()))?;

            let metadata: ddd_cqrs_es::Metadata = serde_json::from_str(&metadata_str)
                .map_err(|e| EventStoreError::Deserialization(e.to_string()))?;

            let recorded_at = std::time::UNIX_EPOCH + std::time::Duration::from_millis(recorded_at_ms as u64);

            envelopes.push(EventEnvelope::new(
                EventId::from_string(event_id_str),
                aggregate_id_val,
                aggregate_type,
                revision,
                Some(sequence),
                event_type,
                event_version,
                payload,
                metadata,
                recorded_at,
            ));
        }

        Ok(envelopes)
    }
}

pub struct SpinSqliteCheckpointStore {
    db_name: String,
}

impl Clone for SpinSqliteCheckpointStore {
    fn clone(&self) -> Self {
        Self {
            db_name: self.db_name.clone(),
        }
    }
}

impl SpinSqliteCheckpointStore {
    pub fn new(db_name: impl Into<String>) -> Self {
        Self {
            db_name: db_name.into(),
        }
    }
}

impl ddd_cqrs_es::CheckpointStore for SpinSqliteCheckpointStore {
    type Error = EventStoreError;

    fn load_checkpoint(&self, projection_name: &str) -> Result<Option<u64>, Self::Error> {
        let connection = block_on(Connection::open(&self.db_name))
            .map_err(|e| EventStoreError::Connection(e.to_string()))?;
        
        let sql = "SELECT last_sequence FROM checkpoints WHERE projection_name = ?;";
        let params = vec![Value::Text(projection_name.to_string())];
        let query_result = block_on(connection.execute(sql, params))
            .map_err(|e| EventStoreError::Backend(e.to_string()))?;

        let rows = block_on(query_result.collect())
            .map_err(|e| EventStoreError::Backend(e.to_string()))?;

        if let Some(row) = rows.first() {
            let last_sequence = row.get::<i64>(0)
                .ok_or_else(|| EventStoreError::Deserialization("Missing last_sequence".to_string()))? as u64;
            Ok(Some(last_sequence))
        } else {
            Ok(None)
        }
    }

    fn save_checkpoint(&self, projection_name: &str, sequence: u64) -> Result<(), Self::Error> {
        let connection = block_on(Connection::open(&self.db_name))
            .map_err(|e| EventStoreError::Connection(e.to_string()))?;
        
        let sql = "INSERT INTO checkpoints (projection_name, last_sequence) VALUES (?, ?) \
                   ON CONFLICT(projection_name) DO UPDATE SET last_sequence = excluded.last_sequence;";
        let params = vec![
            Value::Text(projection_name.to_string()),
            Value::Integer(sequence as i64),
        ];
        let _ = block_on(connection.execute(sql, params))
            .map_err(|e| EventStoreError::Backend(e.to_string()))?;
        Ok(())
    }
}

pub struct CounterProjection {
    db_name: String,
}

impl CounterProjection {
    pub fn new(db_name: impl Into<String>) -> Self {
        Self {
            db_name: db_name.into(),
        }
    }
}

impl ddd_cqrs_es::Projection<crate::domain::CounterEvent, crate::domain::CounterId> for CounterProjection {
    type Error = EventStoreError;

    fn name(&self) -> &'static str {
        "counter_projection"
    }

    fn apply(&mut self, envelope: &EventEnvelope<crate::domain::CounterEvent, crate::domain::CounterId>) -> Result<(), Self::Error> {
        let aggregate_id_str = serde_json::to_string(&envelope.aggregate_id)
            .map_err(|e| EventStoreError::Serialization(e.to_string()))?;
        
        let connection = block_on(Connection::open(&self.db_name))
            .map_err(|e| EventStoreError::Connection(e.to_string()))?;
        
        // 1. Get current read model value
        let query = "SELECT value FROM counter_read_model WHERE id = ?";
        let params = vec![Value::Text(aggregate_id_str.clone())];
        let query_result = block_on(connection.execute(query, params))
            .map_err(|e| EventStoreError::Backend(e.to_string()))?;
        
        let rows = block_on(query_result.collect())
            .map_err(|e| EventStoreError::Backend(e.to_string()))?;

        let mut current_value = 0i32;
        if let Some(row) = rows.first() {
            if let Some(val) = row.get::<i64>(0) {
                current_value = val as i32;
            }
        }

        // 2. Apply event to calculate new value
        let new_value = match envelope.payload {
            crate::domain::CounterEvent::Incremented { amount } => current_value.saturating_add(amount),
            crate::domain::CounterEvent::Decremented { amount } => current_value.saturating_sub(amount),
            crate::domain::CounterEvent::ResetPerformed { value } => value,
        };

        // 3. Save new value to read model using Upsert
        let upsert_sql = "INSERT INTO counter_read_model (id, value) VALUES (?, ?) \
                          ON CONFLICT(id) DO UPDATE SET value = excluded.value;";
        let upsert_params = vec![
            Value::Text(aggregate_id_str),
            Value::Integer(new_value as i64),
        ];
        let _ = block_on(connection.execute(upsert_sql, upsert_params))
            .map_err(|e| EventStoreError::Backend(e.to_string()))?;

        Ok(())
    }
}
