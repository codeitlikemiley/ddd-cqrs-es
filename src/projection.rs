use crate::aggregate::Aggregate;
use crate::event::EventEnvelope;
use crate::event_store::EventStore;
use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::num::NonZeroUsize;

#[cfg(feature = "async")]
use async_trait::async_trait;

/// A read-model updater.
///
/// Projections consume committed event envelopes and update query-optimized
/// state. Implementations should be idempotent because projection runners may
/// retry after failures.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::{Projection, EventEnvelope, EventId, Metadata};
/// use std::time::SystemTime;
///
/// #[derive(Clone)]
/// enum UserEvent {
///     Created(String),
/// }
///
/// struct UserCounter {
///     count: usize,
/// }
///
/// impl Projection<UserEvent, String> for UserCounter {
///     type Error = std::convert::Infallible;
///
///     fn name(&self) -> &'static str { "user_counter" }
///
///     fn apply(&mut self, event: &EventEnvelope<UserEvent, String>) -> Result<(), Self::Error> {
///         match event.event() {
///             UserEvent::Created(_) => self.count += 1,
///         }
///         Ok(())
///     }
/// }
///
/// let mut counter = UserCounter { count: 0 };
/// let envelope = EventEnvelope::new(
///     EventId::new(),
///     "user-1".to_string(),
///     "user",
///     1,
///     None,
///     "UserCreated",
///     1,
///     UserEvent::Created("Alice".to_owned()),
///     Metadata::default(),
///     SystemTime::now(),
/// );
/// counter.apply(&envelope).unwrap();
/// assert_eq!(counter.count, 1);
/// ```
pub trait Projection<E, Id> {
    /// Projection error.
    type Error;

    /// Stable projection name used for checkpoint storage.
    fn name(&self) -> &'static str;

    /// Applies one committed event to the projection.
    fn apply(&mut self, event: &EventEnvelope<E, Id>) -> Result<(), Self::Error>;
}

/// Default maximum event count for bounded projection catch-up.
pub const DEFAULT_PROJECTION_BATCH_SIZE: usize = 500;

/// Controls bounded projection replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectionBatchConfig {
    batch_size: NonZeroUsize,
}

impl ProjectionBatchConfig {
    /// Creates a new projection batch configuration.
    pub const fn new(batch_size: NonZeroUsize) -> Self {
        Self { batch_size }
    }

    /// Returns the maximum number of events loaded and applied in one batch.
    pub const fn batch_size(self) -> NonZeroUsize {
        self.batch_size
    }
}

impl Default for ProjectionBatchConfig {
    fn default() -> Self {
        Self {
            batch_size: NonZeroUsize::new(DEFAULT_PROJECTION_BATCH_SIZE)
                .expect("default projection batch size must be non-zero"),
        }
    }
}

/// Result of one bounded projection replay pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectionBatchOutcome {
    /// Number of events applied in this batch.
    pub applied: usize,
    /// Last global sequence successfully applied in this batch.
    pub last_sequence: Option<u64>,
    /// Whether the runner observed fewer events than the configured batch size.
    pub caught_up: bool,
}

fn projection_batch_outcome(
    applied: usize,
    last_sequence: Option<u64>,
    config: ProjectionBatchConfig,
) -> ProjectionBatchOutcome {
    ProjectionBatchOutcome {
        applied,
        last_sequence,
        caught_up: applied < config.batch_size().get(),
    }
}

/// Decides whether a catch-up loop should request another batch, advancing
/// `position` to the batch's last sequence.
///
/// A full batch normally means more events are waiting. It only means that when
/// the batch also moved the feed position forward: a store that hands back a
/// full batch of events carrying no (or a non-increasing) global sequence would
/// otherwise have the same batch replayed forever, because the next read
/// resumes from the same checkpoint. Such a batch ends the loop instead.
fn batch_advanced(outcome: ProjectionBatchOutcome, position: &mut Option<u64>) -> bool {
    if outcome.caught_up {
        return false;
    }
    match outcome.last_sequence {
        Some(sequence) if position.is_none_or(|previous| sequence > previous) => {
            *position = Some(sequence);
            true
        }
        _ => false,
    }
}

/// In-memory projection runner with a sequence checkpoint.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::{InMemoryProjectionRunner, InMemoryEventStore, Projection, EventEnvelope, EventId, Metadata};
/// use std::time::SystemTime;
/// # use ddd_cqrs_es::Aggregate;
/// # #[derive(Clone)]
/// # enum UserEvent { Created }
/// # impl ddd_cqrs_es::DomainEvent for UserEvent {
/// #     fn event_type(&self) -> &'static str { "user_created" }
/// # }
/// # #[derive(Clone, Debug, PartialEq)]
/// # struct UserAggregate;
/// # impl Aggregate for UserAggregate {
/// #     type Id = String;
/// #     type Command = ();
/// #     type Event = UserEvent;
/// #     type Error = ();
/// #     fn aggregate_type() -> &'static str { "user" }
/// #     fn new() -> Self { UserAggregate }
/// #     fn apply(&mut self, _event: &Self::Event) {}
/// #     fn handle(&self, _command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> { Ok(vec![]) }
/// # }
///
/// struct UserCounter {
///     count: usize,
/// }
///
/// impl Projection<UserEvent, String> for UserCounter {
///     type Error = std::convert::Infallible;
///     fn name(&self) -> &'static str { "user_counter" }
///     fn apply(&mut self, event: &EventEnvelope<UserEvent, String>) -> Result<(), Self::Error> {
///         self.count += 1;
///         Ok(())
///     }
/// }
///
/// let store = InMemoryEventStore::<UserAggregate>::new();
/// let mut runner = InMemoryProjectionRunner::new(UserCounter { count: 0 });
/// runner.run(&store).unwrap();
/// assert_eq!(runner.projection().count, 0);
/// ```
#[derive(Clone, Debug)]
pub struct InMemoryProjectionRunner<P> {
    projection: P,
    checkpoint: Option<u64>,
}

impl<P> InMemoryProjectionRunner<P> {
    /// Creates a runner for a projection.
    pub fn new(projection: P) -> Self {
        Self {
            projection,
            checkpoint: None,
        }
    }

    /// Returns the last successfully applied global sequence.
    pub fn checkpoint(&self) -> Option<u64> {
        self.checkpoint
    }

    /// Returns the wrapped projection.
    pub fn projection(&self) -> &P {
        &self.projection
    }

    /// Returns the wrapped projection mutably.
    pub fn projection_mut(&mut self) -> &mut P {
        &mut self.projection
    }

    /// Consumes the runner and returns the projection.
    pub fn into_projection(self) -> P {
        self.projection
    }
}

impl<P> InMemoryProjectionRunner<P> {
    /// Catches the projection up to the end of the feed, applying events in
    /// [`ProjectionBatchConfig::default`] sized batches.
    ///
    /// The tail is never materialized in one allocation: this repeats
    /// [`Self::run_batch`] until a batch reports
    /// [`ProjectionBatchOutcome::caught_up`], so a backlog of any size costs one
    /// batch of memory at a time.
    pub fn run<A, S>(
        &mut self,
        store: &S,
    ) -> Result<usize, ProjectionRunnerError<P::Error, S::Error>>
    where
        A: Aggregate,
        S: EventStore<A>,
        P: Projection<A::Event, A::Id>,
    {
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!(
            "projection.run",
            runner = "in_memory",
            projection = self.projection.name(),
            aggregate_type = A::aggregate_type()
        )
        .entered();

        let config = ProjectionBatchConfig::default();
        let mut applied = 0;
        let mut position = None;

        loop {
            let outcome = self.run_batch(store, config)?;
            applied += outcome.applied;
            if !batch_advanced(outcome, &mut position) {
                return Ok(applied);
            }
        }
    }

    /// Loads at most `config.batch_size()` global events after the current
    /// checkpoint and applies them.
    pub fn run_batch<A, S>(
        &mut self,
        store: &S,
        config: ProjectionBatchConfig,
    ) -> Result<ProjectionBatchOutcome, ProjectionRunnerError<P::Error, S::Error>>
    where
        A: Aggregate,
        S: EventStore<A>,
        P: Projection<A::Event, A::Id>,
    {
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!(
            "projection.run_batch",
            runner = "in_memory",
            projection = self.projection.name(),
            aggregate_type = A::aggregate_type(),
            batch_size = config.batch_size().get()
        )
        .entered();

        let events = store
            .load_global_after_limited(self.checkpoint, config.batch_size())
            .map_err(ProjectionRunnerError::Store)?;
        let mut applied = 0;
        let mut last_sequence = None;

        for event in events {
            self.projection
                .apply(&event)
                .map_err(ProjectionRunnerError::Projection)?;
            self.checkpoint = event.sequence;
            last_sequence = event.sequence;
            applied += 1;
        }

        Ok(projection_batch_outcome(applied, last_sequence, config))
    }
}

/// Error returned by a projection runner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectionRunnerError<
    ProjectionError,
    StoreError,
    CheckpointError = std::convert::Infallible,
> {
    /// Projection logic failed.
    Projection(ProjectionError),
    /// Event store read failed.
    Store(StoreError),
    /// Checkpoint storage failed.
    Checkpoint(CheckpointError),
}

impl<ProjectionError, StoreError, CheckpointError> Display
    for ProjectionRunnerError<ProjectionError, StoreError, CheckpointError>
where
    ProjectionError: Display,
    StoreError: Display,
    CheckpointError: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectionRunnerError::Projection(error) => Display::fmt(error, f),
            ProjectionRunnerError::Store(error) => Display::fmt(error, f),
            ProjectionRunnerError::Checkpoint(error) => Display::fmt(error, f),
        }
    }
}

impl<ProjectionError, StoreError, CheckpointError> std::error::Error
    for ProjectionRunnerError<ProjectionError, StoreError, CheckpointError>
where
    ProjectionError: std::error::Error + 'static,
    StoreError: std::error::Error + 'static,
    CheckpointError: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProjectionRunnerError::Projection(error) => Some(error),
            ProjectionRunnerError::Store(error) => Some(error),
            ProjectionRunnerError::Checkpoint(error) => Some(error),
        }
    }
}

/// A persistent store for tracking projection sequence checkpoints.
pub trait CheckpointStore {
    /// Error type.
    type Error;

    /// Loads the last successfully processed event global sequence for a given projection name.
    fn load_checkpoint(&self, projection_name: &str) -> Result<Option<u64>, Self::Error>;

    /// Saves the last successfully processed event global sequence for a given projection name.
    fn save_checkpoint(&self, projection_name: &str, sequence: u64) -> Result<(), Self::Error>;
}

/// An async persistent store for tracking projection sequence checkpoints.
#[cfg(feature = "async")]
#[async_trait]
pub trait AsyncCheckpointStore {
    /// Error type.
    type Error;

    /// Loads the last successfully processed event global sequence for a given projection name.
    async fn load_checkpoint(&self, projection_name: &str) -> Result<Option<u64>, Self::Error>;

    /// Saves the last successfully processed event global sequence for a given projection name.
    async fn save_checkpoint(
        &self,
        projection_name: &str,
        sequence: u64,
    ) -> Result<(), Self::Error>;
}

/// Separator between a projection name and its checkpoint scope.
const CHECKPOINT_SCOPE_SEPARATOR: char = '@';

/// Reserved scope for cross-aggregate raw feed replay, which has its own
/// position independent of any single aggregate type's feed.
const RAW_CHECKPOINT_SCOPE: &str = "*raw";

/// Builds the checkpoint key an aggregate-scoped runner uses for one aggregate
/// type.
///
/// Exposed so a deployment upgrading from name-only checkpoints can seed the
/// scoped rows from its existing row instead of replaying from zero:
///
/// ```rust,no_run
/// # use ddd_cqrs_es::projection::{aggregate_scoped_checkpoint_key, CheckpointStore};
/// # fn seed<C: CheckpointStore>(store: &C) -> Result<(), C::Error> {
/// if let Some(sequence) = store.load_checkpoint("order_summary")? {
///     store.save_checkpoint(&aggregate_scoped_checkpoint_key("order_summary", "order"), sequence)?;
/// }
/// # Ok(())
/// # }
/// ```
pub fn aggregate_scoped_checkpoint_key(projection_name: &str, aggregate_type: &str) -> String {
    format!("{projection_name}{CHECKPOINT_SCOPE_SEPARATOR}{aggregate_type}")
}

/// Builds the checkpoint key an aggregate-scoped runner uses for
/// [`PersistedProjectionRunner::run_raw_batch`] cross-aggregate replay.
pub fn raw_checkpoint_key(projection_name: &str) -> String {
    aggregate_scoped_checkpoint_key(projection_name, RAW_CHECKPOINT_SCOPE)
}

/// How a persisted runner derives the checkpoint key it reads and writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CheckpointKeying {
    /// The bare [`Projection::name()`], shared by every feed the projection is
    /// run against.
    ProjectionName,
    /// [`Projection::name()`] scoped by the feed being replayed.
    AggregateScoped,
}

/// A projection runner that uses a persistent `CheckpointStore` to coordinate progress.
///
/// # Checkpoint keys
///
/// [`Self::new`] keys checkpoints on [`Projection::name()`] alone. Because
/// global replay feeds are scoped to one aggregate type (see
/// [`EventStore::load_global_after`]),
/// running one projection against several aggregate types under that keying
/// makes those runs share a single position: whichever feed advances furthest
/// hides the others' events permanently.
///
/// [`Self::with_aggregate_scoped_checkpoints`] gives every feed its own row and
/// is the correct choice for a projection spanning more than one aggregate type.
/// It is opt-in so upgrading does not silently rewind existing deployments to
/// zero; see [`aggregate_scoped_checkpoint_key`] for seeding the new rows.
#[derive(Debug)]
pub struct PersistedProjectionRunner<P, C> {
    projection: P,
    checkpoint_store: C,
    keying: CheckpointKeying,
}

impl<P, C> PersistedProjectionRunner<P, C> {
    /// Creates a new persisted runner that keys checkpoints on
    /// [`Projection::name()`] alone.
    ///
    /// Correct for a projection driven by exactly one aggregate type's feed.
    /// Use [`Self::with_aggregate_scoped_checkpoints`] otherwise.
    pub fn new(projection: P, checkpoint_store: C) -> Self {
        Self {
            projection,
            checkpoint_store,
            keying: CheckpointKeying::ProjectionName,
        }
    }

    /// Creates a persisted runner that keys checkpoints on the projection name
    /// **and** the feed being replayed, so a projection spanning several
    /// aggregate types tracks each feed independently.
    ///
    /// [`Self::run_raw_batch`] gets its own cross-aggregate scope rather than
    /// sharing a position with any typed feed.
    pub fn with_aggregate_scoped_checkpoints(projection: P, checkpoint_store: C) -> Self {
        Self {
            projection,
            checkpoint_store,
            keying: CheckpointKeying::AggregateScoped,
        }
    }

    fn checkpoint_key(&self, name: &'static str, scope: &str) -> Cow<'static, str> {
        match self.keying {
            CheckpointKeying::ProjectionName => Cow::Borrowed(name),
            CheckpointKeying::AggregateScoped => {
                Cow::Owned(aggregate_scoped_checkpoint_key(name, scope))
            }
        }
    }

    /// Returns the wrapped projection.
    pub fn projection(&self) -> &P {
        &self.projection
    }

    /// Returns the wrapped projection mutably.
    pub fn projection_mut(&mut self) -> &mut P {
        &mut self.projection
    }

    /// Consumes the runner and returns the projection.
    pub fn into_projection(self) -> P {
        self.projection
    }
}

impl<P, C> PersistedProjectionRunner<P, C>
where
    C: CheckpointStore,
{
    /// Catches the projection up to the end of the feed, applying events in
    /// [`ProjectionBatchConfig::default`] sized batches and saving the last
    /// applied sequence as the checkpoint once per batch. When a projection
    /// fails mid-batch, the sequence of the last successfully applied event is
    /// still saved before the error is returned, so a retry resumes where the
    /// failure happened.
    ///
    /// The tail is never materialized in one allocation: this repeats
    /// [`Self::run_batch`] until a batch reports
    /// [`ProjectionBatchOutcome::caught_up`], so a backlog of any size costs one
    /// batch of memory at a time.
    ///
    /// Projection side effects and checkpoint writes are not one transaction;
    /// projection implementations must be idempotent for retry safety. Events
    /// applied after the last persisted checkpoint are re-applied when the
    /// process stops before the pass completes.
    #[allow(clippy::type_complexity)]
    pub fn run<A, S>(
        &mut self,
        store: &S,
    ) -> Result<usize, ProjectionRunnerError<P::Error, S::Error, C::Error>>
    where
        A: Aggregate,
        S: EventStore<A>,
        P: Projection<A::Event, A::Id>,
    {
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!(
            "projection.run",
            runner = "persisted",
            projection = self.projection.name(),
            aggregate_type = A::aggregate_type()
        )
        .entered();

        let config = ProjectionBatchConfig::default();
        let mut applied = 0;
        let mut position = None;

        loop {
            let outcome = self.run_batch(store, config)?;
            applied += outcome.applied;
            if !batch_advanced(outcome, &mut position) {
                return Ok(applied);
            }
        }
    }

    /// Loads at most `config.batch_size()` global events after the persistent
    /// checkpoint, applies them, and saves the last applied sequence as the
    /// checkpoint once per batch (also flushing progress before returning a
    /// mid-batch projection error).
    #[allow(clippy::type_complexity)]
    pub fn run_batch<A, S>(
        &mut self,
        store: &S,
        config: ProjectionBatchConfig,
    ) -> Result<ProjectionBatchOutcome, ProjectionRunnerError<P::Error, S::Error, C::Error>>
    where
        A: Aggregate,
        S: EventStore<A>,
        P: Projection<A::Event, A::Id>,
    {
        let name = self.projection.name();
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!(
            "projection.run_batch",
            runner = "persisted",
            projection = name,
            aggregate_type = A::aggregate_type(),
            batch_size = config.batch_size().get()
        )
        .entered();

        let key = self.checkpoint_key(name, A::aggregate_type());
        let checkpoint = self
            .checkpoint_store
            .load_checkpoint(&key)
            .map_err(ProjectionRunnerError::Checkpoint)?;

        let events = store
            .load_global_after_limited(checkpoint, config.batch_size())
            .map_err(ProjectionRunnerError::Store)?;
        let mut applied = 0;
        let mut last_sequence = None;
        let mut failure = None;

        for event in events {
            if let Err(error) = self.projection.apply(&event) {
                failure = Some(ProjectionRunnerError::Projection(error));
                break;
            }
            if event.sequence.is_some() {
                last_sequence = event.sequence;
            }
            applied += 1;
        }

        let flushed = match last_sequence {
            Some(sequence) => self
                .checkpoint_store
                .save_checkpoint(&key, sequence)
                .map_err(ProjectionRunnerError::Checkpoint),
            None => Ok(()),
        };
        if let Some(error) = failure {
            return Err(error);
        }
        flushed?;

        Ok(projection_batch_outcome(applied, last_sequence, config))
    }

    /// Loads at most `config.batch_size()` events of **any aggregate type**
    /// from a [`RawEventFeed`](crate::raw_feed::RawEventFeed) after the
    /// persistent checkpoint and applies them as raw envelopes, with the same
    /// once-per-batch checkpoint semantics as [`Self::run_batch`].
    ///
    /// Under [`Self::with_aggregate_scoped_checkpoints`] this cross-aggregate
    /// replay keeps its own checkpoint row (see [`raw_checkpoint_key`]) rather
    /// than sharing a position with a typed feed.
    #[cfg(feature = "json")]
    #[allow(clippy::type_complexity)]
    pub fn run_raw_batch<S>(
        &mut self,
        feed: &S,
        config: ProjectionBatchConfig,
    ) -> Result<ProjectionBatchOutcome, ProjectionRunnerError<P::Error, S::Error, C::Error>>
    where
        S: crate::raw_feed::RawEventFeed,
        P: Projection<serde_json::Value, String>,
    {
        let name = self.projection.name();
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!(
            "projection.run_raw_batch",
            runner = "persisted",
            projection = name,
            batch_size = config.batch_size().get()
        )
        .entered();

        let key = self.checkpoint_key(name, RAW_CHECKPOINT_SCOPE);
        let checkpoint = self
            .checkpoint_store
            .load_checkpoint(&key)
            .map_err(ProjectionRunnerError::Checkpoint)?;

        let events = feed
            .load_raw_global_after_limited(checkpoint, config.batch_size())
            .map_err(ProjectionRunnerError::Store)?;
        let mut applied = 0;
        let mut last_sequence = None;
        let mut failure = None;

        for event in events {
            if let Err(error) = self.projection.apply(&event) {
                failure = Some(ProjectionRunnerError::Projection(error));
                break;
            }
            if event.sequence.is_some() {
                last_sequence = event.sequence;
            }
            applied += 1;
        }

        let flushed = match last_sequence {
            Some(sequence) => self
                .checkpoint_store
                .save_checkpoint(&key, sequence)
                .map_err(ProjectionRunnerError::Checkpoint),
            None => Ok(()),
        };
        if let Some(error) = failure {
            return Err(error);
        }
        flushed?;

        Ok(projection_batch_outcome(applied, last_sequence, config))
    }
}

/// An async projection runner that uses a persistent `AsyncCheckpointStore` to coordinate progress.
///
/// Checkpoint keying matches [`PersistedProjectionRunner`]: [`Self::new`] keys
/// on [`Projection::name()`] alone, and
/// [`Self::with_aggregate_scoped_checkpoints`] gives each replayed feed its own
/// row.
#[cfg(feature = "async")]
#[derive(Debug)]
pub struct AsyncPersistedProjectionRunner<P, C> {
    projection: P,
    checkpoint_store: C,
    keying: CheckpointKeying,
}

#[cfg(feature = "async")]
impl<P, C> AsyncPersistedProjectionRunner<P, C> {
    /// Creates a new async persisted runner that keys checkpoints on
    /// [`Projection::name()`] alone.
    ///
    /// Correct for a projection driven by exactly one aggregate type's feed.
    /// Use [`Self::with_aggregate_scoped_checkpoints`] otherwise.
    pub fn new(projection: P, checkpoint_store: C) -> Self {
        Self {
            projection,
            checkpoint_store,
            keying: CheckpointKeying::ProjectionName,
        }
    }

    /// Creates an async persisted runner that keys checkpoints on the
    /// projection name **and** the feed being replayed, so a projection
    /// spanning several aggregate types tracks each feed independently.
    pub fn with_aggregate_scoped_checkpoints(projection: P, checkpoint_store: C) -> Self {
        Self {
            projection,
            checkpoint_store,
            keying: CheckpointKeying::AggregateScoped,
        }
    }

    fn checkpoint_key(&self, name: &'static str, scope: &str) -> Cow<'static, str> {
        match self.keying {
            CheckpointKeying::ProjectionName => Cow::Borrowed(name),
            CheckpointKeying::AggregateScoped => {
                Cow::Owned(aggregate_scoped_checkpoint_key(name, scope))
            }
        }
    }

    /// Returns the wrapped projection.
    pub fn projection(&self) -> &P {
        &self.projection
    }

    /// Returns the wrapped projection mutably.
    pub fn projection_mut(&mut self) -> &mut P {
        &mut self.projection
    }

    /// Consumes the runner and returns the projection.
    pub fn into_projection(self) -> P {
        self.projection
    }
}

#[cfg(feature = "async")]
impl<P, C> AsyncPersistedProjectionRunner<P, C>
where
    C: AsyncCheckpointStore,
{
    /// Catches the projection up to the end of the feed, applying events in
    /// [`ProjectionBatchConfig::default`] sized batches and saving the last
    /// applied sequence as the checkpoint once per batch. When a projection
    /// fails mid-batch, the sequence of the last successfully applied event is
    /// still saved before the error is returned, so a retry resumes where the
    /// failure happened.
    ///
    /// The tail is never materialized in one allocation: this repeats
    /// [`Self::run_batch`] until a batch reports
    /// [`ProjectionBatchOutcome::caught_up`], so a backlog of any size costs one
    /// batch of memory at a time.
    ///
    /// Projection side effects and checkpoint writes are not one transaction;
    /// projection implementations must be idempotent for retry safety. Events
    /// applied after the last persisted checkpoint are re-applied when the
    /// process stops before the pass completes.
    #[allow(clippy::type_complexity)]
    pub async fn run<A, S>(
        &mut self,
        store: &S,
    ) -> Result<usize, ProjectionRunnerError<P::Error, S::Error, C::Error>>
    where
        A: Aggregate + Send + Sync,
        S: crate::async_api::AsyncEventStore<A>,
        P: Projection<A::Event, A::Id>,
    {
        let config = ProjectionBatchConfig::default();
        let mut applied = 0;
        let mut position = None;

        loop {
            let outcome = self.run_batch(store, config).await?;
            applied += outcome.applied;
            if !batch_advanced(outcome, &mut position) {
                return Ok(applied);
            }
        }
    }

    /// Loads at most `config.batch_size()` global events after the persistent
    /// checkpoint, applies them, and saves the last applied sequence as the
    /// checkpoint once per batch (also flushing progress before returning a
    /// mid-batch projection error).
    #[allow(clippy::type_complexity)]
    pub async fn run_batch<A, S>(
        &mut self,
        store: &S,
        config: ProjectionBatchConfig,
    ) -> Result<ProjectionBatchOutcome, ProjectionRunnerError<P::Error, S::Error, C::Error>>
    where
        A: Aggregate + Send + Sync,
        S: crate::async_api::AsyncEventStore<A>,
        P: Projection<A::Event, A::Id>,
    {
        let name = self.projection.name();
        let key = self.checkpoint_key(name, A::aggregate_type());
        let checkpoint = self
            .checkpoint_store
            .load_checkpoint(&key)
            .await
            .map_err(ProjectionRunnerError::Checkpoint)?;

        let events = store
            .load_global_after_limited(checkpoint, config.batch_size())
            .await
            .map_err(ProjectionRunnerError::Store)?;
        let mut applied = 0;
        let mut last_sequence = None;
        let mut failure = None;

        for event in events {
            if let Err(error) = self.projection.apply(&event) {
                failure = Some(ProjectionRunnerError::Projection(error));
                break;
            }
            if event.sequence.is_some() {
                last_sequence = event.sequence;
            }
            applied += 1;
        }

        let flushed = match last_sequence {
            Some(sequence) => self
                .checkpoint_store
                .save_checkpoint(&key, sequence)
                .await
                .map_err(ProjectionRunnerError::Checkpoint),
            None => Ok(()),
        };
        if let Some(error) = failure {
            return Err(error);
        }
        flushed?;

        Ok(projection_batch_outcome(applied, last_sequence, config))
    }

    /// Loads at most `config.batch_size()` events of **any aggregate type**
    /// from an [`AsyncRawEventFeed`](crate::raw_feed::AsyncRawEventFeed)
    /// after the persistent checkpoint and applies them as raw envelopes,
    /// with the same once-per-batch checkpoint semantics as
    /// [`Self::run_batch`].
    ///
    /// Under [`Self::with_aggregate_scoped_checkpoints`] this cross-aggregate
    /// replay keeps its own checkpoint row (see [`raw_checkpoint_key`]).
    #[cfg(feature = "json")]
    #[allow(clippy::type_complexity)]
    pub async fn run_raw_batch<S>(
        &mut self,
        feed: &S,
        config: ProjectionBatchConfig,
    ) -> Result<ProjectionBatchOutcome, ProjectionRunnerError<P::Error, S::Error, C::Error>>
    where
        S: crate::raw_feed::AsyncRawEventFeed,
        P: Projection<serde_json::Value, String>,
    {
        let name = self.projection.name();
        let key = self.checkpoint_key(name, RAW_CHECKPOINT_SCOPE);
        let checkpoint = self
            .checkpoint_store
            .load_checkpoint(&key)
            .await
            .map_err(ProjectionRunnerError::Checkpoint)?;

        let events = feed
            .load_raw_global_after_limited(checkpoint, config.batch_size())
            .await
            .map_err(ProjectionRunnerError::Store)?;
        let mut applied = 0;
        let mut last_sequence = None;
        let mut failure = None;

        for event in events {
            if let Err(error) = self.projection.apply(&event) {
                failure = Some(ProjectionRunnerError::Projection(error));
                break;
            }
            if event.sequence.is_some() {
                last_sequence = event.sequence;
            }
            applied += 1;
        }

        let flushed = match last_sequence {
            Some(sequence) => self
                .checkpoint_store
                .save_checkpoint(&key, sequence)
                .await
                .map_err(ProjectionRunnerError::Checkpoint),
            None => Ok(()),
        };
        if let Some(error) = failure {
            return Err(error);
        }
        flushed?;

        Ok(projection_batch_outcome(applied, last_sequence, config))
    }
}

/// A projection that manages its own state and checkpoint persistence atomically.
///
/// # Note on Atomicity
/// While this trait is designed to enable atomic updates, the atomicity itself depends entirely
/// on the implementation of `apply_and_checkpoint` (e.g., executing the state modification and
/// the checkpoint update within a single database transaction). The runner itself does not
/// magically introduce or enforce atomicity for arbitrary non-transactional code.
pub trait CheckpointedProjection<E, Id> {
    /// Projection error.
    type Error;

    /// Stable projection name.
    fn name(&self) -> &'static str;

    /// Loads the last successfully processed event global sequence.
    fn load_checkpoint(&self) -> Result<Option<u64>, Self::Error>;

    /// Atomic operation to apply an event and persist its checkpoint.
    ///
    /// This should typically be executed within a transaction where both the state
    /// modification and checkpoint update are committed atomically.
    fn apply_and_checkpoint(&mut self, event: &EventEnvelope<E, Id>) -> Result<(), Self::Error>;
}

/// A projection runner for projections that manage their own checkpoints atomically.
///
/// # Note on Atomicity
/// This runner coordinates the execution of projection updates but **does not** enforce or introduce
/// database transactions itself. Atomicity of the event processing and checkpoint saving depends
/// entirely on the underlying projection's implementation of `CheckpointedProjection::apply_and_checkpoint`.
#[derive(Debug)]
pub struct CheckpointedProjectionRunner<P> {
    projection: P,
}

impl<P> CheckpointedProjectionRunner<P> {
    /// Creates a new runner for a checkpointed projection.
    pub fn new(projection: P) -> Self {
        Self { projection }
    }

    /// Returns the wrapped projection.
    pub fn projection(&self) -> &P {
        &self.projection
    }

    /// Returns the wrapped projection mutably.
    pub fn projection_mut(&mut self) -> &mut P {
        &mut self.projection
    }

    /// Consumes the runner and returns the projection.
    pub fn into_projection(self) -> P {
        self.projection
    }
}

impl<P> CheckpointedProjectionRunner<P> {
    /// Catches the projection up to the end of the feed, applying events in
    /// [`ProjectionBatchConfig::default`] sized batches through the
    /// projection-owned checkpoint operation.
    ///
    /// The tail is never materialized in one allocation: this repeats
    /// [`Self::run_batch`] until a batch reports
    /// [`ProjectionBatchOutcome::caught_up`], so a backlog of any size costs one
    /// batch of memory at a time.
    #[allow(clippy::type_complexity)]
    pub fn run<A, S>(
        &mut self,
        store: &S,
    ) -> Result<usize, ProjectionRunnerError<P::Error, S::Error, P::Error>>
    where
        A: Aggregate,
        S: EventStore<A>,
        P: CheckpointedProjection<A::Event, A::Id>,
    {
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!(
            "projection.run",
            runner = "checkpointed",
            projection = self.projection.name(),
            aggregate_type = A::aggregate_type()
        )
        .entered();

        let config = ProjectionBatchConfig::default();
        let mut applied = 0;
        let mut position = None;

        loop {
            let outcome = self.run_batch(store, config)?;
            applied += outcome.applied;
            if !batch_advanced(outcome, &mut position) {
                return Ok(applied);
            }
        }
    }

    /// Loads at most `config.batch_size()` global events after the projection's
    /// checkpoint and applies each event through the projection-owned checkpoint operation.
    #[allow(clippy::type_complexity)]
    pub fn run_batch<A, S>(
        &mut self,
        store: &S,
        config: ProjectionBatchConfig,
    ) -> Result<ProjectionBatchOutcome, ProjectionRunnerError<P::Error, S::Error, P::Error>>
    where
        A: Aggregate,
        S: EventStore<A>,
        P: CheckpointedProjection<A::Event, A::Id>,
    {
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!(
            "projection.run_batch",
            runner = "checkpointed",
            projection = self.projection.name(),
            aggregate_type = A::aggregate_type(),
            batch_size = config.batch_size().get()
        )
        .entered();

        let checkpoint = self
            .projection
            .load_checkpoint()
            .map_err(ProjectionRunnerError::Checkpoint)?;

        let events = store
            .load_global_after_limited(checkpoint, config.batch_size())
            .map_err(ProjectionRunnerError::Store)?;
        let mut applied = 0;
        let mut last_sequence = None;

        for event in events {
            self.projection
                .apply_and_checkpoint(&event)
                .map_err(ProjectionRunnerError::Projection)?;
            last_sequence = event.sequence;
            applied += 1;
        }

        Ok(projection_batch_outcome(applied, last_sequence, config))
    }
}

/// A projection that commits read-model updates and checkpoint movement in one transaction.
///
/// Implementations should use one backing-store transaction inside
/// [`TransactionalCheckpointedProjection::apply_and_checkpoint_transactionally`].
/// This trait is intentionally separate from [`Projection`] so production
/// read models can expose their stronger consistency contract explicitly.
pub trait TransactionalCheckpointedProjection<E, Id> {
    /// Projection error.
    type Error;

    /// Stable projection name.
    fn name(&self) -> &'static str;

    /// Loads the last successfully processed event global sequence.
    fn load_checkpoint(&self) -> Result<Option<u64>, Self::Error>;

    /// Applies one event to the read model and saves the event checkpoint in
    /// the same backing-store transaction.
    fn apply_and_checkpoint_transactionally(
        &mut self,
        event: &EventEnvelope<E, Id>,
    ) -> Result<(), Self::Error>;
}

/// Runner for projections that own a transaction-aware read-model/checkpoint update.
#[derive(Debug)]
pub struct TransactionalCheckpointedProjectionRunner<P> {
    projection: P,
}

impl<P> TransactionalCheckpointedProjectionRunner<P> {
    /// Creates a new transactional checkpointed projection runner.
    pub fn new(projection: P) -> Self {
        Self { projection }
    }

    /// Returns the wrapped projection.
    pub fn projection(&self) -> &P {
        &self.projection
    }

    /// Returns the wrapped projection mutably.
    pub fn projection_mut(&mut self) -> &mut P {
        &mut self.projection
    }

    /// Consumes the runner and returns the projection.
    pub fn into_projection(self) -> P {
        self.projection
    }
}

impl<P> TransactionalCheckpointedProjectionRunner<P> {
    /// Catches the projection up to the end of the feed in
    /// [`ProjectionBatchConfig::default`] sized batches, applying each
    /// read-model update with its checkpoint in one projection-owned
    /// transaction.
    ///
    /// The tail is never materialized in one allocation: this repeats
    /// [`Self::run_batch`] until a batch reports
    /// [`ProjectionBatchOutcome::caught_up`], so a backlog of any size costs one
    /// batch of memory at a time.
    #[allow(clippy::type_complexity)]
    pub fn run<A, S>(
        &mut self,
        store: &S,
    ) -> Result<usize, ProjectionRunnerError<P::Error, S::Error, P::Error>>
    where
        A: Aggregate,
        S: EventStore<A>,
        P: TransactionalCheckpointedProjection<A::Event, A::Id>,
    {
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!(
            "projection.run",
            runner = "transactional",
            projection = self.projection.name(),
            aggregate_type = A::aggregate_type()
        )
        .entered();

        let config = ProjectionBatchConfig::default();
        let mut applied = 0;
        let mut position = None;

        loop {
            let outcome = self.run_batch(store, config)?;
            applied += outcome.applied;
            if !batch_advanced(outcome, &mut position) {
                return Ok(applied);
            }
        }
    }

    /// Loads at most `config.batch_size()` global events after the projection's
    /// checkpoint and applies each read-model update with its checkpoint transaction.
    #[allow(clippy::type_complexity)]
    pub fn run_batch<A, S>(
        &mut self,
        store: &S,
        config: ProjectionBatchConfig,
    ) -> Result<ProjectionBatchOutcome, ProjectionRunnerError<P::Error, S::Error, P::Error>>
    where
        A: Aggregate,
        S: EventStore<A>,
        P: TransactionalCheckpointedProjection<A::Event, A::Id>,
    {
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!(
            "projection.run_batch",
            runner = "transactional",
            projection = self.projection.name(),
            aggregate_type = A::aggregate_type(),
            batch_size = config.batch_size().get()
        )
        .entered();

        let checkpoint = self
            .projection
            .load_checkpoint()
            .map_err(ProjectionRunnerError::Checkpoint)?;

        let events = store
            .load_global_after_limited(checkpoint, config.batch_size())
            .map_err(ProjectionRunnerError::Store)?;
        let mut applied = 0;
        let mut last_sequence = None;

        for event in events {
            self.projection
                .apply_and_checkpoint_transactionally(&event)
                .map_err(ProjectionRunnerError::Projection)?;
            last_sequence = event.sequence;
            applied += 1;
        }

        Ok(projection_batch_outcome(applied, last_sequence, config))
    }
}

/// An async projection that manages its own state and checkpoint persistence atomically.
///
/// # Note on Atomicity
/// While this trait is designed to enable atomic updates, the atomicity itself depends entirely
/// on the implementation of `apply_and_checkpoint` (e.g., executing the state modification and
/// the checkpoint update within a single database transaction). The runner itself does not
/// magically introduce or enforce atomicity for arbitrary non-transactional code.
#[cfg(feature = "async")]
#[async_trait]
pub trait AsyncCheckpointedProjection<E, Id> {
    /// Projection error.
    type Error;

    /// Stable projection name.
    fn name(&self) -> &'static str;

    /// Loads the last successfully processed event global sequence.
    async fn load_checkpoint(&self) -> Result<Option<u64>, Self::Error>;

    /// Atomic operation to apply an event and persist its checkpoint.
    ///
    /// This should typically be executed within a transaction where both the state
    /// modification and checkpoint update are committed atomically.
    async fn apply_and_checkpoint(
        &mut self,
        event: &EventEnvelope<E, Id>,
    ) -> Result<(), Self::Error>;
}

/// An async projection runner for projections that manage their own checkpoints atomically.
///
/// # Note on Atomicity
/// This runner coordinates the execution of projection updates but **does not** enforce or introduce
/// database transactions itself. Atomicity of the event processing and checkpoint saving depends
/// entirely on the underlying projection's implementation of `AsyncCheckpointedProjection::apply_and_checkpoint`.
#[cfg(feature = "async")]
#[derive(Debug)]
pub struct AsyncCheckpointedProjectionRunner<P> {
    projection: P,
}

/// Async projection that commits read-model updates and checkpoint movement in one transaction.
#[cfg(feature = "async")]
#[async_trait]
pub trait AsyncTransactionalCheckpointedProjection<E, Id> {
    /// Projection error.
    type Error;

    /// Stable projection name.
    fn name(&self) -> &'static str;

    /// Loads the last successfully processed event global sequence.
    async fn load_checkpoint(&self) -> Result<Option<u64>, Self::Error>;

    /// Applies one event to the read model and saves the event checkpoint in
    /// the same backing-store transaction.
    async fn apply_and_checkpoint_transactionally(
        &mut self,
        event: &EventEnvelope<E, Id>,
    ) -> Result<(), Self::Error>;
}

/// Async runner for transaction-aware checkpointed projections.
#[cfg(feature = "async")]
#[derive(Debug)]
pub struct AsyncTransactionalCheckpointedProjectionRunner<P> {
    projection: P,
}

#[cfg(feature = "async")]
impl<P> AsyncTransactionalCheckpointedProjectionRunner<P> {
    /// Creates a new async transactional checkpointed projection runner.
    pub fn new(projection: P) -> Self {
        Self { projection }
    }

    /// Returns the wrapped projection.
    pub fn projection(&self) -> &P {
        &self.projection
    }

    /// Returns the wrapped projection mutably.
    pub fn projection_mut(&mut self) -> &mut P {
        &mut self.projection
    }

    /// Consumes the runner and returns the projection.
    pub fn into_projection(self) -> P {
        self.projection
    }
}

#[cfg(feature = "async")]
impl<P> AsyncTransactionalCheckpointedProjectionRunner<P> {
    /// Catches the projection up to the end of the feed in
    /// [`ProjectionBatchConfig::default`] sized batches, applying each
    /// read-model update with its checkpoint in one projection-owned
    /// transaction.
    ///
    /// The tail is never materialized in one allocation: this repeats
    /// [`Self::run_batch`] until a batch reports
    /// [`ProjectionBatchOutcome::caught_up`], so a backlog of any size costs one
    /// batch of memory at a time.
    pub async fn run<A, S>(
        &mut self,
        store: &S,
    ) -> Result<usize, ProjectionRunnerError<P::Error, S::Error, P::Error>>
    where
        A: Aggregate + Send + Sync,
        S: crate::async_api::AsyncEventStore<A>,
        P: AsyncTransactionalCheckpointedProjection<A::Event, A::Id> + Send + Sync,
    {
        let config = ProjectionBatchConfig::default();
        let mut applied = 0;
        let mut position = None;

        loop {
            let outcome = self.run_batch(store, config).await?;
            applied += outcome.applied;
            if !batch_advanced(outcome, &mut position) {
                return Ok(applied);
            }
        }
    }

    /// Loads at most `config.batch_size()` global events after the projection's
    /// checkpoint and applies each read-model update with its checkpoint transaction.
    pub async fn run_batch<A, S>(
        &mut self,
        store: &S,
        config: ProjectionBatchConfig,
    ) -> Result<ProjectionBatchOutcome, ProjectionRunnerError<P::Error, S::Error, P::Error>>
    where
        A: Aggregate + Send + Sync,
        S: crate::async_api::AsyncEventStore<A>,
        P: AsyncTransactionalCheckpointedProjection<A::Event, A::Id> + Send + Sync,
    {
        let checkpoint = self
            .projection
            .load_checkpoint()
            .await
            .map_err(ProjectionRunnerError::Checkpoint)?;

        let events = store
            .load_global_after_limited(checkpoint, config.batch_size())
            .await
            .map_err(ProjectionRunnerError::Store)?;
        let mut applied = 0;
        let mut last_sequence = None;

        for event in events {
            self.projection
                .apply_and_checkpoint_transactionally(&event)
                .await
                .map_err(ProjectionRunnerError::Projection)?;
            last_sequence = event.sequence;
            applied += 1;
        }

        Ok(projection_batch_outcome(applied, last_sequence, config))
    }
}

#[cfg(feature = "async")]
impl<P> AsyncCheckpointedProjectionRunner<P> {
    /// Creates a new async runner for a checkpointed projection.
    pub fn new(projection: P) -> Self {
        Self { projection }
    }

    /// Returns the wrapped projection.
    pub fn projection(&self) -> &P {
        &self.projection
    }

    /// Returns the wrapped projection mutably.
    pub fn projection_mut(&mut self) -> &mut P {
        &mut self.projection
    }

    /// Consumes the runner and returns the projection.
    pub fn into_projection(self) -> P {
        self.projection
    }
}

#[cfg(feature = "async")]
impl<P> AsyncCheckpointedProjectionRunner<P> {
    /// Catches the projection up to the end of the feed, applying events in
    /// [`ProjectionBatchConfig::default`] sized batches through the
    /// projection-owned checkpoint operation.
    ///
    /// The tail is never materialized in one allocation: this repeats
    /// [`Self::run_batch`] until a batch reports
    /// [`ProjectionBatchOutcome::caught_up`], so a backlog of any size costs one
    /// batch of memory at a time.
    #[allow(clippy::type_complexity)]
    pub async fn run<A, S>(
        &mut self,
        store: &S,
    ) -> Result<usize, ProjectionRunnerError<P::Error, S::Error, P::Error>>
    where
        A: Aggregate + Send + Sync,
        S: crate::async_api::AsyncEventStore<A>,
        P: AsyncCheckpointedProjection<A::Event, A::Id> + Send + Sync,
    {
        let config = ProjectionBatchConfig::default();
        let mut applied = 0;
        let mut position = None;

        loop {
            let outcome = self.run_batch(store, config).await?;
            applied += outcome.applied;
            if !batch_advanced(outcome, &mut position) {
                return Ok(applied);
            }
        }
    }

    /// Loads at most `config.batch_size()` global events after the projection's
    /// checkpoint and applies each event through the projection-owned checkpoint operation.
    #[allow(clippy::type_complexity)]
    pub async fn run_batch<A, S>(
        &mut self,
        store: &S,
        config: ProjectionBatchConfig,
    ) -> Result<ProjectionBatchOutcome, ProjectionRunnerError<P::Error, S::Error, P::Error>>
    where
        A: Aggregate + Send + Sync,
        S: crate::async_api::AsyncEventStore<A>,
        P: AsyncCheckpointedProjection<A::Event, A::Id> + Send + Sync,
    {
        let checkpoint = self
            .projection
            .load_checkpoint()
            .await
            .map_err(ProjectionRunnerError::Checkpoint)?;

        let events = store
            .load_global_after_limited(checkpoint, config.batch_size())
            .await
            .map_err(ProjectionRunnerError::Store)?;
        let mut applied = 0;
        let mut last_sequence = None;

        for event in events {
            self.projection
                .apply_and_checkpoint(&event)
                .await
                .map_err(ProjectionRunnerError::Projection)?;
            last_sequence = event.sequence;
            applied += 1;
        }

        Ok(projection_batch_outcome(applied, last_sequence, config))
    }
}
