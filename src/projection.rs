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
/// ## Deployment: one runner per projection scope
///
/// Built-in projection runners do not acquire a distributed lease or fencing
/// token before applying events. Run **at most one active runner** for each
/// `(projection name, aggregate type)` pair (or each raw-feed scope) so two
/// replicas cannot double-apply the same batch undetected. Checkpoint saves are
/// monotonic but not mutually exclusive; duplicate runners can corrupt
/// non-idempotent read models even when checkpoints look healthy.
///
/// Runners call [`Self::flush`] immediately before advancing a durable
/// checkpoint. Projections that buffer events in [`Self::apply`] and write the
/// read model later must persist those buffers in `flush`.
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

    /// Applies a batch of committed events.
    ///
    /// The default implementation calls [`Self::apply`] for each event in order.
    /// Override this when a projection can apply an entire batch more efficiently
    /// than repeated single-event updates.
    fn apply_batch(&mut self, events: &[EventEnvelope<E, Id>]) -> Result<(), Self::Error> {
        for event in events {
            self.apply(event)?;
        }
        Ok(())
    }

    /// Persists any buffered read-model updates before the runner saves a checkpoint.
    ///
    /// The default implementation is a no-op. Override when [`Self::apply`] accumulates
    /// work that must be written before the checkpoint advances.
    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Default maximum event count for bounded projection catch-up.
pub const DEFAULT_PROJECTION_BATCH_SIZE: usize = 500;

const DEFAULT_PROJECTION_BATCH: NonZeroUsize = {
    assert!(DEFAULT_PROJECTION_BATCH_SIZE > 0);
    match NonZeroUsize::new(DEFAULT_PROJECTION_BATCH_SIZE) {
        Some(batch_size) => batch_size,
        None => panic!("DEFAULT_PROJECTION_BATCH_SIZE must be non-zero"),
    }
};

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
            batch_size: DEFAULT_PROJECTION_BATCH,
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

/// Repeats bounded `run_batch` passes until the feed reports caught up.
fn run_projection_catch_up<E>(
    mut run_batch: impl FnMut(ProjectionBatchConfig) -> Result<ProjectionBatchOutcome, E>,
) -> Result<usize, E> {
    let config = ProjectionBatchConfig::default();
    let mut applied = 0;
    let mut position = None;

    loop {
        let outcome = run_batch(config)?;
        applied += outcome.applied;
        if !batch_advanced(outcome, &mut position) {
            return Ok(applied);
        }
    }
}

/// Counts a loaded batch and finds the last global sequence in feed order.
fn projection_batch_stats<E, Id>(events: &[EventEnvelope<E, Id>]) -> (usize, Option<u64>) {
    (
        events.len(),
        events.iter().rev().find_map(|event| event.sequence),
    )
}

fn apply_projection_events<P, E, Id>(
    projection: &mut P,
    events: impl IntoIterator<Item = EventEnvelope<E, Id>>,
) -> (usize, Option<u64>, Option<P::Error>)
where
    P: Projection<E, Id>,
{
    let mut applied = 0;
    let mut last_sequence = None;
    let mut failure = None;

    for event in events {
        if let Err(error) = projection.apply(&event) {
            failure = Some(error);
            break;
        }
        if let Some(sequence) = event.sequence {
            last_sequence = Some(sequence);
        }
        applied += 1;
    }

    (applied, last_sequence, failure)
}

fn persist_projection_checkpoint<P, E, Id, C, StoreError>(
    projection: &mut P,
    checkpoint_store: &C,
    key: &str,
    last_sequence: Option<u64>,
) -> Result<(), ProjectionRunnerError<P::Error, StoreError, C::Error>>
where
    P: Projection<E, Id>,
    C: CheckpointStore,
{
    if let Some(sequence) = last_sequence {
        projection
            .flush()
            .map_err(ProjectionRunnerError::Projection)?;
        checkpoint_store
            .save_checkpoint(key, sequence)
            .map_err(ProjectionRunnerError::Checkpoint)?;
    }
    Ok(())
}

fn finish_projection_batch<ProjectionError, StoreError, CheckpointError>(
    failure: Option<ProjectionRunnerError<ProjectionError, StoreError, CheckpointError>>,
    flushed: Result<(), ProjectionRunnerError<ProjectionError, StoreError, CheckpointError>>,
) -> Result<(), ProjectionRunnerError<ProjectionError, StoreError, CheckpointError>> {
    match failure {
        Some(ProjectionRunnerError::Projection(projection_err)) => match flushed {
            Err(ProjectionRunnerError::Checkpoint(checkpoint_err)) => {
                Err(ProjectionRunnerError::PartialBatchFailure {
                    projection: projection_err,
                    checkpoint: checkpoint_err,
                })
            }
            other => {
                other?;
                Err(ProjectionRunnerError::Projection(projection_err))
            }
        },
        Some(other) => Err(other),
        None => flushed,
    }
}

#[cfg(feature = "async")]
async fn persist_projection_checkpoint_async<P, E, Id, C, StoreError>(
    projection: &mut P,
    checkpoint_store: &C,
    key: &str,
    last_sequence: Option<u64>,
) -> Result<(), ProjectionRunnerError<P::Error, StoreError, C::Error>>
where
    P: Projection<E, Id>,
    C: AsyncCheckpointStore,
{
    if let Some(sequence) = last_sequence {
        projection
            .flush()
            .map_err(ProjectionRunnerError::Projection)?;
        checkpoint_store
            .save_checkpoint(key, sequence)
            .await
            .map_err(ProjectionRunnerError::Checkpoint)?;
    }
    Ok(())
}

/// Applies a loaded batch through [`Projection`], flushes, and persists an external checkpoint.
///
/// Used by persisted runners (sync and async). In-memory runners use
/// [`finish_in_memory_projection_batch`] instead because they keep the checkpoint locally and
/// always flush before returning a mid-batch projection error.
fn run_persisted_projection_batch<P, E, Id, C, StoreError>(
    projection: &mut P,
    checkpoint_store: &C,
    key: &str,
    events: impl IntoIterator<Item = EventEnvelope<E, Id>>,
    config: ProjectionBatchConfig,
) -> Result<ProjectionBatchOutcome, ProjectionRunnerError<P::Error, StoreError, C::Error>>
where
    P: Projection<E, Id>,
    C: CheckpointStore,
{
    let (applied, last_sequence, failure) = apply_projection_events(projection, events);
    let flushed = persist_projection_checkpoint(
        projection,
        checkpoint_store,
        key,
        last_sequence,
    );
    finish_projection_batch(failure.map(ProjectionRunnerError::Projection), flushed)?;
    Ok(projection_batch_outcome(applied, last_sequence, config))
}

#[cfg(feature = "async")]
async fn run_persisted_projection_batch_async<P, E, Id, C, StoreError>(
    projection: &mut P,
    checkpoint_store: &C,
    key: &str,
    events: impl IntoIterator<Item = EventEnvelope<E, Id>>,
    config: ProjectionBatchConfig,
) -> Result<ProjectionBatchOutcome, ProjectionRunnerError<P::Error, StoreError, C::Error>>
where
    P: Projection<E, Id>,
    C: AsyncCheckpointStore,
{
    let (applied, last_sequence, failure) = apply_projection_events(projection, events);
    let flushed = persist_projection_checkpoint_async(
        projection,
        checkpoint_store,
        key,
        last_sequence,
    )
    .await;
    finish_projection_batch(failure.map(ProjectionRunnerError::Projection), flushed)?;
    Ok(projection_batch_outcome(applied, last_sequence, config))
}

/// Flushes an in-memory runner checkpoint after [`apply_projection_events`].
fn finish_in_memory_projection_batch<P, E, Id, StoreError>(
    projection: &mut P,
    checkpoint: &mut Option<u64>,
    applied: usize,
    last_sequence: Option<u64>,
    failure: Option<P::Error>,
    config: ProjectionBatchConfig,
) -> Result<ProjectionBatchOutcome, ProjectionRunnerError<P::Error, StoreError>>
where
    P: Projection<E, Id>,
{
    projection
        .flush()
        .map_err(ProjectionRunnerError::Projection)?;
    if let Some(sequence) = last_sequence {
        *checkpoint = Some(sequence);
    }
    if let Some(error) = failure {
        return Err(ProjectionRunnerError::Projection(error));
    }
    Ok(projection_batch_outcome(applied, last_sequence, config))
}

/// Loads one batch and delegates checkpoint movement to the projection-owned batch hook.
///
/// Checkpointed and transactional sync runners share this shape: the projection owns checkpoint
/// persistence, so partial mid-batch progress and error surfaces differ from
/// [`run_persisted_projection_batch`] (external checkpoint store + optional partial flush).
fn run_owned_checkpoint_projection_batch<A, P, S, ProjectionError>(
    projection: &mut P,
    store: &S,
    config: ProjectionBatchConfig,
    load_checkpoint: impl FnOnce(&P) -> Result<Option<u64>, ProjectionError>,
    apply_batch: impl FnOnce(&mut P, &[EventEnvelope<A::Event, A::Id>]) -> Result<(), ProjectionError>,
) -> Result<ProjectionBatchOutcome, ProjectionRunnerError<ProjectionError, S::Error, ProjectionError>>
where
    A: Aggregate,
    S: EventStore<A>,
{
    let checkpoint = load_checkpoint(projection).map_err(ProjectionRunnerError::Checkpoint)?;
    let events = store
        .load_global_after_limited(checkpoint, config.batch_size())
        .map_err(ProjectionRunnerError::Store)?;
    apply_batch(projection, &events).map_err(ProjectionRunnerError::Projection)?;
    let (applied, last_sequence) = projection_batch_stats(&events);
    Ok(projection_batch_outcome(applied, last_sequence, config))
}

/// Async runner catch-up and owned-checkpoint `run_batch` loops stay inline in their
/// runners: async methods return futures that borrow `&mut self` (and often the current
/// envelope) until `.await` completes, so shared closure helpers would need higher-ranked
/// lifetime bounds that Rust cannot express cleanly with `async_trait` methods today.

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

        run_projection_catch_up(|config| self.run_batch(store, config))
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
        let (applied, last_sequence, failure) =
            apply_projection_events(&mut self.projection, events);

        finish_in_memory_projection_batch(
            &mut self.projection,
            &mut self.checkpoint,
            applied,
            last_sequence,
            failure,
            config,
        )
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
    /// Projection failed mid-batch and persisting partial progress also failed.
    PartialBatchFailure {
        /// The projection error that stopped the batch.
        projection: ProjectionError,
        /// Checkpoint persistence failure while saving partial progress.
        checkpoint: CheckpointError,
    },
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
            ProjectionRunnerError::PartialBatchFailure {
                projection,
                checkpoint,
            } => write!(
                f,
                "projection failed ({projection}) and checkpoint save also failed ({checkpoint})"
            ),
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
            ProjectionRunnerError::PartialBatchFailure { projection, .. } => Some(projection),
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
    ///
    /// Implementations must keep checkpoints monotonic: saving an older sequence
    /// must not rewind a newer stored value. See
    /// [`assert_checkpoint_store_contract`](crate::testing::assert_checkpoint_store_contract).
    fn save_checkpoint(&self, projection_name: &str, sequence: u64) -> Result<(), Self::Error>;

    /// Clears the stored checkpoint so a projection can replay from the beginning.
    ///
    /// Use this when rebuilding a read model; [`Self::save_checkpoint`] alone
    /// cannot move a checkpoint backwards.
    fn reset_checkpoint(&self, projection_name: &str) -> Result<(), Self::Error>;
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

    /// Clears the stored checkpoint so a projection can replay from the beginning.
    async fn reset_checkpoint(&self, projection_name: &str) -> Result<(), Self::Error>;
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
/// [`Self::new`] keys checkpoints on the projection name **and** the feed being
/// replayed (see [`aggregate_scoped_checkpoint_key`]). Each aggregate type's
/// global replay feed and each cross-aggregate raw feed therefore keep an
/// independent position, which is required when one projection spans several
/// aggregate types (see [`EventStore::load_global_after`]).
///
/// Deployments upgrading from 0.3 name-only rows should seed scoped keys from
/// the legacy row instead of replaying from zero — see
/// [`aggregate_scoped_checkpoint_key`] and `docs/production/persisted-views.md`.
///
/// [`Self::with_projection_name_checkpoints`] preserves the 0.3 name-only key
/// for single-type feeds that already store checkpoints under
/// [`Projection::name()`] alone.
#[derive(Debug)]
pub struct PersistedProjectionRunner<P, C> {
    projection: P,
    checkpoint_store: C,
    keying: CheckpointKeying,
}

impl<P, C> PersistedProjectionRunner<P, C> {
    /// Creates a persisted runner with aggregate-scoped checkpoint keys.
    ///
    /// This is the recommended default for all projections, including those
    /// driven by exactly one aggregate type.
    pub fn new(projection: P, checkpoint_store: C) -> Self {
        Self {
            projection,
            checkpoint_store,
            keying: CheckpointKeying::AggregateScoped,
        }
    }

    /// Creates a persisted runner that keys checkpoints on the projection name
    /// **and** the feed being replayed.
    ///
    /// Equivalent to [`Self::new`]; kept for explicit call sites and docs.
    pub fn with_aggregate_scoped_checkpoints(projection: P, checkpoint_store: C) -> Self {
        Self::new(projection, checkpoint_store)
    }

    /// Creates a persisted runner that keys checkpoints on [`Projection::name()`]
    /// alone.
    ///
    /// # Migration from 0.3
    ///
    /// Name-only keys make multi-type projections share one position (whichever
    /// feed advances furthest hides the others). Prefer [`Self::new`]. When you
    /// must keep a legacy row, operate one runner per aggregate type or seed
    /// scoped rows via [`aggregate_scoped_checkpoint_key`] before switching to
    /// [`Self::new`].
    #[deprecated(
        since = "0.4.0",
        note = "aggregate-scoped checkpoints are the default since 0.4; seed scoped rows from legacy name-only checkpoints before switching to PersistedProjectionRunner::new"
    )]
    pub fn with_projection_name_checkpoints(projection: P, checkpoint_store: C) -> Self {
        Self {
            projection,
            checkpoint_store,
            keying: CheckpointKeying::ProjectionName,
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

        run_projection_catch_up(|config| self.run_batch(store, config))
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
        run_persisted_projection_batch(
            &mut self.projection,
            &self.checkpoint_store,
            &key,
            events,
            config,
        )
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
        run_persisted_projection_batch(
            &mut self.projection,
            &self.checkpoint_store,
            &key,
            events,
            config,
        )
    }
}

/// An async projection runner that uses a persistent `AsyncCheckpointStore` to coordinate progress.
///
/// Checkpoint keying matches [`PersistedProjectionRunner`]: [`Self::new`] uses
/// aggregate-scoped keys; [`Self::with_projection_name_checkpoints`] preserves
/// the 0.3 name-only layout.
#[cfg(feature = "async")]
#[derive(Debug)]
pub struct AsyncPersistedProjectionRunner<P, C> {
    projection: P,
    checkpoint_store: C,
    keying: CheckpointKeying,
}

#[cfg(feature = "async")]
impl<P, C> AsyncPersistedProjectionRunner<P, C> {
    /// Creates an async persisted runner with aggregate-scoped checkpoint keys.
    pub fn new(projection: P, checkpoint_store: C) -> Self {
        Self {
            projection,
            checkpoint_store,
            keying: CheckpointKeying::AggregateScoped,
        }
    }

    /// Creates an async persisted runner with aggregate-scoped checkpoint keys.
    ///
    /// Equivalent to [`Self::new`].
    pub fn with_aggregate_scoped_checkpoints(projection: P, checkpoint_store: C) -> Self {
        Self::new(projection, checkpoint_store)
    }

    /// Creates an async persisted runner that keys checkpoints on
    /// [`Projection::name()`] alone.
    ///
    /// See [`PersistedProjectionRunner::with_projection_name_checkpoints`].
    #[deprecated(
        since = "0.4.0",
        note = "aggregate-scoped checkpoints are the default since 0.4; seed scoped rows from legacy name-only checkpoints before switching to AsyncPersistedProjectionRunner::new"
    )]
    pub fn with_projection_name_checkpoints(projection: P, checkpoint_store: C) -> Self {
        Self {
            projection,
            checkpoint_store,
            keying: CheckpointKeying::ProjectionName,
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
        run_persisted_projection_batch_async(
            &mut self.projection,
            &self.checkpoint_store,
            &key,
            events,
            config,
        )
        .await
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
        run_persisted_projection_batch_async(
            &mut self.projection,
            &self.checkpoint_store,
            &key,
            events,
            config,
        )
        .await
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

    /// Applies a batch of events, advancing the checkpoint once for the batch.
    ///
    /// The default implementation calls [`Self::apply_and_checkpoint`] for each
    /// event in order. Override to commit one read-model transaction per batch.
    fn apply_batch_and_checkpoint(
        &mut self,
        events: &[EventEnvelope<E, Id>],
    ) -> Result<(), Self::Error> {
        for event in events {
            self.apply_and_checkpoint(event)?;
        }
        Ok(())
    }
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

        run_projection_catch_up(|config| self.run_batch(store, config))
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

        run_owned_checkpoint_projection_batch(
            &mut self.projection,
            store,
            config,
            |projection| projection.load_checkpoint(),
            |projection, events| projection.apply_batch_and_checkpoint(events),
        )
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

    /// Applies a batch of events in one or more backing-store transactions.
    ///
    /// The default implementation calls
    /// [`Self::apply_and_checkpoint_transactionally`] for each event in order,
    /// preserving today's per-event transaction semantics. Override to commit
    /// one read-model transaction per batch.
    fn apply_batch_and_checkpoint_transactionally(
        &mut self,
        events: &[EventEnvelope<E, Id>],
    ) -> Result<(), Self::Error> {
        for event in events {
            self.apply_and_checkpoint_transactionally(event)?;
        }
        Ok(())
    }
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

        run_projection_catch_up(|config| self.run_batch(store, config))
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

        run_owned_checkpoint_projection_batch(
            &mut self.projection,
            store,
            config,
            |projection| projection.load_checkpoint(),
            |projection, events| projection.apply_batch_and_checkpoint_transactionally(events),
        )
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

        // Per-event async `apply_and_checkpoint*` calls must stay inline: the returned futures
        // borrow `&mut self` and the envelope for the duration of `.await`, which a shared
        // closure helper cannot express without higher-ranked lifetime bounds.
        for event in events {
            self.projection
                .apply_and_checkpoint_transactionally(&event)
                .await
                .map_err(ProjectionRunnerError::Projection)?;
            if let Some(sequence) = event.sequence {
                last_sequence = Some(sequence);
            }
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

        // Per-event async `apply_and_checkpoint*` calls must stay inline: the returned futures
        // borrow `&mut self` and the envelope for the duration of `.await`, which a shared
        // closure helper cannot express without higher-ranked lifetime bounds.
        for event in events {
            self.projection
                .apply_and_checkpoint(&event)
                .await
                .map_err(ProjectionRunnerError::Projection)?;
            if let Some(sequence) = event.sequence {
                last_sequence = Some(sequence);
            }
            applied += 1;
        }

        Ok(projection_batch_outcome(applied, last_sequence, config))
    }
}
