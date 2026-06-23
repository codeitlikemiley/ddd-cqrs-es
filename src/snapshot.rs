use crate::aggregate::Aggregate;
use crate::metadata::Metadata;
use std::time::SystemTime;

/// Persisted aggregate snapshot used to speed up replay of long streams.
///
/// Snapshots are optional and must never replace the event log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot<A>
where
    A: Aggregate,
{
    /// Aggregate stream identifier.
    pub aggregate_id: A::Id,
    /// Stable aggregate type name.
    pub aggregate_type: String,
    /// Stream revision represented by the snapshot.
    pub revision: u64,
    /// Aggregate state at the snapshot revision.
    pub state: A,
    /// Snapshot metadata.
    pub metadata: Metadata,
    /// Time the snapshot was recorded.
    pub recorded_at: SystemTime,
}

impl<A> Snapshot<A>
where
    A: Aggregate,
{
    /// Creates a snapshot.
    pub fn new(aggregate_id: A::Id, revision: u64, state: A, metadata: Metadata) -> Self {
        Self {
            aggregate_id,
            aggregate_type: A::aggregate_type().to_owned(),
            revision,
            state,
            metadata,
            recorded_at: SystemTime::now(),
        }
    }
}

/// Snapshot persistence abstraction.
pub trait SnapshotStore<A>
where
    A: Aggregate,
{
    /// Store-specific error type.
    type Error;

    /// Loads the latest snapshot for an aggregate stream.
    fn load_snapshot(&self, aggregate_id: &A::Id) -> Result<Option<Snapshot<A>>, Self::Error>;

    /// Saves a snapshot.
    fn save_snapshot(&self, snapshot: Snapshot<A>) -> Result<(), Self::Error>;
}
