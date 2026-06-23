//! # ddd_cqrs_es
//!
//! A lightweight framework for building Domain-Driven Design, CQRS, and Event
//! Sourcing applications in Rust.
//!
//! The crate provides explicit building blocks for aggregate command handling,
//! optimistic concurrency, event replay, projections, process managers,
//! snapshots, and pluggable persistence backends. The included in-memory store
//! is intended for tests, examples, and local development.

pub mod aggregate;
pub mod command;
pub mod error;
pub mod event;
pub mod event_store;
pub mod memory;
pub mod metadata;
pub mod process_manager;
pub mod projection;
pub mod repository;
pub mod snapshot;
pub mod store;
pub mod testing;

pub use aggregate::{Aggregate, LoadedAggregate};
pub use command::{CommandBus, CommandHandler, QueryHandler};
pub use error::{ConcurrencyError, EventStoreError, RepositoryError};
pub use event::{EventEnvelope, EventId, ExpectedRevision, NewEvent, Revision, INITIAL_REVISION};
pub use event_store::{EventStore, EventStream, StandardEventStore};
pub use memory::InMemoryEventStore;
pub use metadata::Metadata;
pub use process_manager::ProcessManager;
pub use projection::{InMemoryProjectionRunner, Projection, ProjectionRunnerError};
pub use repository::{CommittedEvents, ExecutionOutcome, Repository, RepositoryResult};
pub use snapshot::{Snapshot, SnapshotStore};
pub use testing::AggregateFixture;
