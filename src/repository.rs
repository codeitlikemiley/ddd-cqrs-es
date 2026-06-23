use crate::aggregate::{Aggregate, LoadedAggregate};
use crate::error::{EventStoreError, RepositoryError};
use crate::event::{ExpectedRevision, NewEvent};
use crate::event_store::{EventStore, EventStream};
use crate::metadata::Metadata;
use std::marker::PhantomData;

/// Result type returned by repository operations.
pub type RepositoryResult<A, S, T> =
    Result<T, RepositoryError<<A as Aggregate>::Error, <S as EventStore<A>>::Error>>;

/// Committed events returned by repository command execution.
pub type CommittedEvents<A> = EventStream<A>;

/// Updated aggregate state plus committed events.
pub type ExecutionOutcome<A> = (LoadedAggregate<A>, CommittedEvents<A>);

/// Coordinates aggregate loading, command execution, and event appending.
#[derive(Clone, Debug)]
pub struct Repository<A, S>
where
    A: Aggregate,
    S: EventStore<A>,
{
    store: S,
    _marker: PhantomData<A>,
}

impl<A, S> Repository<A, S>
where
    A: Aggregate,
    S: EventStore<A>,
{
    /// Creates a repository backed by an event store.
    pub fn new(store: S) -> Self {
        Self {
            store,
            _marker: PhantomData,
        }
    }

    /// Returns the backing event store.
    pub fn store(&self) -> &S {
        &self.store
    }

    /// Loads and replays one aggregate stream.
    pub fn load(&self, aggregate_id: &A::Id) -> RepositoryResult<A, S, LoadedAggregate<A>> {
        let events = self
            .store
            .load(aggregate_id)
            .map_err(RepositoryError::Store)?;
        Ok(A::replay(&events))
    }

    /// Persists new events for a previously loaded aggregate.
    pub fn save(
        &self,
        aggregate_id: &A::Id,
        loaded: &LoadedAggregate<A>,
        events: Vec<A::Event>,
        metadata: Metadata,
    ) -> RepositoryResult<A, S, CommittedEvents<A>> {
        let events = events
            .into_iter()
            .map(|event| NewEvent::new(event, metadata.clone()))
            .collect();

        self.store
            .append(
                aggregate_id,
                ExpectedRevision::Exact(loaded.revision),
                events,
            )
            .map_err(RepositoryError::Store)
    }

    /// Persists explicitly named new events for a previously loaded aggregate.
    pub fn save_new_events(
        &self,
        aggregate_id: &A::Id,
        loaded: &LoadedAggregate<A>,
        events: Vec<NewEvent<A::Event>>,
    ) -> RepositoryResult<A, S, CommittedEvents<A>> {
        self.store
            .append(
                aggregate_id,
                ExpectedRevision::Exact(loaded.revision),
                events,
            )
            .map_err(RepositoryError::Store)
    }

    /// Executes a command and returns committed event envelopes.
    pub fn execute(
        &self,
        aggregate_id: &A::Id,
        command: A::Command,
        metadata: Metadata,
    ) -> RepositoryResult<A, S, CommittedEvents<A>> {
        let loaded = self.load(aggregate_id)?;
        let events = loaded
            .state
            .handle(command)
            .map_err(RepositoryError::Domain)?;

        self.save(aggregate_id, &loaded, events, metadata)
    }

    /// Executes a command and returns both committed events and updated state.
    pub fn execute_returning_state(
        &self,
        aggregate_id: &A::Id,
        command: A::Command,
        metadata: Metadata,
    ) -> RepositoryResult<A, S, ExecutionOutcome<A>> {
        let committed = self.execute(aggregate_id, command, metadata)?;
        let loaded = self.load(aggregate_id)?;
        Ok((loaded, committed))
    }
}

impl<A, S> Repository<A, S>
where
    A: Aggregate,
    S: EventStore<A, Error = EventStoreError>,
{
    /// Executes a command and maps standard event store concurrency errors to
    /// [`RepositoryError::Concurrency`].
    pub fn execute_standard(
        &self,
        aggregate_id: &A::Id,
        command: A::Command,
        metadata: Metadata,
    ) -> Result<CommittedEvents<A>, RepositoryError<A::Error, EventStoreError>> {
        self.execute(aggregate_id, command, metadata)
            .map_err(|error| match error {
                RepositoryError::Store(EventStoreError::Concurrency(concurrency)) => {
                    RepositoryError::Concurrency(concurrency)
                }
                RepositoryError::Store(error) => RepositoryError::Store(error),
                RepositoryError::Domain(error) => RepositoryError::Domain(error),
                RepositoryError::Concurrency(error) => RepositoryError::Concurrency(error),
            })
    }
}
