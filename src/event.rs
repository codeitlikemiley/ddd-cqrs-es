use crate::metadata::Metadata;
use std::fmt::{Display, Formatter};
#[cfg(not(feature = "uuid"))]
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;
#[cfg(not(feature = "uuid"))]
use std::time::UNIX_EPOCH;

/// The current stream revision of an aggregate.
///
/// Revision `0` means the stream is empty. The first persisted event has
/// revision `1`.
pub type Revision = u64;

/// The revision of an aggregate stream with no persisted events.
pub const INITIAL_REVISION: Revision = 0;

/// Stable event type name stored with an event envelope.
///
/// Backed by `Cow<'static, str>` so names sourced from
/// [`DomainEvent::event_type`]'s `&'static str` are borrowed instead of
/// allocated on every append.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventType(std::borrow::Cow<'static, str>);

impl EventType {
    /// Creates an event type from a stable event name.
    pub fn new(value: impl Into<String>) -> Self {
        Self(std::borrow::Cow::Owned(value.into()))
    }

    /// Creates an event type that borrows a static event name without
    /// allocating.
    pub const fn from_static(value: &'static str) -> Self {
        Self(std::borrow::Cow::Borrowed(value))
    }

    /// Returns the event type as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the event type and returns the owned string.
    pub fn into_string(self) -> String {
        self.0.into_owned()
    }
}

impl From<&str> for EventType {
    fn from(value: &str) -> Self {
        Self(std::borrow::Cow::Owned(value.to_owned()))
    }
}

impl From<String> for EventType {
    fn from(value: String) -> Self {
        Self(std::borrow::Cow::Owned(value))
    }
}

impl AsRef<str> for EventType {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Display for EventType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<&str> for EventType {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

/// Stable aggregate type name stored with an event envelope.
///
/// Backed by `Cow<'static, str>` so names sourced from
/// [`crate::aggregate::Aggregate::aggregate_type`]'s `&'static str` are
/// borrowed instead of allocated on every append.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AggregateType(std::borrow::Cow<'static, str>);

impl AggregateType {
    /// Creates an aggregate type from a stable aggregate name.
    pub fn new(value: impl Into<String>) -> Self {
        Self(std::borrow::Cow::Owned(value.into()))
    }

    /// Creates an aggregate type that borrows a static aggregate name without
    /// allocating.
    pub const fn from_static(value: &'static str) -> Self {
        Self(std::borrow::Cow::Borrowed(value))
    }

    /// Creates an aggregate type from a dynamic string slice.
    pub fn from_str(value: &str) -> Self {
        Self(std::borrow::Cow::Owned(value.to_owned()))
    }

    /// Returns the aggregate type as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the aggregate type and returns the owned string.
    pub fn into_string(self) -> String {
        self.0.into_owned()
    }
}

impl From<&'static str> for AggregateType {
    fn from(value: &'static str) -> Self {
        Self::from_static(value)
    }
}

impl From<String> for AggregateType {
    fn from(value: String) -> Self {
        Self(std::borrow::Cow::Owned(value))
    }
}

impl AsRef<str> for AggregateType {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Display for AggregateType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<&str> for AggregateType {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<EventType> for &str {
    fn eq(&self, other: &EventType) -> bool {
        *self == other.as_str()
    }
}

/// A unique identifier assigned to a persisted event.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::EventId;
///
/// let id = EventId::new();
/// assert!(!id.as_str().is_empty());
///
/// let custom = EventId::from_string("my-custom-id");
/// assert_eq!(custom.as_str(), "my-custom-id");
/// ```
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventId(String);

impl EventId {
    /// Creates a unique event identifier.
    ///
    /// With the `uuid` feature enabled, this uses UUID v4. Without that
    /// feature it falls back to a process-local identifier suitable for tests.
    pub fn new() -> Self {
        #[cfg(feature = "uuid")]
        {
            Self(uuid::Uuid::new_v4().to_string())
        }

        #[cfg(not(feature = "uuid"))]
        {
            static COUNTER: AtomicU64 = AtomicU64::new(1);

            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default();
            let next = COUNTER.fetch_add(1, Ordering::Relaxed);

            Self(format!("evt-{nanos:x}-{next:x}"))
        }
    }

    /// Creates an event identifier from an existing stable value.
    pub fn from_string(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Creates an event identifier from a UUID.
    #[cfg(feature = "uuid")]
    pub fn from_uuid(value: uuid::Uuid) -> Self {
        Self(value.to_string())
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for EventId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Domain event metadata supplied by the event payload.
///
/// Implement this for event enums so stored envelopes use stable event names
/// and schema versions instead of Rust type paths.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::DomainEvent;
///
/// #[derive(Clone)]
/// enum OrderEvent {
///     Placed { order_id: String },
/// }
///
/// impl DomainEvent for OrderEvent {
///     fn event_type(&self) -> &'static str {
///         "order_placed"
///     }
///     fn event_version(&self) -> u32 {
///         1
///     }
/// }
///
/// let event = OrderEvent::Placed { order_id: "order-1".to_string() };
/// assert_eq!(event.event_type(), "order_placed");
/// assert_eq!(event.event_version(), 1);
/// ```
pub trait DomainEvent: Clone + Send + Sync + 'static {
    /// Stable event type name, such as `account_opened`.
    fn event_type(&self) -> &'static str;

    /// Event schema version. Increment when the payload schema changes.
    fn event_version(&self) -> u32 {
        1
    }
}

/// Concurrency expectation used when appending events.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::ExpectedRevision;
///
/// let expected = ExpectedRevision::Exact(10);
/// assert_eq!(expected, ExpectedRevision::Exact(10));
/// ```
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpectedRevision {
    /// Append without checking the current stream revision.
    Any,
    /// Append only if the stream does not exist yet.
    NoStream,
    /// Append only if the stream is currently at this exact revision.
    Exact(Revision),
}

/// A domain event before persistence assigns revision and sequence values.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::{NewEvent, DomainEvent, Metadata};
///
/// #[derive(Clone)]
/// struct MyEvent;
/// impl DomainEvent for MyEvent {
///     fn event_type(&self) -> &'static str { "my_event" }
/// }
///
/// let new_event = NewEvent::new(MyEvent, Metadata::default());
/// assert_eq!(new_event.event_type.as_str(), "my_event");
/// assert_eq!(new_event.event_version, 1);
/// ```
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewEvent<E> {
    /// Event payload supplied by aggregate command handling.
    pub payload: E,
    /// Stable event type name used by storage adapters and projections.
    pub event_type: EventType,
    /// Schema version for manual migrations and future upcasting support.
    pub event_version: u32,
    /// Audit, tracing, tenancy, and causality metadata.
    pub metadata: Metadata,
}

impl<E> NewEvent<E> {
    /// Creates a new event using stable metadata from [`DomainEvent`].
    pub fn new(payload: E, metadata: Metadata) -> Self
    where
        E: DomainEvent,
    {
        let event_type = EventType::from_static(payload.event_type());
        let event_version = payload.event_version();

        Self {
            payload,
            event_type,
            event_version,
            metadata,
        }
    }

    /// Creates a new event with an explicit stable event type.
    pub fn with_type(payload: E, event_type: impl Into<EventType>, metadata: Metadata) -> Self {
        Self {
            payload,
            event_type: event_type.into(),
            event_version: 1,
            metadata,
        }
    }

    /// Sets the schema version for this event.
    pub fn with_version(mut self, event_version: u32) -> Self {
        self.event_version = event_version;
        self
    }
}

/// A persisted event with stream identity, revision, metadata, and timestamps.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::{EventEnvelope, EventId};
///
/// let envelope = EventEnvelope::builder(
///     EventId::from_string("evt-123"),
///     "aggregate-1".to_string(),
///     "my_aggregate",
///     5,
///     "my_event",
///     "payload_data".to_string(),
/// )
/// .sequence(42)
/// .build();
/// assert_eq!(envelope.revision, 5);
/// assert_eq!(envelope.sequence, Some(42));
/// assert_eq!(envelope.event_id.as_str(), "evt-123");
/// ```
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventEnvelope<E, Id> {
    /// Unique event identifier.
    pub event_id: EventId,
    /// Aggregate stream identifier.
    pub aggregate_id: Id,
    /// Stable aggregate type name.
    pub aggregate_type: AggregateType,
    /// Per-aggregate stream revision assigned on append.
    pub revision: Revision,
    /// Global append order assigned by stores that support sequencing.
    pub sequence: Option<u64>,
    /// Stable event type name.
    pub event_type: EventType,
    /// Event schema version.
    pub event_version: u32,
    /// Domain event payload.
    pub payload: E,
    /// Audit, tracing, tenancy, and causality metadata.
    pub metadata: Metadata,
    /// Time the event was recorded by the event store.
    pub recorded_at: SystemTime,
}

impl<E, Id> EventEnvelope<E, Id> {
    /// Creates a persisted event envelope.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        event_id: EventId,
        aggregate_id: Id,
        aggregate_type: impl Into<AggregateType>,
        revision: Revision,
        sequence: Option<u64>,
        event_type: impl Into<EventType>,
        event_version: u32,
        payload: E,
        metadata: Metadata,
        recorded_at: SystemTime,
    ) -> Self {
        Self {
            event_id,
            aggregate_id,
            aggregate_type: aggregate_type.into(),
            revision,
            sequence,
            event_type: event_type.into(),
            event_version,
            payload,
            metadata,
            recorded_at,
        }
    }

    /// Starts building an envelope from its identity fields.
    ///
    /// Optional fields default: `sequence` to `None`, `event_version` to `1`,
    /// `metadata` to empty, and `recorded_at` to the current time. Prefer
    /// this over [`EventEnvelope::new`]'s ten positional arguments.
    pub fn builder(
        event_id: EventId,
        aggregate_id: Id,
        aggregate_type: impl Into<AggregateType>,
        revision: Revision,
        event_type: impl Into<EventType>,
        payload: E,
    ) -> EventEnvelopeBuilder<E, Id> {
        EventEnvelopeBuilder {
            envelope: Self {
                event_id,
                aggregate_id,
                aggregate_type: aggregate_type.into(),
                revision,
                sequence: None,
                event_type: event_type.into(),
                event_version: 1,
                payload,
                metadata: Metadata::default(),
                recorded_at: SystemTime::now(),
            },
        }
    }

    /// Returns a reference to the domain event payload.
    pub fn event(&self) -> &E {
        &self.payload
    }

    /// Maps the event payload while preserving the envelope metadata.
    pub fn map_payload<T>(self, f: impl FnOnce(E) -> T) -> EventEnvelope<T, Id> {
        EventEnvelope {
            event_id: self.event_id,
            aggregate_id: self.aggregate_id,
            aggregate_type: self.aggregate_type,
            revision: self.revision,
            sequence: self.sequence,
            event_type: self.event_type,
            event_version: self.event_version,
            payload: f(self.payload),
            metadata: self.metadata,
            recorded_at: self.recorded_at,
        }
    }

    /// Returns the recording time as a chrono UTC timestamp.
    #[cfg(feature = "chrono")]
    pub fn recorded_at_utc(&self) -> chrono::DateTime<chrono::Utc> {
        self.recorded_at.into()
    }

    /// Serializes this envelope as JSON.
    #[cfg(feature = "json")]
    pub fn to_json(&self) -> Result<String, crate::error::EventStoreError>
    where
        E: serde::Serialize,
        Id: serde::Serialize,
    {
        serde_json::to_string(self).map_err(|error| {
            crate::error::EventStoreError::serialization_with_source(error.to_string(), error)
        })
    }

    /// Deserializes an envelope from JSON.
    #[cfg(feature = "json")]
    pub fn from_json(json: &str) -> Result<Self, crate::error::EventStoreError>
    where
        E: serde::de::DeserializeOwned,
        Id: serde::de::DeserializeOwned,
    {
        serde_json::from_str(json).map_err(|error| {
            crate::error::EventStoreError::deserialization_with_source(error.to_string(), error)
        })
    }
}

/// Builder returned by [`EventEnvelope::builder`].
#[derive(Clone, Debug)]
pub struct EventEnvelopeBuilder<E, Id> {
    envelope: EventEnvelope<E, Id>,
}

impl<E, Id> EventEnvelopeBuilder<E, Id> {
    /// Sets the global append order assigned by sequencing stores.
    pub fn sequence(mut self, sequence: u64) -> Self {
        self.envelope.sequence = Some(sequence);
        self
    }

    /// Sets the event schema version (defaults to `1`).
    pub fn event_version(mut self, event_version: u32) -> Self {
        self.envelope.event_version = event_version;
        self
    }

    /// Sets the audit and causality metadata (defaults to empty).
    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.envelope.metadata = metadata;
        self
    }

    /// Sets the recording time (defaults to the current time).
    pub fn recorded_at(mut self, recorded_at: SystemTime) -> Self {
        self.envelope.recorded_at = recorded_at;
        self
    }

    /// Finishes the envelope.
    pub fn build(self) -> EventEnvelope<E, Id> {
        self.envelope
    }
}
