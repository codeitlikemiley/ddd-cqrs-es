use ddd_cqrs_es::{
    assert_atomic_idempotent_store_contract, assert_event_store_any_writers_contract,
    assert_event_store_append_race_contract, assert_event_store_contract, Aggregate,
    AggregateFixture, ConcurrencyError, DomainEvent, EventStore, EventStoreContractOptions,
    EventStoreError, EventStream, EventType, ExpectedRevision, IdempotencyKey, IdempotencyStore,
    IdempotencyWaitConfig, InMemoryEventStore, InMemoryIdempotencyStore, InMemoryProjectionRunner,
    InMemorySnapshotStore, Metadata, NewEvent, Projection, ProjectionBatchConfig, Repository,
    RepositoryError, Snapshot, SnapshotStore, DEFAULT_PROJECTION_BATCH_SIZE,
};
#[cfg(any(
    feature = "sqlite",
    feature = "postgres",
    feature = "mysql",
    feature = "json-file"
))]
use ddd_cqrs_es::{
    assert_checkpoint_store_contract, assert_idempotency_store_contract, IdempotencyState,
};
use std::collections::HashMap;
use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
#[cfg(any(
    feature = "postgres",
    feature = "mysql",
    feature = "json-file",
    feature = "sqlite"
))]
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "framework/contract_tests.rs"]
mod contract_tests;

/// Returns `true` when the suite is running under a CI provider.
#[cfg(any(feature = "postgres", feature = "mysql"))]
fn running_in_ci() -> bool {
    std::env::var("CI").is_ok_and(|value| {
        let value = value.trim();
        !value.is_empty() && !value.eq_ignore_ascii_case("false") && value != "0"
    })
}

/// Reports a live-backend test that cannot run because its connection URL is
/// unset, and lets the caller return early.
///
/// Locally this only prints a skip notice. Under CI it panics: the workflow
/// starts the backing service and exports the URL, so an unset variable means
/// the service is missing and a silent skip would hide backend regressions.
#[cfg(any(feature = "postgres", feature = "mysql"))]
fn skip_live_test(test_name: &str, env_var: &str) {
    assert!(
        !running_in_ci(),
        "live {test_name} cannot be skipped in CI: {env_var} is not set"
    );
    eprintln!("skipping live {test_name}: {env_var} is not set");
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
enum CounterEvent {
    Created,
    Incremented { by: u64 },
}

impl DomainEvent for CounterEvent {
    fn event_type(&self) -> &'static str {
        match self {
            CounterEvent::Created => "counter_created",
            CounterEvent::Incremented { .. } => "counter_incremented",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CounterCommand {
    Create,
    Increment { by: u64 },
}

/// Second aggregate type for cross-aggregate raw feed tests.
#[cfg(any(feature = "postgres", feature = "mysql"))]
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum RawAuditEvent {
    Recorded { note: String },
}

#[cfg(any(feature = "postgres", feature = "mysql"))]
impl DomainEvent for RawAuditEvent {
    fn event_type(&self) -> &'static str {
        "raw_audit_recorded"
    }
}

#[cfg(any(feature = "postgres", feature = "mysql"))]
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct RawAudit {
    notes: u64,
}

#[cfg(any(feature = "postgres", feature = "mysql"))]
impl Aggregate for RawAudit {
    type Id = String;
    type Command = String;
    type Event = RawAuditEvent;
    type Error = std::convert::Infallible;

    fn aggregate_type() -> &'static str {
        "raw_audit"
    }

    fn new() -> Self {
        Self::default()
    }

    fn apply(&mut self, _event: &Self::Event) {
        self.notes += 1;
    }

    fn handle(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        Ok(vec![RawAuditEvent::Recorded { note: command }])
    }
}

#[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct StoredIdempotencyResult {
    value: u64,
    label: String,
}

#[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
fn assert_sql_idempotency_store_contract<S>(store: S)
where
    S: IdempotencyStore<StoredIdempotencyResult, Error = EventStoreError>
        + Clone
        + Send
        + Sync
        + 'static,
{
    let key = IdempotencyKey::new("sql-idempotency-complete");
    let value = StoredIdempotencyResult {
        value: 42,
        label: "json-round-trip".to_owned(),
    };
    assert_idempotency_store_contract(store.clone(), key, value);

    let failed_key = IdempotencyKey::new("sql-idempotency-failed");
    assert!(store.reserve(failed_key.clone()).unwrap());
    store.remove(&failed_key).unwrap();
    assert_eq!(store.load(&failed_key).unwrap(), None);
    assert!(store.reserve(failed_key.clone()).unwrap());

    let concurrent_key = IdempotencyKey::new("sql-idempotency-concurrent");
    let store = Arc::new(store);
    let handles = (0..10)
        .map(|_| {
            let store = Arc::clone(&store);
            let key = concurrent_key.clone();
            thread::spawn(move || store.reserve(key).unwrap())
        })
        .collect::<Vec<_>>();
    let winners = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .filter(|reserved| *reserved)
        .count();

    assert_eq!(winners, 1);
    assert!(matches!(
        store.load(&concurrent_key).unwrap(),
        Some(IdempotencyState::Pending(_))
    ));
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Counter {
    id: Option<String>,
    value: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CounterError {
    AlreadyCreated,
    NotCreated,
    InvalidIncrement,
}

impl Aggregate for Counter {
    type Id = String;
    type Command = CounterCommand;
    type Event = CounterEvent;
    type Error = CounterError;

    fn aggregate_type() -> &'static str {
        "counter"
    }

    fn apply(&mut self, event: &Self::Event) {
        match event {
            CounterEvent::Created => {
                self.id = Some("fixture-counter".to_owned());
            }
            CounterEvent::Incremented { by } => {
                self.value += by;
            }
        }
    }

    fn handle(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            CounterCommand::Create => {
                if self.id.is_some() {
                    return Err(CounterError::AlreadyCreated);
                }
                Ok(vec![CounterEvent::Created])
            }
            CounterCommand::Increment { by } => {
                if self.id.is_none() {
                    return Err(CounterError::NotCreated);
                }
                if by == 0 {
                    return Err(CounterError::InvalidIncrement);
                }
                Ok(vec![CounterEvent::Incremented { by }])
            }
        }
    }

    fn new() -> Self {
        Self::default()
    }
}

#[derive(Clone, Debug)]
struct LoadCountingStore {
    inner: InMemoryEventStore<Counter>,
    load_count: Arc<AtomicUsize>,
    unbounded_global_load_count: Arc<AtomicUsize>,
    limited_global_load_count: Arc<AtomicUsize>,
    last_limited_global_limit: Arc<AtomicUsize>,
}

impl LoadCountingStore {
    fn new(inner: InMemoryEventStore<Counter>) -> Self {
        Self {
            inner,
            load_count: Arc::new(AtomicUsize::new(0)),
            unbounded_global_load_count: Arc::new(AtomicUsize::new(0)),
            limited_global_load_count: Arc::new(AtomicUsize::new(0)),
            last_limited_global_limit: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn load_count(&self) -> usize {
        self.load_count.load(Ordering::SeqCst)
    }

    fn unbounded_global_load_count(&self) -> usize {
        self.unbounded_global_load_count.load(Ordering::SeqCst)
    }

    fn limited_global_load_count(&self) -> usize {
        self.limited_global_load_count.load(Ordering::SeqCst)
    }

    fn last_limited_global_limit(&self) -> usize {
        self.last_limited_global_limit.load(Ordering::SeqCst)
    }
}

impl EventStore<Counter> for LoadCountingStore {
    type Error = EventStoreError;

    fn load(&self, aggregate_id: &String) -> Result<EventStream<Counter>, Self::Error> {
        self.load_count.fetch_add(1, Ordering::SeqCst);
        self.inner.load(aggregate_id)
    }

    fn append(
        &self,
        aggregate_id: &String,
        expected_revision: ExpectedRevision,
        events: Vec<NewEvent<CounterEvent>>,
    ) -> Result<EventStream<Counter>, Self::Error> {
        self.inner.append(aggregate_id, expected_revision, events)
    }

    fn load_global_after(
        &self,
        sequence: Option<u64>,
    ) -> Result<EventStream<Counter>, Self::Error> {
        self.unbounded_global_load_count
            .fetch_add(1, Ordering::SeqCst);
        self.inner.load_global_after(sequence)
    }

    fn load_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: NonZeroUsize,
    ) -> Result<EventStream<Counter>, Self::Error> {
        self.limited_global_load_count
            .fetch_add(1, Ordering::SeqCst);
        self.last_limited_global_limit
            .store(limit.get(), Ordering::SeqCst);
        self.inner.load_global_after_limited(sequence, limit)
    }
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl ddd_cqrs_es::async_api::AsyncEventStore<Counter> for LoadCountingStore {
    type Error = EventStoreError;

    async fn load(&self, aggregate_id: &String) -> Result<EventStream<Counter>, Self::Error> {
        EventStore::load(self, aggregate_id)
    }

    async fn append(
        &self,
        aggregate_id: &String,
        expected_revision: ExpectedRevision,
        events: Vec<NewEvent<CounterEvent>>,
    ) -> Result<EventStream<Counter>, Self::Error> {
        EventStore::append(self, aggregate_id, expected_revision, events)
    }

    async fn load_global_after(
        &self,
        sequence: Option<u64>,
    ) -> Result<EventStream<Counter>, Self::Error> {
        EventStore::load_global_after(self, sequence)
    }

    async fn load_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: NonZeroUsize,
    ) -> Result<EventStream<Counter>, Self::Error> {
        EventStore::load_global_after_limited(self, sequence, limit)
    }
}

#[derive(Clone, Debug)]
struct OffsetSequenceStore {
    inner: InMemoryEventStore<Counter>,
    offset: u64,
}

impl OffsetSequenceStore {
    fn new(offset: u64) -> Self {
        Self {
            inner: InMemoryEventStore::new(),
            offset,
        }
    }

    fn map_sequences(&self, mut events: EventStream<Counter>) -> EventStream<Counter> {
        for event in &mut events {
            event.sequence = event.sequence.map(|sequence| sequence + self.offset);
        }
        events
    }
}

impl EventStore<Counter> for OffsetSequenceStore {
    type Error = EventStoreError;

    fn load(&self, aggregate_id: &String) -> Result<EventStream<Counter>, Self::Error> {
        self.inner
            .load(aggregate_id)
            .map(|events| self.map_sequences(events))
    }

    fn append(
        &self,
        aggregate_id: &String,
        expected_revision: ExpectedRevision,
        events: Vec<NewEvent<CounterEvent>>,
    ) -> Result<EventStream<Counter>, Self::Error> {
        self.inner
            .append(aggregate_id, expected_revision, events)
            .map(|events| self.map_sequences(events))
    }

    fn load_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: NonZeroUsize,
    ) -> Result<EventStream<Counter>, Self::Error> {
        let inner_sequence = sequence.map(|sequence| sequence.saturating_sub(self.offset));
        self.inner
            .load_global_after_limited(inner_sequence, limit)
            .map(|events| self.map_sequences(events))
    }
}

#[test]
fn repository_executes_commands_and_replays_state() {
    let store = InMemoryEventStore::<Counter>::new();
    let repo = Repository::new(store);
    let counter_id = "counter-1".to_owned();

    repo.execute(&counter_id, CounterCommand::Create, Metadata::default())
        .unwrap();
    repo.execute(
        &counter_id,
        CounterCommand::Increment { by: 2 },
        Metadata::default(),
    )
    .unwrap();
    repo.execute(
        &counter_id,
        CounterCommand::Increment { by: 3 },
        Metadata::default(),
    )
    .unwrap();

    let loaded = repo.load(&counter_id).unwrap();
    assert_eq!(loaded.state.value, 5);
    assert_eq!(loaded.revision, 3);
}

#[test]
fn repository_execute_returning_state_loads_stream_once() {
    let store = LoadCountingStore::new(InMemoryEventStore::<Counter>::new());
    let observed_store = store.clone();
    let repo = Repository::new(store);
    let counter_id = "counter-load-once".to_owned();

    let (loaded, committed) = repo
        .execute_returning_state(&counter_id, CounterCommand::Create, Metadata::default())
        .unwrap();

    assert_eq!(observed_store.load_count(), 1);
    assert_eq!(committed.len(), 1);
    assert_eq!(loaded.revision, 1);
    assert!(loaded.state.id.is_some());
}

#[test]
fn event_store_rejects_wrong_expected_revision() {
    let store = InMemoryEventStore::<Counter>::new();
    let counter_id = "counter-1".to_owned();

    store
        .append(
            &counter_id,
            ExpectedRevision::NoStream,
            vec![NewEvent::new(CounterEvent::Created, Metadata::default())],
        )
        .unwrap();

    let result = store.append(
        &counter_id,
        ExpectedRevision::NoStream,
        vec![NewEvent::new(
            CounterEvent::Incremented { by: 1 },
            Metadata::default(),
        )],
    );

    assert!(matches!(
        result,
        Err(EventStoreError::Concurrency(
            ConcurrencyError::StreamAlreadyExists
        ))
    ));
}

#[test]
fn event_type_is_a_string_newtype() {
    let event_type = EventType::from("counter_created");

    assert_eq!(event_type.as_str(), "counter_created");
    assert_eq!(event_type.to_string(), "counter_created");
    assert_eq!(event_type.clone().into_string(), "counter_created");
}

#[cfg(feature = "json")]
#[test]
fn event_type_round_trips_through_serde() {
    let event_type = EventType::from("counter_created");
    let json = serde_json::to_string(&event_type).unwrap();
    let restored: EventType = serde_json::from_str(&json).unwrap();

    assert_eq!(restored, event_type);
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_atomic_idempotent_retry_returns_original_committed_events() {
    let store = ddd_cqrs_es::SqliteEventStore::<Counter>::in_memory().unwrap();
    let repo = Repository::new(store.clone());
    let counter_id = "sqlite-atomic-counter".to_owned();
    let key = IdempotencyKey::new("sqlite-atomic-request");

    let first = repo
        .execute_idempotent_atomic(
            &counter_id,
            CounterCommand::Create,
            Metadata::default(),
            key.clone(),
        )
        .unwrap();
    let retry = repo
        .execute_idempotent_atomic(
            &counter_id,
            CounterCommand::Create,
            Metadata::default(),
            key,
        )
        .unwrap();

    assert_eq!(first, retry);
    assert_eq!(first[0].payload, CounterEvent::Created);
    assert_eq!(store.load(&counter_id).unwrap().len(), 1);
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_atomic_idempotent_pending_key_times_out() {
    let database_name = format!(
        "file:sqlite_atomic_pending_{}_{}?mode=memory&cache=shared",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let event_connection = rusqlite::Connection::open(&database_name).unwrap();
    let idempotency_connection = rusqlite::Connection::open(&database_name).unwrap();
    let store = ddd_cqrs_es::SqliteEventStore::<Counter>::new(event_connection).unwrap();
    store.initialize_schema().unwrap();
    let idempotency =
        ddd_cqrs_es::SqliteIdempotencyStore::<EventStream<Counter>>::new(idempotency_connection)
            .unwrap();
    let key = IdempotencyKey::new("sqlite-atomic-pending-request");
    idempotency.reserve(key.clone()).unwrap();

    let repo = Repository::new(store);
    let error = repo
        .execute_idempotent_atomic_with_wait_config(
            &"sqlite-atomic-pending-counter".to_owned(),
            CounterCommand::Create,
            Metadata::default(),
            key.clone(),
            IdempotencyWaitConfig::new(Duration::from_millis(5), Duration::from_millis(1)),
        )
        .unwrap_err();

    assert!(matches!(
        error,
        ddd_cqrs_es::IdempotentRepositoryError::IdempotencyPendingTimeout {
            key: timeout_key,
            ..
        } if timeout_key == key
    ));
}

#[cfg(feature = "postgres")]
#[test]
fn postgres_query_plans_use_expected_indexes_when_url_is_provided() {
    let Ok(database_url) = std::env::var("DDD_CQRS_ES_POSTGRES_URL") else {
        skip_live_test("Postgres query-plan test", "DDD_CQRS_ES_POSTGRES_URL");
        return;
    };
    let table_name = format!(
        "events_plan_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    let store = ddd_cqrs_es::PostgresEventStore::<Counter>::connect_with_table_name(
        &database_url,
        table_name.clone(),
    )
    .unwrap();
    store.initialize_schema().unwrap();

    let repo = Repository::new(store);
    for index in 0..50 {
        let counter_id = format!("postgres-plan-counter-{index}");
        repo.execute(&counter_id, CounterCommand::Create, Metadata::default())
            .unwrap();
        repo.execute(
            &counter_id,
            CounterCommand::Increment { by: 1 },
            Metadata::default(),
        )
        .unwrap();
    }

    let mut client = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
    client.batch_execute("SET enable_seqscan = off;").unwrap();

    let global_plan_rows = client
        .query(
            &format!(
                "EXPLAIN (FORMAT TEXT, COSTS FALSE)
                 SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, event_version, payload, metadata, recorded_at_ms
                 FROM {table_name}
                 WHERE aggregate_type = $1 AND sequence > $2
                 ORDER BY sequence ASC
                 LIMIT $3"
            ),
            &[&"counter", &0i64, &10i64],
        )
        .unwrap();
    let global_plan = global_plan_rows
        .iter()
        .map(|row| row.get::<_, String>(0))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        global_plan.contains(&format!("{table_name}_global_replay_idx")),
        "expected Postgres global replay query to use the global replay index, got:\n{global_plan}"
    );

    let aggregate_id = serde_json::to_string("postgres-plan-counter-1").unwrap();
    let stream_plan_rows = client
        .query(
            &format!(
                "EXPLAIN (FORMAT TEXT, COSTS FALSE)
                 SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, event_version, payload, metadata, recorded_at_ms
                 FROM {table_name}
                 WHERE aggregate_type = $1 AND aggregate_id = $2
                 ORDER BY revision ASC"
            ),
            &[&"counter", &aggregate_id],
        )
        .unwrap();
    let stream_plan = stream_plan_rows
        .iter()
        .map(|row| row.get::<_, String>(0))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        stream_plan.contains("Index Scan"),
        "expected Postgres stream query to use an index scan, got:\n{stream_plan}"
    );

    client
        .batch_execute(&format!("DROP TABLE IF EXISTS {table_name};"))
        .unwrap();
}

#[cfg(feature = "json")]
#[test]
fn postgres_interpolation_escapes_strings_and_rejects_bad_parameter_indexes() {
    let sql = ddd_cqrs_es::adapters::interpolate_query(
        "SELECT $1, $2, $3",
        &[
            serde_json::json!("O'Reilly"),
            serde_json::json!({ "text": "it's quoted" }),
            serde_json::Value::Null,
        ],
    )
    .unwrap();

    assert_eq!(
        sql,
        "SELECT 'O''Reilly', '{\"text\":\"it''s quoted\"}', NULL"
    );
    assert!(
        ddd_cqrs_es::adapters::interpolate_query("SELECT $0", &[serde_json::json!(1)])
            .unwrap_err()
            .contains("out of bounds")
    );
    assert!(
        ddd_cqrs_es::adapters::interpolate_query("SELECT $2", &[serde_json::json!(1)])
            .unwrap_err()
            .contains("out of bounds")
    );
}

#[test]
fn domain_errors_are_not_persisted() {
    let store = InMemoryEventStore::<Counter>::new();
    let repo = Repository::new(store.clone());
    let counter_id = "counter-1".to_owned();

    let result = repo.execute(
        &counter_id,
        CounterCommand::Increment { by: 1 },
        Metadata::default(),
    );

    assert!(matches!(result, Err(RepositoryError::Domain(_))));
    let events = store.load(&counter_id).unwrap();
    assert!(events.is_empty());
}

#[test]
fn metadata_and_global_sequence_are_preserved() {
    let store = InMemoryEventStore::<Counter>::new();
    let repo = Repository::new(store.clone());
    let counter_id = "counter-1".to_owned();
    let metadata = Metadata::new()
        .with_actor_id("user-1")
        .with_correlation_id("corr-1")
        .with_header("source", "test");

    let committed = repo
        .execute(&counter_id, CounterCommand::Create, metadata.clone())
        .unwrap();

    assert_eq!(committed[0].sequence, Some(1));
    assert_eq!(committed[0].revision, 1);
    assert_eq!(committed[0].event_type, "counter_created");
    assert_eq!(committed[0].metadata, metadata);
    assert_eq!(committed[0].aggregate_type, "counter");

    let global = store.load_global_after(None).unwrap();
    assert_eq!(global.len(), 1);
    assert_eq!(global[0].sequence, Some(1));
}

#[test]
fn event_store_limited_global_replay_returns_bounded_tail() {
    let store = InMemoryEventStore::<Counter>::new();
    let repo = Repository::new(store.clone());
    let counter_id = "counter-limited-replay".to_owned();

    repo.execute(&counter_id, CounterCommand::Create, Metadata::default())
        .unwrap();
    repo.execute(
        &counter_id,
        CounterCommand::Increment { by: 1 },
        Metadata::default(),
    )
    .unwrap();
    repo.execute(
        &counter_id,
        CounterCommand::Increment { by: 2 },
        Metadata::default(),
    )
    .unwrap();

    let limit = NonZeroUsize::new(2).unwrap();
    let first_batch = store.load_global_after_limited(None, limit).unwrap();
    assert_eq!(first_batch.len(), 2);
    assert_eq!(first_batch[0].sequence, Some(1));
    assert_eq!(first_batch[1].sequence, Some(2));

    let second_batch = store.load_global_after_limited(Some(1), limit).unwrap();
    assert_eq!(second_batch.len(), 2);
    assert_eq!(second_batch[0].sequence, Some(2));
    assert_eq!(second_batch[1].sequence, Some(3));
}

#[test]
fn projection_runner_error_formats_and_exposes_source() {
    let error: ddd_cqrs_es::ProjectionRunnerError<std::io::Error, std::io::Error, std::io::Error> =
        ddd_cqrs_es::ProjectionRunnerError::Store(std::io::Error::other("store failed"));

    assert_eq!(error.to_string(), "store failed");
    assert!(error.source().is_some());
}

#[test]
fn event_store_error_preserves_sources_without_changing_display() {
    let error = EventStoreError::backend_with_source(
        "database unavailable",
        std::io::Error::other("socket refused"),
    );

    assert_eq!(
        error.to_string(),
        "event store backend error: database unavailable"
    );
    assert!(error.source().is_some());
    assert_eq!(error.code(), None);

    let coded = EventStoreError::backend("duplicate key").with_code("23505");
    assert_eq!(coded.code(), Some("23505"));
    assert_eq!(
        coded.to_string(),
        "event store backend error: duplicate key"
    );
    // Codes participate in equality; sources do not.
    assert_ne!(coded, EventStoreError::backend("duplicate key"));
    assert_eq!(
        EventStoreError::backend_with_source("same", "src"),
        EventStoreError::backend("same")
    );

    #[cfg(feature = "json")]
    {
        let source = serde_json::from_str::<CounterEvent>("not json").unwrap_err();
        let error = EventStoreError::deserialization_with_source(
            format!("event payload: {source}"),
            source,
        );

        assert!(error.to_string().starts_with("deserialization error:"));
        assert!(error.source().is_some());
    }
}

#[test]
fn process_manager_runner_dispatches_emitted_commands() {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        Created,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Command {
        SendEmail,
    }

    #[derive(Clone, Debug)]
    struct WelcomeProcess;

    impl ddd_cqrs_es::ProcessManager<Event, Command> for WelcomeProcess {
        type Error = std::convert::Infallible;

        fn name(&self) -> &'static str {
            "welcome"
        }

        fn handle(&mut self, event: &Event) -> Result<Vec<Command>, Self::Error> {
            match event {
                Event::Created => Ok(vec![Command::SendEmail]),
            }
        }
    }

    #[derive(Clone, Debug)]
    struct RecordingBus;

    impl ddd_cqrs_es::CommandBus<Command> for RecordingBus {
        type Output = &'static str;
        type Error = std::convert::Infallible;

        fn dispatch(&self, command: Command) -> Result<Self::Output, Self::Error> {
            match command {
                Command::SendEmail => Ok("sent"),
            }
        }
    }

    impl ddd_cqrs_es::IdempotentCommandBus<Command> for RecordingBus {}

    let mut runner = ddd_cqrs_es::ProcessManagerRunner::new(WelcomeProcess, RecordingBus);
    let outputs = runner.run(&Event::Created).unwrap();

    assert_eq!(outputs, vec!["sent"]);
}

#[test]
fn process_manager_runner_resumes_partial_dispatch_from_checkpoint() {
    use ddd_cqrs_es::{
        EventEnvelope, EventId, EventType, ProcessManagerDispatchCheckpoint,
        ProcessManagerRunResult,
    };
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        Started,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Command {
        First,
        Second,
    }

    #[derive(Clone, Debug)]
    struct TwoStepProcess;

    impl ddd_cqrs_es::ProcessManager<Event, Command> for TwoStepProcess {
        type Error = std::convert::Infallible;

        fn name(&self) -> &'static str {
            "two_step"
        }

        fn handle(&mut self, event: &Event) -> Result<Vec<Command>, Self::Error> {
            match event {
                Event::Started => Ok(vec![Command::First, Command::Second]),
            }
        }
    }

    #[derive(Clone, Default)]
    struct MemoryCheckpoint(Arc<Mutex<HashMap<(String, String), usize>>>);

    impl ProcessManagerDispatchCheckpoint for MemoryCheckpoint {
        fn load_dispatch_index(
            &self,
            manager_name: &str,
            event_id: &str,
        ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(manager_name.to_owned(), event_id.to_owned()))
                .copied()
                .unwrap_or(0))
        }

        fn save_dispatch_index(
            &self,
            manager_name: &str,
            event_id: &str,
            index: usize,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.0
                .lock()
                .unwrap()
                .insert((manager_name.to_owned(), event_id.to_owned()), index);
            Ok(())
        }

        fn clear_dispatch_index(
            &self,
            manager_name: &str,
            event_id: &str,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.0
                .lock()
                .unwrap()
                .remove(&(manager_name.to_owned(), event_id.to_owned()));
            Ok(())
        }
    }

    #[derive(Clone, Debug)]
    struct FailSecondBus {
        fail_on: Command,
    }

    impl ddd_cqrs_es::CommandBus<Command> for FailSecondBus {
        type Output = &'static str;
        type Error = &'static str;

        fn dispatch(&self, command: Command) -> Result<Self::Output, Self::Error> {
            if command == self.fail_on {
                Err("second command failed")
            } else {
                Ok("ok")
            }
        }
    }

    impl ddd_cqrs_es::IdempotentCommandBus<Command> for FailSecondBus {}

    #[derive(Clone, Debug)]
    struct RecordingOkBus;

    impl ddd_cqrs_es::CommandBus<Command> for RecordingOkBus {
        type Output = &'static str;
        type Error = std::convert::Infallible;

        fn dispatch(&self, _command: Command) -> Result<Self::Output, Self::Error> {
            Ok("ok")
        }
    }

    impl ddd_cqrs_es::IdempotentCommandBus<Command> for RecordingOkBus {}

    let envelope = EventEnvelope::builder(
        EventId::from_string("evt-1"),
        "agg-1".to_owned(),
        "demo",
        1,
        EventType::from_static("started"),
        Event::Started,
    )
    .build();
    let checkpoint = MemoryCheckpoint::default();

    let mut runner = ddd_cqrs_es::ProcessManagerRunner::new(
        TwoStepProcess,
        FailSecondBus {
            fail_on: Command::Second,
        },
    );
    let first = runner.run_envelope_with_checkpoint(&envelope, &checkpoint);
    assert_eq!(
        first,
        ProcessManagerRunResult {
            dispatched: vec!["ok"],
            failed_index: Some(1),
            error: Some(ddd_cqrs_es::ProcessManagerRunnerError::CommandBus(
                "second command failed"
            )),
        }
    );

    let mut runner = ddd_cqrs_es::ProcessManagerRunner::new(TwoStepProcess, RecordingOkBus);
    let resumed = runner.run_envelope_strict(&envelope, &checkpoint).unwrap();
    assert_eq!(resumed, vec!["ok"]);
}

#[test]
fn process_manager_runner_uses_stable_idempotency_keys() {
    use ddd_cqrs_es::{
        process_manager_command_idempotency_key, EventEnvelope, EventId, EventType,
        IdempotencyKey, ProcessManagerDispatchCheckpoint,
    };
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        Started,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Command {
        First,
        Second,
    }

    #[derive(Clone, Debug)]
    struct TwoStepProcess;

    impl ddd_cqrs_es::ProcessManager<Event, Command> for TwoStepProcess {
        type Error = std::convert::Infallible;

        fn name(&self) -> &'static str {
            "two_step"
        }

        fn handle(&mut self, event: &Event) -> Result<Vec<Command>, Self::Error> {
            match event {
                Event::Started => Ok(vec![Command::First, Command::Second]),
            }
        }
    }

    #[derive(Clone, Default)]
    struct MemoryCheckpoint(Arc<Mutex<HashMap<(String, String), usize>>>);

    impl ProcessManagerDispatchCheckpoint for MemoryCheckpoint {
        fn load_dispatch_index(
            &self,
            manager_name: &str,
            event_id: &str,
        ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(manager_name.to_owned(), event_id.to_owned()))
                .copied()
                .unwrap_or(0))
        }

        fn save_dispatch_index(
            &self,
            manager_name: &str,
            event_id: &str,
            index: usize,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.0
                .lock()
                .unwrap()
                .insert((manager_name.to_owned(), event_id.to_owned()), index);
            Ok(())
        }

        fn clear_dispatch_index(
            &self,
            manager_name: &str,
            event_id: &str,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.0
                .lock()
                .unwrap()
                .remove(&(manager_name.to_owned(), event_id.to_owned()));
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct IdempotentRecordingBus {
        seen: Arc<Mutex<HashMap<IdempotencyKey, usize>>>,
    }

    impl ddd_cqrs_es::CommandBus<Command> for IdempotentRecordingBus {
        type Output = usize;
        type Error = std::convert::Infallible;

        fn dispatch(&self, _command: Command) -> Result<Self::Output, Self::Error> {
            Ok(0)
        }
    }

    impl ddd_cqrs_es::IdempotentCommandBus<Command> for IdempotentRecordingBus {
        fn dispatch_idempotent(
            &self,
            idempotency_key: IdempotencyKey,
            command: Command,
        ) -> Result<Self::Output, Self::Error> {
            let _ = command;
            let mut seen = self.seen.lock().unwrap();
            let count = seen.entry(idempotency_key).or_insert(0);
            *count += 1;
            Ok(*count)
        }
    }

    let envelope = EventEnvelope::builder(
        EventId::from_string("evt-1"),
        "agg-1".to_owned(),
        "demo",
        1,
        EventType::from_static("started"),
        Event::Started,
    )
    .build();
    let checkpoint = MemoryCheckpoint::default();
    let bus = IdempotentRecordingBus::default();

    let expected_first = process_manager_command_idempotency_key("two_step", "evt-1", 0);
    let expected_second = process_manager_command_idempotency_key("two_step", "evt-1", 1);

    let mut runner = ddd_cqrs_es::ProcessManagerRunner::new(TwoStepProcess, bus.clone());
    let first = runner
        .run_envelope_strict(&envelope, &checkpoint)
        .unwrap();
    assert_eq!(first, vec![1, 1]);

    let mut runner = ddd_cqrs_es::ProcessManagerRunner::new(TwoStepProcess, bus.clone());
    let redelivered = runner
        .run_envelope_strict(&envelope, &checkpoint)
        .unwrap();
    assert_eq!(redelivered, vec![2, 2]);

    assert_eq!(bus.seen.lock().unwrap().get(&expected_first), Some(&2));
    assert_eq!(bus.seen.lock().unwrap().get(&expected_second), Some(&2));
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_append_contention_surfaces_concurrency_not_backend_lock() {
    use ddd_cqrs_es::{ConcurrencyError, EventStoreError, ExpectedRevision, NewEvent};
    use std::sync::{Arc, Barrier};
    use std::thread;

    let database_name = format!(
        "file:sqlite_contention_{}_{}?mode=memory&cache=shared",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let seed_connection = rusqlite::Connection::open(&database_name).unwrap();
    let seed_store =
        ddd_cqrs_es::SqliteEventStore::<Counter>::new(seed_connection).unwrap();
    seed_store.initialize_schema().unwrap();
    seed_store
        .append(
            &"counter-1".to_owned(),
            ExpectedRevision::NoStream,
            vec![NewEvent::new(CounterEvent::Created, Metadata::default())],
        )
        .unwrap();

    let store = Arc::new({
        let connection = rusqlite::Connection::open(&database_name).unwrap();
        ddd_cqrs_es::SqliteEventStore::<Counter>::new(connection).unwrap()
    });
    let barrier = Arc::new(Barrier::new(2));
    let counter_id = "counter-1".to_owned();

    let handles = (0..2)
        .map(|_| {
            let store = Arc::clone(&store);
            let barrier = Arc::clone(&barrier);
            let counter_id = counter_id.clone();
            thread::spawn(move || {
                barrier.wait();
                store.append(
                    &counter_id,
                    ExpectedRevision::Exact(1),
                    vec![NewEvent::new(
                        CounterEvent::Incremented { by: 1 },
                        Metadata::default(),
                    )],
                )
            })
        })
        .collect::<Vec<_>>();

    let mut saw_concurrency = false;
    for handle in handles {
        match handle.join().unwrap() {
            Ok(_) => {}
            Err(EventStoreError::Concurrency(_)) => saw_concurrency = true,
            Err(other) => panic!("expected concurrency error, got {other:?}"),
        }
    }

    assert!(saw_concurrency);
    assert!(matches!(
        store.load(&counter_id).unwrap().last().unwrap().payload,
        CounterEvent::Incremented { by: 1 }
    ));
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_invalid_payload_surfaces_deserialization_error() {
    use ddd_cqrs_es::EventStoreError;

    let database_name = format!(
        "file:sqlite_bad_payload_{}_{}?mode=memory&cache=shared",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let seed_connection = rusqlite::Connection::open(&database_name).unwrap();
    let store = ddd_cqrs_es::SqliteEventStore::<Counter>::new(seed_connection).unwrap();
    store.initialize_schema().unwrap();

    let writer = rusqlite::Connection::open(&database_name).unwrap();
    writer
        .execute(
            "INSERT INTO events (event_id, aggregate_id, aggregate_type, revision, event_type, event_version, payload, metadata, recorded_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                "evt-bad",
                "\"counter-1\"",
                "counter",
                1_i64,
                "counter_created",
                1_i64,
                "{not-json",
                "{}",
                0_i64,
            ],
        )
        .unwrap();

    let error = store.load(&"counter-1".to_owned()).unwrap_err();
    assert!(matches!(error, EventStoreError::Deserialization { .. }));
}

#[cfg(feature = "uuid")]
#[test]
fn event_ids_use_uuid_when_feature_is_enabled() {
    let store = InMemoryEventStore::<Counter>::new();
    let repo = Repository::new(store);
    let counter_id = "counter-1".to_owned();

    let committed = repo
        .execute(&counter_id, CounterCommand::Create, Metadata::default())
        .unwrap();

    assert!(uuid::Uuid::parse_str(committed[0].event_id.as_str()).is_ok());
}

#[cfg(feature = "json")]
#[test]
fn event_envelopes_round_trip_through_json() {
    let store = InMemoryEventStore::<Counter>::new();
    let repo = Repository::new(store);
    let counter_id = "counter-1".to_owned();

    let committed = repo
        .execute(&counter_id, CounterCommand::Create, Metadata::default())
        .unwrap();
    let json = committed[0].to_json().unwrap();
    let restored = ddd_cqrs_es::EventEnvelope::<CounterEvent, String>::from_json(&json).unwrap();

    assert_eq!(restored, committed[0]);
}

#[test]
fn repository_surfaces_concurrency_on_main_api() {
    let store = InMemoryEventStore::<Counter>::new();
    let repo = Repository::new(store.clone());
    let counter_id = "counter-1".to_owned();
    let stale = repo.load(&counter_id).unwrap();

    store
        .append(
            &counter_id,
            ExpectedRevision::Any,
            vec![NewEvent::new(CounterEvent::Created, Metadata::default())],
        )
        .unwrap();

    let error = repo
        .save(
            &counter_id,
            &stale,
            vec![CounterEvent::Incremented { by: 1 }],
            Metadata::default(),
        )
        .unwrap_err();

    assert!(matches!(
        error,
        RepositoryError::Concurrency(ConcurrencyError::WrongExpectedRevision {
            expected: ExpectedRevision::Exact(0),
            actual: 1,
        })
    ));
}

#[test]
fn exact_revision_conflicts_are_first_class() {
    let store = InMemoryEventStore::<Counter>::new();
    let counter_id = "counter-1".to_owned();

    store
        .append(
            &counter_id,
            ExpectedRevision::Any,
            vec![NewEvent::new(CounterEvent::Created, Metadata::default())],
        )
        .unwrap();

    let error = store
        .append(
            &counter_id,
            ExpectedRevision::Exact(0),
            vec![NewEvent::new(
                CounterEvent::Incremented { by: 1 },
                Metadata::default(),
            )],
        )
        .unwrap_err();

    assert!(matches!(
        error,
        EventStoreError::Concurrency(ConcurrencyError::WrongExpectedRevision {
            expected: ExpectedRevision::Exact(0),
            actual: 1,
        })
    ));
}

#[test]
fn concurrent_appends_to_same_stream_preserve_one_winner_per_revision() {
    let store = Arc::new(InMemoryEventStore::<Counter>::new());
    let counter_id = "counter-1".to_owned();

    let handles = (0..8)
        .map(|_| {
            let store = Arc::clone(&store);
            let counter_id = counter_id.clone();
            thread::spawn(move || {
                store.append(
                    &counter_id,
                    ExpectedRevision::NoStream,
                    vec![NewEvent::new(CounterEvent::Created, Metadata::default())],
                )
            })
        })
        .collect::<Vec<_>>();

    let successes = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .filter(Result::is_ok)
        .count();

    assert_eq!(successes, 1);
    assert_eq!(store.load(&counter_id).unwrap().len(), 1);
}

#[test]
fn aggregate_fixture_asserts_events_errors_state_and_revision() {
    AggregateFixture::<Counter>::new()
        .given_no_events()
        .when(CounterCommand::Create)
        .then_expect_events(vec![CounterEvent::Created])
        .then_expect_revision(0);

    AggregateFixture::<Counter>::new()
        .given(vec![CounterEvent::Created])
        .when(CounterCommand::Increment { by: 2 })
        .then_expect_events(vec![CounterEvent::Incremented { by: 2 }])
        .then_expect_state(|counter| {
            assert_eq!(counter.value, 2);
        })
        .then_expect_revision(1);

    AggregateFixture::<Counter>::new()
        .given(vec![CounterEvent::Created])
        .when(CounterCommand::Increment { by: 0 })
        .then_expect_error(CounterError::InvalidIncrement);
}

#[derive(Default)]
struct CounterProjection {
    values: HashMap<String, u64>,
}

impl Projection<CounterEvent, String> for CounterProjection {
    type Error = ();

    fn name(&self) -> &'static str {
        "counter_projection"
    }

    fn apply(
        &mut self,
        event: &ddd_cqrs_es::EventEnvelope<CounterEvent, String>,
    ) -> Result<(), Self::Error> {
        let value = self.values.entry(event.aggregate_id.clone()).or_default();
        match event.payload {
            CounterEvent::Created => {}
            CounterEvent::Incremented { by } => *value += by,
        }
        Ok(())
    }
}

#[test]
fn projection_runner_resumes_from_checkpoint() {
    let store = InMemoryEventStore::<Counter>::new();
    let repo = Repository::new(store.clone());
    let counter_id = "counter-1".to_owned();

    repo.execute(&counter_id, CounterCommand::Create, Metadata::default())
        .unwrap();

    let mut runner = InMemoryProjectionRunner::new(CounterProjection::default());
    assert_eq!(runner.run::<Counter, _>(&store).unwrap(), 1);
    assert_eq!(runner.checkpoint(), Some(1));

    repo.execute(
        &counter_id,
        CounterCommand::Increment { by: 4 },
        Metadata::default(),
    )
    .unwrap();

    assert_eq!(runner.run::<Counter, _>(&store).unwrap(), 1);
    assert_eq!(runner.checkpoint(), Some(2));
    assert_eq!(runner.projection().values[&counter_id], 4);
}

#[test]
fn projection_runner_batch_applies_only_configured_limit() {
    let inner = InMemoryEventStore::<Counter>::new();
    let repo = Repository::new(inner.clone());
    let counter_id = "counter-batched-projection".to_owned();

    repo.execute(&counter_id, CounterCommand::Create, Metadata::default())
        .unwrap();
    repo.execute(
        &counter_id,
        CounterCommand::Increment { by: 1 },
        Metadata::default(),
    )
    .unwrap();
    repo.execute(
        &counter_id,
        CounterCommand::Increment { by: 2 },
        Metadata::default(),
    )
    .unwrap();

    let store = LoadCountingStore::new(inner);
    let observed_store = store.clone();
    let mut runner = InMemoryProjectionRunner::new(CounterProjection::default());
    let config = ProjectionBatchConfig::new(NonZeroUsize::new(2).unwrap());

    let first = runner.run_batch::<Counter, _>(&store, config).unwrap();
    assert_eq!(first.applied, 2);
    assert_eq!(first.last_sequence, Some(2));
    assert!(!first.caught_up);
    assert_eq!(runner.checkpoint(), Some(2));
    assert_eq!(observed_store.limited_global_load_count(), 1);
    assert_eq!(observed_store.last_limited_global_limit(), 2);
    assert_eq!(runner.projection().values[&counter_id], 1);

    let second = runner.run_batch::<Counter, _>(&store, config).unwrap();
    assert_eq!(second.applied, 1);
    assert_eq!(second.last_sequence, Some(3));
    assert!(second.caught_up);
    assert_eq!(runner.checkpoint(), Some(3));
    assert_eq!(runner.projection().values[&counter_id], 3);
}

/// Backlog larger than two default batches, so a runner that pages is
/// distinguishable from one that reads the tail in a single load.
const BACKLOG_EVENT_COUNT: usize = DEFAULT_PROJECTION_BATCH_SIZE * 2 + 1;

/// Number of bounded loads a paging runner performs over `BACKLOG_EVENT_COUNT`
/// events: two full batches plus the short batch that reports `caught_up`.
const BACKLOG_BOUNDED_LOADS: usize = 3;

/// Appends `count` events to one counter stream in a single append.
fn seed_counter_backlog(store: &InMemoryEventStore<Counter>, count: usize) -> String {
    let aggregate_id = "counter-backlog".to_owned();
    let events = std::iter::once(NewEvent::new(CounterEvent::Created, Metadata::default()))
        .chain(
            (1..count)
                .map(|_| NewEvent::new(CounterEvent::Incremented { by: 1 }, Metadata::default())),
        )
        .collect::<Vec<_>>();

    store
        .append(&aggregate_id, ExpectedRevision::NoStream, events)
        .unwrap();
    aggregate_id
}

#[test]
fn projection_run_pages_the_backlog_instead_of_loading_the_whole_tail() {
    let inner = InMemoryEventStore::<Counter>::new();
    let counter_id = seed_counter_backlog(&inner, BACKLOG_EVENT_COUNT);
    let store = LoadCountingStore::new(inner);
    let observed_store = store.clone();
    let mut runner = InMemoryProjectionRunner::new(CounterProjection::default());

    let applied = runner.run::<Counter, _>(&store).unwrap();

    assert_eq!(applied, BACKLOG_EVENT_COUNT);
    assert_eq!(runner.checkpoint(), Some(BACKLOG_EVENT_COUNT as u64));
    assert_eq!(
        runner.projection().values[&counter_id],
        BACKLOG_EVENT_COUNT as u64 - 1
    );
    assert_eq!(observed_store.unbounded_global_load_count(), 0);
    assert_eq!(
        observed_store.limited_global_load_count(),
        BACKLOG_BOUNDED_LOADS
    );
    assert_eq!(
        observed_store.last_limited_global_limit(),
        DEFAULT_PROJECTION_BATCH_SIZE
    );
}

#[test]
fn persisted_projection_run_pages_the_backlog_and_checkpoints_each_batch() {
    use ddd_cqrs_es::projection::PersistedProjectionRunner;

    let inner = InMemoryEventStore::<Counter>::new();
    seed_counter_backlog(&inner, BACKLOG_EVENT_COUNT);
    let store = LoadCountingStore::new(inner);
    let observed_store = store.clone();
    let checkpoint_store = CountingCheckpointStore::default();
    let mut runner =
        PersistedProjectionRunner::new(CounterProjection::default(), checkpoint_store.clone());

    let applied = runner.run::<Counter, _>(&store).unwrap();

    assert_eq!(applied, BACKLOG_EVENT_COUNT);
    assert_eq!(
        checkpoint_store.checkpoint(),
        Some(BACKLOG_EVENT_COUNT as u64)
    );
    assert_eq!(checkpoint_store.saves(), BACKLOG_BOUNDED_LOADS);
    assert_eq!(observed_store.unbounded_global_load_count(), 0);
    assert_eq!(
        observed_store.limited_global_load_count(),
        BACKLOG_BOUNDED_LOADS
    );
}

/// Store that implements only the bounded global-replay primitive, so the
/// `EventStore::load_global_after` default implementation is exercised.
#[derive(Clone, Debug)]
struct BoundedOnlyStore {
    inner: InMemoryEventStore<Counter>,
    limited_load_count: Arc<AtomicUsize>,
    largest_requested_limit: Arc<AtomicUsize>,
}

impl BoundedOnlyStore {
    fn new(inner: InMemoryEventStore<Counter>) -> Self {
        Self {
            inner,
            limited_load_count: Arc::new(AtomicUsize::new(0)),
            largest_requested_limit: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl EventStore<Counter> for BoundedOnlyStore {
    type Error = EventStoreError;

    fn load(&self, aggregate_id: &String) -> Result<EventStream<Counter>, Self::Error> {
        self.inner.load(aggregate_id)
    }

    fn append(
        &self,
        aggregate_id: &String,
        expected_revision: ExpectedRevision,
        events: Vec<NewEvent<CounterEvent>>,
    ) -> Result<EventStream<Counter>, Self::Error> {
        self.inner.append(aggregate_id, expected_revision, events)
    }

    fn load_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: NonZeroUsize,
    ) -> Result<EventStream<Counter>, Self::Error> {
        self.limited_load_count.fetch_add(1, Ordering::SeqCst);
        self.largest_requested_limit
            .fetch_max(limit.get(), Ordering::SeqCst);
        self.inner.load_global_after_limited(sequence, limit)
    }
}

#[test]
fn unbounded_global_load_default_pages_through_the_bounded_primitive() {
    use ddd_cqrs_es::event_store::GLOBAL_REPLAY_PAGE_SIZE;

    let inner = InMemoryEventStore::<Counter>::new();
    seed_counter_backlog(&inner, BACKLOG_EVENT_COUNT);
    let store = BoundedOnlyStore::new(inner);

    let events = store.load_global_after(None).unwrap();

    assert_eq!(events.len(), BACKLOG_EVENT_COUNT);
    assert_eq!(events[0].sequence, Some(1));
    assert_eq!(
        events[BACKLOG_EVENT_COUNT - 1].sequence,
        Some(BACKLOG_EVENT_COUNT as u64)
    );
    assert_eq!(
        store.limited_load_count.load(Ordering::SeqCst),
        BACKLOG_BOUNDED_LOADS
    );
    assert_eq!(
        store.largest_requested_limit.load(Ordering::SeqCst),
        GLOBAL_REPLAY_PAGE_SIZE.get()
    );
}

/// Store whose bounded load always returns a full batch of events with no
/// global sequence, so a naive catch-up loop would never make progress.
#[derive(Clone, Debug)]
struct SequencelessStore;

impl EventStore<Counter> for SequencelessStore {
    type Error = EventStoreError;

    fn load(&self, _aggregate_id: &String) -> Result<EventStream<Counter>, Self::Error> {
        Ok(Vec::new())
    }

    fn append(
        &self,
        _aggregate_id: &String,
        _expected_revision: ExpectedRevision,
        _events: Vec<NewEvent<CounterEvent>>,
    ) -> Result<EventStream<Counter>, Self::Error> {
        Ok(Vec::new())
    }

    fn load_global_after_limited(
        &self,
        _sequence: Option<u64>,
        limit: NonZeroUsize,
    ) -> Result<EventStream<Counter>, Self::Error> {
        Ok((0..limit.get())
            .map(|_| {
                ddd_cqrs_es::EventEnvelope::new(
                    ddd_cqrs_es::EventId::new(),
                    "counter-sequenceless".to_owned(),
                    "counter",
                    1,
                    None,
                    "counter_created",
                    1,
                    CounterEvent::Created,
                    Metadata::default(),
                    std::time::SystemTime::now(),
                )
            })
            .collect())
    }
}

#[test]
fn projection_run_stops_when_a_full_batch_does_not_advance_the_feed() {
    let mut runner = InMemoryProjectionRunner::new(CounterProjection::default());

    let applied = runner.run::<Counter, _>(&SequencelessStore).unwrap();

    assert_eq!(applied, DEFAULT_PROJECTION_BATCH_SIZE);
    assert_eq!(runner.checkpoint(), None);
}

#[test]
fn unbounded_global_load_default_stops_when_a_full_page_does_not_advance() {
    use ddd_cqrs_es::event_store::GLOBAL_REPLAY_PAGE_SIZE;

    let events = EventStore::<Counter>::load_global_after(&SequencelessStore, None).unwrap();

    assert_eq!(events.len(), GLOBAL_REPLAY_PAGE_SIZE.get());
}

#[cfg(feature = "sqlite")]
#[test]
fn transactional_projection_rolls_back_read_model_and_checkpoint_together() {
    use rusqlite::OptionalExtension;

    struct SqliteTransactionalCounterProjection {
        connection: rusqlite::Connection,
        fail_on_sequence: Option<u64>,
    }

    impl SqliteTransactionalCounterProjection {
        fn new(fail_on_sequence: Option<u64>) -> Self {
            let connection = rusqlite::Connection::open_in_memory().unwrap();
            connection
                .execute_batch(
                    r#"
                    CREATE TABLE counter_values (
                        id TEXT PRIMARY KEY,
                        value INTEGER NOT NULL
                    );
                    CREATE TABLE tx_checkpoints (
                        projection_name TEXT PRIMARY KEY,
                        sequence INTEGER NOT NULL
                    );
                    "#,
                )
                .unwrap();
            Self {
                connection,
                fail_on_sequence,
            }
        }

        fn counter_value(&self, id: &str) -> u64 {
            self.connection
                .query_row(
                    "SELECT value FROM counter_values WHERE id = ?1",
                    [id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .unwrap()
                .unwrap_or(0) as u64
        }
    }

    impl ddd_cqrs_es::TransactionalCheckpointedProjection<CounterEvent, String>
        for SqliteTransactionalCounterProjection
    {
        type Error = String;

        fn name(&self) -> &'static str {
            "sqlite_tx_counter_projection"
        }

        fn load_checkpoint(&self) -> Result<Option<u64>, Self::Error> {
            self.connection
                .query_row(
                    "SELECT sequence FROM tx_checkpoints WHERE projection_name = ?1",
                    [self.name()],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map(|value| value.map(|sequence| sequence as u64))
                .map_err(|error| error.to_string())
        }

        fn apply_and_checkpoint_transactionally(
            &mut self,
            event: &ddd_cqrs_es::EventEnvelope<CounterEvent, String>,
        ) -> Result<(), Self::Error> {
            let projection_name = self.name();
            let transaction = self
                .connection
                .transaction()
                .map_err(|error| error.to_string())?;

            match event.payload {
                CounterEvent::Created => {
                    transaction
                        .execute(
                            "INSERT INTO counter_values (id, value)
                             VALUES (?1, 0)
                             ON CONFLICT(id) DO NOTHING",
                            [event.aggregate_id.as_str()],
                        )
                        .map_err(|error| error.to_string())?;
                }
                CounterEvent::Incremented { by } => {
                    transaction
                        .execute(
                            "INSERT INTO counter_values (id, value)
                             VALUES (?1, ?2)
                             ON CONFLICT(id) DO UPDATE SET value = value + excluded.value",
                            rusqlite::params![event.aggregate_id.as_str(), by as i64],
                        )
                        .map_err(|error| error.to_string())?;
                }
            }

            if event.sequence == self.fail_on_sequence {
                return Err("projection failed".to_owned());
            }

            if let Some(sequence) = event.sequence {
                transaction
                    .execute(
                        "INSERT INTO tx_checkpoints (projection_name, sequence)
                         VALUES (?1, ?2)
                         ON CONFLICT(projection_name) DO UPDATE
                         SET sequence = excluded.sequence
                         WHERE excluded.sequence > tx_checkpoints.sequence",
                        rusqlite::params![projection_name, sequence as i64],
                    )
                    .map_err(|error| error.to_string())?;
            }

            transaction.commit().map_err(|error| error.to_string())
        }
    }

    let store = InMemoryEventStore::<Counter>::new();
    let repo = Repository::new(store.clone());
    let counter_id = "sqlite-transactional-projection".to_owned();
    repo.execute(&counter_id, CounterCommand::Create, Metadata::default())
        .unwrap();
    repo.execute(
        &counter_id,
        CounterCommand::Increment { by: 4 },
        Metadata::default(),
    )
    .unwrap();

    let projection = SqliteTransactionalCounterProjection::new(Some(2));
    let mut runner = ddd_cqrs_es::TransactionalCheckpointedProjectionRunner::new(projection);
    assert!(runner.run::<Counter, _>(&store).is_err());
    assert_eq!(runner.projection().counter_value(&counter_id), 0);

    runner.projection_mut().fail_on_sequence = None;
    assert_eq!(runner.run::<Counter, _>(&store).unwrap(), 1);
    assert_eq!(runner.projection().counter_value(&counter_id), 4);
}

#[test]
fn repository_loads_from_snapshot_and_replays_later_events() {
    let store = InMemoryEventStore::<Counter>::new();
    let snapshots = InMemorySnapshotStore::<Counter>::new();
    let repo = Repository::new(store.clone());
    let counter_id = "counter-1".to_owned();

    repo.execute(&counter_id, CounterCommand::Create, Metadata::default())
        .unwrap();
    repo.execute(
        &counter_id,
        CounterCommand::Increment { by: 2 },
        Metadata::default(),
    )
    .unwrap();

    let loaded = repo.load(&counter_id).unwrap();
    snapshots
        .save_snapshot(Snapshot::new(
            counter_id.clone(),
            loaded.revision,
            loaded.state,
            Metadata::default(),
        ))
        .unwrap();

    repo.execute(
        &counter_id,
        CounterCommand::Increment { by: 3 },
        Metadata::default(),
    )
    .unwrap();

    let loaded = repo.load_with_snapshot(&counter_id, &snapshots).unwrap();
    assert_eq!(loaded.state.value, 5);
    assert_eq!(loaded.revision, 3);
}

#[test]
fn repository_returns_previous_result_for_idempotent_retry() {
    let store = InMemoryEventStore::<Counter>::new();
    let idempotency = InMemoryIdempotencyStore::new();
    let repo = Repository::new(store.clone());
    let counter_id = "counter-1".to_owned();
    let key = IdempotencyKey::new("request-1");

    let first = repo
        .execute_idempotent(
            &counter_id,
            CounterCommand::Create,
            Metadata::default(),
            key.clone(),
            &idempotency,
        )
        .unwrap();
    let retry = repo
        .execute_idempotent(
            &counter_id,
            CounterCommand::Increment { by: 9 },
            Metadata::default(),
            key,
            &idempotency,
        )
        .unwrap();

    assert_eq!(first, retry);
    assert_eq!(store.load(&counter_id).unwrap().len(), 1);
}

#[test]
fn repository_idempotent_pending_key_times_out() {
    let store = InMemoryEventStore::<Counter>::new();
    let idempotency = InMemoryIdempotencyStore::new();
    let repo = Repository::new(store);
    let counter_id = "pending-counter".to_owned();
    let key = IdempotencyKey::new("pending-request");
    idempotency.reserve(key.clone()).unwrap();

    let error = repo
        .execute_idempotent_with_wait_config(
            &counter_id,
            CounterCommand::Create,
            Metadata::default(),
            key.clone(),
            &idempotency,
            IdempotencyWaitConfig::new(Duration::from_millis(5), Duration::from_millis(1)),
        )
        .unwrap_err();

    assert!(matches!(
        error,
        ddd_cqrs_es::IdempotentRepositoryError::IdempotencyPendingTimeout {
            key: timeout_key,
            ..
        } if timeout_key == key
    ));
}

#[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
#[test]
fn sql_schema_config_rejects_invalid_table_names_eagerly() {
    let result = ddd_cqrs_es::SqlSchemaConfig::new(ddd_cqrs_es::SqlDialect::Sqlite)
        .with_events_table("not-valid-table-name");

    assert!(result.is_err());
    let result = ddd_cqrs_es::SqlSchemaConfig::new(ddd_cqrs_es::SqlDialect::Sqlite)
        .with_snapshots_table("not-valid-table-name");

    assert!(result.is_err());
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_schema_creates_replay_index_without_duplicate_stream_index() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let config = ddd_cqrs_es::SqlSchemaConfig::new(ddd_cqrs_es::SqlDialect::Sqlite)
        .with_events_table("custom_events")
        .unwrap();
    let migrator = ddd_cqrs_es::SchemaMigrator::new(config);

    migrator.run_sqlite(&conn).unwrap();

    let replay_index_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'custom_events_global_replay_idx'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let duplicate_stream_index_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'custom_events_stream_idx'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(replay_index_count, 1);
    assert_eq!(duplicate_stream_index_count, 0);
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_schema_migration_v6_drops_legacy_duplicate_stream_index() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE custom_events (
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
        CREATE INDEX custom_events_stream_idx
            ON custom_events (aggregate_type, aggregate_id, revision);
        "#,
    )
    .unwrap();

    let config = ddd_cqrs_es::SqlSchemaConfig::new(ddd_cqrs_es::SqlDialect::Sqlite)
        .with_events_table("custom_events")
        .unwrap();
    let migrator = ddd_cqrs_es::SchemaMigrator::new(config);

    migrator.run_sqlite(&conn).unwrap();

    let legacy_stream_index_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'custom_events_stream_idx'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let replay_index_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'custom_events_global_replay_idx'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(legacy_stream_index_count, 0);
    assert_eq!(replay_index_count, 1);
}

/// The migrator used to answer "does the bookkeeping table have a
/// `table_name` column?" with `DROP TABLE`, so a probe error or a name
/// collision with an application table destroyed data. It must refuse instead.
#[cfg(feature = "sqlite")]
#[test]
fn sqlite_schema_refuses_to_drop_an_unexpected_migrations_table() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE legacy_migrations (
            version INTEGER PRIMARY KEY,
            description TEXT NOT NULL,
            applied_at_ms INTEGER NOT NULL
        );
        INSERT INTO legacy_migrations (version, description, applied_at_ms)
            VALUES (1, 'create_events_table', 0);
        "#,
    )
    .unwrap();

    let config = ddd_cqrs_es::SqlSchemaConfig::new(ddd_cqrs_es::SqlDialect::Sqlite)
        .with_migrations_table("legacy_migrations")
        .unwrap();
    let error = ddd_cqrs_es::SchemaMigrator::new(config)
        .run_sqlite(&conn)
        .unwrap_err();

    assert!(
        error.to_string().contains("will not drop it"),
        "unexpected error: {error}"
    );

    let surviving_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM legacy_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(surviving_rows, 1);
}

#[cfg(feature = "postgres")]
#[test]
fn postgres_schema_refuses_to_drop_an_unexpected_migrations_table() {
    let Ok(database_url) = std::env::var("DDD_CQRS_ES_POSTGRES_URL") else {
        skip_live_test(
            "Postgres migrations table refusal test",
            "DDD_CQRS_ES_POSTGRES_URL",
        );
        return;
    };
    use postgres::{Client, NoTls};

    let mut client = Client::connect(&database_url, NoTls).unwrap();
    let table_name = format!(
        "legacy_migrations_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    client
        .batch_execute(&format!(
            "DROP TABLE IF EXISTS {table_name};
             CREATE TABLE {table_name} (
                 version INT PRIMARY KEY,
                 description TEXT NOT NULL,
                 applied_at_ms BIGINT NOT NULL
             );
             INSERT INTO {table_name} (version, description, applied_at_ms)
                 VALUES (1, 'create_events_table', 0);"
        ))
        .unwrap();

    let config = ddd_cqrs_es::SqlSchemaConfig::new(ddd_cqrs_es::SqlDialect::Postgres)
        .with_migrations_table(table_name.clone())
        .unwrap();
    let error = ddd_cqrs_es::SchemaMigrator::new(config)
        .run_postgres(&mut client)
        .unwrap_err();

    assert!(
        error.to_string().contains("will not drop it"),
        "unexpected error: {error}"
    );

    let surviving_rows: i64 = client
        .query_one(&format!("SELECT COUNT(*) FROM {table_name}"), &[])
        .unwrap()
        .get(0);
    assert_eq!(surviving_rows, 1);

    client
        .batch_execute(&format!("DROP TABLE IF EXISTS {table_name};"))
        .unwrap();
}

#[cfg(feature = "mysql")]
#[test]
fn mysql_schema_refuses_to_drop_an_unexpected_migrations_table() {
    let Ok(database_url) = std::env::var("DDD_CQRS_ES_MYSQL_URL") else {
        skip_live_test(
            "MySQL migrations table refusal test",
            "DDD_CQRS_ES_MYSQL_URL",
        );
        return;
    };
    use mysql::prelude::Queryable;

    let mut conn = mysql::Conn::new(mysql::Opts::from_url(&database_url).unwrap()).unwrap();
    let table_name = format!(
        "legacy_migrations_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    conn.query_drop(format!("DROP TABLE IF EXISTS {table_name};"))
        .unwrap();
    conn.query_drop(format!(
        "CREATE TABLE {table_name} (
             version INT PRIMARY KEY,
             description VARCHAR(255) NOT NULL,
             applied_at_ms BIGINT NOT NULL
         );"
    ))
    .unwrap();
    conn.query_drop(format!(
        "INSERT INTO {table_name} (version, description, applied_at_ms)
             VALUES (1, 'create_events_table', 0);"
    ))
    .unwrap();

    let config = ddd_cqrs_es::SqlSchemaConfig::new(ddd_cqrs_es::SqlDialect::MySql)
        .with_migrations_table(table_name.clone())
        .unwrap();
    let error = ddd_cqrs_es::SchemaMigrator::new(config)
        .run_mysql(&mut conn)
        .unwrap_err();

    assert!(
        error.to_string().contains("will not drop it"),
        "unexpected error: {error}"
    );

    let surviving_rows: Option<i64> = conn
        .query_first(format!("SELECT COUNT(*) FROM {table_name}"))
        .unwrap();
    assert_eq!(surviving_rows, Some(1));

    conn.query_drop(format!("DROP TABLE IF EXISTS {table_name};"))
        .unwrap();
}

#[cfg(feature = "sqlite")]
fn sqlite_query_plan(conn: &rusqlite::Connection, query: &str) -> String {
    let explain = format!("EXPLAIN QUERY PLAN {query}");
    let mut statement = conn.prepare(&explain).unwrap();
    let rows = statement
        .query_map([], |row| row.get::<_, String>(3))
        .unwrap();
    rows.collect::<Result<Vec<_>, _>>().unwrap().join("\n")
}

#[cfg(feature = "sqlite")]
fn assert_sqlite_plan_uses_index(plan: &str) {
    let plan = plan.to_ascii_lowercase();
    assert!(
        plan.contains("using index") || plan.contains("using covering index"),
        "expected SQLite query plan to use an index, got:\n{plan}"
    );
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_query_plans_use_expected_access_paths() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let config = ddd_cqrs_es::SqlSchemaConfig::new(ddd_cqrs_es::SqlDialect::Sqlite)
        .with_events_table("qp_events")
        .unwrap()
        .with_checkpoints_table("qp_checkpoints")
        .unwrap()
        .with_idempotency_table("qp_idempotency")
        .unwrap()
        .with_snapshots_table("qp_snapshots")
        .unwrap();
    let migrator = ddd_cqrs_es::SchemaMigrator::new(config);
    migrator.run_sqlite(&conn).unwrap();

    let stream_plan = sqlite_query_plan(
        &conn,
        "SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, event_version, payload, metadata, recorded_at_ms
         FROM qp_events
         WHERE aggregate_type = 'counter' AND aggregate_id = '\"counter-1\"'
         ORDER BY revision ASC",
    );
    assert_sqlite_plan_uses_index(&stream_plan);

    let revision_plan = sqlite_query_plan(
        &conn,
        "SELECT COALESCE(MAX(revision), 0)
         FROM qp_events
         WHERE aggregate_type = 'counter' AND aggregate_id = '\"counter-1\"'",
    );
    assert_sqlite_plan_uses_index(&revision_plan);

    let global_replay_plan = sqlite_query_plan(
        &conn,
        "SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, event_version, payload, metadata, recorded_at_ms
         FROM qp_events
         WHERE aggregate_type = 'counter' AND sequence > 10
         ORDER BY sequence ASC
         LIMIT 5",
    );
    assert!(
        global_replay_plan
            .to_ascii_lowercase()
            .contains("qp_events_global_replay_idx"),
        "expected global replay plan to use qp_events_global_replay_idx, got:\n{global_replay_plan}"
    );

    let latest_ledger_plan = sqlite_query_plan(
        &conn,
        "SELECT sequence, event_type, revision, payload, recorded_at_ms
         FROM qp_events
         ORDER BY sequence DESC
         LIMIT 5",
    );
    assert!(
        !latest_ledger_plan
            .to_ascii_lowercase()
            .contains("use temp b-tree"),
        "latest ledger query should not need a temp b-tree, got:\n{latest_ledger_plan}"
    );

    assert_sqlite_plan_uses_index(&sqlite_query_plan(
        &conn,
        "SELECT sequence FROM qp_checkpoints WHERE projection_name = 'counter_projection'",
    ));
    assert_sqlite_plan_uses_index(&sqlite_query_plan(
        &conn,
        "SELECT state, value FROM qp_idempotency WHERE idempotency_key = 'key-1'",
    ));
    assert_sqlite_plan_uses_index(&sqlite_query_plan(
        &conn,
        "SELECT revision, state, metadata, recorded_at_ms
         FROM qp_snapshots
         WHERE aggregate_type = 'counter' AND aggregate_id = '\"counter-1\"'",
    ));
}

#[test]
fn repository_idempotent_concurrency() {
    let store = InMemoryEventStore::<Counter>::new();
    let idempotency = InMemoryIdempotencyStore::new();
    let repo = Repository::new(store.clone());
    let counter_id = "concurrent-counter".to_owned();
    let key = IdempotencyKey::new("concurrent-req");

    let repo_arc = Arc::new(repo);
    let idempotency_arc = Arc::new(idempotency);
    let counter_id_arc = Arc::new(counter_id.clone());
    let key_arc = Arc::new(key);

    let mut handles = vec![];
    for _ in 0..10 {
        let repo = Arc::clone(&repo_arc);
        let idempotency = Arc::clone(&idempotency_arc);
        let counter_id = Arc::clone(&counter_id_arc);
        let key = Arc::clone(&key_arc);

        handles.push(thread::spawn(move || {
            repo.execute_idempotent(
                &counter_id,
                CounterCommand::Create,
                Metadata::default(),
                (*key).clone(),
                &*idempotency,
            )
        }));
    }

    let mut results = vec![];
    for handle in handles {
        results.push(handle.join().unwrap().unwrap());
    }

    let first_result = &results[0];
    for r in &results {
        assert_eq!(r, first_result);
    }

    assert_eq!(store.load(&counter_id).unwrap().len(), 1);
}

#[cfg(feature = "async")]
mod async_tests {
    use super::*;
    use ddd_cqrs_es::{
        async_api::AsyncEventStore, AsyncRepository, AsyncSnapshotStore, InMemoryEventStore,
        InMemoryIdempotencyStore, InMemorySnapshotStore, Snapshot,
    };

    #[tokio::test]
    async fn test_async_repository_flow() {
        let store = InMemoryEventStore::<Counter>::new();
        let repo = AsyncRepository::new(store);
        let counter_id = "async-counter-1".to_owned();

        repo.execute(&counter_id, CounterCommand::Create, Metadata::default())
            .await
            .unwrap();

        repo.execute(
            &counter_id,
            CounterCommand::Increment { by: 5 },
            Metadata::default(),
        )
        .await
        .unwrap();

        let loaded = repo.load(&counter_id).await.unwrap();
        assert_eq!(loaded.state.value, 5);
        assert_eq!(loaded.revision, 2);
    }

    #[tokio::test]
    async fn async_repository_execute_returning_state_loads_stream_once() {
        let store = LoadCountingStore::new(InMemoryEventStore::<Counter>::new());
        let observed_store = store.clone();
        let repo = AsyncRepository::new(store);
        let counter_id = "async-counter-load-once".to_owned();

        let (loaded, committed) = repo
            .execute_returning_state(&counter_id, CounterCommand::Create, Metadata::default())
            .await
            .unwrap();

        assert_eq!(observed_store.load_count(), 1);
        assert_eq!(committed.len(), 1);
        assert_eq!(loaded.revision, 1);
        assert!(loaded.state.id.is_some());
    }

    #[tokio::test]
    async fn test_async_repository_with_snapshots() {
        let store = InMemoryEventStore::<Counter>::new();
        let snapshots = InMemorySnapshotStore::<Counter>::new();
        let repo = AsyncRepository::new(store);
        let counter_id = "async-counter-snapshot".to_owned();

        repo.execute(&counter_id, CounterCommand::Create, Metadata::default())
            .await
            .unwrap();

        let loaded = repo.load(&counter_id).await.unwrap();

        let snapshot = Snapshot::new(
            counter_id.clone(),
            loaded.revision,
            loaded.state.clone(),
            Metadata::default(),
        );
        AsyncSnapshotStore::save_snapshot(&snapshots, snapshot)
            .await
            .unwrap();

        repo.execute(
            &counter_id,
            CounterCommand::Increment { by: 10 },
            Metadata::default(),
        )
        .await
        .unwrap();

        let loaded_snap = repo
            .load_with_snapshot(&counter_id, &snapshots)
            .await
            .unwrap();
        assert_eq!(loaded_snap.state.value, 10);
        assert_eq!(loaded_snap.revision, 2);
    }

    #[tokio::test]
    async fn test_async_repository_idempotent() {
        let store = InMemoryEventStore::<Counter>::new();
        let idempotency = InMemoryIdempotencyStore::new();
        let repo = AsyncRepository::new(store.clone());
        let counter_id = "async-counter-idempotent".to_owned();
        let key = IdempotencyKey::new("async-req-1");

        let first = repo
            .execute_idempotent(
                &counter_id,
                CounterCommand::Create,
                Metadata::default(),
                key.clone(),
                &idempotency,
            )
            .await
            .unwrap();

        let retry = repo
            .execute_idempotent(
                &counter_id,
                CounterCommand::Increment { by: 9 },
                Metadata::default(),
                key,
                &idempotency,
            )
            .await
            .unwrap();

        assert_eq!(first, retry);
        let events = AsyncEventStore::load(&store, &counter_id).await.unwrap();
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn async_idempotency_store_passes_reusable_contract() {
        ddd_cqrs_es::assert_async_idempotency_store_contract(
            InMemoryIdempotencyStore::<String>::new(),
            IdempotencyKey::new("async-contract-key"),
            "completed".to_owned(),
        )
        .await;
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn sqlite_async_atomic_idempotent_retry_returns_original_committed_events() {
        let store = ddd_cqrs_es::SqliteEventStore::<Counter>::in_memory().unwrap();
        let repo = AsyncRepository::new(store.clone());
        let counter_id = "sqlite-async-atomic-counter".to_owned();
        let key = IdempotencyKey::new("sqlite-async-atomic-request");

        let first = repo
            .execute_idempotent_atomic(
                &counter_id,
                CounterCommand::Create,
                Metadata::default(),
                key.clone(),
            )
            .await
            .unwrap();
        let retry = repo
            .execute_idempotent_atomic(
                &counter_id,
                CounterCommand::Create,
                Metadata::default(),
                key,
            )
            .await
            .unwrap();

        assert_eq!(first, retry);
        let events = AsyncEventStore::load(&store, &counter_id).await.unwrap();
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn async_repository_idempotent_pending_key_times_out() {
        let store = InMemoryEventStore::<Counter>::new();
        let idempotency = InMemoryIdempotencyStore::new();
        let repo = AsyncRepository::new(store);
        let counter_id = "async-pending-counter".to_owned();
        let key = IdempotencyKey::new("async-pending-request");
        idempotency.reserve(key.clone()).unwrap();

        let error = repo
            .execute_idempotent_with_wait_config(
                &counter_id,
                CounterCommand::Create,
                Metadata::default(),
                key.clone(),
                &idempotency,
                IdempotencyWaitConfig::new(Duration::from_millis(5), Duration::from_millis(1)),
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            ddd_cqrs_es::IdempotentRepositoryError::IdempotencyPendingTimeout {
                key: timeout_key,
                ..
            } if timeout_key == key
        ));
    }

    #[tokio::test]
    async fn async_process_manager_runner_dispatches_emitted_commands() {
        #[derive(Clone, Debug, PartialEq, Eq)]
        enum Event {
            Created,
        }

        #[derive(Clone, Debug, PartialEq, Eq)]
        enum Command {
            SendEmail,
        }

        #[derive(Clone, Debug)]
        struct WelcomeProcess;

        impl ddd_cqrs_es::ProcessManager<Event, Command> for WelcomeProcess {
            type Error = std::convert::Infallible;

            fn name(&self) -> &'static str {
                "welcome"
            }

            fn handle(&mut self, event: &Event) -> Result<Vec<Command>, Self::Error> {
                match event {
                    Event::Created => Ok(vec![Command::SendEmail]),
                }
            }
        }

        #[derive(Clone, Debug)]
        struct RecordingBus;

        #[async_trait::async_trait]
        impl ddd_cqrs_es::AsyncCommandBus<Command> for RecordingBus {
            type Output = &'static str;
            type Error = std::convert::Infallible;

            async fn dispatch(&self, command: Command) -> Result<Self::Output, Self::Error> {
                match command {
                    Command::SendEmail => Ok("sent"),
                }
            }
        }

        let mut runner = ddd_cqrs_es::AsyncProcessManagerRunner::new(WelcomeProcess, RecordingBus);
        let outputs = runner.run(&Event::Created).await.unwrap();

        assert_eq!(outputs, vec!["sent"]);
    }

    #[tokio::test]
    async fn test_async_repository_idempotent_concurrency() {
        let store = InMemoryEventStore::<Counter>::new();
        let idempotency = InMemoryIdempotencyStore::new();
        let repo = Arc::new(AsyncRepository::new(store.clone()));
        let idempotency_arc = Arc::new(idempotency);
        let counter_id = Arc::new("async-concurrent-counter".to_owned());
        let key = Arc::new(IdempotencyKey::new("async-concurrent-req"));

        let mut tasks = vec![];
        for _ in 0..10 {
            let repo = Arc::clone(&repo);
            let idempotency = Arc::clone(&idempotency_arc);
            let counter_id = Arc::clone(&counter_id);
            let key = Arc::clone(&key);

            tasks.push(tokio::spawn(async move {
                repo.execute_idempotent(
                    &counter_id,
                    CounterCommand::Create,
                    Metadata::default(),
                    (*key).clone(),
                    &*idempotency,
                )
                .await
            }));
        }

        let mut results = vec![];
        for task in tasks {
            results.push(task.await.unwrap().unwrap());
        }

        let first_result = &results[0];
        for r in &results {
            assert_eq!(r, first_result);
        }

        let events = AsyncEventStore::load(&store, &*counter_id).await.unwrap();
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    #[cfg(feature = "sqlite")]
    async fn test_async_persisted_projection_runner() {
        use ddd_cqrs_es::projection::{AsyncCheckpointStore, AsyncPersistedProjectionRunner};
        use ddd_cqrs_es::SqliteCheckpointStore;
        use rusqlite::Connection;

        let conn = Connection::open_in_memory().unwrap();
        let checkpoint_store = SqliteCheckpointStore::new(conn).unwrap();

        let store = InMemoryEventStore::<Counter>::new();
        let repo = AsyncRepository::new(store.clone());
        let counter_id = "counter-1".to_owned();

        repo.execute(&counter_id, CounterCommand::Create, Metadata::default())
            .await
            .unwrap();

        let projection = CounterProjection::default();
        let mut runner = AsyncPersistedProjectionRunner::new(projection, checkpoint_store.clone());

        let applied = runner.run::<Counter, _>(&store).await.unwrap();
        assert_eq!(applied, 1);

        let cp = checkpoint_store
            .load_checkpoint("counter_projection")
            .await
            .unwrap();
        assert_eq!(cp, Some(1));
    }

    #[tokio::test]
    async fn async_persisted_projection_runner_checkpoints_once_per_pass() {
        use ddd_cqrs_es::projection::AsyncPersistedProjectionRunner;

        let store = InMemoryEventStore::<Counter>::new();
        let repo = AsyncRepository::new(store.clone());
        let counter_id = "counter-1".to_owned();

        repo.execute(&counter_id, CounterCommand::Create, Metadata::default())
            .await
            .unwrap();
        for _ in 0..4 {
            repo.execute(
                &counter_id,
                CounterCommand::Increment { by: 1 },
                Metadata::default(),
            )
            .await
            .unwrap();
        }

        let checkpoint_store = CountingCheckpointStore::default();
        let mut runner = AsyncPersistedProjectionRunner::new(
            CounterProjection::default(),
            checkpoint_store.clone(),
        );

        let applied = runner.run::<Counter, _>(&store).await.unwrap();

        assert_eq!(applied, 5);
        assert_eq!(checkpoint_store.checkpoint(), Some(5));
        assert_eq!(checkpoint_store.saves(), 1);
    }
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_store_upcasts_chained_event_versions_on_load() {
    use ddd_cqrs_es::EventUpcaster;

    struct Upcaster1To2;
    impl EventUpcaster for Upcaster1To2 {
        type Error = std::convert::Infallible;
        fn source_version(&self) -> u32 {
            1
        }
        fn target_version(&self) -> u32 {
            2
        }
        fn upcast(&self, raw_payload: Vec<u8>) -> Result<Vec<u8>, Self::Error> {
            let s = String::from_utf8(raw_payload).unwrap();
            let upgraded = s.replace("OldCreated", "V2Created");
            Ok(upgraded.into_bytes())
        }
    }

    struct Upcaster2To3;
    impl EventUpcaster for Upcaster2To3 {
        type Error = std::convert::Infallible;
        fn source_version(&self) -> u32 {
            2
        }
        fn target_version(&self) -> u32 {
            3
        }
        fn upcast(&self, raw_payload: Vec<u8>) -> Result<Vec<u8>, Self::Error> {
            let s = String::from_utf8(raw_payload).unwrap();
            let upgraded = s.replace("V2Created", "Created");
            Ok(upgraded.into_bytes())
        }
    }

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE events (
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
        "#,
    )
    .unwrap();

    conn.execute(
        "INSERT INTO events (event_id, aggregate_id, aggregate_type, revision, event_type, event_version, payload, metadata, recorded_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            "event-123",
            "\"counter-123\"",
            "counter",
            1,
            "counter_created",
            1,
            "\"OldCreated\"",
            serde_json::to_string(&Metadata::default()).unwrap(),
            1700000000000i64,
        ]
    ).unwrap();

    let store = ddd_cqrs_es::SqliteEventStore::<Counter>::new(conn).unwrap();
    store
        .register_upcaster("counter_created", Upcaster1To2)
        .unwrap();
    store
        .register_upcaster("counter_created", Upcaster2To3)
        .unwrap();

    let events = ddd_cqrs_es::EventStore::load(&store, &"counter-123".to_owned()).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].payload, CounterEvent::Created);
    assert_eq!(events[0].event_version, 3);
}

#[cfg(feature = "sqlite")]
#[test]
fn test_sqlite_checkpoint_store() {
    use ddd_cqrs_es::SqliteCheckpointStore;
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let store = SqliteCheckpointStore::new(conn).unwrap();

    assert_checkpoint_store_contract(store, "proj1");
}

#[cfg(feature = "sqlite")]
#[test]
fn test_sync_persisted_projection_runner() {
    use ddd_cqrs_es::projection::PersistedProjectionRunner;
    use ddd_cqrs_es::SqliteCheckpointStore;

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let checkpoint_store = SqliteCheckpointStore::new(conn).unwrap();

    let store = InMemoryEventStore::<Counter>::new();
    let repo = Repository::new(store.clone());
    let counter_id = "counter-1".to_owned();

    repo.execute(&counter_id, CounterCommand::Create, Metadata::default())
        .unwrap();

    let projection = CounterProjection::default();
    let mut runner = PersistedProjectionRunner::new(projection, checkpoint_store.clone());

    let applied = runner.run::<Counter, _>(&store).unwrap();
    assert_eq!(applied, 1);

    use ddd_cqrs_es::projection::CheckpointStore;
    let cp = checkpoint_store
        .load_checkpoint("counter_projection")
        .unwrap();
    assert_eq!(cp, Some(1));
}

/// In-memory checkpoint store that counts writes, for asserting the
/// once-per-batch checkpoint flush behavior.
#[derive(Clone, Default)]
struct CountingCheckpointStore {
    state: std::sync::Arc<std::sync::Mutex<(Option<u64>, usize)>>,
}

impl CountingCheckpointStore {
    fn checkpoint(&self) -> Option<u64> {
        self.state.lock().unwrap().0
    }

    fn saves(&self) -> usize {
        self.state.lock().unwrap().1
    }
}

impl ddd_cqrs_es::projection::CheckpointStore for CountingCheckpointStore {
    type Error = std::convert::Infallible;

    fn load_checkpoint(&self, _projection_name: &str) -> Result<Option<u64>, Self::Error> {
        Ok(self.state.lock().unwrap().0)
    }

    fn save_checkpoint(&self, _projection_name: &str, sequence: u64) -> Result<(), Self::Error> {
        let mut state = self.state.lock().unwrap();
        state.0 = Some(sequence);
        state.1 += 1;
        Ok(())
    }

    fn reset_checkpoint(&self, _projection_name: &str) -> Result<(), Self::Error> {
        let mut state = self.state.lock().unwrap();
        state.0 = None;
        Ok(())
    }
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl ddd_cqrs_es::projection::AsyncCheckpointStore for CountingCheckpointStore {
    type Error = std::convert::Infallible;

    async fn load_checkpoint(&self, projection_name: &str) -> Result<Option<u64>, Self::Error> {
        ddd_cqrs_es::projection::CheckpointStore::load_checkpoint(self, projection_name)
    }

    async fn save_checkpoint(
        &self,
        projection_name: &str,
        sequence: u64,
    ) -> Result<(), Self::Error> {
        ddd_cqrs_es::projection::CheckpointStore::save_checkpoint(self, projection_name, sequence)
    }

    async fn reset_checkpoint(&self, projection_name: &str) -> Result<(), Self::Error> {
        ddd_cqrs_es::projection::CheckpointStore::reset_checkpoint(self, projection_name)
    }
}

#[test]
fn persisted_projection_runner_checkpoints_once_per_pass() {
    use ddd_cqrs_es::projection::PersistedProjectionRunner;

    let store = InMemoryEventStore::<Counter>::new();
    let repo = Repository::new(store.clone());
    let counter_id = "counter-1".to_owned();

    repo.execute(&counter_id, CounterCommand::Create, Metadata::default())
        .unwrap();
    for _ in 0..4 {
        repo.execute(
            &counter_id,
            CounterCommand::Increment { by: 1 },
            Metadata::default(),
        )
        .unwrap();
    }

    let checkpoint_store = CountingCheckpointStore::default();
    let mut runner =
        PersistedProjectionRunner::new(CounterProjection::default(), checkpoint_store.clone());

    let applied = runner.run::<Counter, _>(&store).unwrap();

    assert_eq!(applied, 5);
    assert_eq!(checkpoint_store.checkpoint(), Some(5));
    assert_eq!(checkpoint_store.saves(), 1);

    // A caught-up pass with no new events writes no checkpoint at all.
    let applied = runner.run::<Counter, _>(&store).unwrap();
    assert_eq!(applied, 0);
    assert_eq!(checkpoint_store.saves(), 1);
}

/// Checkpoint store that honours the projection key, so checkpoint collisions
/// between two feeds of the same projection are observable.
#[derive(Clone, Default)]
struct KeyedCheckpointStore {
    checkpoints: std::sync::Arc<std::sync::Mutex<HashMap<String, u64>>>,
}

impl KeyedCheckpointStore {
    fn keys(&self) -> Vec<String> {
        let mut keys = self
            .checkpoints
            .lock()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        keys.sort();
        keys
    }
}

impl ddd_cqrs_es::projection::CheckpointStore for KeyedCheckpointStore {
    type Error = std::convert::Infallible;

    fn load_checkpoint(&self, projection_name: &str) -> Result<Option<u64>, Self::Error> {
        Ok(self
            .checkpoints
            .lock()
            .unwrap()
            .get(projection_name)
            .copied())
    }

    fn save_checkpoint(&self, projection_name: &str, sequence: u64) -> Result<(), Self::Error> {
        self.checkpoints
            .lock()
            .unwrap()
            .insert(projection_name.to_owned(), sequence);
        Ok(())
    }

    fn reset_checkpoint(&self, projection_name: &str) -> Result<(), Self::Error> {
        self.checkpoints.lock().unwrap().remove(projection_name);
        Ok(())
    }
}

/// Two aggregate types share one events table, so their global sequences
/// interleave while each typed feed is filtered to its own type. A projection
/// registered against both types therefore needs one checkpoint per type:
/// keying on `Projection::name()` alone lets the further-along feed hide the
/// other's events forever.
#[cfg(feature = "sqlite")]
#[test]
fn persisted_runner_needs_aggregate_scoped_checkpoints_across_aggregate_types() {
    use ddd_cqrs_es::projection::PersistedProjectionRunner;

    #[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    enum LedgerEvent {
        Posted,
    }

    impl DomainEvent for LedgerEvent {
        fn event_type(&self) -> &'static str {
            "ledger_posted"
        }
    }

    #[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    struct Ledger;

    impl Aggregate for Ledger {
        type Id = String;
        type Command = ();
        type Event = LedgerEvent;
        type Error = std::convert::Infallible;

        fn aggregate_type() -> &'static str {
            "ledger"
        }

        fn new() -> Self {
            Self
        }

        fn apply(&mut self, _event: &Self::Event) {}

        fn handle(&self, _command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
            Ok(vec![LedgerEvent::Posted])
        }
    }

    /// One read model fed by both aggregate types, so both runs report the
    /// same `name()`.
    #[derive(Default)]
    struct EventTally {
        applied: usize,
    }

    impl Projection<CounterEvent, String> for EventTally {
        type Error = std::convert::Infallible;

        fn name(&self) -> &'static str {
            "event_tally"
        }

        fn apply(
            &mut self,
            _event: &ddd_cqrs_es::EventEnvelope<CounterEvent, String>,
        ) -> Result<(), Self::Error> {
            self.applied += 1;
            Ok(())
        }
    }

    impl Projection<LedgerEvent, String> for EventTally {
        type Error = std::convert::Infallible;

        fn name(&self) -> &'static str {
            "event_tally"
        }

        fn apply(
            &mut self,
            _event: &ddd_cqrs_es::EventEnvelope<LedgerEvent, String>,
        ) -> Result<(), Self::Error> {
            self.applied += 1;
            Ok(())
        }
    }

    /// Appends counter (sequence 1), ledger (sequence 2), counter (sequence 3)
    /// into one shared events table and returns both typed stores.
    fn seed_interleaved_stores(
        label: &str,
    ) -> (
        ddd_cqrs_es::SqliteEventStore<Counter>,
        ddd_cqrs_es::SqliteEventStore<Ledger>,
        rusqlite::Connection,
    ) {
        let db_uri = format!("file:checkpoint_scope_{label}?mode=memory&cache=shared");
        // Keep the shared in-memory database alive for the whole test.
        let anchor = rusqlite::Connection::open(&db_uri).unwrap();

        let counters = ddd_cqrs_es::SqliteEventStore::<Counter>::new(
            rusqlite::Connection::open(&db_uri).unwrap(),
        )
        .unwrap();
        counters.initialize_schema().unwrap();
        let ledgers = ddd_cqrs_es::SqliteEventStore::<Ledger>::new(
            rusqlite::Connection::open(&db_uri).unwrap(),
        )
        .unwrap();

        counters
            .append(
                &"counter-1".to_owned(),
                ExpectedRevision::NoStream,
                vec![NewEvent::new(CounterEvent::Created, Metadata::default())],
            )
            .unwrap();
        ledgers
            .append(
                &"ledger-1".to_owned(),
                ExpectedRevision::NoStream,
                vec![NewEvent::new(LedgerEvent::Posted, Metadata::default())],
            )
            .unwrap();
        counters
            .append(
                &"counter-1".to_owned(),
                ExpectedRevision::Exact(1),
                vec![NewEvent::new(
                    CounterEvent::Incremented { by: 1 },
                    Metadata::default(),
                )],
            )
            .unwrap();

        (counters, ledgers, anchor)
    }

    // Name-only keying (the 0.3 default): replaying the counter feed advances
    // the shared row to sequence 3, which is past the ledger event at
    // sequence 2, so the ledger feed reports nothing to do.
    let (counters, ledgers, _anchor) = seed_interleaved_stores("shared");
    let shared = KeyedCheckpointStore::default();
    let mut counter_runner = PersistedProjectionRunner::new(EventTally::default(), shared.clone());
    let mut ledger_runner = PersistedProjectionRunner::new(EventTally::default(), shared.clone());

    assert_eq!(counter_runner.run::<Counter, _>(&counters).unwrap(), 2);
    assert_eq!(
        ledger_runner.run::<Ledger, _>(&ledgers).unwrap(),
        0,
        "the shared checkpoint row hides the ledger event"
    );
    assert_eq!(shared.keys(), vec!["event_tally".to_owned()]);

    // Aggregate-scoped keying: one row per feed, so both events land and the
    // skip does not come back on a later pass either.
    let (counters, ledgers, _anchor) = seed_interleaved_stores("scoped");
    let scoped = KeyedCheckpointStore::default();
    let mut counter_runner = PersistedProjectionRunner::with_aggregate_scoped_checkpoints(
        EventTally::default(),
        scoped.clone(),
    );
    let mut ledger_runner = PersistedProjectionRunner::with_aggregate_scoped_checkpoints(
        EventTally::default(),
        scoped.clone(),
    );

    assert_eq!(counter_runner.run::<Counter, _>(&counters).unwrap(), 2);
    assert_eq!(ledger_runner.run::<Ledger, _>(&ledgers).unwrap(), 1);
    assert_eq!(counter_runner.run::<Counter, _>(&counters).unwrap(), 0);
    assert_eq!(ledger_runner.run::<Ledger, _>(&ledgers).unwrap(), 0);
    assert_eq!(
        scoped.keys(),
        vec![
            ddd_cqrs_es::aggregate_scoped_checkpoint_key("event_tally", "counter"),
            ddd_cqrs_es::aggregate_scoped_checkpoint_key("event_tally", "ledger"),
        ]
    );
}

#[test]
fn persisted_projection_runner_flushes_progress_before_reporting_failure() {
    use ddd_cqrs_es::projection::{PersistedProjectionRunner, ProjectionRunnerError};

    /// Fails when the counter value would exceed the trip point.
    struct TrippingProjection {
        total: u64,
        trip_at: u64,
    }

    impl Projection<CounterEvent, String> for TrippingProjection {
        type Error = &'static str;

        fn name(&self) -> &'static str {
            "tripping_projection"
        }

        fn apply(
            &mut self,
            event: &ddd_cqrs_es::EventEnvelope<CounterEvent, String>,
        ) -> Result<(), Self::Error> {
            if let CounterEvent::Incremented { by } = event.payload {
                if self.total + by > self.trip_at {
                    return Err("tripped");
                }
                self.total += by;
            }
            Ok(())
        }
    }

    let store = InMemoryEventStore::<Counter>::new();
    let repo = Repository::new(store.clone());
    let counter_id = "counter-1".to_owned();

    repo.execute(&counter_id, CounterCommand::Create, Metadata::default())
        .unwrap();
    for _ in 0..4 {
        repo.execute(
            &counter_id,
            CounterCommand::Increment { by: 1 },
            Metadata::default(),
        )
        .unwrap();
    }

    let checkpoint_store = CountingCheckpointStore::default();
    let projection = TrippingProjection {
        total: 0,
        trip_at: 2,
    };
    let mut runner = PersistedProjectionRunner::new(projection, checkpoint_store.clone());

    // Events: Created (seq 1), then 4 increments (seq 2-5). The projection
    // trips on seq 4, so progress through seq 3 must still be checkpointed.
    let error = runner.run::<Counter, _>(&store).unwrap_err();

    assert!(matches!(
        error,
        ProjectionRunnerError::Projection("tripped")
    ));
    assert_eq!(checkpoint_store.checkpoint(), Some(3));
    assert_eq!(checkpoint_store.saves(), 1);
}

#[test]
fn persisted_projection_runner_surfaces_checkpoint_failure_with_projection_error() {
    use ddd_cqrs_es::projection::{PersistedProjectionRunner, ProjectionRunnerError};

    struct TrippingProjection {
        total: u64,
        trip_at: u64,
    }

    impl Projection<CounterEvent, String> for TrippingProjection {
        type Error = &'static str;

        fn name(&self) -> &'static str {
            "tripping_projection"
        }

        fn apply(
            &mut self,
            event: &ddd_cqrs_es::EventEnvelope<CounterEvent, String>,
        ) -> Result<(), Self::Error> {
            if let CounterEvent::Incremented { by } = event.payload {
                if self.total + by > self.trip_at {
                    return Err("tripped");
                }
                self.total += by;
            }
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct FailingCheckpointStore;

    impl ddd_cqrs_es::projection::CheckpointStore for FailingCheckpointStore {
        type Error = &'static str;

        fn load_checkpoint(&self, _projection_name: &str) -> Result<Option<u64>, Self::Error> {
            Ok(None)
        }

        fn save_checkpoint(
            &self,
            _projection_name: &str,
            _sequence: u64,
        ) -> Result<(), Self::Error> {
            Err("checkpoint down")
        }

        fn reset_checkpoint(&self, _projection_name: &str) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    let store = InMemoryEventStore::<Counter>::new();
    let repo = Repository::new(store.clone());
    let counter_id = "counter-1".to_owned();

    repo.execute(&counter_id, CounterCommand::Create, Metadata::default())
        .unwrap();
    for _ in 0..4 {
        repo.execute(
            &counter_id,
            CounterCommand::Increment { by: 1 },
            Metadata::default(),
        )
        .unwrap();
    }

    let mut runner = PersistedProjectionRunner::new(
        TrippingProjection {
            total: 0,
            trip_at: 2,
        },
        FailingCheckpointStore,
    );

    let error = runner.run::<Counter, _>(&store).unwrap_err();
    assert!(matches!(
        error,
        ProjectionRunnerError::PartialBatchFailure {
            projection: "tripped",
            checkpoint: "checkpoint down",
        }
    ));
}

#[test]
fn persisted_projection_runner_flushes_before_advancing_checkpoint() {
    use ddd_cqrs_es::projection::PersistedProjectionRunner;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct BatchingProjection {
        pending: u64,
        persisted: u64,
        flush_calls: Arc<Mutex<usize>>,
    }

    impl Projection<CounterEvent, String> for BatchingProjection {
        type Error = std::convert::Infallible;

        fn name(&self) -> &'static str {
            "batching_projection"
        }

        fn apply(
            &mut self,
            event: &ddd_cqrs_es::EventEnvelope<CounterEvent, String>,
        ) -> Result<(), Self::Error> {
            if let CounterEvent::Incremented { by } = event.payload {
                self.pending += by;
            }
            Ok(())
        }

        fn flush(&mut self) -> Result<(), Self::Error> {
            *self.flush_calls.lock().unwrap() += 1;
            self.persisted += self.pending;
            self.pending = 0;
            Ok(())
        }
    }

    let store = InMemoryEventStore::<Counter>::new();
    let repo = Repository::new(store.clone());
    let counter_id = "counter-1".to_owned();

    repo.execute(&counter_id, CounterCommand::Create, Metadata::default())
        .unwrap();
    for _ in 0..3 {
        repo.execute(
            &counter_id,
            CounterCommand::Increment { by: 1 },
            Metadata::default(),
        )
        .unwrap();
    }

    let flush_calls = Arc::new(Mutex::new(0));
    let projection = BatchingProjection {
        flush_calls: Arc::clone(&flush_calls),
        ..Default::default()
    };
    let mut runner = PersistedProjectionRunner::new(projection, CountingCheckpointStore::default());

    assert_eq!(runner.run::<Counter, _>(&store).unwrap(), 4);
    let projection = runner.into_projection();
    assert_eq!(projection.persisted, 3);
    assert_eq!(projection.pending, 0);
    assert_eq!(*flush_calls.lock().unwrap(), 1);
}

#[test]
fn checkpointed_projection_runner_supports_batch_apply_hook() {
    use ddd_cqrs_es::projection::{CheckpointedProjection, CheckpointedProjectionRunner};

    struct BatchCounter {
        total: u64,
        checkpoint: Option<u64>,
        batch_sizes: Vec<usize>,
    }

    impl CheckpointedProjection<CounterEvent, String> for BatchCounter {
        type Error = std::convert::Infallible;

        fn name(&self) -> &'static str {
            "batch_counter"
        }

        fn load_checkpoint(&self) -> Result<Option<u64>, Self::Error> {
            Ok(self.checkpoint)
        }

        fn apply_batch_and_checkpoint(
            &mut self,
            events: &[ddd_cqrs_es::EventEnvelope<CounterEvent, String>],
        ) -> Result<(), Self::Error> {
            self.batch_sizes.push(events.len());
            for event in events {
                if let CounterEvent::Incremented { by } = event.payload {
                    self.total += by;
                }
                self.checkpoint = event.sequence;
            }
            Ok(())
        }

        fn apply_and_checkpoint(
            &mut self,
            _event: &ddd_cqrs_es::EventEnvelope<CounterEvent, String>,
        ) -> Result<(), Self::Error> {
            panic!("runner should call apply_batch_and_checkpoint");
        }
    }

    let store = InMemoryEventStore::<Counter>::new();
    let repo = Repository::new(store.clone());
    let counter_id = "counter-1".to_owned();

    repo.execute(&counter_id, CounterCommand::Create, Metadata::default())
        .unwrap();
    for _ in 0..2 {
        repo.execute(
            &counter_id,
            CounterCommand::Increment { by: 1 },
            Metadata::default(),
        )
        .unwrap();
    }

    let mut runner = CheckpointedProjectionRunner::new(BatchCounter {
        total: 0,
        checkpoint: None,
        batch_sizes: Vec::new(),
    });

    assert_eq!(runner.run::<Counter, _>(&store).unwrap(), 3);
    let projection = runner.into_projection();
    assert_eq!(projection.total, 2);
    assert_eq!(projection.batch_sizes, vec![3]);
}

#[cfg(feature = "postgres")]
#[test]
fn postgres_store_upcasts_chained_event_versions_on_load() {
    let Ok(database_url) = std::env::var("DDD_CQRS_ES_POSTGRES_URL") else {
        skip_live_test("Postgres upcaster test", "DDD_CQRS_ES_POSTGRES_URL");
        return;
    };
    use ddd_cqrs_es::EventUpcaster;

    struct Upcaster1To2;
    impl EventUpcaster for Upcaster1To2 {
        type Error = std::convert::Infallible;
        fn source_version(&self) -> u32 {
            1
        }
        fn target_version(&self) -> u32 {
            2
        }
        fn upcast(&self, raw_payload: Vec<u8>) -> Result<Vec<u8>, Self::Error> {
            let s = String::from_utf8(raw_payload).unwrap();
            let upgraded = s.replace("OldCreated", "V2Created");
            Ok(upgraded.into_bytes())
        }
    }

    struct Upcaster2To3;
    impl EventUpcaster for Upcaster2To3 {
        type Error = std::convert::Infallible;
        fn source_version(&self) -> u32 {
            2
        }
        fn target_version(&self) -> u32 {
            3
        }
        fn upcast(&self, raw_payload: Vec<u8>) -> Result<Vec<u8>, Self::Error> {
            let s = String::from_utf8(raw_payload).unwrap();
            let upgraded = s.replace("V2Created", "Created");
            Ok(upgraded.into_bytes())
        }
    }

    let mut client = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
    let table_name = format!("events_upcast_{}", std::process::id());
    let _ = client.execute(&format!("DROP TABLE IF EXISTS {};", table_name), &[]);

    let store = ddd_cqrs_es::PostgresEventStore::<Counter>::connect_with_table_name(
        &database_url,
        table_name.clone(),
    )
    .unwrap();
    store.initialize_schema().unwrap();

    client.execute(
        &format!(
            "INSERT INTO {} (event_id, aggregate_id, aggregate_type, revision, event_type, event_version, payload, metadata, recorded_at_ms)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            table_name
        ),
        &[
            &"my-test-event-id".to_owned(),
            &"\"counter-123\"".to_owned(),
            &"counter".to_owned(),
            &1i64,
            &"counter_created".to_owned(),
            &1i32,
            &serde_json::to_value("OldCreated").unwrap(),
            &serde_json::to_value(Metadata::default()).unwrap(),
            &1700000000000i64,
        ]
    ).unwrap();

    store
        .register_upcaster("counter_created", Upcaster1To2)
        .unwrap();
    store
        .register_upcaster("counter_created", Upcaster2To3)
        .unwrap();

    let events = ddd_cqrs_es::EventStore::load(&store, &"counter-123".to_owned()).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].payload, CounterEvent::Created);
    assert_eq!(events[0].event_version, 3);

    let _ = client.execute(&format!("DROP TABLE IF EXISTS {};", table_name), &[]);
}

#[cfg(feature = "postgres")]
#[test]
fn test_postgres_checkpoint_store() {
    let Ok(database_url) = std::env::var("DDD_CQRS_ES_POSTGRES_URL") else {
        skip_live_test("Postgres checkpoint test", "DDD_CQRS_ES_POSTGRES_URL");
        return;
    };
    use ddd_cqrs_es::PostgresCheckpointStore;

    let mut client = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
    let table_name = format!("checkpoints_{}", std::process::id());
    let _ = client.execute(&format!("DROP TABLE IF EXISTS {};", table_name), &[]);

    let store = PostgresCheckpointStore::with_table_name(client, table_name.clone()).unwrap();

    assert_checkpoint_store_contract(store, "proj1");

    let mut client = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
    let _ = client.execute(&format!("DROP TABLE IF EXISTS {};", table_name), &[]);
}

#[cfg(feature = "postgres")]
#[test]
fn test_postgres_raw_feed_interleaves_aggregate_types() {
    let Ok(database_url) = std::env::var("DDD_CQRS_ES_POSTGRES_URL") else {
        skip_live_test("Postgres raw-feed test", "DDD_CQRS_ES_POSTGRES_URL");
        return;
    };
    use ddd_cqrs_es::raw_feed::RawEventFeed;
    use ddd_cqrs_es::PostgresEventStore;

    let table = format!("raw_feed_events_{}", std::process::id());
    let drop_table = |database_url: &str| {
        let mut client = postgres::Client::connect(database_url, postgres::NoTls).unwrap();
        let _ = client.execute(&format!("DROP TABLE IF EXISTS {};", table), &[]);
    };
    drop_table(&database_url);

    let counters =
        PostgresEventStore::<Counter>::connect_with_table_name(&database_url, &table).unwrap();
    counters.initialize_schema().unwrap();
    let audits =
        PostgresEventStore::<RawAudit>::connect_with_table_name(&database_url, &table).unwrap();

    counters
        .append(
            &"counter-raw".to_owned(),
            ExpectedRevision::NoStream,
            vec![NewEvent::new(CounterEvent::Created, Metadata::default())],
        )
        .unwrap();
    audits
        .append(
            &"audit-raw".to_owned(),
            ExpectedRevision::NoStream,
            vec![NewEvent::new(
                RawAuditEvent::Recorded {
                    note: "first".to_owned(),
                },
                Metadata::default(),
            )],
        )
        .unwrap();

    let raw = counters
        .load_raw_global_after_limited(None, std::num::NonZeroUsize::new(10).unwrap())
        .unwrap();
    assert_eq!(raw.len(), 2);
    assert_eq!(raw[0].aggregate_type, "counter");
    assert_eq!(raw[1].aggregate_type, "raw_audit");
    assert_eq!(raw[1].payload["Recorded"]["note"], "first");
    let typed = EventStore::load_global_after(&counters, None).unwrap();
    assert_eq!(typed.len(), 1);

    drop_table(&database_url);
}

#[cfg(feature = "postgres")]
#[test]
fn test_postgres_pool_sharing_auxiliary_stores() {
    let Ok(database_url) = std::env::var("DDD_CQRS_ES_POSTGRES_URL") else {
        skip_live_test("Postgres pool-sharing test", "DDD_CQRS_ES_POSTGRES_URL");
        return;
    };
    use ddd_cqrs_es::{assert_snapshot_store_contract, PostgresEventStore};

    let pid = std::process::id();
    let events_table = format!("shared_events_{pid}");
    let checkpoints_table = format!("shared_checkpoints_{pid}");
    let idempotency_table = format!("shared_idem_{pid}");
    let snapshots_table = format!("shared_snapshots_{pid}");
    let tables = [
        events_table.clone(),
        checkpoints_table.clone(),
        idempotency_table.clone(),
        snapshots_table.clone(),
    ];
    let drop_tables = |database_url: &str| {
        let mut client = postgres::Client::connect(database_url, postgres::NoTls).unwrap();
        for table in &tables {
            let _ = client.execute(&format!("DROP TABLE IF EXISTS {};", table), &[]);
        }
    };
    drop_tables(&database_url);

    let store = PostgresEventStore::<Counter>::connect_pooled_with_table_name(
        &database_url,
        &events_table,
        4,
    )
    .unwrap();

    let checkpoints = store
        .checkpoint_store_with_table_name(checkpoints_table.clone())
        .unwrap();
    assert_checkpoint_store_contract(checkpoints, "shared_pool_projection");

    let idempotency = store
        .idempotency_store_with_table_name::<StoredIdempotencyResult>(idempotency_table.clone())
        .unwrap();
    assert_sql_idempotency_store_contract(idempotency);

    let snapshots = store
        .snapshot_store_with_table_name(snapshots_table.clone())
        .unwrap();
    let older = Counter {
        id: Some("shared-counter".to_owned()),
        value: 1,
    };
    let newer = Counter {
        id: Some("shared-counter".to_owned()),
        value: 2,
    };
    assert_snapshot_store_contract(snapshots, "shared-counter".to_owned(), older, newer);

    drop_tables(&database_url);
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_raw_feed_interleaves_aggregate_types_and_drives_raw_projections() {
    use ddd_cqrs_es::projection::PersistedProjectionRunner;
    use ddd_cqrs_es::raw_feed::RawEventFeed;

    #[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    enum AuditEvent {
        Recorded { note: String },
    }

    impl DomainEvent for AuditEvent {
        fn event_type(&self) -> &'static str {
            "audit_recorded"
        }
    }

    #[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    struct Audit {
        notes: u64,
    }

    impl Aggregate for Audit {
        type Id = String;
        type Command = String;
        type Event = AuditEvent;
        type Error = std::convert::Infallible;

        fn aggregate_type() -> &'static str {
            "audit"
        }

        fn new() -> Self {
            Self::default()
        }

        fn apply(&mut self, _event: &Self::Event) {
            self.notes += 1;
        }

        fn handle(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
            Ok(vec![AuditEvent::Recorded { note: command }])
        }
    }

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let db_uri = format!("file:raw_feed_{nanos}?mode=memory&cache=shared");
    // Keep the shared in-memory database alive for the whole test.
    let _anchor = rusqlite::Connection::open(&db_uri).unwrap();

    let counters =
        ddd_cqrs_es::SqliteEventStore::<Counter>::new(rusqlite::Connection::open(&db_uri).unwrap())
            .unwrap();
    counters.initialize_schema().unwrap();
    let audits =
        ddd_cqrs_es::SqliteEventStore::<Audit>::new(rusqlite::Connection::open(&db_uri).unwrap())
            .unwrap();

    // Interleave appends across the two aggregate types in one events table.
    counters
        .append(
            &"counter-1".to_owned(),
            ExpectedRevision::NoStream,
            vec![NewEvent::new(CounterEvent::Created, Metadata::default())],
        )
        .unwrap();
    audits
        .append(
            &"audit-1".to_owned(),
            ExpectedRevision::NoStream,
            vec![NewEvent::new(
                AuditEvent::Recorded {
                    note: "first".to_owned(),
                },
                Metadata::default(),
            )],
        )
        .unwrap();
    counters
        .append(
            &"counter-1".to_owned(),
            ExpectedRevision::Exact(1),
            vec![NewEvent::new(
                CounterEvent::Incremented { by: 2 },
                Metadata::default(),
            )],
        )
        .unwrap();

    // The raw feed sees every event of both types in global sequence order,
    // while the typed feed stays scoped to one aggregate type.
    let raw = counters
        .load_raw_global_after_limited(None, std::num::NonZeroUsize::new(10).unwrap())
        .unwrap();
    assert_eq!(raw.len(), 3);
    assert_eq!(
        raw.iter().map(|e| e.sequence.unwrap()).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(raw[0].aggregate_type, "counter");
    assert_eq!(raw[1].aggregate_type, "audit");
    assert_eq!(raw[1].payload["Recorded"]["note"], "first");
    assert_eq!(raw[1].aggregate_id, "\"audit-1\"");
    let typed = EventStore::load_global_after(&counters, None).unwrap();
    assert_eq!(typed.len(), 2);

    // A raw projection is just Projection<serde_json::Value, String>; the
    // persisted runner drives it with the shared checkpoint semantics.
    #[derive(Default)]
    struct TypeTally {
        counters: usize,
        audits: usize,
    }

    impl Projection<serde_json::Value, String> for TypeTally {
        type Error = std::convert::Infallible;

        fn name(&self) -> &'static str {
            "raw_type_tally"
        }

        fn apply(
            &mut self,
            event: &ddd_cqrs_es::EventEnvelope<serde_json::Value, String>,
        ) -> Result<(), Self::Error> {
            match event.aggregate_type.as_str() {
                "counter" => self.counters += 1,
                _ => self.audits += 1,
            }
            Ok(())
        }
    }

    let checkpoint_store = CountingCheckpointStore::default();
    let mut runner = PersistedProjectionRunner::new(TypeTally::default(), checkpoint_store.clone());

    let outcome = runner
        .run_raw_batch(&counters, ddd_cqrs_es::ProjectionBatchConfig::default())
        .unwrap();
    assert_eq!(outcome.applied, 3);
    assert!(outcome.caught_up);
    assert_eq!(runner.projection().counters, 2);
    assert_eq!(runner.projection().audits, 1);
    assert_eq!(checkpoint_store.checkpoint(), Some(3));
    assert_eq!(checkpoint_store.saves(), 1);

    // Resumes from the checkpoint: nothing new, no checkpoint write.
    let outcome = runner
        .run_raw_batch(&counters, ddd_cqrs_es::ProjectionBatchConfig::default())
        .unwrap();
    assert_eq!(outcome.applied, 0);
    assert_eq!(checkpoint_store.saves(), 1);

    // Cross-aggregate replay is a different position from any typed feed, so
    // under aggregate-scoped keying it gets its own checkpoint row.
    let keyed = KeyedCheckpointStore::default();
    let mut scoped_runner = PersistedProjectionRunner::with_aggregate_scoped_checkpoints(
        TypeTally::default(),
        keyed.clone(),
    );
    scoped_runner
        .run_raw_batch(&counters, ddd_cqrs_es::ProjectionBatchConfig::default())
        .unwrap();
    let mut typed_runner = PersistedProjectionRunner::with_aggregate_scoped_checkpoints(
        CounterProjection::default(),
        keyed.clone(),
    );
    typed_runner.run::<Counter, _>(&counters).unwrap();

    assert!(keyed
        .keys()
        .contains(&ddd_cqrs_es::raw_checkpoint_key("raw_type_tally")));
    assert!(!keyed.keys().contains(&"raw_type_tally".to_owned()));
}

#[cfg(feature = "sqlite")]
#[test]
fn test_sqlite_sequential_custom_table_initialization() {
    let connection = rusqlite::Connection::open("file::memory:?cache=shared").unwrap();

    // 1. Create event store with "custom_events_a" table name.
    let event_store = ddd_cqrs_es::sqlite::SqliteEventStore::<Counter>::with_table_name(
        connection,
        "custom_events_a".to_owned(),
    )
    .unwrap();
    event_store.initialize_schema().unwrap();

    // 2. Open another connection to the shared in-memory DB and create a checkpoint store with "custom_checkpoints_a".
    let connection2 = rusqlite::Connection::open("file::memory:?cache=shared").unwrap();
    let checkpoint_store = ddd_cqrs_es::sqlite::SqliteCheckpointStore::with_table_name(
        connection2,
        "custom_checkpoints_a",
    )
    .unwrap();

    // 3. Let's write to both of them to verify they work together and both tables exist!
    let id = "counter-123".to_owned();
    let events = vec![ddd_cqrs_es::NewEvent::new(
        CounterEvent::Created,
        Metadata::default(),
    )];
    event_store
        .append(&id, ddd_cqrs_es::ExpectedRevision::Any, events)
        .unwrap();

    use ddd_cqrs_es::projection::CheckpointStore;
    checkpoint_store
        .save_checkpoint("projection-a", 99)
        .unwrap();

    assert_eq!(event_store.load(&id).unwrap().len(), 1);
    assert_eq!(
        checkpoint_store.load_checkpoint("projection-a").unwrap(),
        Some(99)
    );
}

#[cfg(feature = "json-file")]
#[test]
fn in_memory_and_json_file_raw_feeds_serve_untyped_envelopes() {
    use ddd_cqrs_es::raw_feed::RawEventFeed;

    let limit = std::num::NonZeroUsize::new(10).unwrap();

    let store = InMemoryEventStore::<Counter>::new();
    store
        .append(
            &"counter-raw".to_owned(),
            ExpectedRevision::NoStream,
            vec![
                NewEvent::new(CounterEvent::Created, Metadata::default()),
                NewEvent::new(CounterEvent::Incremented { by: 4 }, Metadata::default()),
            ],
        )
        .unwrap();
    let raw = store.load_raw_global_after_limited(None, limit).unwrap();
    assert_eq!(raw.len(), 2);
    assert_eq!(raw[0].aggregate_id, "\"counter-raw\"");
    assert_eq!(raw[1].payload["Incremented"]["by"], 4);
    let resumed = store.load_raw_global_after_limited(Some(1), limit).unwrap();
    assert_eq!(resumed.len(), 1);

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let events_path = std::env::temp_dir().join(format!("test_events_raw_{}.json", nanos));
    let file_store = ddd_cqrs_es::JsonFileEventStore::<Counter>::new(events_path.clone());
    file_store
        .append(
            &"counter-raw".to_owned(),
            ExpectedRevision::NoStream,
            vec![NewEvent::new(CounterEvent::Created, Metadata::default())],
        )
        .unwrap();
    let raw = file_store
        .load_raw_global_after_limited(None, limit)
        .unwrap();
    assert_eq!(raw.len(), 1);
    assert_eq!(raw[0].aggregate_type, "counter");
    let _ = std::fs::remove_file(&events_path);
}

#[cfg(feature = "json-file")]
#[test]
fn json_file_store_migrates_legacy_array_files_to_json_lines() {
    let dir = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let events_path = dir.join(format!("test_events_legacy_{}.json", nanos));

    // Write a legacy whole-array file the way older releases did.
    let legacy = ddd_cqrs_es::EventEnvelope::builder(
        ddd_cqrs_es::EventId::from_string("legacy-1"),
        "counter-legacy".to_owned(),
        "counter",
        1,
        "counter_created",
        CounterEvent::Created,
    )
    .sequence(1)
    .build();
    let array = serde_json::to_string(&vec![&legacy]).unwrap();
    std::fs::write(&events_path, array).unwrap();

    let store = ddd_cqrs_es::JsonFileEventStore::<Counter>::new(events_path.clone());

    // First read parses legacy arrays in memory without rewriting the file.
    let loaded = EventStore::load(&store, &"counter-legacy".to_owned()).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].event_id.as_str(), "legacy-1");
    let still_legacy = std::fs::read_to_string(&events_path).unwrap();
    assert!(still_legacy.trim_start().starts_with('['));

    // Appends migrate the file to JSON Lines and add new events.
    store
        .append(
            &"counter-legacy".to_owned(),
            ExpectedRevision::Exact(1),
            vec![ddd_cqrs_es::NewEvent::new(
                CounterEvent::Incremented { by: 2 },
                Metadata::default(),
            )],
        )
        .unwrap();
    let migrated = std::fs::read_to_string(&events_path).unwrap();
    assert!(migrated.trim_start().starts_with('{'));
    assert_eq!(migrated.lines().count(), 2);
    let reloaded = EventStore::load(&store, &"counter-legacy".to_owned()).unwrap();
    assert_eq!(reloaded.len(), 2);
    assert_eq!(reloaded[1].revision, 2);
    assert_eq!(reloaded[1].sequence, Some(2));

    let _ = std::fs::remove_file(&events_path);
}

#[cfg(feature = "json-file")]
#[test]
fn test_json_file_concurrency_and_atomicity() {
    let dir = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let events_path = dir.join(format!("test_events_{}.json", nanos));
    let checkpoints_path = dir.join(format!("test_checkpoints_{}.json", nanos));

    let event_store = ddd_cqrs_es::JsonFileEventStore::<Counter>::new(events_path.clone());
    let checkpoint_store = ddd_cqrs_es::JsonFileCheckpointStore::new(checkpoints_path.clone());

    // Run parallel threads appending events concurrently
    let store_arc = std::sync::Arc::new(event_store);
    let mut handles = Vec::new();

    for i in 0..10 {
        let store = std::sync::Arc::clone(&store_arc);
        let agg_id = format!("thread-{}", i);
        let handle = thread::spawn(move || {
            let events = vec![ddd_cqrs_es::NewEvent::new(
                CounterEvent::Created,
                Metadata::default(),
            )];
            store
                .append(&agg_id, ddd_cqrs_es::ExpectedRevision::Any, events)
                .unwrap();
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    // Verify all 10 aggregates got created and saved without corruption!
    for i in 0..10 {
        let agg_id = format!("thread-{}", i);
        let stream = store_arc.load(&agg_id).unwrap();
        assert_eq!(stream.len(), 1);
    }

    // Verify checkpoints concurrent writes
    let cp_store = std::sync::Arc::new(checkpoint_store);
    let mut cp_handles = Vec::new();
    for i in 0..10 {
        let store = std::sync::Arc::clone(&cp_store);
        let proj_name = format!("proj-{}", i);
        let handle = thread::spawn(move || {
            use ddd_cqrs_es::projection::CheckpointStore;
            store.save_checkpoint(&proj_name, i as u64).unwrap();
        });
        cp_handles.push(handle);
    }

    for h in cp_handles {
        h.join().unwrap();
    }

    for i in 0..10 {
        let proj_name = format!("proj-{}", i);
        use ddd_cqrs_es::projection::CheckpointStore;
        assert_eq!(
            cp_store.load_checkpoint(&proj_name).unwrap(),
            Some(i as u64)
        );
    }

    // Cleanup
    let _ = std::fs::remove_file(events_path);
    let _ = std::fs::remove_file(checkpoints_path);
}

#[cfg(feature = "mysql")]
static MYSQL_TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(feature = "mysql")]
struct MySqlTestDb {
    test_url: String,
}

#[cfg(feature = "mysql")]
impl MySqlTestDb {
    fn new() -> Result<Option<Self>, String> {
        let test_url = match std::env::var("DDD_CQRS_ES_MYSQL_URL") {
            Ok(value) if value.trim().is_empty() => return Ok(None),
            Ok(value) => value.trim().to_owned(),
            Err(std::env::VarError::NotPresent) => return Ok(None),
            Err(error) => {
                return Err(format!(
                    "DDD_CQRS_ES_MYSQL_URL contains invalid unicode: {error}"
                ));
            }
        };

        mysql::Conn::new(test_url.as_str()).map_err(|error| {
            format!("failed to connect to MySQL URL from DDD_CQRS_ES_MYSQL_URL: {error}")
        })?;

        Ok(Some(Self { test_url }))
    }
}

#[cfg(feature = "mysql")]
fn unique_mysql_table(prefix: &str) -> String {
    format!(
        "{}_{}_{}",
        prefix,
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

#[cfg(feature = "mysql")]
struct MySqlTableCleanup {
    test_url: String,
    tables: Vec<String>,
}

#[cfg(feature = "mysql")]
impl MySqlTableCleanup {
    fn new(test_url: &str, tables: Vec<String>) -> Self {
        Self {
            test_url: test_url.to_owned(),
            tables,
        }
    }
}

#[cfg(feature = "mysql")]
impl Drop for MySqlTableCleanup {
    fn drop(&mut self) {
        if let Ok(mut conn) = mysql::Conn::new(self.test_url.as_str()) {
            use mysql::prelude::Queryable;
            for table in &self.tables {
                let _ = conn.query_drop(format!("DROP TABLE IF EXISTS `{table}`;"));
            }
        }
    }
}

#[cfg(feature = "mysql")]
fn mysql_test_db_or_skip(test_name: &str) -> Option<MySqlTestDb> {
    match MySqlTestDb::new() {
        Ok(Some(db)) => Some(db),
        Ok(None) => {
            skip_live_test(&format!("MySQL {test_name}"), "DDD_CQRS_ES_MYSQL_URL");
            None
        }
        Err(error) => panic!("failed to prepare live MySQL {test_name}: {error}"),
    }
}

#[cfg(feature = "mysql")]
#[test]
fn test_mysql_query_plans_and_v6_duplicate_index_cleanup() {
    let _guard = MYSQL_TEST_MUTEX
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(db) = mysql_test_db_or_skip("query-plan test") else {
        return;
    };
    use mysql::prelude::Queryable;

    let table_name = unique_mysql_table("events_plan");
    let _cleanup = MySqlTableCleanup::new(&db.test_url, vec![table_name.clone()]);

    let store = ddd_cqrs_es::MySqlEventStore::<Counter>::connect_with_table_name(
        &db.test_url,
        table_name.clone(),
    )
    .unwrap();
    store.initialize_schema().unwrap();

    let mut conn = mysql::Conn::new(db.test_url.as_str()).unwrap();
    let duplicate_index_name = format!("{table_name}_stream_idx");
    conn.query_drop(format!(
        "CREATE INDEX {duplicate_index_name} ON {table_name} (aggregate_type, aggregate_id, revision);"
    ))
    .unwrap();
    conn.exec_drop(
        "DELETE FROM schema_migrations WHERE version = ? AND table_name = ?;",
        (6i32, table_name.as_str()),
    )
    .unwrap();
    let config = ddd_cqrs_es::SqlSchemaConfig::new(ddd_cqrs_es::SqlDialect::MySql)
        .with_events_table(table_name.clone())
        .unwrap();
    ddd_cqrs_es::SchemaMigrator::new(config)
        .run_mysql(&mut conn)
        .unwrap();

    let duplicate_index_count: u64 = conn
        .exec_first(
            "SELECT COUNT(1)
             FROM information_schema.statistics
             WHERE table_schema = DATABASE()
               AND table_name = ?
               AND index_name = ?;",
            (table_name.as_str(), duplicate_index_name.as_str()),
        )
        .unwrap()
        .unwrap_or(0);
    assert_eq!(duplicate_index_count, 0);

    let repo = Repository::new(store);
    for index in 0..50 {
        let counter_id = format!("mysql-plan-counter-{index}");
        repo.execute(&counter_id, CounterCommand::Create, Metadata::default())
            .unwrap();
        repo.execute(
            &counter_id,
            CounterCommand::Increment { by: 1 },
            Metadata::default(),
        )
        .unwrap();
    }

    let global_plan: String = conn
        .exec_first(
            format!(
                "EXPLAIN FORMAT=JSON
                 SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, event_version, payload, metadata, recorded_at_ms
                 FROM {table_name}
                 WHERE aggregate_type = ? AND sequence > ?
                 ORDER BY sequence ASC
                 LIMIT ?"
            ),
            ("counter", 0i64, 10u64),
        )
        .unwrap()
        .unwrap();
    assert!(
        global_plan.contains(&format!("{table_name}_global_replay_idx")),
        "expected MySQL global replay query to use the global replay index, got:\n{global_plan}"
    );

    let aggregate_id = serde_json::to_string("mysql-plan-counter-1").unwrap();
    let stream_plan: String = conn
        .exec_first(
            format!(
                "EXPLAIN FORMAT=JSON
                 SELECT event_id, aggregate_id, aggregate_type, revision, sequence, event_type, event_version, payload, metadata, recorded_at_ms
                 FROM {table_name}
                 WHERE aggregate_type = ? AND aggregate_id = ?
                 ORDER BY revision ASC"
            ),
            ("counter", aggregate_id.as_str()),
        )
        .unwrap()
        .unwrap();
    let stream_plan = stream_plan.to_ascii_lowercase();
    assert!(
        stream_plan.contains("\"key\": \"aggregate_type\""),
        "expected MySQL stream query to use the unique stream key, got:\n{stream_plan}"
    );

    let latest_plan: String = conn
        .query_first(format!(
            "EXPLAIN FORMAT=JSON
             SELECT sequence, event_type, revision, payload, recorded_at_ms
             FROM {table_name}
             ORDER BY sequence DESC
             LIMIT 5"
        ))
        .unwrap()
        .unwrap();
    assert!(
        latest_plan.to_ascii_lowercase().contains("primary"),
        "expected MySQL latest-ledger query to use the primary key order, got:\n{latest_plan}"
    );
}

#[cfg(feature = "mysql")]
#[test]
fn test_mysql_checkpoint_store() {
    let _guard = MYSQL_TEST_MUTEX
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(db) = mysql_test_db_or_skip("checkpoint test") else {
        return;
    };
    use ddd_cqrs_es::MySqlCheckpointStore;

    let conn = mysql::Conn::new(db.test_url.as_str()).unwrap();
    let table_name = unique_mysql_table("checkpoints");
    let _cleanup = MySqlTableCleanup::new(&db.test_url, vec![table_name.clone()]);

    let store = MySqlCheckpointStore::with_table_name(conn, table_name.clone()).unwrap();

    assert_checkpoint_store_contract(store, "proj1");
}

#[cfg(feature = "mysql")]
#[test]
fn test_mysql_raw_feed_interleaves_aggregate_types() {
    let _guard = MYSQL_TEST_MUTEX
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(db) = mysql_test_db_or_skip("raw-feed test") else {
        return;
    };
    use ddd_cqrs_es::raw_feed::RawEventFeed;
    use ddd_cqrs_es::MySqlEventStore;

    let table = unique_mysql_table("raw_feed_events");
    let _cleanup = MySqlTableCleanup::new(&db.test_url, vec![table.clone()]);

    let counters =
        MySqlEventStore::<Counter>::connect_with_table_name(&db.test_url, &table).unwrap();
    counters.initialize_schema().unwrap();
    let audits =
        MySqlEventStore::<RawAudit>::connect_with_table_name(&db.test_url, &table).unwrap();

    counters
        .append(
            &"counter-raw".to_owned(),
            ExpectedRevision::NoStream,
            vec![NewEvent::new(CounterEvent::Created, Metadata::default())],
        )
        .unwrap();
    audits
        .append(
            &"audit-raw".to_owned(),
            ExpectedRevision::NoStream,
            vec![NewEvent::new(
                RawAuditEvent::Recorded {
                    note: "first".to_owned(),
                },
                Metadata::default(),
            )],
        )
        .unwrap();

    let raw = counters
        .load_raw_global_after_limited(None, std::num::NonZeroUsize::new(10).unwrap())
        .unwrap();
    assert_eq!(raw.len(), 2);
    assert_eq!(raw[0].aggregate_type, "counter");
    assert_eq!(raw[1].aggregate_type, "raw_audit");
    assert_eq!(raw[1].payload["Recorded"]["note"], "first");
    let typed = EventStore::load_global_after(&counters, None).unwrap();
    assert_eq!(typed.len(), 1);
}

#[cfg(feature = "mysql")]
#[test]
fn test_mysql_pool_sharing_auxiliary_stores() {
    let _guard = MYSQL_TEST_MUTEX
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(db) = mysql_test_db_or_skip("pool-sharing test") else {
        return;
    };
    use ddd_cqrs_es::{assert_snapshot_store_contract, MySqlEventStore};

    let events_table = unique_mysql_table("shared_events");
    let checkpoints_table = unique_mysql_table("shared_checkpoints");
    let idempotency_table = unique_mysql_table("shared_idem");
    let snapshots_table = unique_mysql_table("shared_snapshots");
    let _cleanup = MySqlTableCleanup::new(
        &db.test_url,
        vec![
            events_table.clone(),
            checkpoints_table.clone(),
            idempotency_table.clone(),
            snapshots_table.clone(),
        ],
    );

    let store =
        MySqlEventStore::<Counter>::connect_pooled_with_table_name(&db.test_url, &events_table, 4)
            .unwrap();

    let checkpoints = store
        .checkpoint_store_with_table_name(checkpoints_table.clone())
        .unwrap();
    assert_checkpoint_store_contract(checkpoints, "shared_pool_projection");

    let idempotency = store
        .idempotency_store_with_table_name::<StoredIdempotencyResult>(idempotency_table.clone())
        .unwrap();
    assert_sql_idempotency_store_contract(idempotency);

    let snapshots = store
        .snapshot_store_with_table_name(snapshots_table.clone())
        .unwrap();
    let older = Counter {
        id: Some("shared-counter".to_owned()),
        value: 1,
    };
    let newer = Counter {
        id: Some("shared-counter".to_owned()),
        value: 2,
    };
    assert_snapshot_store_contract(snapshots, "shared-counter".to_owned(), older, newer);
}
