//! In-memory event store implementation used by tests, examples, and local
//! development. It preserves stream/revision behavior for API compatibility while
//! remaining non-durable.

use crate::aggregate::Aggregate;
use crate::error::{ConcurrencyError, EventStoreError};
use crate::event::{EventEnvelope, EventId, ExpectedRevision, NewEvent};
use crate::event_store::{EventStore, EventStream};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

struct MemoryState<A>
where
    A: Aggregate,
{
    /// Per-stream indices into `global`, so each envelope is stored once.
    streams: HashMap<A::Id, Vec<usize>>,
    /// All envelopes in global sequence order.
    global: EventStream<A>,
    next_sequence: u64,
}

impl<A> Default for MemoryState<A>
where
    A: Aggregate,
{
    fn default() -> Self {
        Self {
            streams: HashMap::new(),
            global: Vec::new(),
            next_sequence: 1,
        }
    }
}

impl<A> MemoryState<A>
where
    A: Aggregate,
{
    /// Index of the first global event with a sequence greater than
    /// `checkpoint`. `global` is sorted by its always-assigned sequences, so
    /// this is a binary search.
    fn first_index_after(&self, checkpoint: Option<u64>) -> usize {
        match checkpoint {
            None => 0,
            Some(checkpoint) => self
                .global
                .partition_point(|event| event.sequence.is_some_and(|s| s <= checkpoint)),
        }
    }
}

/// Thread-safe in-memory event store.
///
/// This store is intended for tests, examples, and local development. It is
/// not durable, but it enforces the same stream revision checks production
/// adapters should enforce.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::{InMemoryEventStore, EventStore, NewEvent, ExpectedRevision, Metadata};
/// # use ddd_cqrs_es::{Aggregate, DomainEvent};
/// #
/// # #[derive(Clone)]
/// # enum MyEvent { Created }
/// # impl DomainEvent for MyEvent {
/// #     fn event_type(&self) -> &'static str { "my_event" }
/// # }
/// # struct MyAggregate;
/// # impl Aggregate for MyAggregate {
/// #     type Id = String;
/// #     type Command = ();
/// #     type Event = MyEvent;
/// #     type Error = ();
/// #     fn aggregate_type() -> &'static str { "my_aggregate" }
/// #     fn new() -> Self { MyAggregate }
/// #     fn apply(&mut self, _event: &Self::Event) {}
/// #     fn handle(&self, _command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> { Ok(vec![]) }
/// # }
///
/// let store = InMemoryEventStore::<MyAggregate>::new();
/// assert_eq!(store.stream_count().unwrap(), 0);
///
/// let event = NewEvent::new(MyEvent::Created, Metadata::default());
/// store.append(&"stream-1".to_string(), ExpectedRevision::NoStream, vec![event]).unwrap();
/// assert_eq!(store.stream_count().unwrap(), 1);
/// ```
pub struct InMemoryEventStore<A>
where
    A: Aggregate,
{
    state: Arc<RwLock<MemoryState<A>>>,
}

impl<A> Clone for InMemoryEventStore<A>
where
    A: Aggregate,
{
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
        }
    }
}

impl<A> std::fmt::Debug for InMemoryEventStore<A>
where
    A: Aggregate,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryEventStore").finish_non_exhaustive()
    }
}

impl<A> Default for InMemoryEventStore<A>
where
    A: Aggregate,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<A> InMemoryEventStore<A>
where
    A: Aggregate,
{
    /// Creates an empty in-memory event store.
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(MemoryState::default())),
        }
    }

    /// Returns the number of aggregate streams currently stored.
    pub fn stream_count(&self) -> Result<usize, EventStoreError> {
        let state = self.state.read().map_err(|_| EventStoreError::Poisoned)?;
        Ok(state.streams.len())
    }

    /// Removes all streams and resets the global sequence.
    pub fn clear(&self) -> Result<(), EventStoreError> {
        let mut state = self.state.write().map_err(|_| EventStoreError::Poisoned)?;
        state.streams.clear();
        state.global.clear();
        state.next_sequence = 1;
        Ok(())
    }
}

impl<A> EventStore<A> for InMemoryEventStore<A>
where
    A: Aggregate + 'static,
{
    type Error = EventStoreError;

    fn load(&self, aggregate_id: &A::Id) -> Result<EventStream<A>, Self::Error> {
        let state = self.state.read().map_err(|_| EventStoreError::Poisoned)?;
        Ok(state
            .streams
            .get(aggregate_id)
            .map(|indices| {
                indices
                    .iter()
                    .map(|&index| state.global[index].clone())
                    .collect()
            })
            .unwrap_or_default())
    }

    fn append(
        &self,
        aggregate_id: &A::Id,
        expected_revision: ExpectedRevision,
        events: Vec<NewEvent<A::Event>>,
    ) -> Result<EventStream<A>, Self::Error> {
        let mut state = self.state.write().map_err(|_| EventStoreError::Poisoned)?;
        let actual_revision = state
            .streams
            .get(aggregate_id)
            .map(|stream| stream.len() as u64)
            .unwrap_or_default();

        match expected_revision {
            ExpectedRevision::Any => {}
            ExpectedRevision::NoStream if actual_revision == 0 => {}
            ExpectedRevision::NoStream => {
                return Err(EventStoreError::Concurrency(
                    ConcurrencyError::StreamAlreadyExists,
                ));
            }
            ExpectedRevision::Exact(expected) if expected == actual_revision => {}
            ExpectedRevision::Exact(_) => {
                return Err(EventStoreError::Concurrency(
                    ConcurrencyError::WrongExpectedRevision {
                        expected: expected_revision,
                        actual: actual_revision,
                    },
                ));
            }
        }

        if events.is_empty() {
            return Ok(Vec::new());
        }

        let mut stream_events = Vec::with_capacity(events.len());
        let mut new_indices = Vec::with_capacity(events.len());
        for new_event in events {
            let sequence = state.next_sequence;
            state.next_sequence += 1;

            let revision = actual_revision + stream_events.len() as u64 + 1;
            let envelope = EventEnvelope::new(
                EventId::new(),
                aggregate_id.clone(),
                A::aggregate_type(),
                revision,
                Some(sequence),
                new_event.event_type,
                new_event.event_version,
                new_event.payload,
                new_event.metadata,
                SystemTime::now(),
            );

            new_indices.push(state.global.len());
            state.global.push(envelope.clone());
            stream_events.push(envelope);
        }

        state
            .streams
            .entry(aggregate_id.clone())
            .or_default()
            .extend(new_indices);

        Ok(stream_events)
    }

    fn load_global_after(&self, sequence: Option<u64>) -> Result<EventStream<A>, Self::Error> {
        let state = self.state.read().map_err(|_| EventStoreError::Poisoned)?;
        let start = state.first_index_after(sequence);
        Ok(state.global[start..].to_vec())
    }

    fn load_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: NonZeroUsize,
    ) -> Result<EventStream<A>, Self::Error> {
        let state = self.state.read().map_err(|_| EventStoreError::Poisoned)?;
        let start = state.first_index_after(sequence);
        let end = state.global.len().min(start + limit.get());
        Ok(state.global[start..end].to_vec())
    }
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl<A> crate::async_api::AsyncEventStore<A> for InMemoryEventStore<A>
where
    A: Aggregate + Send + Sync + 'static,
{
    type Error = EventStoreError;

    async fn load(&self, aggregate_id: &A::Id) -> Result<EventStream<A>, Self::Error> {
        EventStore::load(self, aggregate_id)
    }

    async fn append(
        &self,
        aggregate_id: &A::Id,
        expected_revision: ExpectedRevision,
        events: Vec<NewEvent<A::Event>>,
    ) -> Result<EventStream<A>, Self::Error> {
        EventStore::append(self, aggregate_id, expected_revision, events)
    }

    async fn load_global_after(
        &self,
        sequence: Option<u64>,
    ) -> Result<EventStream<A>, Self::Error> {
        EventStore::load_global_after(self, sequence)
    }

    async fn load_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: NonZeroUsize,
    ) -> Result<EventStream<A>, Self::Error> {
        EventStore::load_global_after_limited(self, sequence, limit)
    }
}

#[cfg(feature = "json")]
impl<A> crate::raw_feed::RawEventFeed for InMemoryEventStore<A>
where
    A: Aggregate + 'static,
    A::Event: serde::Serialize,
    A::Id: serde::Serialize,
{
    type Error = EventStoreError;

    /// Serves this store's own global log as raw envelopes.
    ///
    /// An in-memory store holds exactly one aggregate type, so unlike the SQL
    /// feeds this cannot interleave other types; it exists so raw projections
    /// can be exercised without a database.
    fn load_raw_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: std::num::NonZeroUsize,
    ) -> Result<Vec<crate::raw_feed::RawEventEnvelope>, Self::Error> {
        let state = self.state.read().map_err(|_| EventStoreError::Poisoned)?;
        let start = state.first_index_after(sequence);
        let end = state.global.len().min(start + limit.get());
        state.global[start..end]
            .iter()
            .map(|envelope| {
                let payload = serde_json::to_value(&envelope.payload).map_err(|error| {
                    EventStoreError::serialization(format!("payload JSON: {error}"))
                })?;
                let aggregate_id =
                    serde_json::to_string(&envelope.aggregate_id).map_err(|error| {
                        EventStoreError::serialization(format!("aggregate_id: {error}"))
                    })?;
                Ok(EventEnvelope::new(
                    envelope.event_id.clone(),
                    aggregate_id,
                    envelope.aggregate_type.clone(),
                    envelope.revision,
                    envelope.sequence,
                    envelope.event_type.clone(),
                    envelope.event_version,
                    payload,
                    envelope.metadata.clone(),
                    envelope.recorded_at,
                ))
            })
            .collect()
    }
}
