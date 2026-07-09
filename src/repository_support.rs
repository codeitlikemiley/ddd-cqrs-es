use crate::aggregate::{Aggregate, LoadedAggregate};
use crate::event::{EventEnvelope, NewEvent};
use crate::metadata::Metadata;

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
