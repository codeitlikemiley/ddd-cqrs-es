use crate::aggregate::{Aggregate, LoadedAggregate};
use crate::event::{EventEnvelope, NewEvent};
use crate::metadata::Metadata;

/// How many times a failed idempotency-key release is retried. Shared by the
/// sync and async repositories so the cleanup policy cannot drift between
/// them.
pub(crate) const RELEASE_MAX_ATTEMPTS: usize = 3;

/// Delay between idempotency-key release attempts.
pub(crate) const RELEASE_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(50);

/// Final-failure warning shared by both release twins.
#[cfg(feature = "tracing")]
pub(crate) const RELEASE_FAILED_MESSAGE: &str =
    "failed to release idempotency key after execution failure; \
     the key stays Pending and will block retries until removed";

/// Per-attempt debug message shared by both release twins.
#[cfg(feature = "tracing")]
pub(crate) const RELEASE_RETRY_MESSAGE: &str = "retrying failed idempotency-key release";

pub(crate) fn new_events_with_metadata<A>(
    events: Vec<A::Event>,
    metadata: &Metadata,
) -> Vec<NewEvent<A::Event>>
where
    A: Aggregate,
{
    events
        .into_iter()
        .map(|event| NewEvent::new(event, metadata.clone()))
        .collect()
}

pub(crate) fn handle_command_as_new_events<A>(
    state: &A,
    command: A::Command,
    metadata: &Metadata,
) -> Result<Vec<NewEvent<A::Event>>, A::Error>
where
    A: Aggregate,
{
    state
        .handle(command)
        .map(|events| new_events_with_metadata::<A>(events, metadata))
}

pub(crate) fn apply_committed_events<A>(
    mut loaded: LoadedAggregate<A>,
    committed: &[EventEnvelope<A::Event, A::Id>],
) -> LoadedAggregate<A>
where
    A: Aggregate,
{
    for envelope in committed {
        loaded.state.apply(&envelope.payload);
        loaded.revision = envelope.revision;
    }

    loaded
}
