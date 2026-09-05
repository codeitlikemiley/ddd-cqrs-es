use crate::event::{ExpectedRevision, Revision};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

/// Optimistic concurrency failure.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::{ConcurrencyError, ExpectedRevision};
///
/// let error = ConcurrencyError::WrongExpectedRevision {
///     expected: ExpectedRevision::Exact(4),
///     actual: 3,
/// };
/// assert_eq!(
///     error.to_string(),
///     "wrong expected revision: expected Exact(4), actual revision 3"
/// );
/// ```
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConcurrencyError {
    /// The append expected an empty stream, but events already exist.
    StreamAlreadyExists,
    /// The append expected one revision but found another.
    WrongExpectedRevision {
        /// Expected revision constraint.
        expected: ExpectedRevision,
        /// Actual stream revision at append time.
        actual: Revision,
    },
    /// A snapshot save offered a revision older than the stored snapshot.
    StaleSnapshotRevision {
        /// Revision offered by the caller.
        offered: Revision,
        /// Revision currently stored for the stream.
        current: Revision,
    },
}

impl ConcurrencyError {
    /// Optimistic concurrency failures are safe to retry when the caller can
    /// reload aggregate state and re-issue the command.
    pub fn is_retryable(&self) -> bool {
        true
    }
}

impl Display for ConcurrencyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ConcurrencyError::StreamAlreadyExists => f.write_str("event stream already exists"),
            ConcurrencyError::WrongExpectedRevision { expected, actual } => {
                write!(
                    f,
                    "wrong expected revision: expected {:?}, actual revision {}",
                    expected, actual
                )
            }
            ConcurrencyError::StaleSnapshotRevision { offered, current } => {
                write!(
                    f,
                    "stale snapshot revision: offered {}, current {}",
                    offered, current
                )
            }
        }
    }
}

impl Error for ConcurrencyError {}

/// Stable classification for [`EventStoreError`] without parsing display text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventStoreErrorKind {
    /// Optimistic concurrency check failed.
    Concurrency,
    /// Event serialization failed.
    Serialization,
    /// Event deserialization failed.
    Deserialization,
    /// Backend connection or availability failure.
    Connection,
    /// Shared state was poisoned by a panic while holding a lock.
    Poisoned,
    /// Adapter-specific failure.
    Backend,
    /// Unknown adapter failure.
    Unknown,
}

/// Stored source error used when a backend-specific error type cannot be part
/// of the public enum without leaking adapter implementation details.
#[derive(Clone, Debug)]
pub struct EventStoreErrorSource {
    inner: Arc<dyn Error + Send + Sync>,
}

impl EventStoreErrorSource {
    /// Creates a stored source error from a stable adapter message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(StaticSourceError(message.into())),
        }
    }

    /// Preserves a typed adapter error for [`Self::downcast_ref`].
    pub fn from_error(error: impl Error + Send + Sync + 'static) -> Self {
        Self {
            inner: Arc::new(error),
        }
    }

    /// Returns the preserved error when it matches `T`.
    pub fn downcast_ref<T: Error + 'static>(&self) -> Option<&T> {
        self.inner.downcast_ref()
    }
}

impl Display for EventStoreErrorSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self.inner.as_ref(), f)
    }
}

impl Error for EventStoreErrorSource {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.inner.source()
    }
}

#[derive(Debug)]
struct StaticSourceError(String);

impl Display for StaticSourceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for StaticSourceError {}

/// Errors produced by event store implementations.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::{EventStoreError, ConcurrencyError};
/// use std::error::Error;
///
/// let error = EventStoreError::Concurrency(ConcurrencyError::StreamAlreadyExists);
/// assert!(error.source().is_some());
/// ```
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug)]
pub enum EventStoreError {
    /// Optimistic concurrency check failed.
    Concurrency(ConcurrencyError),
    /// Event serialization failed.
    Serialization {
        /// Public error message.
        message: String,
        /// Machine-readable backend code (SQLSTATE, errno) when available.
        code: Option<String>,
        /// Source error for error-chain aware callers.
        #[cfg_attr(feature = "serde", serde(skip))]
        source: Option<Arc<EventStoreErrorSource>>,
    },
    /// Event deserialization failed.
    Deserialization {
        /// Public error message.
        message: String,
        /// Machine-readable backend code (SQLSTATE, errno) when available.
        code: Option<String>,
        /// Source error for error-chain aware callers.
        #[cfg_attr(feature = "serde", serde(skip))]
        source: Option<Arc<EventStoreErrorSource>>,
    },
    /// Backend connection or availability failure.
    Connection {
        /// Public error message.
        message: String,
        /// Machine-readable backend code (SQLSTATE, errno) when available.
        code: Option<String>,
        /// Source error for error-chain aware callers.
        #[cfg_attr(feature = "serde", serde(skip))]
        source: Option<Arc<EventStoreErrorSource>>,
    },
    /// Shared state was poisoned by a panic while holding a lock.
    Poisoned,
    /// Adapter-specific failure.
    Backend {
        /// Public error message.
        message: String,
        /// Machine-readable backend code (SQLSTATE, errno) when available.
        code: Option<String>,
        /// Source error for error-chain aware callers.
        #[cfg_attr(feature = "serde", serde(skip))]
        source: Option<Arc<EventStoreErrorSource>>,
    },
    /// Unknown adapter failure.
    Unknown {
        /// Public error message.
        message: String,
        /// Machine-readable backend code (SQLSTATE, errno) when available.
        code: Option<String>,
        /// Source error for error-chain aware callers.
        #[cfg_attr(feature = "serde", serde(skip))]
        source: Option<Arc<EventStoreErrorSource>>,
    },
}

impl EventStoreError {
    /// Creates a serialization error.
    pub fn serialization(message: impl Into<String>) -> Self {
        Self::Serialization {
            message: message.into(),
            code: None,
            source: None,
        }
    }

    /// Creates a deserialization error.
    pub fn deserialization(message: impl Into<String>) -> Self {
        Self::Deserialization {
            message: message.into(),
            code: None,
            source: None,
        }
    }

    /// Creates a connection error.
    pub fn connection(message: impl Into<String>) -> Self {
        Self::Connection {
            message: message.into(),
            code: None,
            source: None,
        }
    }

    /// Creates a backend error.
    pub fn backend(message: impl Into<String>) -> Self {
        Self::Backend {
            message: message.into(),
            code: None,
            source: None,
        }
    }

    /// Creates an unknown error.
    pub fn unknown(message: impl Into<String>) -> Self {
        Self::Unknown {
            message: message.into(),
            code: None,
            source: None,
        }
    }

    /// Creates a serialization error that preserves source context.
    pub fn serialization_with_source(
        message: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::serialization(message).with_source(source)
    }

    /// Creates a deserialization error that preserves source context.
    pub fn deserialization_with_source(
        message: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::deserialization(message).with_source(source)
    }

    /// Creates a connection error that preserves source context.
    pub fn connection_with_source(
        message: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::connection(message).with_source(source)
    }

    /// Creates a backend error that preserves source context.
    pub fn backend_with_source(
        message: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::backend(message).with_source(source)
    }

    /// Creates an unknown error that preserves source context.
    pub fn unknown_with_source(
        message: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::unknown(message).with_source(source)
    }

    /// Attaches a machine-readable backend code (SQLSTATE, errno) to the
    /// error. No-op for [`EventStoreError::Concurrency`] and
    /// [`EventStoreError::Poisoned`].
    pub fn with_code(mut self, value: impl Into<String>) -> Self {
        match &mut self {
            Self::Serialization { code, .. }
            | Self::Deserialization { code, .. }
            | Self::Connection { code, .. }
            | Self::Backend { code, .. }
            | Self::Unknown { code, .. } => *code = Some(value.into()),
            Self::Concurrency(_) | Self::Poisoned => {}
        }
        self
    }

    /// Attaches a preserved source error for error-chain aware callers.
    /// No-op for [`EventStoreError::Concurrency`] and
    /// [`EventStoreError::Poisoned`].
    pub fn with_source(mut self, value: impl Error + Send + Sync + 'static) -> Self {
        match &mut self {
            Self::Serialization { source, .. }
            | Self::Deserialization { source, .. }
            | Self::Connection { source, .. }
            | Self::Backend { source, .. }
            | Self::Unknown { source, .. } => {
                *source = Some(Arc::new(EventStoreErrorSource::from_error(value)));
            }
            Self::Concurrency(_) | Self::Poisoned => {}
        }
        self
    }

    /// Returns the stable error kind without parsing display text.
    pub fn kind(&self) -> EventStoreErrorKind {
        match self {
            Self::Concurrency(_) => EventStoreErrorKind::Concurrency,
            Self::Serialization { .. } => EventStoreErrorKind::Serialization,
            Self::Deserialization { .. } => EventStoreErrorKind::Deserialization,
            Self::Connection { .. } => EventStoreErrorKind::Connection,
            Self::Poisoned => EventStoreErrorKind::Poisoned,
            Self::Backend { .. } => EventStoreErrorKind::Backend,
            Self::Unknown { .. } => EventStoreErrorKind::Unknown,
        }
    }

    /// Returns the scrubbed, transport-safe message stored on the error.
    ///
    /// Adapters should keep SQL fragments, connection URLs, and other backend
    /// internals in the attached [`EventStoreErrorSource`] (via
    /// [`Self::with_source`]) rather than in this field.
    pub fn public_message(&self) -> &str {
        match self {
            Self::Concurrency(error) => match error {
                ConcurrencyError::StreamAlreadyExists => "event stream already exists",
                ConcurrencyError::WrongExpectedRevision { .. } => {
                    "wrong expected revision for append"
                }
                ConcurrencyError::StaleSnapshotRevision { .. } => "stale snapshot revision",
            },
            Self::Serialization { message, .. }
            | Self::Deserialization { message, .. }
            | Self::Connection { message, .. }
            | Self::Backend { message, .. }
            | Self::Unknown { message, .. } => message,
            Self::Poisoned => "event store lock was poisoned",
        }
    }

    /// Returns the preserved adapter source when one was attached.
    pub fn store_source(&self) -> Option<&EventStoreErrorSource> {
        match self {
            Self::Serialization { source, .. }
            | Self::Deserialization { source, .. }
            | Self::Connection { source, .. }
            | Self::Backend { source, .. }
            | Self::Unknown { source, .. } => source.as_deref(),
            Self::Concurrency(_) | Self::Poisoned => None,
        }
    }

    /// Returns whether callers may retry the failed operation safely.
    ///
    /// Concurrency and connection failures are retryable. Backend failures are
    /// retryable when they represent transient locks or stream-revision unique
    /// violations surfaced during [`ExpectedRevision::Any`] races.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Concurrency(_) | Self::Connection { .. } => true,
            Self::Backend { message, code, .. } => {
                is_retryable_backend_conflict(message, code.as_deref())
            }
            Self::Serialization { .. }
            | Self::Deserialization { .. }
            | Self::Poisoned
            | Self::Unknown { .. } => false,
        }
    }

    /// Returns the machine-readable backend code (SQLSTATE, errno) when the
    /// adapter recorded one.
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Serialization { code, .. }
            | Self::Deserialization { code, .. }
            | Self::Connection { code, .. }
            | Self::Backend { code, .. }
            | Self::Unknown { code, .. } => code.as_deref(),
            Self::Concurrency(_) | Self::Poisoned => None,
        }
    }
}

fn is_retryable_backend_conflict(message: &str, code: Option<&str>) -> bool {
    let lower = message.to_ascii_lowercase();
    if lower.contains("locked") {
        return true;
    }

    if matches!(code, Some("23505") | Some("1062") | Some("40001")) {
        return is_stream_revision_unique_violation_message(&lower);
    }

    (lower.contains("unique") || lower.contains("duplicate") || lower.contains("constraint"))
        && is_stream_revision_unique_violation_message(&lower)
}

fn is_stream_revision_unique_violation_message(lower: &str) -> bool {
    if lower.contains("event_id") {
        return false;
    }
    lower.contains("revision")
        && (lower.contains("aggregate")
            || lower.contains("aggregate_id")
            || lower.contains("aggregate_type")
            || lower.contains("idx_aggregate_revision"))
}

impl PartialEq for EventStoreError {
    /// Compares kind, message, and code; preserved sources are ignored.
    fn eq(&self, other: &Self) -> bool {
        use EventStoreError::*;

        match (self, other) {
            (Concurrency(left), Concurrency(right)) => left == right,
            (Poisoned, Poisoned) => true,
            (
                Serialization {
                    message: left,
                    code: left_code,
                    ..
                },
                Serialization {
                    message: right,
                    code: right_code,
                    ..
                },
            )
            | (
                Deserialization {
                    message: left,
                    code: left_code,
                    ..
                },
                Deserialization {
                    message: right,
                    code: right_code,
                    ..
                },
            )
            | (
                Connection {
                    message: left,
                    code: left_code,
                    ..
                },
                Connection {
                    message: right,
                    code: right_code,
                    ..
                },
            )
            | (
                Backend {
                    message: left,
                    code: left_code,
                    ..
                },
                Backend {
                    message: right,
                    code: right_code,
                    ..
                },
            )
            | (
                Unknown {
                    message: left,
                    code: left_code,
                    ..
                },
                Unknown {
                    message: right,
                    code: right_code,
                    ..
                },
            ) => left == right && left_code == right_code,
            _ => false,
        }
    }
}

impl Eq for EventStoreError {}

/// Classifies store errors for repository-level error mapping.
///
/// Custom event stores can implement this trait to surface concurrency failures
/// as [`RepositoryError::Concurrency`] while preserving all other errors as
/// [`RepositoryError::Store`].
pub trait EventStoreFailure: Sized {
    /// Converts a store error into a repository error.
    fn into_repository_error<DomainError>(self) -> RepositoryError<DomainError, Self> {
        RepositoryError::Store(self)
    }

    /// Returns whether the store failure may be retried safely.
    fn is_retryable(&self) -> bool {
        false
    }
}

impl EventStoreFailure for EventStoreError {
    fn into_repository_error<DomainError>(self) -> RepositoryError<DomainError, Self> {
        match self {
            EventStoreError::Concurrency(error) => RepositoryError::Concurrency(error),
            error => RepositoryError::Store(error),
        }
    }

    fn is_retryable(&self) -> bool {
        EventStoreError::is_retryable(self)
    }
}

impl Display for EventStoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            EventStoreError::Concurrency(error) => Display::fmt(error, f),
            EventStoreError::Serialization { message, .. } => {
                write!(f, "serialization error: {message}")
            }
            EventStoreError::Deserialization { message, .. } => {
                write!(f, "deserialization error: {message}")
            }
            EventStoreError::Connection { message, .. } => {
                write!(f, "connection error: {message}")
            }
            EventStoreError::Poisoned => f.write_str("event store lock was poisoned"),
            EventStoreError::Backend { message, .. } => {
                write!(f, "event store backend error: {message}")
            }
            EventStoreError::Unknown { message, .. } => {
                write!(f, "unknown event store error: {message}")
            }
        }
    }
}

impl Error for EventStoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            EventStoreError::Concurrency(error) => Some(error),
            EventStoreError::Serialization { source, .. }
            | EventStoreError::Deserialization { source, .. }
            | EventStoreError::Connection { source, .. }
            | EventStoreError::Backend { source, .. }
            | EventStoreError::Unknown { source, .. } => source
                .as_deref()
                .map(|source| source as &(dyn Error + 'static)),
            EventStoreError::Poisoned => None,
        }
    }
}

/// Error returned by repository operations.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::{RepositoryError, EventStoreError};
///
/// let store_err = EventStoreError::connection("db offline".to_string());
/// let error: RepositoryError<&'static str, EventStoreError> = RepositoryError::Store(store_err);
/// assert_eq!(error.to_string(), "connection error: db offline");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepositoryError<DomainError, StoreError = EventStoreError> {
    /// Aggregate command handling rejected the command.
    Domain(DomainError),
    /// Event store rejected the append due to optimistic concurrency.
    Concurrency(ConcurrencyError),
    /// Event store or infrastructure operation failed.
    Store(StoreError),
}

impl<DomainError, StoreError> RepositoryError<DomainError, StoreError> {
    /// Returns whether the repository failure may be retried safely.
    pub fn is_retryable(&self) -> bool
    where
        StoreError: EventStoreFailure,
    {
        match self {
            RepositoryError::Concurrency(error) => error.is_retryable(),
            RepositoryError::Store(error) => error.is_retryable(),
            RepositoryError::Domain(_) => false,
        }
    }
}

impl<DomainError, StoreError> Display for RepositoryError<DomainError, StoreError>
where
    DomainError: Display,
    StoreError: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RepositoryError::Domain(error) => Display::fmt(error, f),
            RepositoryError::Concurrency(error) => Display::fmt(error, f),
            RepositoryError::Store(error) => Display::fmt(error, f),
        }
    }
}

impl<DomainError, StoreError> Error for RepositoryError<DomainError, StoreError>
where
    DomainError: Error + 'static,
    StoreError: Error + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            RepositoryError::Domain(error) => Some(error),
            RepositoryError::Concurrency(error) => Some(error),
            RepositoryError::Store(error) => Some(error),
        }
    }
}

impl<DomainError> From<EventStoreError> for RepositoryError<DomainError, EventStoreError> {
    fn from(value: EventStoreError) -> Self {
        match value {
            EventStoreError::Concurrency(error) => RepositoryError::Concurrency(error),
            error => RepositoryError::Store(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn event_store_error_source_downcasts_typed_error() {
        let io_error = io::Error::other("socket refused");
        let source = EventStoreErrorSource::from_error(io_error);
        assert!(source.downcast_ref::<io::Error>().is_some());
    }

    #[test]
    fn event_store_error_is_retryable_taxonomy() {
        assert!(EventStoreError::Concurrency(ConcurrencyError::StreamAlreadyExists).is_retryable());
        assert!(EventStoreError::connection("db offline").is_retryable());
        assert!(!EventStoreError::serialization("bad json").is_retryable());

        let unique =
            EventStoreError::backend("duplicate revision for aggregate").with_code("23505");
        assert!(unique.is_retryable());

        let event_id = EventStoreError::backend("duplicate event_id value").with_code("23505");
        assert!(!event_id.is_retryable());
    }

    #[test]
    fn repository_error_is_retryable_delegates_to_store() {
        let error: RepositoryError<(), EventStoreError> =
            RepositoryError::Store(EventStoreError::connection("offline"));
        assert!(error.is_retryable());
    }

    #[test]
    fn public_message_is_scrubbed_surface() {
        let error = EventStoreError::backend("storage unavailable");
        assert_eq!(error.public_message(), "storage unavailable");
        assert_eq!(
            error.to_string(),
            "event store backend error: storage unavailable"
        );
    }
}
