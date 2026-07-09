//! # ddd_cqrs_es
//!
//! A lightweight framework for building Domain-Driven Design, CQRS, and Event
//! Sourcing applications in Rust.
//!
//! The crate provides explicit building blocks for aggregate command handling,
//! optimistic concurrency, event replay, projections, process managers,
//! snapshots, and pluggable persistence backends. The included in-memory store
//! is intended for tests, examples, and local development.
//!
//! # Example
//!
//! ```
//! use ddd_cqrs_es::{Aggregate, DomainEvent, InMemoryEventStore, Metadata, Repository};
//!
//! #[derive(Clone)]
//! enum CounterEvent {
//!     Created,
//!     Incremented(u64),
//! }
//!
//! impl DomainEvent for CounterEvent {
//!     fn event_type(&self) -> &'static str {
//!         match self {
//!             CounterEvent::Created => "counter_created",
//!             CounterEvent::Incremented(_) => "counter_incremented",
//!         }
//!     }
//! }
//!
//! enum CounterCommand {
//!     Create,
//!     Increment(u64),
//! }
//!
//! #[derive(Default)]
//! struct Counter {
//!     exists: bool,
//!     value: u64,
//!     revision: u64,
//! }
//!
//! #[derive(Debug)]
//! enum CounterError {
//!     AlreadyCreated,
//!     NotCreated,
//! }
//!
//! impl Aggregate for Counter {
//!     type Id = String;
//!     type Command = CounterCommand;
//!     type Event = CounterEvent;
//!     type Error = CounterError;
//!
//!     fn aggregate_type() -> &'static str { "counter" }
//!     fn revision(&self) -> u64 { self.revision }
//!     fn new() -> Self { Self::default() }
//!
//!     fn apply(&mut self, event: &Self::Event) {
//!         match event {
//!             CounterEvent::Created => self.exists = true,
//!             CounterEvent::Incremented(by) => self.value += by,
//!         }
//!         self.revision += 1;
//!     }
//!
//!     fn handle(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
//!         match command {
//!             CounterCommand::Create if self.exists => Err(CounterError::AlreadyCreated),
//!             CounterCommand::Create => Ok(vec![CounterEvent::Created]),
//!             CounterCommand::Increment(_) if !self.exists => Err(CounterError::NotCreated),
//!             CounterCommand::Increment(by) => Ok(vec![CounterEvent::Incremented(by)]),
//!         }
//!     }
//! }
//!
//! let store = InMemoryEventStore::<Counter>::new();
//! let repo = Repository::new(store);
//! let counter_id = "counter-1".to_owned();
//!
//! repo.execute(&counter_id, CounterCommand::Create, Metadata::default())?;
//! repo.execute(&counter_id, CounterCommand::Increment(5), Metadata::default())?;
//! let loaded = repo.load(&counter_id)?;
//!
//! assert_eq!(loaded.state.value, 5);
//! # Ok::<(), ddd_cqrs_es::RepositoryError<CounterError>>(())
//! ```

#[cfg(feature = "json")]
pub mod adapters;
pub mod aggregate;
#[cfg(feature = "async")]
pub mod async_api;
pub mod command;
pub mod error;
pub mod event;
pub mod event_store;
pub mod idempotency;
pub mod memory;
pub mod metadata;
#[cfg(feature = "mysql")]
pub mod mysql;
#[cfg(feature = "postgres")]
pub mod postgres;
pub mod process_manager;
pub mod projection;
#[cfg(feature = "redis")]
pub mod redis;
pub mod repository;
mod repository_support;
pub mod schema;
pub mod snapshot;
mod sql_common;
#[cfg(feature = "sqlite")]
pub mod sqlite;
pub mod testing;
pub mod upcast;

/// Core aggregate and command types for building DDD/CQRS domains.
pub use aggregate::{Aggregate, LoadedAggregate};
#[cfg(feature = "async")]
/// Async service traits and result types for asynchronous command execution.
pub use async_api::{
    AsyncAtomicIdempotentEventStore, AsyncAtomicIdempotentRepositoryResult, AsyncCommandBus,
    AsyncCommandHandler, AsyncEventStore, AsyncIdempotencyStore, AsyncQueryHandler,
    AsyncRepository, AsyncRepositoryResult, AsyncSnapshotStore,
};

#[cfg(feature = "json-file")]
/// JSON-backed adapters for local persistence and replay workflows.
pub use adapters::{JsonFileCheckpointStore, JsonFileEventStore};
/// Command and query dispatch interfaces.
pub use command::{CommandBus, CommandHandler, QueryHandler};
/// Top-level repository and event-store error types.
pub use error::{ConcurrencyError, EventStoreError, EventStoreFailure, RepositoryError};
/// Shared event envelope and stream metadata types.
pub use event::{
    DomainEvent, EventEnvelope, EventId, EventType, ExpectedRevision, NewEvent, Revision,
    INITIAL_REVISION,
};
/// Primary event store contracts and append semantics.
pub use event_store::{
    AtomicIdempotentEventStore, EventStore, EventStream, IdempotentAppendError, StandardEventStore,
};
/// Idempotency APIs and error types shared across adapters.
pub use idempotency::{
    IdempotencyKey, IdempotencyState, IdempotencyStore, IdempotencyWaitConfig,
    IdempotentRepositoryError, InMemoryIdempotencyError, InMemoryIdempotencyStore,
    DEFAULT_IDEMPOTENCY_PENDING_TIMEOUT, DEFAULT_IDEMPOTENCY_POLL_INTERVAL,
};
/// In-memory event store implementation for tests and local development.
pub use memory::InMemoryEventStore;
/// Correlation and metadata carrier used by all emitted events.
pub use metadata::Metadata;
#[cfg(feature = "mysql")]
/// MySQL persistence adapters for events, checkpoints, idempotency, and snapshots.
pub use mysql::{MySqlCheckpointStore, MySqlEventStore, MySqlIdempotencyStore, MySqlSnapshotStore};
#[cfg(feature = "postgres")]
/// PostgreSQL persistence adapters for events, checkpoints, idempotency, and snapshots.
pub use postgres::{
    PostgresCheckpointStore, PostgresEventStore, PostgresIdempotencyStore, PostgresSnapshotStore,
};
#[cfg(feature = "async")]
/// Async process manager implementation for background consistency jobs.
pub use process_manager::AsyncProcessManagerRunner;
/// Synchronous process manager abstractions and runner APIs.
pub use process_manager::{ProcessManager, ProcessManagerRunner, ProcessManagerRunnerError};
#[cfg(feature = "async")]
/// Async projection and replay runner interfaces.
pub use projection::{
    AsyncCheckpointStore, AsyncCheckpointedProjection, AsyncCheckpointedProjectionRunner,
    AsyncPersistedProjectionRunner, AsyncTransactionalCheckpointedProjection,
    AsyncTransactionalCheckpointedProjectionRunner,
};
/// Projection runners, batch controls, and checkpoint integrations.
pub use projection::{
    CheckpointStore, CheckpointedProjection, CheckpointedProjectionRunner,
    InMemoryProjectionRunner, PersistedProjectionRunner, Projection, ProjectionBatchConfig,
    ProjectionBatchOutcome, ProjectionRunnerError, TransactionalCheckpointedProjection,
    TransactionalCheckpointedProjectionRunner, DEFAULT_PROJECTION_BATCH_SIZE,
};
#[cfg(feature = "spin-redis")]
/// Spin Redis client handle when the Spin Redis client feature is enabled.
pub use redis::SpinRedisClient;
#[cfg(feature = "wasi-redis")]
/// WASI Redis client handle when the WASI Redis feature is enabled.
pub use redis::WasiRedisClient;
#[cfg(feature = "redis")]
/// Redis checkpoint/event/publish adapters for production distributed usage.
pub use redis::{
    RedisCheckpointStore, RedisCommandExecutor, RedisEventStore, RedisPubSubPublisher,
};
/// Repository contracts and execution outcome types.
pub use repository::{
    AtomicIdempotentRepositoryResult, CommittedEvents, ExecutionOutcome,
    IdempotentRepositoryResult, Repository, RepositoryResult, SnapshotRepositoryResult,
};
#[cfg(feature = "async")]
/// Async schema bootstrap helper.
pub use schema::AsyncSchemaInitializer;
/// Schema migration metadata and schema bootstrap types.
pub use schema::{SchemaMigration, SchemaMigrator, SqlDialect, SqlSchemaConfig};
/// Snapshot domain types and store/error contracts.
pub use snapshot::{
    InMemorySnapshotError, InMemorySnapshotStore, Snapshot, SnapshotRepositoryError, SnapshotStore,
};
#[cfg(feature = "sqlite")]
/// SQLite persistence adapters for events, checkpoints, idempotency, and snapshots.
pub use sqlite::{
    SqliteCheckpointStore, SqliteEventStore, SqliteIdempotencyStore, SqliteSnapshotStore,
};
#[cfg(feature = "async")]
/// Async contract tests for async adapters.
pub use testing::{
    assert_async_checkpoint_store_contract, assert_async_event_store_contract,
    assert_async_idempotency_store_contract,
};
/// Contract tests for adapters and supporting test utilities.
pub use testing::{
    assert_checkpoint_store_contract, assert_event_store_contract,
    assert_event_store_global_replay_contract, assert_idempotency_store_contract,
    assert_snapshot_store_contract, AggregateFixture, EventStoreContractOptions,
};
/// Upcaster abstractions for event schema migration over time.
pub use upcast::{ErasedUpcaster, EventUpcaster, UpcasterRegistry};
