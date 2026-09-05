use crate::aggregate::Aggregate;
use crate::error::EventStoreError;
use crate::event::{EventEnvelope, ExpectedRevision, NewEvent};
use crate::idempotency::{IdempotencyKey, IdempotencyState};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroUsize;

/// Committed events for one aggregate type.
pub type EventStream<A> = Vec<EventEnvelope<<A as Aggregate>::Event, <A as Aggregate>::Id>>;

/// Page size used by the provided `load_global_after` implementations when they
/// drain a feed through the bounded `load_global_after_limited` primitive.
pub const GLOBAL_REPLAY_PAGE_SIZE: NonZeroUsize = NonZeroUsize::new(500).expect("non-zero");

/// Error returned by transaction-aware idempotent append operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdempotentAppendError<StoreError> {
    /// Another executor has reserved the key and has not completed yet.
    Pending {
        /// Key that is still pending.
        key: IdempotencyKey,
    },
    /// The backing event store failed.
    Store(StoreError),
}

impl<StoreError> Display for IdempotentAppendError<StoreError>
where
    StoreError: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            IdempotentAppendError::Pending { key } => {
                write!(f, "idempotency key `{key}` is pending")
            }
            IdempotentAppendError::Store(error) => Display::fmt(error, f),
        }
    }
}

impl<StoreError> Error for IdempotentAppendError<StoreError>
where
    StoreError: Error + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            IdempotentAppendError::Pending { .. } => None,
            IdempotentAppendError::Store(error) => Some(error),
        }
    }
}

/// Event persistence abstraction for one aggregate type.
///
/// Durable adapters such as PostgreSQL, SQLite, Kafka, or object storage should
/// implement this trait while preserving stream order and optimistic
/// concurrency semantics.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::{EventStore, InMemoryEventStore, NewEvent, ExpectedRevision, Metadata};
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
/// let event = NewEvent::new(MyEvent::Created, Metadata::default());
///
/// store.append(&"stream-1".to_string(), ExpectedRevision::NoStream, vec![event]).unwrap();
/// let events = store.load(&"stream-1".to_string()).unwrap();
/// assert_eq!(events.len(), 1);
/// assert_eq!(events[0].revision, 1);
/// ```
pub trait EventStore<A>: Clone + Send + Sync + 'static
where
    A: Aggregate,
{
    /// Store-specific error type.
    type Error;

    /// Loads all events for one aggregate stream.
    fn load(&self, aggregate_id: &A::Id) -> Result<EventStream<A>, Self::Error>;

    /// Loads events for one aggregate stream after the given revision.
    ///
    /// The default implementation loads the full stream and filters in memory
    /// (`O(stream)`). Stores with per-stream indexes should override this to
    /// slice or query only the tail after `revision`.
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

    /// Loads at most `limit` globally ordered events after a global sequence number.
    ///
    /// This is the bounded replay primitive every store must provide, and the
    /// only global-feed read projection runners perform. Implementations must
    /// push `limit` down to the backend (SQL `LIMIT`, Redis
    /// `ZRANGEBYSCORE ... LIMIT`, a slice of an in-memory log) rather than
    /// materializing the tail and truncating it.
    ///
    /// "Global" order is scoped to this store's aggregate type `A`: backends
    /// filter the feed by aggregate type, so a read model spanning several
    /// aggregate types needs one projection runner (and checkpoint) per type
    /// and must not assume ordering across those feeds. Use
    /// [`PersistedProjectionRunner::new`](crate::projection::PersistedProjectionRunner::new),
    /// which keys checkpoints per feed by default. The legacy name-only keying of
    /// [`PersistedProjectionRunner::with_projection_name_checkpoints`](crate::projection::PersistedProjectionRunner::with_projection_name_checkpoints)
    /// makes those per-type runs share one position and hide events. See
    /// ADR-0003 (per-aggregate global feeds) in `docs/adr/` for the
    /// rationale and migration path.
    fn load_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: NonZeroUsize,
    ) -> Result<EventStream<A>, Self::Error>;

    /// Loads **every** globally ordered event after a global sequence number.
    ///
    /// > [!WARNING]
    /// > The result is unbounded: the whole backlog after `sequence` is held in
    /// > memory at once. Use it for tests, small fixtures, and explicit
    /// > maintenance jobs only. Production replay should call
    /// > [`Self::load_global_after_limited`], or a projection runner, which
    /// > pages through it.
    ///
    /// The provided implementation drains the feed in
    /// [`GLOBAL_REPLAY_PAGE_SIZE`] pages so a store never has to answer one
    /// unbounded backend query; adapters that can stream the tail in a single
    /// query may override it. Paging is not a consistent snapshot — events
    /// committed between pages are included.
    ///
    /// See [`Self::load_global_after_limited`] for how "global" order is scoped
    /// to this store's aggregate type.
    fn load_global_after(&self, sequence: Option<u64>) -> Result<EventStream<A>, Self::Error> {
        let mut cursor = sequence;
        let mut all = Vec::new();

        loop {
            let page = self.load_global_after_limited(cursor, GLOBAL_REPLAY_PAGE_SIZE)?;
            let page_len = page.len();
            let last_sequence = page.last().and_then(|event| event.sequence);
            all.extend(page);

            if page_len < GLOBAL_REPLAY_PAGE_SIZE.get() {
                return Ok(all);
            }
            // A full page whose cursor does not advance would be re-read
            // forever; stop instead of looping on it.
            match last_sequence {
                Some(sequence) if cursor.is_none_or(|cursor| sequence > cursor) => {
                    cursor = Some(sequence);
                }
                _ => return Ok(all),
            }
        }
    }
}

/// Event store extension for crash-atomic idempotent appends.
///
/// Implementations must reserve the idempotency key, append events, and persist
/// the completed committed event stream in one backing-store transaction. A
/// retry with a completed key returns the originally committed events without
/// appending again. A pending key returns [`IdempotentAppendError::Pending`] so
/// repositories can apply a bounded wait policy.
pub trait AtomicIdempotentEventStore<A>: EventStore<A>
where
    A: Aggregate,
{
    /// Loads an existing atomic idempotency record before command evaluation.
    ///
    /// Repositories use this to replay a completed command even when evaluating
    /// that command against the now-current aggregate state would fail.
    fn load_idempotent(
        &self,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<IdempotencyState<EventStream<A>>>, Self::Error>;

    /// Appends events once for the idempotency key, atomically with the
    /// idempotency completion record.
    fn append_idempotent(
        &self,
        idempotency_key: IdempotencyKey,
        aggregate_id: &A::Id,
        expected_revision: ExpectedRevision,
        events: Vec<NewEvent<A::Event>>,
    ) -> Result<EventStream<A>, IdempotentAppendError<Self::Error>>;
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
