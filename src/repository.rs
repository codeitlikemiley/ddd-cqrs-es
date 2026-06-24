use crate::aggregate::{Aggregate, LoadedAggregate};
use crate::error::{EventStoreError, EventStoreFailure, RepositoryError};
use crate::event::{ExpectedRevision, NewEvent};
use crate::event_store::{EventStore, EventStream};
use crate::idempotency::{
    IdempotencyKey, IdempotencyState, IdempotencyStore, IdempotentRepositoryError,
};
use crate::metadata::Metadata;
use crate::snapshot::{SnapshotRepositoryError, SnapshotStore};
use std::marker::PhantomData;

/// Result type returned by repository operations.
pub type RepositoryResult<A, S, T> =
    Result<T, RepositoryError<<A as Aggregate>::Error, <S as EventStore<A>>::Error>>;

/// Committed events returned by repository command execution.
pub type CommittedEvents<A> = EventStream<A>;

/// Updated aggregate state plus committed events.
pub type ExecutionOutcome<A> = (LoadedAggregate<A>, CommittedEvents<A>);

/// Result type returned by snapshot-aware repository operations.
pub type SnapshotRepositoryResult<A, S, SS, T> = Result<
    T,
    SnapshotRepositoryError<
        <A as Aggregate>::Error,
        <S as EventStore<A>>::Error,
        <SS as SnapshotStore<A>>::Error,
    >,
>;

/// Result type returned by idempotent repository operations.
pub type IdempotentRepositoryResult<A, S, I, T> = Result<
    T,
    IdempotentRepositoryError<
        <A as Aggregate>::Error,
        <S as EventStore<A>>::Error,
        <I as IdempotencyStore<CommittedEvents<A>>>::Error,
    >,
>;

/// Coordinates aggregate loading, command execution, and event appending.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::{Repository, InMemoryEventStore, Metadata};
/// # use ddd_cqrs_es::{Aggregate, DomainEvent};
/// #
/// # #[derive(Clone)]
/// # enum CounterEvent { Created }
/// # impl DomainEvent for CounterEvent {
/// #     fn event_type(&self) -> &'static str { "counter_created" }
/// # }
/// # struct CounterAggregate { revision: u64 }
/// # impl Aggregate for CounterAggregate {
/// #     type Id = String;
/// #     type Command = ();
/// #     type Event = CounterEvent;
/// #     type Error = ();
/// #     fn aggregate_type() -> &'static str { "counter" }
/// #     fn id(&self) -> Option<&Self::Id> { None }
/// #     fn revision(&self) -> u64 { self.revision }
/// #     fn new() -> Self { CounterAggregate { revision: 0 } }
/// #     fn apply(&mut self, _event: &Self::Event) { self.revision += 1; }
/// #     fn handle(&self, _command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> { Ok(vec![CounterEvent::Created]) }
/// # }
///
/// let store = InMemoryEventStore::<CounterAggregate>::new();
/// let repo = Repository::new(store);
///
/// let aggregate_id = "counter-1".to_string();
/// repo.execute(&aggregate_id, (), Metadata::default()).unwrap();
///
/// let loaded = repo.load(&aggregate_id).unwrap();
/// assert_eq!(loaded.revision, 1);
/// ```
#[derive(Clone, Debug)]
pub struct Repository<A, S>
where
    A: Aggregate,
    S: EventStore<A>,
    S::Error: EventStoreFailure,
{
    store: S,
    _marker: PhantomData<A>,
}

impl<A, S> Repository<A, S>
where
    A: Aggregate,
    S: EventStore<A>,
    S::Error: EventStoreFailure,
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
            .map_err(EventStoreFailure::into_repository_error)?;
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
            .map_err(EventStoreFailure::into_repository_error)
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
            .map_err(EventStoreFailure::into_repository_error)
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
        #[cfg(feature = "tracing")]
        let event_count = events.len();

        let committed = self.save(aggregate_id, &loaded, events, metadata)?;

        #[cfg(feature = "tracing")]
        tracing::debug!(
            aggregate_type = A::aggregate_type(),
            expected_revision = loaded.revision,
            event_count,
            "committed aggregate events"
        );

        Ok(committed)
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

    /// Loads an aggregate using the latest snapshot, then replays events after
    /// the snapshot revision.
    pub fn load_with_snapshot<SS>(
        &self,
        aggregate_id: &A::Id,
        snapshots: &SS,
    ) -> SnapshotRepositoryResult<A, S, SS, LoadedAggregate<A>>
    where
        SS: SnapshotStore<A>,
    {
        let snapshot = snapshots
            .load_snapshot(aggregate_id)
            .map_err(SnapshotRepositoryError::Snapshot)?;

        let Some(snapshot) = snapshot else {
            let events = self
                .store
                .load(aggregate_id)
                .map_err(SnapshotRepositoryError::from_store_error)?;
            return Ok(A::replay(&events));
        };

        let events = self
            .store
            .load_after_revision(aggregate_id, snapshot.revision)
            .map_err(SnapshotRepositoryError::from_store_error)?;
        let mut state = snapshot.state;
        let mut revision = snapshot.revision;

        for envelope in events {
            state.apply(&envelope.payload);
            revision = envelope.revision;
        }

        Ok(LoadedAggregate::new(state, revision))
    }

    /// Executes a command using snapshot-aware loading before appending events.
    pub fn execute_with_snapshot<SS>(
        &self,
        aggregate_id: &A::Id,
        command: A::Command,
        metadata: Metadata,
        snapshots: &SS,
    ) -> SnapshotRepositoryResult<A, S, SS, CommittedEvents<A>>
    where
        SS: SnapshotStore<A>,
    {
        let loaded = self.load_with_snapshot(aggregate_id, snapshots)?;
        let events = loaded
            .state
            .handle(command)
            .map_err(SnapshotRepositoryError::Domain)?;
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
            .map_err(SnapshotRepositoryError::from_store_error)
    }

    /// Executes a command once for an idempotency key and returns the previous
    /// committed events when the same key is retried.
    pub fn execute_idempotent<I>(
        &self,
        aggregate_id: &A::Id,
        command: A::Command,
        metadata: Metadata,
        idempotency_key: IdempotencyKey,
        idempotency_store: &I,
    ) -> IdempotentRepositoryResult<A, S, I, CommittedEvents<A>>
    where
        I: IdempotencyStore<CommittedEvents<A>>,
    {
        loop {
            match idempotency_store
                .load(&idempotency_key)
                .map_err(IdempotentRepositoryError::Idempotency)?
            {
                Some(IdempotencyState::Complete(committed)) => {
                    return Ok(committed);
                }
                Some(IdempotencyState::Pending) => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    continue;
                }
                None => {
                    if idempotency_store
                        .reserve(idempotency_key.clone())
                        .map_err(IdempotentRepositoryError::Idempotency)?
                    {
                        break;
                    }
                }
            }
        }

        let committed =
            match (|| -> Result<CommittedEvents<A>, RepositoryError<A::Error, S::Error>> {
                let loaded = self.load(aggregate_id)?;
                let events = loaded
                    .state
                    .handle(command)
                    .map_err(RepositoryError::Domain)?;
                let events = events
                    .into_iter()
                    .map(|event| NewEvent::new(event, metadata.clone()))
                    .collect();
                let committed = self
                    .store
                    .append(
                        aggregate_id,
                        ExpectedRevision::Exact(loaded.revision),
                        events,
                    )
                    .map_err(EventStoreFailure::into_repository_error)?;
                Ok(committed)
            })() {
                Ok(committed) => committed,
                Err(err) => {
                    let _ = idempotency_store.remove(&idempotency_key);
                    return Err(match err {
                        RepositoryError::Domain(error) => IdempotentRepositoryError::Domain(error),
                        RepositoryError::Concurrency(error) => {
                            IdempotentRepositoryError::Concurrency(error)
                        }
                        RepositoryError::Store(error) => IdempotentRepositoryError::Store(error),
                    });
                }
            };

        idempotency_store
            .save(idempotency_key, committed.clone())
            .map_err(IdempotentRepositoryError::Idempotency)?;

        Ok(committed)
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
    }
}
