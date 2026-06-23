use crate::aggregate::Aggregate;
use crate::error::EventStoreError;
use crate::event::{EventEnvelope, ExpectedRevision, NewEvent};

/// Committed events for one aggregate type.
pub type EventStream<A> = Vec<EventEnvelope<<A as Aggregate>::Event, <A as Aggregate>::Id>>;

/// Event persistence abstraction for one aggregate type.
///
/// Durable adapters such as PostgreSQL, SQLite, Kafka, or object storage should
/// implement this trait while preserving stream order and optimistic
/// concurrency semantics.
pub trait EventStore<A>: Clone + Send + Sync + 'static
where
    A: Aggregate,
{
    /// Store-specific error type.
    type Error;

    /// Loads all events for one aggregate stream.
    fn load(&self, aggregate_id: &A::Id) -> Result<EventStream<A>, Self::Error>;

    /// Loads events for one aggregate stream after the given revision.
    fn load_after_revision(
        &self,
        aggregate_id: &A::Id,
        revision: u64,
    ) -> Result<EventStream<A>, Self::Error> {
        let events = self.load(aggregate_id)?;
        Ok(events
            .into_iter()
            .filter(|event| event.revision > revision)
            .collect())
    }

    /// Appends events to one aggregate stream.
    fn append(
        &self,
        aggregate_id: &A::Id,
        expected_revision: ExpectedRevision,
        events: Vec<NewEvent<A::Event>>,
    ) -> Result<EventStream<A>, Self::Error>;

    /// Loads globally ordered events after a global sequence number.
    fn load_global_after(&self, sequence: Option<u64>) -> Result<EventStream<A>, Self::Error>;
}

/// Convenience alias for stores that use the framework's standard error type.
pub trait StandardEventStore<A>: EventStore<A, Error = EventStoreError>
where
    A: Aggregate,
{
}

impl<A, S> StandardEventStore<A> for S
where
    A: Aggregate,
    S: EventStore<A, Error = EventStoreError>,
{
}
