//! SQLite event store adapter.

use crate::aggregate::Aggregate;
use crate::error::EventStoreError;
use crate::event::{EventEnvelope, EventId, ExpectedRevision, NewEvent};
use crate::event_store::{EventStore, EventStream};
use crate::sql_common::{
    check_expected_revision, deserialize_id, deserialize_metadata, deserialize_payload,
    millis_to_system_time, serialize_id, serialize_metadata, serialize_payload,
    system_time_to_millis, validate_table_name,
};
use rusqlite::{params, Connection, ErrorCode};
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// SQLite-backed event store.
///
/// The adapter stores aggregate IDs, payloads, and metadata as JSON text. It
/// uses SQLite transactions and a unique `(aggregate_type, aggregate_id,
/// revision)` constraint for optimistic concurrency.
pub struct SqliteEventStore<A>
where
    A: Aggregate,
{
    connection: Arc<Mutex<Connection>>,
    table_name: String,
    _marker: PhantomData<fn() -> A>,
}

impl<A> Clone for SqliteEventStore<A>
where
    A: Aggregate,
{
    fn clone(&self) -> Self {
        Self {
            connection: Arc::clone(&self.connection),
            table_name: self.table_name.clone(),
            _marker: PhantomData,
        }
    }
}

impl<A> std::fmt::Debug for SqliteEventStore<A>
where
    A: Aggregate,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteEventStore")
            .field("table_name", &self.table_name)
            .finish_non_exhaustive()
    }
}

impl<A> SqliteEventStore<A>
where
    A: Aggregate,
{
    /// Creates a SQLite event store using the default `events` table.
    pub fn new(connection: Connection) -> Result<Self, EventStoreError> {
        Self::with_table_name(connection, "events")
    }

    /// Creates an in-memory SQLite event store and initializes its schema.
    pub fn in_memory() -> Result<Self, EventStoreError> {
        let store = Self::new(Connection::open_in_memory().map_err(map_sqlite_error)?)?;
        store.initialize_schema()?;
        Ok(store)
    }

    /// Creates a SQLite event store with a custom table name.
    pub fn with_table_name(
        connection: Connection,
        table_name: impl Into<String>,
    ) -> Result<Self, EventStoreError> {
        let table_name = table_name.into();
        validate_table_name(&table_name)?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            table_name,
            _marker: PhantomData,
        })
    }

    /// Initializes the SQLite event table and indexes.
    pub fn initialize_schema(&self) -> Result<(), EventStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| EventStoreError::Poisoned)?;
        connection
            .execute_batch(&format!(
                r#"
                CREATE TABLE IF NOT EXISTS {table} (
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

                CREATE INDEX IF NOT EXISTS {table}_stream_idx
                    ON {table} (aggregate_type, aggregate_id, revision);
                "#,
                table = self.table_name
            ))
            .map_err(map_sqlite_error)?;

        Ok(())
    }

    fn current_revision_locked(
        table_name: &str,
        connection: &Connection,
        aggregate_id: &str,
    ) -> Result<u64, EventStoreError> {
        let query = format!(
            "SELECT COALESCE(MAX(revision), 0) FROM {table} \
             WHERE aggregate_type = ?1 AND aggregate_id = ?2",
            table = table_name
        );
        let revision: i64 = connection
            .query_row(&query, params![A::aggregate_type(), aggregate_id], |row| {
                row.get(0)
            })
            .map_err(map_sqlite_error)?;

        u64::try_from(revision).map_err(|_| {
            EventStoreError::Deserialization("stored revision cannot be negative".to_owned())
        })
    }
}

impl<A> EventStore<A> for SqliteEventStore<A>
where
    A: Aggregate + 'static,
    A::Event: serde::Serialize + serde::de::DeserializeOwned,
    A::Id: serde::Serialize + serde::de::DeserializeOwned,
{
    type Error = EventStoreError;

    fn load(&self, aggregate_id: &A::Id) -> Result<EventStream<A>, Self::Error> {
        let aggregate_id = serialize_id(aggregate_id)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| EventStoreError::Poisoned)?;
        let query = format!(
            "SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, \
             event_version, payload, metadata, recorded_at_ms FROM {table} \
             WHERE aggregate_type = ?1 AND aggregate_id = ?2 ORDER BY revision ASC",
            table = self.table_name
        );
        let mut statement = connection.prepare(&query).map_err(map_sqlite_error)?;
        let rows = statement
            .query_map(
                params![A::aggregate_type(), aggregate_id],
                row_to_envelope::<A>,
            )
            .map_err(map_sqlite_error)?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(map_sqlite_error)
    }

    fn append(
        &self,
        aggregate_id: &A::Id,
        expected_revision: ExpectedRevision,
        events: Vec<NewEvent<A::Event>>,
    ) -> Result<EventStream<A>, Self::Error> {
        let aggregate_id_key = serialize_id(aggregate_id)?;
        let prepared = events
            .into_iter()
            .map(PreparedSqliteEvent::new)
            .collect::<Result<Vec<_>, _>>()?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| EventStoreError::Poisoned)?;
        let transaction = connection.transaction().map_err(map_sqlite_error)?;
        let actual_revision =
            Self::current_revision_locked(&self.table_name, &transaction, &aggregate_id_key)?;
        check_expected_revision(expected_revision, actual_revision)?;

        if prepared.is_empty() {
            transaction.commit().map_err(map_sqlite_error)?;
            return Ok(Vec::new());
        }

        let insert = format!(
            "INSERT INTO {table} \
             (event_id, aggregate_id, aggregate_type, revision, event_type, event_version, \
              payload, metadata, recorded_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            table = self.table_name
        );
        let mut committed = Vec::with_capacity(prepared.len());

        for (index, event) in prepared.into_iter().enumerate() {
            let revision = actual_revision + index as u64 + 1;
            let revision_i64 = i64::try_from(revision).map_err(|_| {
                EventStoreError::Serialization("revision exceeds SQLite INTEGER".to_owned())
            })?;
            let event_version_i64 = i64::from(event.event_version);

            transaction
                .execute(
                    &insert,
                    params![
                        event.event_id.as_str(),
                        aggregate_id_key,
                        A::aggregate_type(),
                        revision_i64,
                        event.event_type,
                        event_version_i64,
                        event.payload_json,
                        event.metadata_json,
                        event.recorded_at_ms,
                    ],
                )
                .map_err(|error| {
                    map_sqlite_insert_error(error, expected_revision, actual_revision)
                })?;
            let sequence = transaction.last_insert_rowid();
            let sequence = u64::try_from(sequence).map_err(|_| {
                EventStoreError::Deserialization("SQLite sequence cannot be negative".to_owned())
            })?;

            committed.push(EventEnvelope::new(
                event.event_id,
                aggregate_id.clone(),
                A::aggregate_type(),
                revision,
                Some(sequence),
                event.event_type,
                event.event_version,
                event.payload,
                event.metadata,
                event.recorded_at,
            ));
        }

        transaction.commit().map_err(map_sqlite_error)?;
        Ok(committed)
    }

    fn load_global_after(&self, sequence: Option<u64>) -> Result<EventStream<A>, Self::Error> {
        let sequence = sequence.unwrap_or_default();
        let sequence = i64::try_from(sequence).map_err(|_| {
            EventStoreError::Deserialization("global sequence exceeds SQLite INTEGER".to_owned())
        })?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| EventStoreError::Poisoned)?;
        let query = format!(
            "SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, \
             event_version, payload, metadata, recorded_at_ms FROM {table} \
             WHERE aggregate_type = ?1 AND sequence > ?2 ORDER BY sequence ASC",
            table = self.table_name
        );
        let mut statement = connection.prepare(&query).map_err(map_sqlite_error)?;
        let rows = statement
            .query_map(params![A::aggregate_type(), sequence], row_to_envelope::<A>)
            .map_err(map_sqlite_error)?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(map_sqlite_error)
    }
}

struct PreparedSqliteEvent<E> {
    event_id: EventId,
    event_type: String,
    event_version: u32,
    payload: E,
    payload_json: String,
    metadata: crate::Metadata,
    metadata_json: String,
    recorded_at: SystemTime,
    recorded_at_ms: i64,
}

impl<E> PreparedSqliteEvent<E>
where
    E: serde::Serialize,
{
    fn new(event: NewEvent<E>) -> Result<Self, EventStoreError> {
        let event_id = EventId::new();
        let recorded_at = SystemTime::now();
        let recorded_at_ms = system_time_to_millis(recorded_at)?;
        let payload_json = serialize_payload(&event.payload)?.to_string();
        let metadata_json = serialize_metadata(&event.metadata)?.to_string();

        Ok(Self {
            event_id,
            event_type: event.event_type,
            event_version: event.event_version,
            payload: event.payload,
            payload_json,
            metadata: event.metadata,
            metadata_json,
            recorded_at,
            recorded_at_ms,
        })
    }
}

fn row_to_envelope<A>(row: &rusqlite::Row<'_>) -> rusqlite::Result<EventEnvelope<A::Event, A::Id>>
where
    A: Aggregate,
    A::Event: serde::de::DeserializeOwned,
    A::Id: serde::de::DeserializeOwned,
{
    let event_id: String = row.get(0)?;
    let aggregate_id: String = row.get(1)?;
    let aggregate_type: String = row.get(2)?;
    let revision: i64 = row.get(3)?;
    let sequence: i64 = row.get(4)?;
    let event_type: String = row.get(5)?;
    let event_version: i64 = row.get(6)?;
    let payload: String = row.get(7)?;
    let metadata: String = row.get(8)?;
    let recorded_at_ms: i64 = row.get(9)?;

    let revision = u64::try_from(revision).map_err(|_| {
        from_event_store_error(EventStoreError::Deserialization(
            "stored revision cannot be negative".to_owned(),
        ))
    })?;
    let sequence = u64::try_from(sequence).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Integer,
            Box::new(EventStoreError::Deserialization(
                "SQLite sequence cannot be negative".to_owned(),
            )),
        )
    })?;
    let event_version = u32::try_from(event_version).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            6,
            rusqlite::types::Type::Integer,
            Box::new(EventStoreError::Deserialization(
                "event_version exceeds u32".to_owned(),
            )),
        )
    })?;
    let aggregate_id = deserialize_id(&aggregate_id).map_err(from_event_store_error)?;
    let payload_value = serde_json::from_str(&payload).map_err(|error| {
        from_event_store_error(EventStoreError::Deserialization(format!(
            "payload JSON: {error}"
        )))
    })?;
    let payload = deserialize_payload(&event_id, &event_type, payload_value)
        .map_err(from_event_store_error)?;
    let metadata_value = serde_json::from_str(&metadata).map_err(|error| {
        from_event_store_error(EventStoreError::Deserialization(format!(
            "metadata JSON: {error}"
        )))
    })?;
    let metadata =
        deserialize_metadata(&event_id, metadata_value).map_err(from_event_store_error)?;
    let recorded_at = millis_to_system_time(recorded_at_ms).map_err(from_event_store_error)?;

    Ok(EventEnvelope::new(
        EventId::from_string(event_id),
        aggregate_id,
        aggregate_type,
        revision,
        Some(sequence),
        event_type,
        event_version,
        payload,
        metadata,
        recorded_at,
    ))
}

fn map_sqlite_insert_error(
    error: rusqlite::Error,
    expected: ExpectedRevision,
    actual: u64,
) -> EventStoreError {
    match &error {
        rusqlite::Error::SqliteFailure(failure, _)
            if failure.code == ErrorCode::ConstraintViolation =>
        {
            EventStoreError::Concurrency(crate::ConcurrencyError::WrongExpectedRevision {
                expected,
                actual,
            })
        }
        _ => map_sqlite_error(error),
    }
}

fn map_sqlite_error(error: rusqlite::Error) -> EventStoreError {
    EventStoreError::Backend(error.to_string())
}

fn from_event_store_error(error: EventStoreError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}
