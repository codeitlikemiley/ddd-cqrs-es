//! MySQL event store adapter.

use crate::aggregate::Aggregate;
use crate::error::EventStoreError;
use crate::event::{EventEnvelope, EventId, ExpectedRevision, NewEvent};
use crate::event_store::{
    AtomicIdempotentEventStore, EventStore, EventStream, IdempotentAppendError,
};
use crate::idempotency::{
    IdempotencyKey, IdempotencyLeaseConfig, IdempotencyState, IdempotencyStore,
};
use crate::pool::{resolve_pool_size, ConnectionPool};
use crate::projection::CheckpointStore;
use crate::snapshot::{Snapshot, SnapshotStore};
use crate::sql_common::{
    aggregate_id_lookup_keys, check_expected_revision, deserialize_id, deserialize_metadata,
    deserialize_payload, max_revision_for_lookup_keys, millis_to_system_time, serialize_id,
    serialize_metadata, serialize_payload, system_time_to_millis, validate_table_name,
};
use crate::upcast::UpcasterRegistry;
use mysql::prelude::*;
use mysql::{Conn, Error as MySqlError, Opts, Row, TxOpts};
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::time::SystemTime;

fn validate_mysql_conn(conn: &mut Conn) -> bool {
    conn.ping().is_ok()
}

fn mysql_connect(url: &str) -> Result<Conn, EventStoreError> {
    let opts = Opts::from_url(url).map_err(|e| EventStoreError::backend(e.to_string()))?;
    Conn::new(opts).map_err(map_mysql_error)
}

fn mysql_pool(url: &str) -> ConnectionPool<Conn> {
    let url = url.to_owned();
    ConnectionPool::pooled_validated(1, None, move || mysql_connect(&url), validate_mysql_conn)
}

fn mysql_pool_seeded(connection: Conn, url: &str) -> ConnectionPool<Conn> {
    let url = url.to_owned();
    ConnectionPool::pooled_validated(
        1,
        Some(connection),
        move || mysql_connect(&url),
        validate_mysql_conn,
    )
}

fn mysql_pooled(max_size: usize, url: &str) -> ConnectionPool<Conn> {
    let url = url.to_owned();
    ConnectionPool::pooled_validated(
        resolve_pool_size(Some(max_size)),
        None,
        move || mysql_connect(&url),
        validate_mysql_conn,
    )
}

/// MySQL-backed event store.
pub struct MySqlEventStore<A>
where
    A: Aggregate,
{
    pool: ConnectionPool<Conn>,
    table_name: String,
    idempotency_table: String,
    upcasters: UpcasterRegistry,
    _marker: PhantomData<fn() -> A>,
}

impl<A> Clone for MySqlEventStore<A>
where
    A: Aggregate,
{
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            table_name: self.table_name.clone(),
            idempotency_table: self.idempotency_table.clone(),
            upcasters: self.upcasters.clone(),
            _marker: PhantomData,
        }
    }
}

impl<A> std::fmt::Debug for MySqlEventStore<A>
where
    A: Aggregate,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MySqlEventStore")
            .field("table_name", &self.table_name)
            .field("idempotency_table", &self.idempotency_table)
            .finish_non_exhaustive()
    }
}

impl<A> MySqlEventStore<A>
where
    A: Aggregate,
{
    /// Connects to MySQL using standard URL params and the default `events` table.
    pub fn connect(url: &str) -> Result<Self, EventStoreError> {
        Self::connect_with_table_name(url, "events")
    }

    /// Connects to MySQL using standard URL params and a custom table name.
    pub fn connect_with_table_name(
        url: &str,
        table_name: impl Into<String>,
    ) -> Result<Self, EventStoreError> {
        let table_name = table_name.into();
        validate_table_name(&table_name)?;
        validate_table_name("idempotency_keys")?;

        Ok(Self::with_table_names_impl(
            mysql_pool(url),
            table_name,
            "idempotency_keys".to_owned(),
        ))
    }

    /// Creates a MySQL event store using the default `events` table.
    ///
    /// **Test-only.** The wrapped connection cannot be replaced when it goes
    /// stale; prefer [`Self::connect`] or [`Self::with_client`] in application
    /// code.
    pub fn new(connection: Conn) -> Result<Self, EventStoreError> {
        Self::with_table_name(connection, "events")
    }

    /// Wraps an existing connection in a reconnect-capable size-1 pool.
    pub fn with_client(connection: Conn, url: &str) -> Result<Self, EventStoreError> {
        Self::with_client_and_table_name(connection, url, "events")
    }

    /// Wraps an existing connection in a reconnect-capable size-1 pool.
    pub fn with_client_and_table_name(
        connection: Conn,
        url: &str,
        table_name: impl Into<String>,
    ) -> Result<Self, EventStoreError> {
        let table_name = table_name.into();
        validate_table_name(&table_name)?;
        validate_table_name("idempotency_keys")?;

        Ok(Self::with_table_names_impl(
            mysql_pool_seeded(connection, url),
            table_name,
            "idempotency_keys".to_owned(),
        ))
    }

    /// Creates a MySQL event store with a custom table name.
    pub fn with_table_name(
        connection: Conn,
        table_name: impl Into<String>,
    ) -> Result<Self, EventStoreError> {
        Self::with_table_names(connection, table_name, "idempotency_keys")
    }

    /// Creates a MySQL event store with custom event and idempotency table names.
    pub fn with_table_names(
        connection: Conn,
        table_name: impl Into<String>,
        idempotency_table: impl Into<String>,
    ) -> Result<Self, EventStoreError> {
        let table_name = table_name.into();
        let idempotency_table = idempotency_table.into();
        validate_table_name(&table_name)?;
        validate_table_name(&idempotency_table)?;

        Ok(Self {
            pool: ConnectionPool::single(connection),
            table_name,
            idempotency_table,
            upcasters: UpcasterRegistry::new(),
            _marker: PhantomData,
        })
    }

    /// Connects a bounded connection pool using standard URL params.
    ///
    /// Pool size resolves from `DDD_CQRS_ES_POOL_SIZE`, or the CPU count
    /// clamped to `[2, 8]` when the variable is unset.
    pub fn connect_pooled(url: &str) -> Result<Self, EventStoreError> {
        Self::connect_pooled_with_table_name(url, "events", resolve_pool_size(None))
    }

    /// Connects a bounded pool with explicit size and custom table name.
    ///
    /// The size is clamped to `[1, 128]`; connections are opened lazily up to
    /// that bound and reused across operations.
    pub fn connect_pooled_with_table_name(
        url: &str,
        table_name: impl Into<String>,
        max_size: usize,
    ) -> Result<Self, EventStoreError> {
        let table_name = table_name.into();
        validate_table_name(&table_name)?;
        validate_table_name("idempotency_keys")?;

        let store = Self::with_table_names_impl(
            mysql_pooled(max_size, url),
            table_name,
            "idempotency_keys".to_owned(),
        );
        Ok(store)
    }

    fn with_table_names_impl(
        pool: ConnectionPool<Conn>,
        table_name: String,
        idempotency_table: String,
    ) -> Self {
        Self {
            pool,
            table_name,
            idempotency_table,
            upcasters: UpcasterRegistry::new(),
            _marker: PhantomData,
        }
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

    /// Migrates the MySQL schemas to the latest version.
    pub fn migrate_schema(&self) -> Result<(), EventStoreError> {
        let config = crate::schema::SqlSchemaConfig::new(crate::schema::SqlDialect::MySql)
            .with_events_table(&self.table_name)?
            .with_idempotency_table(&self.idempotency_table)?;
        let migrator = crate::schema::SchemaMigrator::new(config);
        self.pool.write(|connection| migrator.run_mysql(connection))
    }

    /// Initializes the MySQL event table and indexes.
    pub fn initialize_schema(&self) -> Result<(), EventStoreError> {
        self.migrate_schema()
    }

    /// Creates a checkpoint store that shares this event store's connection
    /// pool, using the default table name.
    pub fn checkpoint_store(&self) -> Result<MySqlCheckpointStore, EventStoreError> {
        self.checkpoint_store_with_table_name("projection_checkpoints")
    }

    /// Creates a pool-sharing checkpoint store with a custom table name.
    pub fn checkpoint_store_with_table_name(
        &self,
        table_name: impl Into<String>,
    ) -> Result<MySqlCheckpointStore, EventStoreError> {
        MySqlCheckpointStore::from_pool(self.pool.clone(), table_name)
    }

    /// Creates an idempotency store that shares this event store's connection
    /// pool, using the event store's idempotency table.
    pub fn idempotency_store<V>(&self) -> Result<MySqlIdempotencyStore<V>, EventStoreError>
    where
        V: Clone,
    {
        self.idempotency_store_with_table_name(self.idempotency_table.clone())
    }

    /// Creates a pool-sharing idempotency store with a custom table name.
    pub fn idempotency_store_with_table_name<V>(
        &self,
        table_name: impl Into<String>,
    ) -> Result<MySqlIdempotencyStore<V>, EventStoreError>
    where
        V: Clone,
    {
        MySqlIdempotencyStore::from_pool(self.pool.clone(), table_name)
    }

    /// Creates a snapshot store that shares this event store's connection
    /// pool, using the default table name.
    pub fn snapshot_store(&self) -> Result<MySqlSnapshotStore<A>, EventStoreError> {
        self.snapshot_store_with_table_name("snapshots")
    }

    /// Creates a pool-sharing snapshot store with a custom table name.
    pub fn snapshot_store_with_table_name(
        &self,
        table_name: impl Into<String>,
    ) -> Result<MySqlSnapshotStore<A>, EventStoreError> {
        MySqlSnapshotStore::from_pool(self.pool.clone(), table_name)
    }
}

impl<A> EventStore<A> for MySqlEventStore<A>
where
    A: Aggregate + 'static,
    A::Event: serde::Serialize + serde::de::DeserializeOwned,
    A::Id: serde::Serialize + serde::de::DeserializeOwned,
{
    type Error = EventStoreError;

    fn load(&self, aggregate_id: &A::Id) -> Result<EventStream<A>, Self::Error> {
        let keys = aggregate_id_lookup_keys(aggregate_id)?;
        let table_name = self.table_name.clone();
        let upcasters = self.upcasters.clone();
        self.pool.read(move |connection| {
            let query = format!(
                "SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, \
                 event_version, payload, metadata, recorded_at_ms FROM {table} \
                 WHERE aggregate_type = ? AND aggregate_id = ? ORDER BY revision ASC",
                table = table_name
            );
            let aggregate_type = A::aggregate_type();
            let mut rows = Vec::new();
            for key in &keys {
                rows = connection
                    .exec(&query, (aggregate_type, key))
                    .map_err(map_mysql_error)?;
                if !rows.is_empty() {
                    break;
                }
            }

            rows.into_iter()
                .map(|row| row_to_envelope::<A>(&upcasters, row))
                .collect()
        })
    }

    fn load_after_revision(
        &self,
        aggregate_id: &A::Id,
        revision: u64,
    ) -> Result<EventStream<A>, Self::Error> {
        let revision_i64 = i64::try_from(revision)
            .map_err(|_| EventStoreError::serialization("revision exceeds i64".to_owned()))?;
        let keys = aggregate_id_lookup_keys(aggregate_id)?;
        let table_name = self.table_name.clone();
        let upcasters = self.upcasters.clone();
        self.pool.read(move |connection| {
            let query = format!(
                "SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, \
                 event_version, payload, metadata, recorded_at_ms FROM {table} \
                 WHERE aggregate_type = ? AND aggregate_id = ? AND revision > ? \
                 ORDER BY revision ASC",
                table = table_name
            );
            let aggregate_type = A::aggregate_type();
            let mut rows = Vec::new();
            for key in &keys {
                rows = connection
                    .exec(&query, (aggregate_type, key, revision_i64))
                    .map_err(map_mysql_error)?;
                if !rows.is_empty() {
                    break;
                }
            }

            rows.into_iter()
                .map(|row| row_to_envelope::<A>(&upcasters, row))
                .collect()
        })
    }

    fn append(
        &self,
        aggregate_id: &A::Id,
        expected_revision: ExpectedRevision,
        events: Vec<NewEvent<A::Event>>,
    ) -> Result<EventStream<A>, Self::Error> {
        let keys = aggregate_id_lookup_keys(aggregate_id)?;
        let aggregate_id_key = keys[0].clone();
        let prepared = events
            .into_iter()
            .map(PreparedMySqlEvent::new)
            .collect::<Result<Vec<_>, _>>()?;
        let table_name = self.table_name.clone();
        self.pool.write(|connection| {
            let mut transaction = connection
                .start_transaction(TxOpts::default())
                .map_err(map_mysql_error)?;

            let actual_revision = max_revision_for_lookup_keys(&keys, |key| {
                current_revision_mysql(&mut transaction, &table_name, A::aggregate_type(), key)
            })?;
            check_expected_revision(expected_revision, actual_revision)?;

            if prepared.is_empty() {
                transaction.commit().map_err(map_mysql_error)?;
                return Ok(Vec::new());
            }

            let committed = insert_prepared_mysql_events::<A>(
                &mut transaction,
                &table_name,
                aggregate_id,
                &aggregate_id_key,
                actual_revision,
                expected_revision,
                prepared.clone(),
            )?;

            transaction.commit().map_err(map_mysql_error)?;
            Ok(committed)
        })
    }

    fn load_global_after(&self, sequence: Option<u64>) -> Result<EventStream<A>, Self::Error> {
        let sequence = sequence.unwrap_or_default();
        let sequence_i64 = i64::try_from(sequence).map_err(|_| {
            EventStoreError::deserialization("global sequence exceeds BIGINT".to_owned())
        })?;
        let table_name = self.table_name.clone();
        let upcasters = self.upcasters.clone();
        self.pool.read(move |connection| {
            let query = format!(
                "SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, \
                 event_version, payload, metadata, recorded_at_ms FROM {table} \
                 WHERE aggregate_type = ? AND sequence > ? ORDER BY sequence ASC",
                table = table_name
            );
            let rows: Vec<Row> = connection
                .exec(&query, (A::aggregate_type(), sequence_i64))
                .map_err(map_mysql_error)?;

            rows.into_iter()
                .map(|row| row_to_envelope::<A>(&upcasters, row))
                .collect()
        })
    }

    fn load_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: NonZeroUsize,
    ) -> Result<EventStream<A>, Self::Error> {
        let sequence = sequence.unwrap_or_default();
        let sequence_i64 = i64::try_from(sequence).map_err(|_| {
            EventStoreError::deserialization("global sequence exceeds BIGINT".to_owned())
        })?;
        let limit_u64 = u64::try_from(limit.get()).map_err(|_| {
            EventStoreError::deserialization("event replay limit exceeds BIGINT".to_owned())
        })?;
        let table_name = self.table_name.clone();
        let upcasters = self.upcasters.clone();
        self.pool.read(move |connection| {
            let query = format!(
                "SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, \
                 event_version, payload, metadata, recorded_at_ms FROM {table} \
                 WHERE aggregate_type = ? AND sequence > ? ORDER BY sequence ASC LIMIT ?",
                table = table_name
            );
            let rows: Vec<Row> = connection
                .exec(&query, (A::aggregate_type(), sequence_i64, limit_u64))
                .map_err(map_mysql_error)?;

            rows.into_iter()
                .map(|row| row_to_envelope::<A>(&upcasters, row))
                .collect()
        })
    }
}

impl<A> crate::raw_feed::RawEventFeed for MySqlEventStore<A>
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
            EventStoreError::deserialization("global sequence exceeds BIGINT".to_owned())
        })?;
        let limit_u64 = u64::try_from(limit.get()).map_err(|_| {
            EventStoreError::deserialization("event replay limit exceeds BIGINT".to_owned())
        })?;
        let table_name = self.table_name.clone();
        let upcasters = self.upcasters.clone();
        self.pool.read(move |connection| {
            let query = format!(
                "SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, \
                 event_version, payload, metadata, recorded_at_ms FROM {table} \
                 WHERE sequence > ? ORDER BY sequence ASC LIMIT ?",
                table = table_name
            );
            let rows: Vec<Row> = connection
                .exec(&query, (sequence_i64, limit_u64))
                .map_err(map_mysql_error)?;

            rows.into_iter()
                .map(|row| row_to_raw_envelope(&upcasters, row))
                .collect()
        })
    }
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl<A> crate::raw_feed::AsyncRawEventFeed for MySqlEventStore<A>
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

impl<A> AtomicIdempotentEventStore<A> for MySqlEventStore<A>
where
    A: Aggregate + 'static,
    A::Event: serde::Serialize + serde::de::DeserializeOwned,
    A::Id: serde::Serialize + serde::de::DeserializeOwned,
{
    fn load_idempotent(
        &self,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<IdempotencyState<EventStream<A>>>, Self::Error> {
        let idempotency_table = self.idempotency_table.clone();
        let key = idempotency_key.as_str().to_owned();
        self.pool.read(move |connection| {
            let query = format!(
                "SELECT state, value FROM {table} WHERE idempotency_key = ?;",
                table = idempotency_table
            );
            let row: Option<Row> = connection
                .exec_first(&query, (&key,))
                .map_err(map_mysql_error)?;

            row.map(|row| {
                let state: String = row.get(0).ok_or_else(|| {
                    EventStoreError::deserialization("missing state column".to_owned())
                })?;
                let value: Option<String> = row.get::<Option<String>, _>(1).flatten();
                match (state.as_str(), value) {
                    ("pending", _) => crate::idempotency::pending_state_from_row(
                        None,
                        None,
                        crate::idempotency::now_ms(),
                    )
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
                }
            })
            .transpose()
        })
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
            dialect = "mysql",
            aggregate_type = A::aggregate_type(),
            expected_revision = ?expected_revision,
            event_count = events.len()
        )
        .entered();

        let aggregate_id_key = serialize_id(aggregate_id).map_err(IdempotentAppendError::Store)?;
        let prepared = events
            .into_iter()
            .map(PreparedMySqlEvent::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(IdempotentAppendError::Store)?;
        let table_name = self.table_name.clone();
        let idempotency_table = self.idempotency_table.clone();
        let aggregate_id = aggregate_id.clone();

        let committed = self
            .pool
            .write(|connection| {
                run_idempotent_append::<A>(
                    connection,
                    &table_name,
                    &idempotency_table,
                    &idempotency_key,
                    &aggregate_id,
                    &aggregate_id_key,
                    expected_revision,
                    prepared.clone(),
                )
            })
            .map_err(IdempotentAppendError::Store)??;
        Ok(committed)
    }
}

#[allow(clippy::too_many_arguments)]
fn run_idempotent_append<A>(
    connection: &mut Conn,
    table_name: &str,
    idempotency_table: &str,
    idempotency_key: &IdempotencyKey,
    aggregate_id: &A::Id,
    aggregate_id_key: &str,
    expected_revision: ExpectedRevision,
    prepared: Vec<PreparedMySqlEvent<A::Event>>,
) -> Result<Result<EventStream<A>, IdempotentAppendError<EventStoreError>>, EventStoreError>
where
    A: Aggregate + 'static,
    A::Event: serde::Serialize + serde::de::DeserializeOwned,
    A::Id: serde::Serialize + serde::de::DeserializeOwned,
{
    let mut transaction = connection
        .start_transaction(TxOpts::default())
        .map_err(map_mysql_error)?;

    let load_idempotency =
        format!("SELECT state, value FROM {idempotency_table} WHERE idempotency_key = ?;");
    let row_opt: Option<Row> = transaction
        .exec_first(&load_idempotency, (idempotency_key.as_str(),))
        .map_err(map_mysql_error)?;

    if let Some(row) = row_opt {
        let state: String = row
            .get(0)
            .ok_or_else(|| EventStoreError::deserialization("missing state column".to_owned()))?;
        let value: Option<String> = row.get::<Option<String>, _>(1).flatten();
        match (state.as_str(), value) {
            ("complete", Some(value)) => {
                let committed = serde_json::from_str(&value).map_err(|error| {
                    EventStoreError::deserialization(format!(
                        "idempotent committed events JSON: {error}"
                    ))
                })?;
                transaction.commit().map_err(map_mysql_error)?;
                return Ok(Ok(committed));
            }
            ("complete", None) => {
                return Ok(Err(IdempotentAppendError::Store(
                    EventStoreError::deserialization(
                        "completed idempotency row is missing value".to_owned(),
                    ),
                )));
            }
            ("pending", _) => {
                return Ok(Err(IdempotentAppendError::Pending {
                    key: idempotency_key.clone(),
                }));
            }
            (state, _) => {
                return Ok(Err(IdempotentAppendError::Store(
                    EventStoreError::deserialization(format!("unknown idempotency state: {state}")),
                )));
            }
        }
    }

    let updated_at_ms = system_time_to_millis(SystemTime::now())?;
    let reserve = format!(
        "INSERT INTO {idempotency_table} (idempotency_key, state, value, updated_at_ms) \
         VALUES (?, 'pending', NULL, ?);"
    );
    if let Err(error) = transaction.exec_drop(&reserve, (idempotency_key.as_str(), updated_at_ms)) {
        // Another connection reserved the same key between our SELECT and this
        // INSERT. Report Pending so the caller's wait loop re-polls and finds
        // the winner's committed value instead of surfacing a fatal error.
        if matches!(&error, MySqlError::MySqlError(e) if e.code == 1062) {
            return Ok(Err(IdempotentAppendError::Pending {
                key: idempotency_key.clone(),
            }));
        }
        return Err(map_mysql_error(error));
    }

    let revision_query = format!(
        "SELECT COALESCE(MAX(revision), 0) FROM {table} \
         WHERE aggregate_type = ? AND aggregate_id = ?",
        table = table_name
    );
    let actual_revision: i64 = transaction
        .exec_first(&revision_query, (A::aggregate_type(), aggregate_id_key))
        .map_err(map_mysql_error)?
        .unwrap_or(0);
    let actual_revision = u64::try_from(actual_revision).map_err(|_| {
        EventStoreError::deserialization("stored revision cannot be negative".to_owned())
    })?;
    check_expected_revision(expected_revision, actual_revision)?;

    let committed = insert_prepared_mysql_events::<A>(
        &mut transaction,
        table_name,
        aggregate_id,
        aggregate_id_key,
        actual_revision,
        expected_revision,
        prepared,
    )?;

    let value_json = serde_json::to_string(&committed).map_err(|error| {
        EventStoreError::serialization(format!("idempotent committed events JSON: {error}"))
    })?;
    let complete = format!(
        "UPDATE {idempotency_table} SET state = 'complete', value = ?, updated_at_ms = ?
         WHERE idempotency_key = ?;"
    );
    transaction
        .exec_drop(
            &complete,
            (value_json, updated_at_ms, idempotency_key.as_str()),
        )
        .map_err(map_mysql_error)?;
    transaction.commit().map_err(map_mysql_error)?;
    Ok(Ok(committed))
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl<A> crate::async_api::AsyncEventStore<A> for MySqlEventStore<A>
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
            .map_err(|e| EventStoreError::backend(e.to_string()))?
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
        .map_err(|e| EventStoreError::backend(e.to_string()))?
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
        .map_err(|e| EventStoreError::backend(e.to_string()))?
    }

    async fn load_global_after(
        &self,
        sequence: Option<u64>,
    ) -> Result<EventStream<A>, Self::Error> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || EventStore::load_global_after(&this, sequence))
            .await
            .map_err(|e| EventStoreError::backend(e.to_string()))?
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
        .map_err(|e| EventStoreError::backend(e.to_string()))?
    }
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl<A> crate::async_api::AsyncAtomicIdempotentEventStore<A> for MySqlEventStore<A>
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

#[derive(Clone)]
struct PreparedMySqlEvent<E> {
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

impl<E> PreparedMySqlEvent<E>
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

/// Inserts all prepared events with one multi-row `INSERT`, then reads the
/// assigned sequences back in a second query, returning the committed
/// envelopes. Two round trips regardless of event count.
///
/// Sequences are read back instead of derived from `last_insert_id` because
/// InnoDB's interleaved auto-increment lock mode does not guarantee a
/// consecutive block for every statement shape.
fn insert_prepared_mysql_events<A>(
    transaction: &mut mysql::Transaction<'_>,
    table_name: &str,
    aggregate_id: &A::Id,
    aggregate_id_key: &str,
    actual_revision: u64,
    expected_revision: ExpectedRevision,
    prepared: Vec<PreparedMySqlEvent<A::Event>>,
) -> Result<EventStream<A>, EventStoreError>
where
    A: Aggregate,
{
    let count = prepared.len();
    let actual_revision_i64 = i64::try_from(actual_revision)
        .map_err(|_| EventStoreError::serialization("revision exceeds BIGINT".to_owned()))?;
    let mut params: Vec<mysql::Value> = Vec::with_capacity(count * 9);
    for (index, event) in prepared.iter().enumerate() {
        let revision = actual_revision + index as u64 + 1;
        let revision_i64 = i64::try_from(revision)
            .map_err(|_| EventStoreError::serialization("revision exceeds BIGINT".to_owned()))?;
        let event_version_i32 = i32::try_from(event.event_version)
            .map_err(|_| EventStoreError::serialization("event_version exceeds i32".to_owned()))?;
        params.push(event.event_id.as_str().into());
        params.push(aggregate_id_key.into());
        params.push(A::aggregate_type().into());
        params.push(revision_i64.into());
        params.push(event.event_type.as_str().into());
        params.push(event_version_i32.into());
        params.push(event.payload_json.as_str().into());
        params.push(event.metadata_json.as_str().into());
        params.push(event.recorded_at_ms.into());
    }

    let placeholders = vec!["(?, ?, ?, ?, ?, ?, ?, ?, ?)"; count].join(", ");
    let insert = format!(
        "INSERT INTO {table} \
         (event_id, aggregate_id, aggregate_type, revision, event_type, event_version, \
          payload, metadata, recorded_at_ms) \
         VALUES {placeholders}",
        table = table_name
    );
    transaction.exec_drop(&insert, params).map_err(|error| {
        map_mysql_insert_error(error, expected_revision, actual_revision, || {
            current_revision_mysql(
                transaction,
                table_name,
                A::aggregate_type(),
                aggregate_id_key,
            )
        })
    })?;

    let select = format!(
        "SELECT revision, sequence FROM {table} \
         WHERE aggregate_type = ? AND aggregate_id = ? AND revision > ?",
        table = table_name
    );
    let rows: Vec<(i64, u64)> = transaction
        .exec(
            &select,
            (A::aggregate_type(), aggregate_id_key, actual_revision_i64),
        )
        .map_err(map_mysql_error)?;
    if rows.len() != count {
        return Err(EventStoreError::backend(format!(
            "multi-row insert returned {} of {} sequences",
            rows.len(),
            count
        )));
    }
    let sequences: std::collections::HashMap<i64, u64> = rows.into_iter().collect();

    let mut committed = Vec::with_capacity(count);
    for (index, event) in prepared.into_iter().enumerate() {
        let revision = actual_revision + index as u64 + 1;
        let revision_i64 = i64::try_from(revision)
            .map_err(|_| EventStoreError::serialization("revision exceeds BIGINT".to_owned()))?;
        let sequence = sequences.get(&revision_i64).copied().ok_or_else(|| {
            EventStoreError::backend(format!(
                "multi-row insert returned no sequence for revision {revision}"
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

    Ok(committed)
}

/// Maps a full event row into an untyped envelope, applying upcasters but
/// keeping the payload as raw JSON and the aggregate id as its stored string.
fn row_to_raw_envelope(
    upcasters: &UpcasterRegistry,
    row: Row,
) -> Result<crate::raw_feed::RawEventEnvelope, EventStoreError> {
    let event_id: String = row
        .get(0)
        .ok_or_else(|| EventStoreError::deserialization("missing event_id column".to_owned()))?;
    let aggregate_id: String = row.get(1).ok_or_else(|| {
        EventStoreError::deserialization("missing aggregate_id column".to_owned())
    })?;
    let aggregate_type: String = row.get(2).ok_or_else(|| {
        EventStoreError::deserialization("missing aggregate_type column".to_owned())
    })?;
    let revision: i64 = row
        .get(3)
        .ok_or_else(|| EventStoreError::deserialization("missing revision column".to_owned()))?;
    let sequence: u64 = row
        .get(4)
        .ok_or_else(|| EventStoreError::deserialization("missing sequence column".to_owned()))?;
    let event_type: String = row
        .get(5)
        .ok_or_else(|| EventStoreError::deserialization("missing event_type column".to_owned()))?;
    let event_version: i32 = row.get(6).ok_or_else(|| {
        EventStoreError::deserialization("missing event_version column".to_owned())
    })?;
    let payload_str: String = row
        .get(7)
        .ok_or_else(|| EventStoreError::deserialization("missing payload column".to_owned()))?;
    let metadata_str: String = row
        .get(8)
        .ok_or_else(|| EventStoreError::deserialization("missing metadata column".to_owned()))?;
    let recorded_at_ms: i64 = row.get(9).ok_or_else(|| {
        EventStoreError::deserialization("missing recorded_at_ms column".to_owned())
    })?;

    let revision = u64::try_from(revision).map_err(|_| {
        EventStoreError::deserialization("stored revision cannot be negative".to_owned())
    })?;
    let event_version = u32::try_from(event_version).map_err(|_| {
        EventStoreError::deserialization("event_version cannot be negative".to_owned())
    })?;

    let (event_version, upcasted_bytes) = upcasters
        .prepare_payload(&event_type, event_version, payload_str.into_bytes())
        .map_err(|err| EventStoreError::deserialization(err.to_string()))?;
    let payload: serde_json::Value = serde_json::from_slice(&upcasted_bytes)
        .map_err(|error| EventStoreError::deserialization(format!("payload JSON: {error}")))?;

    let metadata_val: serde_json::Value = serde_json::from_str(&metadata_str)
        .map_err(|error| EventStoreError::deserialization(format!("metadata JSON: {error}")))?;
    let metadata = deserialize_metadata(&event_id, metadata_val)?;
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

fn row_to_envelope<A>(
    upcasters: &UpcasterRegistry,
    row: Row,
) -> Result<EventEnvelope<A::Event, A::Id>, EventStoreError>
where
    A: Aggregate,
    A::Event: serde::de::DeserializeOwned,
    A::Id: serde::de::DeserializeOwned,
{
    let event_id: String = row
        .get(0)
        .ok_or_else(|| EventStoreError::deserialization("missing event_id column".to_owned()))?;
    let aggregate_id: String = row.get(1).ok_or_else(|| {
        EventStoreError::deserialization("missing aggregate_id column".to_owned())
    })?;
    let aggregate_type: String = row.get(2).ok_or_else(|| {
        EventStoreError::deserialization("missing aggregate_type column".to_owned())
    })?;
    let revision: i64 = row
        .get(3)
        .ok_or_else(|| EventStoreError::deserialization("missing revision column".to_owned()))?;
    let sequence: u64 = row
        .get(4)
        .ok_or_else(|| EventStoreError::deserialization("missing sequence column".to_owned()))?;
    let event_type: String = row
        .get(5)
        .ok_or_else(|| EventStoreError::deserialization("missing event_type column".to_owned()))?;
    let event_version: i32 = row.get(6).ok_or_else(|| {
        EventStoreError::deserialization("missing event_version column".to_owned())
    })?;
    let payload_str: String = row
        .get(7)
        .ok_or_else(|| EventStoreError::deserialization("missing payload column".to_owned()))?;
    let metadata_str: String = row
        .get(8)
        .ok_or_else(|| EventStoreError::deserialization("missing metadata column".to_owned()))?;
    let recorded_at_ms: i64 = row.get(9).ok_or_else(|| {
        EventStoreError::deserialization("missing recorded_at_ms column".to_owned())
    })?;

    let revision = u64::try_from(revision).map_err(|_| {
        EventStoreError::deserialization("stored revision cannot be negative".to_owned())
    })?;
    let event_version = u32::try_from(event_version).map_err(|_| {
        EventStoreError::deserialization("event_version cannot be negative".to_owned())
    })?;
    let aggregate_id = deserialize_id(&aggregate_id)?;

    let payload_val: serde_json::Value = serde_json::from_str(&payload_str)
        .map_err(|error| EventStoreError::deserialization(format!("payload JSON: {error}")))?;

    let (event_version, payload) = if upcasters.is_empty() || !upcasters.has_upcasters(&event_type)
    {
        (event_version, payload_val)
    } else {
        let payload_bytes = serde_json::to_vec(&payload_val).map_err(|error| {
            EventStoreError::deserialization(format!(
                "payload serialization for upcasting failed: {error}"
            ))
        })?;
        let (event_version, upcasted_bytes) = upcasters
            .prepare_payload(&event_type, event_version, payload_bytes)
            .map_err(|err| EventStoreError::deserialization(err.to_string()))?;
        let payload = serde_json::from_slice(&upcasted_bytes)
            .map_err(|error| EventStoreError::deserialization(format!("payload JSON: {error}")))?;
        (event_version, payload)
    };

    let payload = deserialize_payload(&event_id, &event_type, payload)?;
    let metadata_val: serde_json::Value = serde_json::from_str(&metadata_str)
        .map_err(|error| EventStoreError::deserialization(format!("metadata JSON: {error}")))?;
    let metadata = deserialize_metadata(&event_id, metadata_val)?;
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

fn map_mysql_error(error: MySqlError) -> EventStoreError {
    let code = match &error {
        MySqlError::MySqlError(server) => Some(server.code.to_string()),
        _ => None,
    };
    let mapped = EventStoreError::backend_with_source(error.to_string(), error);
    match code {
        Some(code) => mapped.with_code(code),
        None => mapped,
    }
}

fn current_revision_mysql(
    transaction: &mut mysql::Transaction<'_>,
    table_name: &str,
    aggregate_type: &str,
    aggregate_id: &str,
) -> Result<u64, EventStoreError> {
    let query = format!(
        "SELECT COALESCE(MAX(revision), 0) FROM {table} \
         WHERE aggregate_type = ? AND aggregate_id = ?",
        table = table_name
    );
    let revision: Option<i64> = transaction
        .exec_first(&query, (aggregate_type, aggregate_id))
        .map_err(map_mysql_error)?;
    let revision = revision.unwrap_or(0);
    u64::try_from(revision).map_err(|_| {
        EventStoreError::deserialization("stored revision cannot be negative".to_owned())
    })
}

fn is_mysql_stream_revision_unique_violation(error: &MySqlError) -> bool {
    match error {
        MySqlError::MySqlError(server) => {
            crate::sql_common::is_mysql_stream_revision_unique_violation_message(&server.message)
        }
        _ => false,
    }
}

fn map_mysql_insert_error(
    error: MySqlError,
    expected_revision: ExpectedRevision,
    stale_actual: u64,
    reread_revision: impl FnOnce() -> Result<u64, EventStoreError>,
) -> EventStoreError {
    match &error {
        MySqlError::MySqlError(e)
            if e.code == 1062 && is_mysql_stream_revision_unique_violation(&error) =>
        {
            let current_revision = reread_revision().unwrap_or(stale_actual);
            crate::sql_common::map_stream_unique_violation(expected_revision, current_revision)
        }
        _ => map_mysql_error(error),
    }
}

/// MySQL checkpoint store implementation.
#[derive(Clone, Debug)]
pub struct MySqlCheckpointStore {
    pool: ConnectionPool<Conn>,
    table_name: String,
}

impl MySqlCheckpointStore {
    /// Creates a MySQL checkpoint store using the default table name.
    pub fn new(connection: Conn) -> Result<Self, EventStoreError> {
        Self::with_table_name(connection, "projection_checkpoints")
    }

    /// Creates a MySQL checkpoint store with a custom table name.
    pub fn with_table_name(
        connection: Conn,
        table_name: impl Into<String>,
    ) -> Result<Self, EventStoreError> {
        Self::from_pool(ConnectionPool::single(connection), table_name)
    }

    fn from_pool(
        pool: ConnectionPool<Conn>,
        table_name: impl Into<String>,
    ) -> Result<Self, EventStoreError> {
        let table_name = table_name.into();
        validate_table_name(&table_name)?;

        let store = Self { pool, table_name };
        store.initialize_schema()?;
        Ok(store)
    }

    /// Initializes the checkpoint schema table.
    pub fn initialize_schema(&self) -> Result<(), EventStoreError> {
        let config = crate::schema::SqlSchemaConfig::new(crate::schema::SqlDialect::MySql)
            .with_checkpoints_table(&self.table_name)?;
        let migrator = crate::schema::SchemaMigrator::for_checkpoints(config);
        self.pool.write(|connection| migrator.run_mysql(connection))
    }
}

impl CheckpointStore for MySqlCheckpointStore {
    type Error = EventStoreError;

    fn load_checkpoint(&self, projection_name: &str) -> Result<Option<u64>, Self::Error> {
        let sql = format!(
            "SELECT sequence FROM {} WHERE projection_name = ?;",
            self.table_name
        );
        self.pool.read(|connection| {
            let row_opt: Option<Row> = connection
                .exec_first(&sql, (projection_name,))
                .map_err(map_mysql_error)?;

            if let Some(row) = row_opt {
                let sequence: u64 = row.get(0).ok_or_else(|| {
                    EventStoreError::deserialization(
                        "missing sequence in checkpoint row".to_owned(),
                    )
                })?;
                Ok(Some(sequence))
            } else {
                Ok(None)
            }
        })
    }

    fn save_checkpoint(&self, projection_name: &str, sequence: u64) -> Result<(), Self::Error> {
        let sql = format!(
            "INSERT INTO {} (projection_name, sequence) VALUES (?, ?) \
             ON DUPLICATE KEY UPDATE sequence = GREATEST(sequence, VALUES(sequence));",
            self.table_name
        );
        self.pool.write(|connection| {
            connection
                .exec_drop(&sql, (projection_name, sequence))
                .map_err(map_mysql_error)?;
            Ok(())
        })
    }

    fn reset_checkpoint(&self, projection_name: &str) -> Result<(), Self::Error> {
        let sql = format!("DELETE FROM {} WHERE projection_name = ?;", self.table_name);
        self.pool.write(|connection| {
            connection
                .exec_drop(&sql, (projection_name,))
                .map_err(map_mysql_error)?;
            Ok(())
        })
    }
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl crate::projection::AsyncCheckpointStore for MySqlCheckpointStore {
    type Error = EventStoreError;

    async fn load_checkpoint(&self, projection_name: &str) -> Result<Option<u64>, Self::Error> {
        let this = self.clone();
        let name = projection_name.to_owned();
        tokio::task::spawn_blocking(move || CheckpointStore::load_checkpoint(&this, &name))
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
            CheckpointStore::save_checkpoint(&this, &name, sequence)
        })
        .await
        .map_err(|e| EventStoreError::backend(e.to_string()))?
    }

    async fn reset_checkpoint(&self, projection_name: &str) -> Result<(), Self::Error> {
        let this = self.clone();
        let name = projection_name.to_owned();
        tokio::task::spawn_blocking(move || CheckpointStore::reset_checkpoint(&this, &name))
            .await
            .map_err(|e| EventStoreError::backend(e.to_string()))?
    }
}

/// MySQL-backed idempotency store.
pub struct MySqlIdempotencyStore<V>
where
    V: Clone,
{
    pool: ConnectionPool<Conn>,
    table_name: String,
    _marker: PhantomData<fn() -> V>,
}

impl<V> Clone for MySqlIdempotencyStore<V>
where
    V: Clone,
{
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            table_name: self.table_name.clone(),
            _marker: PhantomData,
        }
    }
}

impl<V> std::fmt::Debug for MySqlIdempotencyStore<V>
where
    V: Clone,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MySqlIdempotencyStore")
            .field("table_name", &self.table_name)
            .finish_non_exhaustive()
    }
}

impl<V> MySqlIdempotencyStore<V>
where
    V: Clone,
{
    /// Creates a MySQL idempotency store using the default table name.
    pub fn new(connection: Conn) -> Result<Self, EventStoreError> {
        Self::with_table_name(connection, "idempotency_keys")
    }

    /// Creates a MySQL idempotency store with a custom table name.
    pub fn with_table_name(
        connection: Conn,
        table_name: impl Into<String>,
    ) -> Result<Self, EventStoreError> {
        Self::from_pool(ConnectionPool::single(connection), table_name)
    }

    fn from_pool(
        pool: ConnectionPool<Conn>,
        table_name: impl Into<String>,
    ) -> Result<Self, EventStoreError> {
        let table_name = table_name.into();
        validate_table_name(&table_name)?;

        let store = Self {
            pool,
            table_name,
            _marker: PhantomData,
        };
        store.initialize_schema()?;
        Ok(store)
    }

    /// Initializes the idempotency schema table.
    pub fn initialize_schema(&self) -> Result<(), EventStoreError> {
        let config = crate::schema::SqlSchemaConfig::new(crate::schema::SqlDialect::MySql)
            .with_idempotency_table(&self.table_name)?;
        let migrator = crate::schema::SchemaMigrator::for_idempotency(config);
        self.pool.write(|connection| migrator.run_mysql(connection))
    }
}

impl<V> IdempotencyStore<V> for MySqlIdempotencyStore<V>
where
    V: Clone + serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
{
    type Error = EventStoreError;

    fn load(&self, key: &IdempotencyKey) -> Result<Option<IdempotencyState<V>>, Self::Error> {
        let sql = format!(
            "SELECT state, value FROM {} WHERE idempotency_key = ?;",
            self.table_name
        );
        self.pool.read(|connection| {
            let row_opt: Option<Row> = connection
                .exec_first(&sql, (key.as_str(),))
                .map_err(map_mysql_error)?;

            let Some(row) = row_opt else {
                return Ok(None);
            };

            let state: String = row.get(0).ok_or_else(|| {
                EventStoreError::deserialization("missing state column".to_owned())
            })?;
            let value_str: Option<String> = row.get::<Option<String>, _>(1).flatten();

            match (state.as_str(), value_str) {
                ("pending", _) => Ok(crate::idempotency::pending_state_from_row(
                    None,
                    None,
                    crate::idempotency::now_ms(),
                )
                .map(IdempotencyState::Pending)),
                ("complete", Some(value_str)) => {
                    let value = serde_json::from_str(&value_str).map_err(|error| {
                        EventStoreError::deserialization(format!("idempotency value JSON: {error}"))
                    })?;
                    Ok(Some(IdempotencyState::Complete(value)))
                }
                ("complete", None) => Err(EventStoreError::deserialization(
                    "completed idempotency row is missing value".to_owned(),
                )),
                (state, _) => Err(EventStoreError::deserialization(format!(
                    "unknown idempotency state: {state}"
                ))),
            }
        })
    }

    fn reserve(&self, key: IdempotencyKey) -> Result<bool, Self::Error> {
        key.validate_storage_length()
            .map_err(|error| EventStoreError::backend(error.to_string()))?;
        let updated_at_ms = system_time_to_millis(SystemTime::now())?;

        // MySQL reserve uses a no-op ON DUPLICATE KEY UPDATE so overlong keys
        // fail at validation instead of being silently truncated by INSERT IGNORE.
        let sql = format!(
            "INSERT INTO {} (idempotency_key, state, value, updated_at_ms) \
             VALUES (?, 'pending', NULL, ?) \
             ON DUPLICATE KEY UPDATE idempotency_key = idempotency_key;",
            self.table_name
        );
        self.pool.write(|connection| {
            connection
                .exec_drop(&sql, (key.as_str(), updated_at_ms))
                .map_err(map_mysql_error)?;

            let affected = connection.affected_rows();
            Ok(affected == 1)
        })
    }

    fn save(&self, key: IdempotencyKey, value: V) -> Result<(), Self::Error> {
        let updated_at_ms = system_time_to_millis(SystemTime::now())?;
        let value_json = serde_json::to_string(&value).map_err(|error| {
            EventStoreError::serialization(format!("idempotency value JSON: {error}"))
        })?;
        let sql = format!(
            "INSERT INTO {} (idempotency_key, state, value, updated_at_ms) \
             VALUES (?, 'complete', ?, ?) \
             ON DUPLICATE KEY UPDATE \
                state = VALUES(state), \
                value = VALUES(value), \
                updated_at_ms = VALUES(updated_at_ms);",
            self.table_name
        );
        self.pool.write(|connection| {
            connection
                .exec_drop(&sql, (key.as_str(), value_json.as_str(), updated_at_ms))
                .map_err(map_mysql_error)?;
            Ok(())
        })
    }

    fn remove(&self, key: &IdempotencyKey) -> Result<(), Self::Error> {
        let sql = format!("DELETE FROM {} WHERE idempotency_key = ?;", self.table_name);
        self.pool.write(|connection| {
            connection
                .exec_drop(&sql, (key.as_str(),))
                .map_err(map_mysql_error)?;
            Ok(())
        })
    }

    fn reserve_with_lease(
        &self,
        key: IdempotencyKey,
        _config: &IdempotencyLeaseConfig,
    ) -> Result<bool, Self::Error> {
        IdempotencyStore::reserve(self, key)
    }

    fn heartbeat(&self, _key: &IdempotencyKey, _owner: &str) -> Result<bool, Self::Error> {
        Ok(false)
    }

    fn expire_stale_pending(&self, _now_ms: u64) -> Result<usize, Self::Error> {
        Ok(0)
    }

    fn expire_completed_before(&self, cutoff_ms: u64) -> Result<usize, Self::Error> {
        let sql = format!(
            "DELETE FROM {} WHERE state = 'complete' AND updated_at_ms < ?;",
            self.table_name
        );
        self.pool.write(|connection| {
            connection
                .exec_drop(&sql, (cutoff_ms,))
                .map_err(map_mysql_error)?;
            Ok(connection.affected_rows() as usize)
        })
    }
}

/// MySQL-backed durable snapshot store.
pub struct MySqlSnapshotStore<A>
where
    A: Aggregate,
{
    pool: ConnectionPool<Conn>,
    table_name: String,
    _marker: PhantomData<fn() -> A>,
}

impl<A> Clone for MySqlSnapshotStore<A>
where
    A: Aggregate,
{
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            table_name: self.table_name.clone(),
            _marker: PhantomData,
        }
    }
}

impl<A> std::fmt::Debug for MySqlSnapshotStore<A>
where
    A: Aggregate,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MySqlSnapshotStore")
            .field("table_name", &self.table_name)
            .finish_non_exhaustive()
    }
}

impl<A> MySqlSnapshotStore<A>
where
    A: Aggregate,
{
    /// Creates a MySQL snapshot store using the default table name.
    pub fn new(connection: Conn) -> Result<Self, EventStoreError> {
        Self::with_table_name(connection, "snapshots")
    }

    /// Creates a MySQL snapshot store with a custom table name.
    pub fn with_table_name(
        connection: Conn,
        table_name: impl Into<String>,
    ) -> Result<Self, EventStoreError> {
        Self::from_pool(ConnectionPool::single(connection), table_name)
    }

    fn from_pool(
        pool: ConnectionPool<Conn>,
        table_name: impl Into<String>,
    ) -> Result<Self, EventStoreError> {
        let table_name = table_name.into();
        validate_table_name(&table_name)?;

        let store = Self {
            pool,
            table_name,
            _marker: PhantomData,
        };
        store.initialize_schema()?;
        Ok(store)
    }

    /// Initializes the snapshot schema table.
    pub fn initialize_schema(&self) -> Result<(), EventStoreError> {
        let config = crate::schema::SqlSchemaConfig::new(crate::schema::SqlDialect::MySql)
            .with_snapshots_table(&self.table_name)?;
        let migrator = crate::schema::SchemaMigrator::for_snapshots(config);
        self.pool.write(|connection| migrator.run_mysql(connection))
    }
}

impl<A> SnapshotStore<A> for MySqlSnapshotStore<A>
where
    A: Aggregate + serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static,
    A::Id: serde::Serialize + serde::de::DeserializeOwned,
{
    type Error = EventStoreError;

    fn load_snapshot(&self, aggregate_id: &A::Id) -> Result<Option<Snapshot<A>>, Self::Error> {
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!(
            "snapshot.load",
            dialect = "mysql",
            aggregate_type = A::aggregate_type()
        )
        .entered();

        let aggregate_id = serialize_id(aggregate_id)?;
        let sql = format!(
            "SELECT revision, state, metadata, recorded_at_ms FROM {} \
             WHERE aggregate_type = ? AND aggregate_id = ?;",
            self.table_name
        );
        self.pool.read(|connection| {
            let row_opt: Option<Row> = connection
                .exec_first(&sql, (A::aggregate_type(), &aggregate_id))
                .map_err(map_mysql_error)?;
            let Some(row) = row_opt else {
                return Ok(None);
            };

            let revision: i64 = row.get(0).ok_or_else(|| {
                EventStoreError::deserialization("missing revision in snapshot row".to_owned())
            })?;
            let state_json: String = row.get(1).ok_or_else(|| {
                EventStoreError::deserialization("missing state in snapshot row".to_owned())
            })?;
            let metadata_json: String = row.get(2).ok_or_else(|| {
                EventStoreError::deserialization("missing metadata in snapshot row".to_owned())
            })?;
            let recorded_at_ms: i64 = row.get(3).ok_or_else(|| {
                EventStoreError::deserialization(
                    "missing recorded_at_ms in snapshot row".to_owned(),
                )
            })?;
            let revision = u64::try_from(revision).map_err(|_| {
                EventStoreError::deserialization(
                    "MySQL snapshot revision cannot be negative".to_owned(),
                )
            })?;
            let state = serde_json::from_str(&state_json).map_err(|error| {
                EventStoreError::deserialization(format!("snapshot state JSON: {error}"))
            })?;
            let metadata = serde_json::from_str(&metadata_json).map_err(|error| {
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
        })
    }

    fn save_snapshot(&self, snapshot: Snapshot<A>) -> Result<(), Self::Error> {
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!(
            "snapshot.save",
            dialect = "mysql",
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
        let sql = format!(
            "INSERT INTO {} (aggregate_type, aggregate_id, revision, state, metadata, recorded_at_ms)
             VALUES (?, ?, ?, ?, ?, ?)
             ON DUPLICATE KEY UPDATE
                revision = IF(VALUES(revision) >= revision, VALUES(revision), revision),
                state = IF(VALUES(revision) >= revision, VALUES(state), state),
                metadata = IF(VALUES(revision) >= revision, VALUES(metadata), metadata),
                recorded_at_ms = IF(VALUES(revision) >= revision, VALUES(recorded_at_ms), recorded_at_ms);",
            self.table_name
        );
        self.pool.write(|connection| {
            connection
                .exec_drop(
                    &sql,
                    (
                        A::aggregate_type(),
                        aggregate_id.as_str(),
                        revision_i64,
                        state_json.as_str(),
                        metadata_json.as_str(),
                        recorded_at_ms,
                    ),
                )
                .map_err(map_mysql_error)?;
            let affected = connection.affected_rows();
            if affected == 0 {
                let current_sql = format!(
                    "SELECT revision FROM {} WHERE aggregate_type = ? AND aggregate_id = ?;",
                    self.table_name
                );
                let current: Option<i64> = connection
                    .exec_first(&current_sql, (A::aggregate_type(), aggregate_id.as_str()))
                    .map_err(map_mysql_error)?;
                if let Some(current) = current {
                    let current = u64::try_from(current).map_err(|_| {
                        EventStoreError::deserialization(
                            "MySQL snapshot revision cannot be negative".to_owned(),
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
        })
    }
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl<A> crate::async_api::AsyncSnapshotStore<A> for MySqlSnapshotStore<A>
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
impl<V> crate::async_api::AsyncIdempotencyStore<V> for MySqlIdempotencyStore<V>
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

    async fn expire_completed_before(&self, cutoff_ms: u64) -> Result<usize, Self::Error> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            IdempotencyStore::expire_completed_before(&this, cutoff_ms)
        })
        .await
        .map_err(|e| EventStoreError::backend(e.to_string()))?
    }
}
