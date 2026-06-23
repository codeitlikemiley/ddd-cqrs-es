use crate::aggregate::Aggregate;
use crate::event::EventEnvelope;
use crate::event_store::EventStore;

/// A read-model updater.
///
/// Projections consume committed event envelopes and update query-optimized
/// state. Implementations should be idempotent because projection runners may
/// retry after failures.
pub trait Projection<E, Id> {
    /// Projection error.
    type Error;

    /// Stable projection name used for checkpoint storage.
    fn name(&self) -> &'static str;

    /// Applies one committed event to the projection.
    fn apply(&mut self, event: &EventEnvelope<E, Id>) -> Result<(), Self::Error>;
}

/// In-memory projection runner with a sequence checkpoint.
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
    /// Loads global events after the current checkpoint and applies them.
    pub fn run<A, S>(
        &mut self,
        store: &S,
    ) -> Result<usize, ProjectionRunnerError<P::Error, S::Error>>
    where
        A: Aggregate,
        S: EventStore<A>,
        P: Projection<A::Event, A::Id>,
    {
        let events = store
            .load_global_after(self.checkpoint)
            .map_err(ProjectionRunnerError::Store)?;
        let mut applied = 0;

        for event in events {
            self.projection
                .apply(&event)
                .map_err(ProjectionRunnerError::Projection)?;
            self.checkpoint = event.sequence;
            applied += 1;
        }

        Ok(applied)
    }
}

/// Error returned by a projection runner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectionRunnerError<ProjectionError, StoreError> {
    /// Projection logic failed.
    Projection(ProjectionError),
    /// Event store read failed.
    Store(StoreError),
}
