//! SQLite event store adapter.

use crate::aggregate::Aggregate;
use crate::error::EventStoreError;
use crate::event::{EventEnvelope, EventId, ExpectedRevision, NewEvent};
use crate::event_store::{
    AtomicIdempotentEventStore, EventStore, EventStream, IdempotentAppendError,
};
use crate::idempotency::{
    expires_at_ms, new_lease, now_ms, pending_state_from_row, IdempotencyKey,
    IdempotencyLeaseConfig, IdempotencyState, IdempotencyStore,
};
use crate::snapshot::{Snapshot, SnapshotStore};
use crate::sql_common::{
    check_expected_revision, deserialize_id, deserialize_metadata, deserialize_payload,
    millis_to_system_time, serialize_id, serialize_metadata, serialize_payload,
    system_time_to_millis, validate_table_name,
};
use crate::upcast::UpcasterRegistry;
use rusqlite::{params, Connection, ErrorCode, OptionalExtension, TransactionBehavior};
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

fn lock_connection(mutex: &Mutex<Connection>) -> MutexGuard<'_, Connection> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Clone, Debug)]
struct StoredSqliteEventRow {
    event_id: String,
    aggregate_id: String,
    aggregate_type: String,
    revision: i64,
    sequence: i64,
    event_type: String,
    event_version: i64,
    payload: String,
    metadata: String,
    recorded_at_ms: i64,
}

fn stored_row_from_sqlite(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredSqliteEventRow> {
    Ok(StoredSqliteEventRow {
        event_id: row.get(0)?,
        aggregate_id: row.get(1)?,
        aggregate_type: row.get(2)?,
        revision: row.get(3)?,
        sequence: row.get(4)?,
        event_type: row.get(5)?,
        event_version: row.get(6)?,
        payload: row.get(7)?,
        metadata: row.get(8)?,
        recorded_at_ms: row.get(9)?,
    })
}

fn query_stored_event_rows(
    connection: &Connection,
    query: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<StoredSqliteEventRow>, EventStoreError> {
    let mut statement = connection.prepare_cached(query).map_err(map_sqlite_error)?;
    let rows = statement
        .query_map(params, stored_row_from_sqlite)
        .map_err(map_sqlite_error)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(map_sqlite_row_collect_error)
}

fn begin_immediate_transaction<'connection>(
    connection: &'connection mut Connection,
) -> Result<rusqlite::Transaction<'connection>, EventStoreError> {
    connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| map_sqlite_contention_error(error, None, || Ok(0)))
}

fn is_sqlite_contention(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if failure.code == ErrorCode::DatabaseBusy || failure.code == ErrorCode::DatabaseLocked
    )
}

fn map_sqlite_contention_error(
    error: rusqlite::Error,
    expected: Option<ExpectedRevision>,
    reread_revision: impl FnOnce() -> Result<u64, EventStoreError>,
) -> EventStoreError {
    if is_sqlite_contention(&error) {
        if let Some(expected) = expected {
            let actual = reread_revision().unwrap_or(0);
            return crate::sql_common::map_stream_unique_violation(expected, actual);
        }
    }
    map_sqlite_error(error)
}

fn map_sqlite_row_collect_error(error: rusqlite::Error) -> EventStoreError {
    if let rusqlite::Error::FromSqlConversionFailure(_, _, source) = &error {
        if let Some(store_error) = source.downcast_ref::<EventStoreError>() {
            return store_error.clone();
        }
    }
    map_sqlite_error(error)
}

/// Applies production-oriented pragmas for file-backed and in-memory SQLite
/// connections used by the event and checkpoint stores.
pub fn configure_sqlite_connection(connection: &Connection) -> Result<(), EventStoreError> {
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(map_sqlite_error)?;
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(map_sqlite_error)?;
    connection
        .pragma_update(None, "synchronous", "NORMAL")
        .map_err(map_sqlite_error)?;
    Ok(())
}

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
    idempotency_table: String,
    upcasters: UpcasterRegistry,
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
            idempotency_table: self.idempotency_table.clone(),
            upcasters: self.upcasters.clone(),
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
            .field("idempotency_table", &self.idempotency_table)
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
        Self::with_table_names(connection, table_name, "idempotency_keys")
    }

    /// Creates a SQLite event store with custom event and idempotency table names.
    pub fn with_table_names(
        connection: Connection,
        table_name: impl Into<String>,
        idempotency_table: impl Into<String>,
    ) -> Result<Self, EventStoreError> {
        let table_name = table_name.into();
        let idempotency_table = idempotency_table.into();
        validate_table_name(&table_name)?;
        validate_table_name(&idempotency_table)?;
        configure_sqlite_connection(&connection)?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            table_name,
            idempotency_table,
            upcasters: UpcasterRegistry::new(),
            _marker: PhantomData,
        })
    }

    /// Returns the upcaster registry.
    pub fn upcasters(&self) -> &UpcasterRegistry {
        &self.upcasters
    }

    /// Registers a sequential schema version upcaster for a specific event type.
    pub fn register_upcaster<U>(
        &self,
        event_type: impl Into<String>,
        upcaster: U,
    ) -> Result<(), crate::upcast::UpcasterRegistrationError>
    where
        U: crate::upcast::EventUpcaster + Send + Sync + 'static,
        U::Error: std::fmt::Debug + std::fmt::Display + Send + Sync + 'static,
    {
        self.upcasters.register(event_type, upcaster)
    }

    /// Migrates the SQLite schemas to the latest version.
    pub fn migrate_schema(&self) -> Result<(), EventStoreError> {
        let config = crate::schema::SqlSchemaConfig::new(crate::schema::SqlDialect::Sqlite)
            .with_events_table(&self.table_name)?
            .with_idempotency_table(&self.idempotency_table)?;
        let migrator = crate::schema::SchemaMigrator::new(config);
        let connection = lock_connection(&self.connection);
        migrator.run_sqlite(&connection)
    }

    /// Initializes the SQLite event table and indexes.
    pub fn initialize_schema(&self) -> Result<(), EventStoreError> {
        self.migrate_schema()
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
            .prepare_cached(&query)
            .map_err(map_sqlite_error)?
            .query_row(params![A::aggregate_type(), aggregate_id], |row| row.get(0))
            .map_err(map_sqlite_error)?;

        u64::try_from(revision).map_err(|_| {
            EventStoreError::deserialization("stored revision cannot be negative".to_owned())
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
        let query = format!(
            "SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, \
             event_version, payload, metadata, recorded_at_ms FROM {table} \
             WHERE aggregate_type = ?1 AND aggregate_id = ?2 ORDER BY revision ASC",
            table = self.table_name
        );
        let stored_rows = {
            let connection = lock_connection(&self.connection);
            query_stored_event_rows(&connection, &query, params![A::aggregate_type(), aggregate_id])?
        };
        let upcasters = self.upcasters.clone();
        stored_rows
            .into_iter()
            .map(|row| envelope_from_stored_row::<A>(&upcasters, row))
            .collect()
    }

    fn load_after_revision(
        &self,
        aggregate_id: &A::Id,
        revision: u64,
    ) -> Result<EventStream<A>, Self::Error> {
        let revision_i64 = i64::try_from(revision).map_err(|_| {
            EventStoreError::serialization("revision exceeds SQLite INTEGER".to_owned())
        })?;
        let aggregate_id = serialize_id(aggregate_id)?;
        let query = format!(
            "SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, \
             event_version, payload, metadata, recorded_at_ms FROM {table} \
             WHERE aggregate_type = ?1 AND aggregate_id = ?2 AND revision > ?3 \
             ORDER BY revision ASC",
            table = self.table_name
        );
        let stored_rows = {
            let connection = lock_connection(&self.connection);
            query_stored_event_rows(
                &connection,
                &query,
                params![A::aggregate_type(), aggregate_id, revision_i64],
            )?
        };
        let upcasters = self.upcasters.clone();
        stored_rows
            .into_iter()
            .map(|row| envelope_from_stored_row::<A>(&upcasters, row))
            .collect()
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
        let mut connection = lock_connection(&self.connection);
        let transaction = begin_immediate_transaction(&mut connection)?;
        let actual_revision =
            Self::current_revision_locked(&self.table_name, &transaction, &aggregate_id_key)?;
        check_expected_revision(expected_revision, actual_revision)?;

        if prepared.is_empty() {
            transaction.commit().map_err(|error| {
                map_sqlite_contention_error(error, Some(expected_revision), || {
                    Self::current_revision_locked(&self.table_name, &connection, &aggregate_id_key)
                })
            })?;
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
        let mut insert_statement = transaction
            .prepare_cached(&insert)
            .map_err(map_sqlite_error)?;

        for (index, event) in prepared.into_iter().enumerate() {
            let revision = actual_revision + index as u64 + 1;
            let revision_i64 = i64::try_from(revision).map_err(|_| {
                EventStoreError::serialization("revision exceeds SQLite INTEGER".to_owned())
            })?;
            let event_version_i64 = i64::from(event.event_version);

            insert_statement
                .execute(params![
                    event.event_id.as_str(),
                    aggregate_id_key,
                    A::aggregate_type(),
                    revision_i64,
                    event.event_type,
                    event_version_i64,
                    event.payload_json,
                    event.metadata_json,
                    event.recorded_at_ms,
                ])
                .map_err(|error| {
                    map_sqlite_insert_error(error, expected_revision, actual_revision, || {
                        Self::current_revision_locked(
                            &self.table_name,
                            &transaction,
                            &aggregate_id_key,
                        )
                    })
                })?;
            let sequence = transaction.last_insert_rowid();
            let sequence = u64::try_from(sequence).map_err(|_| {
                EventStoreError::deserialization("SQLite sequence cannot be negative".to_owned())
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

        drop(insert_statement);
        transaction.commit().map_err(|error| {
            map_sqlite_contention_error(error, Some(expected_revision), || {
                Self::current_revision_locked(&self.table_name, &connection, &aggregate_id_key)
            })
        })?;
        Ok(committed)
    }

    fn load_global_after(&self, sequence: Option<u64>) -> Result<EventStream<A>, Self::Error> {
        let sequence = sequence.unwrap_or_default();
        let sequence = i64::try_from(sequence).map_err(|_| {
            EventStoreError::deserialization("global sequence exceeds SQLite INTEGER".to_owned())
        })?;
        let query = format!(
            "SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, \
             event_version, payload, metadata, recorded_at_ms FROM {table} \
             WHERE aggregate_type = ?1 AND sequence > ?2 ORDER BY sequence ASC",
            table = self.table_name
        );
        let stored_rows = {
            let connection = lock_connection(&self.connection);
            query_stored_event_rows(&connection, &query, params![A::aggregate_type(), sequence])?
        };
        let upcasters = self.upcasters.clone();
        stored_rows
            .into_iter()
            .map(|row| envelope_from_stored_row::<A>(&upcasters, row))
            .collect()
    }

    fn load_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: NonZeroUsize,
    ) -> Result<EventStream<A>, Self::Error> {
        let sequence = sequence.unwrap_or_default();
        let sequence = i64::try_from(sequence).map_err(|_| {
            EventStoreError::deserialization("global sequence exceeds SQLite INTEGER".to_owned())
        })?;
        let limit = i64::try_from(limit.get()).map_err(|_| {
            EventStoreError::deserialization("event replay limit exceeds SQLite INTEGER".to_owned())
        })?;
        let query = format!(
            "SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, \
             event_version, payload, metadata, recorded_at_ms FROM {table} \
             WHERE aggregate_type = ?1 AND sequence > ?2 ORDER BY sequence ASC LIMIT ?3",
            table = self.table_name
        );
        let stored_rows = {
            let connection = lock_connection(&self.connection);
            query_stored_event_rows(
                &connection,
                &query,
                params![A::aggregate_type(), sequence, limit],
            )?
        };
        let upcasters = self.upcasters.clone();
        stored_rows
            .into_iter()
            .map(|row| envelope_from_stored_row::<A>(&upcasters, row))
            .collect()
    }
}

impl<A> crate::raw_feed::RawEventFeed for SqliteEventStore<A>
where
    A: Aggregate,
{
    type Error = EventStoreError;

    fn load_raw_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: NonZeroUsize,
    ) -> Result<Vec<crate::raw_feed::RawEventEnvelope>, Self::Error> {
        let sequence_i64 = i64::try_from(sequence.unwrap_or_default()).map_err(|_| {
            EventStoreError::deserialization("global sequence exceeds SQLite INTEGER".to_owned())
        })?;
        let limit_i64 = i64::try_from(limit.get()).map_err(|_| {
            EventStoreError::deserialization("event replay limit exceeds SQLite INTEGER".to_owned())
        })?;
        let query = format!(
            "SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, \
             event_version, payload, metadata, recorded_at_ms FROM {table} \
             WHERE sequence > ?1 ORDER BY sequence ASC LIMIT ?2",
            table = self.table_name
        );
        let stored_rows = {
            let connection = lock_connection(&self.connection);
            query_stored_event_rows(&connection, &query, params![sequence_i64, limit_i64])?
        };
        let upcasters = self.upcasters.clone();
        stored_rows
            .into_iter()
            .map(|row| raw_envelope_from_stored_row(&upcasters, row))
            .collect()
    }
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl<A> crate::raw_feed::AsyncRawEventFeed for SqliteEventStore<A>
where
    A: Aggregate + Send + Sync + 'static,
{
    type Error = EventStoreError;

    async fn load_raw_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: NonZeroUsize,
    ) -> Result<Vec<crate::raw_feed::RawEventEnvelope>, Self::Error> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            crate::raw_feed::RawEventFeed::load_raw_global_after_limited(&this, sequence, limit)
        })
        .await
        .map_err(|error| EventStoreError::backend(error.to_string()))?
    }
}

impl<A> AtomicIdempotentEventStore<A> for SqliteEventStore<A>
where
    A: Aggregate + 'static,
    A::Event: serde::Serialize + serde::de::DeserializeOwned,
    A::Id: serde::Serialize + serde::de::DeserializeOwned,
{
    fn load_idempotent(
        &self,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<IdempotencyState<EventStream<A>>>, Self::Error> {
        let connection = lock_connection(&self.connection);
        let query = format!(
            "SELECT state, value, owner, expires_at_ms FROM {} WHERE idempotency_key = ?1;",
            self.idempotency_table
        );
        let row = connection
            .query_row(&query, params![idempotency_key.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            })
            .optional()
            .map_err(map_sqlite_error)?;

        let now = now_ms();
        row.map(
            |(state, value, owner, expires_at_ms)| match (state.as_str(), value) {
                ("pending", _) => pending_state_from_row(owner.clone(), expires_at_ms, now)
                    .map(IdempotencyState::Pending)
                    .ok_or_else(|| {
                        EventStoreError::deserialization(
                            "pending idempotency row has expired or is missing lease metadata"
                                .to_owned(),
                        )
                    }),
                ("complete", Some(value)) => serde_json::from_str(&value)
                    .map(IdempotencyState::Complete)
                    .map_err(|error| {
                        EventStoreError::deserialization(format!(
                            "idempotent committed events JSON: {error}"
                        ))
                    }),
                ("complete", None) => Err(EventStoreError::deserialization(
                    "completed idempotency row is missing value".to_owned(),
                )),
                (state, _) => Err(EventStoreError::deserialization(format!(
                    "unknown idempotency state: {state}"
                ))),
            },
        )
        .transpose()
    }

    fn append_idempotent(
        &self,
        idempotency_key: IdempotencyKey,
        aggregate_id: &A::Id,
        expected_revision: ExpectedRevision,
        events: Vec<NewEvent<A::Event>>,
    ) -> Result<EventStream<A>, IdempotentAppendError<Self::Error>> {
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!(
            "event_store.append_idempotent",
            dialect = "sqlite",
            aggregate_type = A::aggregate_type(),
            expected_revision = ?expected_revision,
            event_count = events.len()
        )
        .entered();

        let aggregate_id_key = serialize_id(aggregate_id).map_err(IdempotentAppendError::Store)?;
        let prepared = events
            .into_iter()
            .map(PreparedSqliteEvent::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(IdempotentAppendError::Store)?;
        let mut connection = lock_connection(&self.connection);
        let transaction = begin_immediate_transaction(&mut connection)
            .map_err(IdempotentAppendError::Store)?;

        let load_idempotency = format!(
            "SELECT state, value, owner, expires_at_ms FROM {} WHERE idempotency_key = ?1;",
            self.idempotency_table
        );
        let row = transaction
            .prepare_cached(&load_idempotency)
            .map_err(|error| IdempotentAppendError::Store(map_sqlite_error(error)))?
            .query_row(params![idempotency_key.as_str()], |row| {
                let state: String = row.get(0)?;
                let value: Option<String> = row.get(1)?;
                let owner: Option<String> = row.get(2)?;
                let expires_at_ms: Option<i64> = row.get(3)?;
                Ok((state, value, owner, expires_at_ms))
            })
            .optional()
            .map_err(|error| IdempotentAppendError::Store(map_sqlite_error(error)))?;

        match row {
            Some((state, Some(value), _, _)) if state == "complete" => {
                let committed = serde_json::from_str(&value).map_err(|error| {
                    IdempotentAppendError::Store(EventStoreError::deserialization(format!(
                        "idempotent committed events JSON: {error}"
                    )))
                })?;
                transaction
                    .commit()
                    .map_err(|error| IdempotentAppendError::Store(map_sqlite_error(error)))?;
                return Ok(committed);
            }
            Some((state, None, ..)) if state == "complete" => {
                return Err(IdempotentAppendError::Store(
                    EventStoreError::deserialization(
                        "completed idempotency row is missing value".to_owned(),
                    ),
                ));
            }
            Some((state, _, owner, expires_at_ms))
                if state == "pending"
                    && pending_state_from_row(owner.clone(), expires_at_ms, now_ms()).is_some() =>
            {
                return Err(IdempotentAppendError::Pending {
                    key: idempotency_key,
                });
            }
            Some((state, ..)) => {
                return Err(IdempotentAppendError::Store(
                    EventStoreError::deserialization(format!("unknown idempotency state: {state}")),
                ));
            }
            None => {}
        }

        let updated_at_ms =
            system_time_to_millis(SystemTime::now()).map_err(IdempotentAppendError::Store)?;
        let lease = new_lease(&IdempotencyLeaseConfig::default());
        let reserve = format!(
            "INSERT INTO {} (idempotency_key, state, value, updated_at_ms, owner, expires_at_ms)
             VALUES (?1, 'pending', NULL, ?2, ?3, ?4);",
            self.idempotency_table
        );
        transaction
            .prepare_cached(&reserve)
            .map_err(|error| IdempotentAppendError::Store(map_sqlite_error(error)))?
            .execute(params![
                idempotency_key.as_str(),
                updated_at_ms,
                lease.owner.as_str(),
                i64::try_from(lease.expires_at_ms).unwrap_or(i64::MAX)
            ])
            .map_err(|error| IdempotentAppendError::Store(map_sqlite_error(error)))?;

        let actual_revision =
            Self::current_revision_locked(&self.table_name, &transaction, &aggregate_id_key)
                .map_err(IdempotentAppendError::Store)?;
        check_expected_revision(expected_revision, actual_revision)
            .map_err(IdempotentAppendError::Store)?;

        let insert = format!(
            "INSERT INTO {table} \
             (event_id, aggregate_id, aggregate_type, revision, event_type, event_version, \
              payload, metadata, recorded_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            table = self.table_name
        );
        let mut committed = Vec::with_capacity(prepared.len());
        let mut insert_statement = transaction
            .prepare_cached(&insert)
            .map_err(|error| IdempotentAppendError::Store(map_sqlite_error(error)))?;

        for (index, event) in prepared.into_iter().enumerate() {
            let revision = actual_revision + index as u64 + 1;
            let revision_i64 = i64::try_from(revision).map_err(|_| {
                IdempotentAppendError::Store(EventStoreError::serialization(
                    "revision exceeds SQLite INTEGER".to_owned(),
                ))
            })?;
            let event_version_i64 = i64::from(event.event_version);

            insert_statement
                .execute(params![
                    event.event_id.as_str(),
                    aggregate_id_key,
                    A::aggregate_type(),
                    revision_i64,
                    event.event_type,
                    event_version_i64,
                    event.payload_json,
                    event.metadata_json,
                    event.recorded_at_ms,
                ])
                .map_err(|error| {
                    IdempotentAppendError::Store(map_sqlite_insert_error(
                        error,
                        expected_revision,
                        actual_revision,
                        || {
                            Self::current_revision_locked(
                                &self.table_name,
                                &transaction,
                                &aggregate_id_key,
                            )
                        },
                    ))
                })?;
            let sequence = transaction.last_insert_rowid();
            let sequence = u64::try_from(sequence).map_err(|_| {
                IdempotentAppendError::Store(EventStoreError::deserialization(
                    "SQLite sequence cannot be negative".to_owned(),
                ))
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

        let value_json = serde_json::to_string(&committed).map_err(|error| {
            IdempotentAppendError::Store(EventStoreError::serialization(format!(
                "idempotent committed events JSON: {error}"
            )))
        })?;
        let complete = format!(
            "UPDATE {} SET state = 'complete', value = ?2, updated_at_ms = ?3
             WHERE idempotency_key = ?1;",
            self.idempotency_table
        );
        drop(insert_statement);
        transaction
            .prepare_cached(&complete)
            .map_err(|error| IdempotentAppendError::Store(map_sqlite_error(error)))?
            .execute(params![idempotency_key.as_str(), value_json, updated_at_ms])
            .map_err(|error| IdempotentAppendError::Store(map_sqlite_error(error)))?;
        transaction
            .commit()
            .map_err(|error| IdempotentAppendError::Store(map_sqlite_error(error)))?;
        Ok(committed)
    }
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl<A> crate::async_api::AsyncEventStore<A> for SqliteEventStore<A>
where
    A: Aggregate + Send + Sync + 'static,
    A::Event: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
    A::Id: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
{
    type Error = EventStoreError;

    async fn load(&self, aggregate_id: &A::Id) -> Result<EventStream<A>, Self::Error> {
        let this = self.clone();
        let aggregate_id = aggregate_id.clone();
        tokio::task::spawn_blocking(move || EventStore::load(&this, &aggregate_id))
            .await
            .map_err(|error| EventStoreError::backend(error.to_string()))?
    }

    async fn load_after_revision(
        &self,
        aggregate_id: &A::Id,
        revision: u64,
    ) -> Result<EventStream<A>, Self::Error> {
        let this = self.clone();
        let aggregate_id = aggregate_id.clone();
        tokio::task::spawn_blocking(move || {
            EventStore::load_after_revision(&this, &aggregate_id, revision)
        })
        .await
        .map_err(|error| EventStoreError::backend(error.to_string()))?
    }

    async fn append(
        &self,
        aggregate_id: &A::Id,
        expected_revision: ExpectedRevision,
        events: Vec<NewEvent<A::Event>>,
    ) -> Result<EventStream<A>, Self::Error> {
        let this = self.clone();
        let aggregate_id = aggregate_id.clone();
        tokio::task::spawn_blocking(move || {
            EventStore::append(&this, &aggregate_id, expected_revision, events)
        })
        .await
        .map_err(|error| EventStoreError::backend(error.to_string()))?
    }

    async fn load_global_after(
        &self,
        sequence: Option<u64>,
    ) -> Result<EventStream<A>, Self::Error> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || EventStore::load_global_after(&this, sequence))
            .await
            .map_err(|error| EventStoreError::backend(error.to_string()))?
    }

    async fn load_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: NonZeroUsize,
    ) -> Result<EventStream<A>, Self::Error> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            EventStore::load_global_after_limited(&this, sequence, limit)
        })
        .await
        .map_err(|error| EventStoreError::backend(error.to_string()))?
    }
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl<A> crate::async_api::AsyncAtomicIdempotentEventStore<A> for SqliteEventStore<A>
where
    A: Aggregate + Send + Sync + 'static,
    A::Event: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
    A::Id: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
{
    async fn load_idempotent(
        &self,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<IdempotencyState<EventStream<A>>>, Self::Error> {
        let this = self.clone();
        let idempotency_key = idempotency_key.clone();
        tokio::task::spawn_blocking(move || {
            AtomicIdempotentEventStore::load_idempotent(&this, &idempotency_key)
        })
        .await
        .map_err(|error| EventStoreError::backend(error.to_string()))?
    }

    async fn append_idempotent(
        &self,
        idempotency_key: IdempotencyKey,
        aggregate_id: &A::Id,
        expected_revision: ExpectedRevision,
        events: Vec<NewEvent<A::Event>>,
    ) -> Result<EventStream<A>, IdempotentAppendError<Self::Error>> {
        let this = self.clone();
        let aggregate_id = aggregate_id.clone();
        tokio::task::spawn_blocking(move || {
            AtomicIdempotentEventStore::append_idempotent(
                &this,
                idempotency_key,
                &aggregate_id,
                expected_revision,
                events,
            )
        })
        .await
        .map_err(|error| {
            IdempotentAppendError::Store(EventStoreError::backend(error.to_string()))
        })?
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
            event_type: event.event_type.into_string(),
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

/// Maps a full event row into an untyped envelope, applying upcasters but
/// keeping the payload as raw JSON and the aggregate id as its stored string.
fn raw_envelope_from_stored_row(
    upcasters: &UpcasterRegistry,
    row: StoredSqliteEventRow,
) -> Result<crate::raw_feed::RawEventEnvelope, EventStoreError> {
    let StoredSqliteEventRow {
        event_id,
        aggregate_id,
        aggregate_type,
        revision,
        sequence,
        event_type,
        event_version,
        payload,
        metadata,
        recorded_at_ms,
    } = row;

    let revision = u64::try_from(revision).map_err(|_| {
        EventStoreError::deserialization("stored revision cannot be negative".to_owned())
    })?;
    let sequence = u64::try_from(sequence).map_err(|_| {
        EventStoreError::deserialization("SQLite sequence cannot be negative".to_owned())
    })?;
    let event_version = u32::try_from(event_version).map_err(|_| {
        EventStoreError::deserialization("event_version exceeds u32".to_owned())
    })?;

    let (event_version, upcasted_bytes) = upcasters
        .prepare_payload(&event_type, event_version, payload.into_bytes())
        .map_err(|err| EventStoreError::deserialization(err.to_string()))?;
    let payload: serde_json::Value = serde_json::from_slice(&upcasted_bytes).map_err(|error| {
        EventStoreError::deserialization(format!("payload JSON: {error}"))
    })?;

    let metadata_value = serde_json::from_str(&metadata).map_err(|error| {
        EventStoreError::deserialization(format!("metadata JSON: {error}"))
    })?;
    let metadata = deserialize_metadata(&event_id, metadata_value)?;
    let recorded_at = millis_to_system_time(recorded_at_ms)?;

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

fn envelope_from_stored_row<A>(
    upcasters: &UpcasterRegistry,
    row: StoredSqliteEventRow,
) -> Result<EventEnvelope<A::Event, A::Id>, EventStoreError>
where
    A: Aggregate,
    A::Event: serde::de::DeserializeOwned,
    A::Id: serde::de::DeserializeOwned,
{
    let StoredSqliteEventRow {
        event_id,
        aggregate_id,
        aggregate_type,
        revision,
        sequence,
        event_type,
        event_version,
        payload,
        metadata,
        recorded_at_ms,
    } = row;

    let revision = u64::try_from(revision).map_err(|_| {
        EventStoreError::deserialization("stored revision cannot be negative".to_owned())
    })?;
    let sequence = u64::try_from(sequence).map_err(|_| {
        EventStoreError::deserialization("SQLite sequence cannot be negative".to_owned())
    })?;
    let event_version = u32::try_from(event_version).map_err(|_| {
        EventStoreError::deserialization("event_version exceeds u32".to_owned())
    })?;
    let aggregate_id = deserialize_id(&aggregate_id)?;

    let (event_version, upcasted_bytes) = upcasters
        .prepare_payload(&event_type, event_version, payload.into_bytes())
        .map_err(|err| EventStoreError::deserialization(err.to_string()))?;

    let payload_value = serde_json::from_slice(&upcasted_bytes).map_err(|error| {
        EventStoreError::deserialization(format!("payload JSON: {error}"))
    })?;
    let payload = deserialize_payload(&event_id, &event_type, payload_value)?;
    let metadata_value = serde_json::from_str(&metadata).map_err(|error| {
        EventStoreError::deserialization(format!("metadata JSON: {error}"))
    })?;
    let metadata = deserialize_metadata(&event_id, metadata_value)?;
    let recorded_at = millis_to_system_time(recorded_at_ms)?;

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

fn is_sqlite_stream_revision_unique_violation(message: Option<&str>) -> bool {
    message.is_some_and(crate::sql_common::is_stream_revision_unique_violation_message)
}

fn map_sqlite_insert_error(
    error: rusqlite::Error,
    expected: ExpectedRevision,
    stale_actual: u64,
    reread_revision: impl FnOnce() -> Result<u64, EventStoreError>,
) -> EventStoreError {
    match &error {
        rusqlite::Error::SqliteFailure(failure, message)
            if failure.code == ErrorCode::ConstraintViolation
                && matches!(
                    failure.extended_code,
                    rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                        | rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY
                )
                && is_sqlite_stream_revision_unique_violation(message.as_deref()) =>
        {
            let current_revision = reread_revision().unwrap_or(stale_actual);
            return crate::sql_common::map_stream_unique_violation(expected, current_revision);
        }
        _ => {}
    }
    if is_sqlite_contention(&error) {
        let current_revision = reread_revision().unwrap_or(stale_actual);
        return crate::sql_common::map_stream_unique_violation(expected, current_revision);
    }
    map_sqlite_error(error)
}

fn map_sqlite_error(error: rusqlite::Error) -> EventStoreError {
    if let rusqlite::Error::FromSqlConversionFailure(_, _, source) = &error {
        if let Some(store_error) = source.downcast_ref::<EventStoreError>() {
            return store_error.clone();
        }
    }
    let code = match &error {
        rusqlite::Error::SqliteFailure(failure, _) => Some(failure.extended_code.to_string()),
        _ => None,
    };
    let mapped = EventStoreError::backend_with_source(error.to_string(), error);
    match code {
        Some(code) => mapped.with_code(code),
        None => mapped,
    }
}

/// SQLite checkpoint store implementation.
#[derive(Clone, Debug)]
pub struct SqliteCheckpointStore {
    connection: Arc<Mutex<Connection>>,
    table_name: String,
}

impl SqliteCheckpointStore {
    /// Creates a SQLite checkpoint store using the default table name.
    pub fn new(connection: Connection) -> Result<Self, EventStoreError> {
        Self::with_table_name(connection, "projection_checkpoints")
    }

    /// Creates a SQLite checkpoint store with a custom table name.
    pub fn with_table_name(
        connection: Connection,
        table_name: impl Into<String>,
    ) -> Result<Self, EventStoreError> {
        let table_name = table_name.into();
        validate_table_name(&table_name)?;
        configure_sqlite_connection(&connection)?;

        let store = Self {
            connection: Arc::new(Mutex::new(connection)),
            table_name,
        };
        store.initialize_schema()?;
        Ok(store)
    }

    /// Initializes the checkpoint schema table.
    pub fn initialize_schema(&self) -> Result<(), EventStoreError> {
        let config = crate::schema::SqlSchemaConfig::new(crate::schema::SqlDialect::Sqlite)
            .with_checkpoints_table(&self.table_name)?;
        let migrator = crate::schema::SchemaMigrator::new(config);
        let connection = lock_connection(&self.connection);
        migrator.run_sqlite(&connection)
    }
}

impl crate::projection::CheckpointStore for SqliteCheckpointStore {
    type Error = EventStoreError;

    fn load_checkpoint(&self, projection_name: &str) -> Result<Option<u64>, Self::Error> {
        let connection = lock_connection(&self.connection);
        let sql = format!(
            "SELECT sequence FROM {} WHERE projection_name = ?1;",
            self.table_name
        );
        let mut stmt = connection.prepare_cached(&sql).map_err(map_sqlite_error)?;
        let mut rows = stmt
            .query(params![projection_name])
            .map_err(map_sqlite_error)?;

        if let Some(row) = rows.next().map_err(map_sqlite_error)? {
            let sequence: i64 = row.get(0).map_err(map_sqlite_error)?;
            let sequence = u64::try_from(sequence).map_err(|_| {
                EventStoreError::deserialization("SQLite checkpoint cannot be negative".to_owned())
            })?;
            Ok(Some(sequence))
        } else {
            Ok(None)
        }
    }

    fn save_checkpoint(&self, projection_name: &str, sequence: u64) -> Result<(), Self::Error> {
        let connection = lock_connection(&self.connection);
        let sql = format!(
            "INSERT INTO {} (projection_name, sequence) VALUES (?1, ?2)
             ON CONFLICT(projection_name) DO UPDATE SET sequence = CASE
                WHEN excluded.sequence > {table}.sequence THEN excluded.sequence
                ELSE {table}.sequence
             END;",
            self.table_name,
            table = self.table_name
        );
        let sequence_i64 = i64::try_from(sequence)
            .map_err(|_| EventStoreError::deserialization("checkpoint exceeds i64".to_owned()))?;
        connection
            .prepare_cached(&sql)
            .map_err(map_sqlite_error)?
            .execute(params![projection_name, sequence_i64])
            .map_err(map_sqlite_error)?;
        Ok(())
    }

    fn reset_checkpoint(&self, projection_name: &str) -> Result<(), Self::Error> {
        let connection = lock_connection(&self.connection);
        let sql = format!(
            "DELETE FROM {} WHERE projection_name = ?1;",
            self.table_name
        );
        connection
            .execute(&sql, params![projection_name])
            .map_err(map_sqlite_error)?;
        Ok(())
    }
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl crate::projection::AsyncCheckpointStore for SqliteCheckpointStore {
    type Error = EventStoreError;

    async fn load_checkpoint(&self, projection_name: &str) -> Result<Option<u64>, Self::Error> {
        let this = self.clone();
        let name = projection_name.to_owned();
        tokio::task::spawn_blocking(move || {
            crate::projection::CheckpointStore::load_checkpoint(&this, &name)
        })
        .await
        .map_err(|e| EventStoreError::backend(e.to_string()))?
    }

    async fn save_checkpoint(
        &self,
        projection_name: &str,
        sequence: u64,
    ) -> Result<(), Self::Error> {
        let this = self.clone();
        let name = projection_name.to_owned();
        tokio::task::spawn_blocking(move || {
            crate::projection::CheckpointStore::save_checkpoint(&this, &name, sequence)
        })
        .await
        .map_err(|e| EventStoreError::backend(e.to_string()))?
    }

    async fn reset_checkpoint(&self, projection_name: &str) -> Result<(), Self::Error> {
        let this = self.clone();
        let name = projection_name.to_owned();
        tokio::task::spawn_blocking(move || {
            crate::projection::CheckpointStore::reset_checkpoint(&this, &name)
        })
        .await
        .map_err(|e| EventStoreError::backend(e.to_string()))?
    }
}

/// SQLite-backed idempotency store.
///
/// The store persists pending reservations and completed JSON-serializable
/// values so command retries can be deduplicated across process restarts.
pub struct SqliteIdempotencyStore<V>
where
    V: Clone,
{
    connection: Arc<Mutex<Connection>>,
    table_name: String,
    _marker: PhantomData<fn() -> V>,
}

impl<V> Clone for SqliteIdempotencyStore<V>
where
    V: Clone,
{
    fn clone(&self) -> Self {
        Self {
            connection: Arc::clone(&self.connection),
            table_name: self.table_name.clone(),
            _marker: PhantomData,
        }
    }
}

impl<V> std::fmt::Debug for SqliteIdempotencyStore<V>
where
    V: Clone,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteIdempotencyStore")
            .field("table_name", &self.table_name)
            .finish_non_exhaustive()
    }
}

impl<V> SqliteIdempotencyStore<V>
where
    V: Clone,
{
    /// Creates a SQLite idempotency store using the default table name.
    pub fn new(connection: Connection) -> Result<Self, EventStoreError> {
        Self::with_table_name(connection, "idempotency_keys")
    }

    /// Creates a SQLite idempotency store with a custom table name.
    pub fn with_table_name(
        connection: Connection,
        table_name: impl Into<String>,
    ) -> Result<Self, EventStoreError> {
        let table_name = table_name.into();
        validate_table_name(&table_name)?;

        let store = Self {
            connection: Arc::new(Mutex::new(connection)),
            table_name,
            _marker: PhantomData,
        };
        store.initialize_schema()?;
        Ok(store)
    }

    /// Initializes the idempotency schema table.
    pub fn initialize_schema(&self) -> Result<(), EventStoreError> {
        let config = crate::schema::SqlSchemaConfig::new(crate::schema::SqlDialect::Sqlite)
            .with_idempotency_table(&self.table_name)?;
        let migrator = crate::schema::SchemaMigrator::new(config);
        let connection = lock_connection(&self.connection);
        migrator.run_sqlite(&connection)
    }
}

impl<V> IdempotencyStore<V> for SqliteIdempotencyStore<V>
where
    V: Clone + serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
{
    type Error = EventStoreError;

    fn load(&self, key: &IdempotencyKey) -> Result<Option<IdempotencyState<V>>, Self::Error> {
        let connection = lock_connection(&self.connection);
        let sql = format!(
            "SELECT state, value, owner, expires_at_ms FROM {} WHERE idempotency_key = ?1;",
            self.table_name
        );
        let row = connection
            .query_row(&sql, params![key.as_str()], |row| {
                let state: String = row.get(0)?;
                let value: Option<String> = row.get(1)?;
                let owner: Option<String> = row.get(2)?;
                let expires_at_ms: Option<i64> = row.get(3)?;
                Ok((state, value, owner, expires_at_ms))
            })
            .optional()
            .map_err(map_sqlite_error)?;

        let now = now_ms();
        match row {
            None => Ok(None),
            Some((state, _, owner, expires_at_ms)) if state == "pending" => {
                if let Some(lease) = pending_state_from_row(owner.clone(), expires_at_ms, now) {
                    Ok(Some(IdempotencyState::Pending(lease)))
                } else {
                    Ok(None)
                }
            }
            Some((state, Some(value), ..)) if state == "complete" => {
                let value = serde_json::from_str(&value).map_err(|error| {
                    EventStoreError::deserialization(format!("idempotency value JSON: {error}"))
                })?;
                Ok(Some(IdempotencyState::Complete(value)))
            }
            Some((state, None, ..)) if state == "complete" => {
                Err(EventStoreError::deserialization(
                    "completed idempotency row is missing value".to_owned(),
                ))
            }
            Some((state, ..)) => Err(EventStoreError::deserialization(format!(
                "unknown idempotency state: {state}"
            ))),
        }
    }

    fn reserve_with_lease(
        &self,
        key: IdempotencyKey,
        config: &IdempotencyLeaseConfig,
    ) -> Result<bool, Self::Error> {
        key.validate_storage_length()
            .map_err(|error| EventStoreError::backend(error.to_string()))?;
        let connection = lock_connection(&self.connection);
        let updated_at_ms = system_time_to_millis(SystemTime::now())?;
        let lease = new_lease(config);
        let insert_sql = format!(
            "INSERT OR IGNORE INTO {} (idempotency_key, state, value, updated_at_ms, owner, expires_at_ms)
             VALUES (?1, 'pending', NULL, ?2, ?3, ?4);",
            self.table_name
        );
        let changed = connection
            .execute(
                &insert_sql,
                params![
                    key.as_str(),
                    updated_at_ms,
                    lease.owner.as_str(),
                    i64::try_from(lease.expires_at_ms).unwrap_or(i64::MAX)
                ],
            )
            .map_err(map_sqlite_error)?;
        if changed == 1 {
            return Ok(true);
        }
        let reclaim_sql = format!(
            "UPDATE {} SET owner = ?2, expires_at_ms = ?3, updated_at_ms = ?4
             WHERE idempotency_key = ?1 AND state = 'pending'
               AND (expires_at_ms IS NULL OR expires_at_ms <= ?5);",
            self.table_name
        );
        let reclaimed = connection
            .execute(
                &reclaim_sql,
                params![
                    key.as_str(),
                    lease.owner.as_str(),
                    i64::try_from(lease.expires_at_ms).unwrap_or(i64::MAX),
                    updated_at_ms,
                    i64::try_from(now_ms()).unwrap_or(0)
                ],
            )
            .map_err(map_sqlite_error)?;
        Ok(reclaimed == 1)
    }

    fn heartbeat(&self, key: &IdempotencyKey, owner: &str) -> Result<bool, Self::Error> {
        let connection = lock_connection(&self.connection);
        let updated_at_ms = system_time_to_millis(SystemTime::now())?;
        let new_expiry = expires_at_ms(crate::idempotency::DEFAULT_IDEMPOTENCY_LEASE);
        let sql = format!(
            "UPDATE {} SET expires_at_ms = ?3, updated_at_ms = ?4
             WHERE idempotency_key = ?1 AND state = 'pending' AND owner = ?2;",
            self.table_name
        );
        let changed = connection
            .execute(
                &sql,
                params![
                    key.as_str(),
                    owner,
                    i64::try_from(new_expiry).unwrap_or(i64::MAX),
                    updated_at_ms
                ],
            )
            .map_err(map_sqlite_error)?;
        Ok(changed == 1)
    }

    fn expire_stale_pending(&self, now_ms: u64) -> Result<usize, Self::Error> {
        let connection = lock_connection(&self.connection);
        let sql = format!(
            "DELETE FROM {} WHERE state = 'pending'
               AND expires_at_ms IS NOT NULL AND expires_at_ms <= ?1;",
            self.table_name
        );
        let removed = connection
            .execute(&sql, params![i64::try_from(now_ms).unwrap_or(i64::MAX)])
            .map_err(map_sqlite_error)?;
        Ok(removed)
    }

    fn expire_completed_before(&self, cutoff_ms: u64) -> Result<usize, Self::Error> {
        let connection = lock_connection(&self.connection);
        let sql = format!(
            "DELETE FROM {} WHERE state = 'complete' AND updated_at_ms < ?1;",
            self.table_name
        );
        let removed = connection
            .execute(&sql, params![i64::try_from(cutoff_ms).unwrap_or(i64::MAX)])
            .map_err(map_sqlite_error)?;
        Ok(removed)
    }

    fn save(&self, key: IdempotencyKey, value: V) -> Result<(), Self::Error> {
        let connection = lock_connection(&self.connection);
        let updated_at_ms = system_time_to_millis(SystemTime::now())?;
        let value_json = serde_json::to_string(&value).map_err(|error| {
            EventStoreError::serialization(format!("idempotency value JSON: {error}"))
        })?;
        let sql = format!(
            "INSERT INTO {} (idempotency_key, state, value, updated_at_ms)
             VALUES (?1, 'complete', ?2, ?3)
             ON CONFLICT(idempotency_key) DO UPDATE SET
                state = excluded.state,
                value = excluded.value,
                updated_at_ms = excluded.updated_at_ms;",
            self.table_name
        );
        connection
            .execute(&sql, params![key.as_str(), value_json, updated_at_ms])
            .map_err(map_sqlite_error)?;
        Ok(())
    }

    fn remove(&self, key: &IdempotencyKey) -> Result<(), Self::Error> {
        let connection = lock_connection(&self.connection);
        let sql = format!(
            "DELETE FROM {} WHERE idempotency_key = ?1;",
            self.table_name
        );
        connection
            .execute(&sql, params![key.as_str()])
            .map_err(map_sqlite_error)?;
        Ok(())
    }
}

/// SQLite-backed durable snapshot store.
pub struct SqliteSnapshotStore<A>
where
    A: Aggregate,
{
    connection: Arc<Mutex<Connection>>,
    table_name: String,
    _marker: PhantomData<fn() -> A>,
}

impl<A> Clone for SqliteSnapshotStore<A>
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

impl<A> std::fmt::Debug for SqliteSnapshotStore<A>
where
    A: Aggregate,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteSnapshotStore")
            .field("table_name", &self.table_name)
            .finish_non_exhaustive()
    }
}

impl<A> SqliteSnapshotStore<A>
where
    A: Aggregate,
{
    /// Creates a SQLite snapshot store using the default table name.
    pub fn new(connection: Connection) -> Result<Self, EventStoreError> {
        Self::with_table_name(connection, "snapshots")
    }

    /// Creates a SQLite snapshot store with a custom table name.
    pub fn with_table_name(
        connection: Connection,
        table_name: impl Into<String>,
    ) -> Result<Self, EventStoreError> {
        let table_name = table_name.into();
        validate_table_name(&table_name)?;

        let store = Self {
            connection: Arc::new(Mutex::new(connection)),
            table_name,
            _marker: PhantomData,
        };
        store.initialize_schema()?;
        Ok(store)
    }

    /// Initializes the snapshot schema table.
    pub fn initialize_schema(&self) -> Result<(), EventStoreError> {
        let config = crate::schema::SqlSchemaConfig::new(crate::schema::SqlDialect::Sqlite)
            .with_snapshots_table(&self.table_name)?;
        let migrator = crate::schema::SchemaMigrator::new(config);
        let connection = lock_connection(&self.connection);
        migrator.run_sqlite(&connection)
    }
}

impl<A> SnapshotStore<A> for SqliteSnapshotStore<A>
where
    A: Aggregate + serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
    A::Id: serde::Serialize + serde::de::DeserializeOwned,
{
    type Error = EventStoreError;

    fn load_snapshot(&self, aggregate_id: &A::Id) -> Result<Option<Snapshot<A>>, Self::Error> {
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!(
            "snapshot.load",
            dialect = "sqlite",
            aggregate_type = A::aggregate_type()
        )
        .entered();

        let aggregate_id = serialize_id(aggregate_id)?;
        let connection = lock_connection(&self.connection);
        let sql = format!(
            "SELECT revision, state, metadata, recorded_at_ms FROM {} \
             WHERE aggregate_type = ?1 AND aggregate_id = ?2;",
            self.table_name
        );
        let row = connection
            .query_row(&sql, params![A::aggregate_type(), aggregate_id], |row| {
                let revision: i64 = row.get(0)?;
                let state: String = row.get(1)?;
                let metadata: String = row.get(2)?;
                let recorded_at_ms: i64 = row.get(3)?;
                Ok((revision, state, metadata, recorded_at_ms))
            })
            .optional()
            .map_err(map_sqlite_error)?;

        let Some((revision, state, metadata, recorded_at_ms)) = row else {
            return Ok(None);
        };

        let revision = u64::try_from(revision).map_err(|_| {
            EventStoreError::deserialization(
                "SQLite snapshot revision cannot be negative".to_owned(),
            )
        })?;
        let state = serde_json::from_str(&state).map_err(|error| {
            EventStoreError::deserialization(format!("snapshot state JSON: {error}"))
        })?;
        let metadata = serde_json::from_str(&metadata).map_err(|error| {
            EventStoreError::deserialization(format!("snapshot metadata JSON: {error}"))
        })?;
        let recorded_at = millis_to_system_time(recorded_at_ms)?;
        let aggregate_id = deserialize_id(&aggregate_id)?;

        Ok(Some(Snapshot {
            aggregate_id,
            aggregate_type: A::aggregate_type().to_owned(),
            revision,
            state,
            metadata,
            recorded_at,
        }))
    }

    fn save_snapshot(&self, snapshot: Snapshot<A>) -> Result<(), Self::Error> {
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!(
            "snapshot.save",
            dialect = "sqlite",
            aggregate_type = A::aggregate_type(),
            revision = snapshot.revision
        )
        .entered();

        let aggregate_id = serialize_id(&snapshot.aggregate_id)?;
        let revision_i64 = i64::try_from(snapshot.revision).map_err(|_| {
            EventStoreError::serialization("snapshot revision exceeds i64".to_owned())
        })?;
        let state_json = serde_json::to_string(&snapshot.state).map_err(|error| {
            EventStoreError::serialization(format!("snapshot state JSON: {error}"))
        })?;
        let metadata_json = serde_json::to_string(&snapshot.metadata).map_err(|error| {
            EventStoreError::serialization(format!("snapshot metadata JSON: {error}"))
        })?;
        let recorded_at_ms = system_time_to_millis(snapshot.recorded_at)?;
        let connection = lock_connection(&self.connection);
        let sql = format!(
            "INSERT INTO {} (aggregate_type, aggregate_id, revision, state, metadata, recorded_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(aggregate_type, aggregate_id) DO UPDATE SET
                revision = excluded.revision,
                state = excluded.state,
                metadata = excluded.metadata,
                recorded_at_ms = excluded.recorded_at_ms
             WHERE excluded.revision >= {}.revision;",
            self.table_name, self.table_name
        );
        let changed = connection
            .execute(
                &sql,
                params![
                    A::aggregate_type(),
                    aggregate_id,
                    revision_i64,
                    state_json,
                    metadata_json,
                    recorded_at_ms,
                ],
            )
            .map_err(map_sqlite_error)?;
        if changed == 0 {
            let current_sql = format!(
                "SELECT revision FROM {} WHERE aggregate_type = ?1 AND aggregate_id = ?2;",
                self.table_name
            );
            if let Some(current) = connection
                .query_row(
                    &current_sql,
                    params![A::aggregate_type(), aggregate_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map_err(map_sqlite_error)?
            {
                let current = u64::try_from(current).map_err(|_| {
                    EventStoreError::deserialization(
                        "SQLite snapshot revision cannot be negative".to_owned(),
                    )
                })?;
                if snapshot.revision < current {
                    return Err(crate::sql_common::stale_snapshot_revision_error(
                        snapshot.revision,
                        current,
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl<A> crate::async_api::AsyncSnapshotStore<A> for SqliteSnapshotStore<A>
where
    A: Aggregate + serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
    A::Id: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
{
    type Error = EventStoreError;

    async fn load_snapshot(
        &self,
        aggregate_id: &A::Id,
    ) -> Result<Option<Snapshot<A>>, Self::Error> {
        let this = self.clone();
        let aggregate_id = aggregate_id.clone();
        tokio::task::spawn_blocking(move || SnapshotStore::load_snapshot(&this, &aggregate_id))
            .await
            .map_err(|e| EventStoreError::backend(e.to_string()))?
    }

    async fn save_snapshot(&self, snapshot: Snapshot<A>) -> Result<(), Self::Error> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || SnapshotStore::save_snapshot(&this, snapshot))
            .await
            .map_err(|e| EventStoreError::backend(e.to_string()))?
    }
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl<V> crate::async_api::AsyncIdempotencyStore<V> for SqliteIdempotencyStore<V>
where
    V: Clone + serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
{
    type Error = EventStoreError;

    async fn load(&self, key: &IdempotencyKey) -> Result<Option<IdempotencyState<V>>, Self::Error> {
        let this = self.clone();
        let key = key.clone();
        tokio::task::spawn_blocking(move || IdempotencyStore::load(&this, &key))
            .await
            .map_err(|e| EventStoreError::backend(e.to_string()))?
    }

    async fn reserve(&self, key: IdempotencyKey) -> Result<bool, Self::Error> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || IdempotencyStore::reserve(&this, key))
            .await
            .map_err(|e| EventStoreError::backend(e.to_string()))?
    }

    async fn reserve_with_lease(
        &self,
        key: IdempotencyKey,
        config: &crate::idempotency::IdempotencyLeaseConfig,
    ) -> Result<bool, Self::Error> {
        let this = self.clone();
        let config = config.clone();
        tokio::task::spawn_blocking(move || {
            IdempotencyStore::reserve_with_lease(&this, key, &config)
        })
        .await
        .map_err(|e| EventStoreError::backend(e.to_string()))?
    }

    async fn heartbeat(&self, key: &IdempotencyKey, owner: &str) -> Result<bool, Self::Error> {
        let this = self.clone();
        let key = key.clone();
        let owner = owner.to_owned();
        tokio::task::spawn_blocking(move || IdempotencyStore::heartbeat(&this, &key, &owner))
            .await
            .map_err(|e| EventStoreError::backend(e.to_string()))?
    }

    async fn expire_stale_pending(&self, now_ms: u64) -> Result<usize, Self::Error> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || IdempotencyStore::expire_stale_pending(&this, now_ms))
            .await
            .map_err(|e| EventStoreError::backend(e.to_string()))?
    }

    async fn save(&self, key: IdempotencyKey, value: V) -> Result<(), Self::Error> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || IdempotencyStore::save(&this, key, value))
            .await
            .map_err(|e| EventStoreError::backend(e.to_string()))?
    }

    async fn remove(&self, key: &IdempotencyKey) -> Result<(), Self::Error> {
        let this = self.clone();
        let key = key.clone();
        tokio::task::spawn_blocking(move || IdempotencyStore::remove(&this, &key))
            .await
            .map_err(|e| EventStoreError::backend(e.to_string()))?
    }
}
