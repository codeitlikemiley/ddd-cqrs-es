use crate::aggregate::Aggregate;
#[cfg(feature = "async")]
use crate::async_api::{AsyncEventStore, AsyncIdempotencyStore};
use crate::error::{ConcurrencyError, EventStoreFailure, RepositoryError};
use crate::event::{ExpectedRevision, NewEvent};
use crate::event_store::{AtomicIdempotentEventStore, EventStore};
use crate::idempotency::{IdempotencyKey, IdempotencyState, IdempotencyStore};
use crate::metadata::Metadata;
#[cfg(feature = "async")]
use crate::projection::AsyncCheckpointStore;
use crate::projection::CheckpointStore;
use crate::snapshot::{Snapshot, SnapshotStore};
use std::fmt::{Debug, Display};
use std::num::NonZeroUsize;
use std::sync::{Arc, Barrier};
use std::thread;

/// Fluent aggregate test fixture.
///
/// The fixture exercises aggregate decision logic without requiring a
/// repository or event store.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::AggregateFixture;
/// # use ddd_cqrs_es::Aggregate;
/// # #[derive(Clone, Debug, PartialEq)]
/// # enum CounterEvent { Incremented(u32) }
/// # impl ddd_cqrs_es::DomainEvent for CounterEvent {
/// #     fn event_type(&self) -> &'static str { "incremented" }
/// # }
/// # #[derive(Clone, Debug, Default, PartialEq)]
/// # struct Counter { value: u32 }
/// # impl Aggregate for Counter {
/// #     type Id = String;
/// #     type Command = u32;
/// #     type Event = CounterEvent;
/// #     type Error = &'static str;
/// #     fn aggregate_type() -> &'static str { "counter" }
/// #     fn new() -> Self { Self::default() }
/// #     fn apply(&mut self, event: &Self::Event) {
/// #         match event { CounterEvent::Incremented(by) => self.value += by }
/// #     }
/// #     fn handle(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
/// #         if command == 0 { return Err("must be > 0"); }
/// #         Ok(vec![CounterEvent::Incremented(command)])
/// #     }
/// # }
///
/// let fixture = AggregateFixture::<Counter>::new();
///
/// fixture
///     .given(vec![CounterEvent::Incremented(5)])
///     .when(3)
///     .then_expect_events(vec![CounterEvent::Incremented(3)])
///     .then_expect_state(|state| {
///         assert_eq!(state.value, 8);
///     });
/// ```
#[derive(Clone, Debug)]
pub struct AggregateFixture<A>
where
    A: Aggregate,
{
    given: Vec<A::Event>,
}

/// Options for the reusable event-store contract test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventStoreContractOptions {
    expected_first_global_sequence: Option<u64>,
}

impl EventStoreContractOptions {
    /// Expects the first appended event to have the provided global sequence.
    pub fn with_expected_first_global_sequence(sequence: u64) -> Self {
        Self {
            expected_first_global_sequence: Some(sequence),
        }
    }

    /// Skips exact global sequence-number assertions.
    pub fn without_exact_global_sequence_assertions() -> Self {
        Self {
            expected_first_global_sequence: None,
        }
    }
}

impl Default for EventStoreContractOptions {
    fn default() -> Self {
        Self::with_expected_first_global_sequence(1)
    }
}

/// Runs the common event-store contract against a store implementation.
///
/// Adapter crates can call this from their own integration tests to verify
/// stream loading, optimistic concurrency, metadata preservation, revision
/// assignment, multi-event batch appends, empty-batch no-ops, stale `Exact`
/// failures, and global sequencing.
pub fn assert_event_store_contract<A, S>(
    store: S,
    aggregate_id: A::Id,
    first_event: A::Event,
    second_event: A::Event,
    third_event: A::Event,
    options: EventStoreContractOptions,
) where
    A: Aggregate,
    A::Event: PartialEq + Debug,
    S: EventStore<A>,
    S::Error: EventStoreFailure + Debug,
{
    assert!(store.load(&aggregate_id).unwrap().is_empty());

    let first_metadata = Metadata::new().with_correlation_id("contract-1");
    let first = store
        .append(
            &aggregate_id,
            ExpectedRevision::NoStream,
            vec![NewEvent::new(first_event.clone(), first_metadata.clone())],
        )
        .unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].revision, 1);
    if let Some(expected) = options.expected_first_global_sequence {
        assert_eq!(first[0].sequence, Some(expected));
    }
    assert_eq!(first[0].metadata, first_metadata);

    let duplicate = store.append(
        &aggregate_id,
        ExpectedRevision::NoStream,
        vec![NewEvent::new(second_event.clone(), Metadata::default())],
    );
    let Err(duplicate) = duplicate else {
        panic!("expected NoStream append to fail after stream creation");
    };
    assert!(matches!(
        duplicate.into_repository_error::<()>(),
        RepositoryError::Concurrency(ConcurrencyError::StreamAlreadyExists)
    ));

    let second = store
        .append(
            &aggregate_id,
            ExpectedRevision::Exact(1),
            vec![NewEvent::new(second_event.clone(), Metadata::default())],
        )
        .unwrap();
    assert_eq!(second[0].revision, 2);
    if let Some(expected) = options.expected_first_global_sequence {
        assert_eq!(second[0].sequence, Some(expected + 1));
    }

    let empty = store
        .append(&aggregate_id, ExpectedRevision::Exact(2), Vec::new())
        .unwrap();
    assert!(empty.is_empty());

    let batch_metadata = Metadata::new().with_correlation_id("contract-batch");
    let batch = store
        .append(
            &aggregate_id,
            ExpectedRevision::Exact(2),
            vec![
                NewEvent::new(second_event.clone(), batch_metadata.clone()),
                NewEvent::new(third_event.clone(), Metadata::default()),
            ],
        )
        .unwrap();
    assert_eq!(batch.len(), 2);
    assert_eq!(batch[0].revision, 3);
    assert_eq!(batch[1].revision, 4);
    assert_eq!(batch[0].metadata, batch_metadata);
    if let Some(expected) = options.expected_first_global_sequence {
        assert_eq!(batch[0].sequence, Some(expected + 2));
        assert_eq!(batch[1].sequence, Some(expected + 3));
    }

    let stale = store.append(
        &aggregate_id,
        ExpectedRevision::Exact(99),
        vec![NewEvent::new(third_event.clone(), Metadata::default())],
    );
    let Err(stale) = stale else {
        panic!("expected stale Exact append to fail");
    };
    assert!(matches!(
        stale.into_repository_error::<()>(),
        RepositoryError::Concurrency(ConcurrencyError::WrongExpectedRevision {
            expected: ExpectedRevision::Exact(99),
            actual: 4,
        })
    ));

    let stream = store.load(&aggregate_id).unwrap();
    assert_eq!(stream.len(), 4);
    assert_eq!(stream[0].payload, first_event);
    assert_eq!(stream[1].payload, second_event);
    assert_eq!(stream[2].payload, second_event);
    assert_eq!(stream[3].payload, third_event);

    if let Some(first_sequence) = first[0].sequence {
        let global = store.load_global_after(Some(first_sequence)).unwrap();
        assert_eq!(global.len(), 3);
        assert_eq!(global[0].revision, 2);
        assert_eq!(global[2].revision, 4);

        // `load_global_after_limited` is the primitive projection runners page
        // with, so the limit must be honoured by the backend read and resuming
        // from the last returned sequence must yield the next event exactly
        // once.
        let one = NonZeroUsize::new(1).unwrap();
        let page = store
            .load_global_after_limited(Some(first_sequence.saturating_sub(1)), one)
            .unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].revision, 1);

        let mut cursor = page[0].sequence;
        for expected_revision in 2..=4 {
            let next = store.load_global_after_limited(cursor, one).unwrap();
            assert_eq!(next.len(), 1);
            assert_eq!(next[0].revision, expected_revision);
            cursor = next[0].sequence;
        }
        assert!(store
            .load_global_after_limited(cursor, one)
            .unwrap()
            .is_empty());
    }

    let tail = store.load_after_revision(&aggregate_id, 1).unwrap();
    assert_eq!(tail.len(), 3);
    assert_eq!(tail[0].revision, 2);
    assert_eq!(tail[2].revision, 4);
    assert_eq!(tail[0].payload, second_event);
    assert!(store
        .load_after_revision(&aggregate_id, 4)
        .unwrap()
        .is_empty());
}

/// Runs a focused global replay contract against a store implementation.
pub fn assert_event_store_global_replay_contract<A, S>(
    store: S,
    first_id: A::Id,
    second_id: A::Id,
    first_event: A::Event,
    second_event: A::Event,
) where
    A: Aggregate,
    A::Event: PartialEq + Debug,
    S: EventStore<A>,
    S::Error: Debug,
{
    store
        .append(
            &first_id,
            ExpectedRevision::NoStream,
            vec![NewEvent::new(first_event.clone(), Metadata::default())],
        )
        .unwrap();
    let first_global = store.load_global_after(None).unwrap();
    let first_sequence = first_global[0].sequence;

    store
        .append(
            &second_id,
            ExpectedRevision::NoStream,
            vec![NewEvent::new(second_event.clone(), Metadata::default())],
        )
        .unwrap();

    let all_global = store.load_global_after(None).unwrap();
    assert_eq!(all_global.len(), 2);
    assert_eq!(all_global[0].payload, first_event);
    assert_eq!(all_global[1].payload, second_event);

    if let Some(sequence) = first_sequence {
        let after_first = store.load_global_after(Some(sequence)).unwrap();
        assert_eq!(after_first.len(), 1);
        assert_eq!(after_first[0].payload, second_event);
    }
}

fn is_retryable_any_append_error<StoreError: Display>(
    error: &RepositoryError<(), StoreError>,
) -> bool {
    match error {
        RepositoryError::Concurrency(_) => true,
        RepositoryError::Store(store_error) => {
            let message = store_error.to_string().to_ascii_lowercase();
            if message.contains("locked") {
                return true;
            }
            (message.contains("unique")
                || message.contains("23505")
                || message.contains("duplicate"))
                && (message.contains("revision") || message.contains("aggregate"))
        }
        _ => false,
    }
}

/// Verifies that concurrent `ExpectedRevision::Any` writers on one stream receive
/// distinct revisions instead of colliding on the same optimistic target.
///
/// Review ledger **#5**: `ExpectedRevision::Any` must not spuriously fail when
/// writers race; adapters should surface retryable concurrency (or succeed with
/// distinct revisions), never opaque backend lock errors. SQLite, Postgres,
/// MySQL, and Redis contract tests call this harness.
pub fn assert_event_store_any_writers_contract<A, S, F>(
    make_store: F,
    aggregate_id: A::Id,
    seed_event: A::Event,
    append_event: A::Event,
) where
    A: Aggregate,
    A::Event: PartialEq + Debug + Clone,
    A::Id: Clone + Send + Sync,
    S: EventStore<A> + Send + Sync + 'static,
    S::Error: EventStoreFailure + Debug + Display + Send + 'static,
    F: Fn() -> S + Send + Sync + 'static,
{
    let seed_store = make_store();
    seed_store
        .append(
            &aggregate_id,
            ExpectedRevision::NoStream,
            vec![NewEvent::new(seed_event, Metadata::default())],
        )
        .unwrap();

    let factory = Arc::new(make_store);
    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::with_capacity(2);
    for _ in 0..2 {
        let factory = Arc::clone(&factory);
        let barrier = Arc::clone(&barrier);
        let aggregate_id = aggregate_id.clone();
        let append_event = append_event.clone();
        handles.push(thread::spawn(move || {
            barrier.wait();
            for attempt in 0..8 {
                match factory().append(
                    &aggregate_id,
                    ExpectedRevision::Any,
                    vec![NewEvent::new(append_event.clone(), Metadata::default())],
                ) {
                    Ok(committed) => return committed,
                    Err(error) => {
                        let repo = error.into_repository_error::<()>();
                        if is_retryable_any_append_error(&repo) && attempt + 1 < 8 {
                            continue;
                        }
                        panic!("append with ExpectedRevision::Any failed: {repo:?}");
                    }
                }
            }
            panic!("append with ExpectedRevision::Any exhausted retries");
        }));
    }

    let mut revisions = Vec::new();
    for handle in handles {
        let committed = handle.join().unwrap();
        assert_eq!(committed.len(), 1);
        revisions.push(committed[0].revision);
    }
    revisions.sort_unstable();
    assert_eq!(revisions, vec![2, 3]);

    let stream = factory().load(&aggregate_id).unwrap();
    assert_eq!(stream.len(), 3);
}

/// Races `racers` independent store handles appending to the same revision.
///
/// Exactly one append succeeds; every loser must surface
/// [`RepositoryError::Concurrency`] with the post-win `actual` revision. When
/// the store assigns global sequences, the feed must remain gap-free.
pub fn assert_event_store_append_race_contract<A, S, F>(
    make_store: F,
    aggregate_id: A::Id,
    seed_event: A::Event,
    race_event: A::Event,
    racers: usize,
) where
    A: Aggregate,
    A::Event: PartialEq + Debug + Clone,
    A::Id: Clone + Send + Sync + Debug,
    S: EventStore<A> + Send + Sync + 'static,
    S::Error: EventStoreFailure + Debug + Display + Send + 'static,
    F: Fn() -> S + Send + Sync + 'static,
{
    assert!(racers >= 2, "race contract requires at least two racers");

    let seed_store = make_store();
    seed_store
        .append(
            &aggregate_id,
            ExpectedRevision::NoStream,
            vec![NewEvent::new(seed_event, Metadata::default())],
        )
        .unwrap();

    let factory = Arc::new(make_store);
    let barrier = Arc::new(Barrier::new(racers));
    let mut handles = Vec::with_capacity(racers);
    for _ in 0..racers {
        let factory = Arc::clone(&factory);
        let barrier = Arc::clone(&barrier);
        let aggregate_id = aggregate_id.clone();
        let race_event = race_event.clone();
        handles.push(thread::spawn(move || {
            barrier.wait();
            for attempt in 0..8 {
                match factory().append(
                    &aggregate_id,
                    ExpectedRevision::Exact(1),
                    vec![NewEvent::new(race_event.clone(), Metadata::default())],
                ) {
                    Ok(committed) => return Ok(committed),
                    Err(error) => match error.into_repository_error::<()>() {
                        RepositoryError::Store(store_error)
                            if store_error
                                .to_string()
                                .to_ascii_lowercase()
                                .contains("locked")
                                && attempt + 1 < 8 =>
                        {
                            continue;
                        }
                        other => return Err(other),
                    },
                }
            }
            panic!("race append exhausted retries");
        }));
    }

    let mut winners = 0usize;
    let mut losers = 0usize;
    for handle in handles {
        match handle.join().unwrap() {
            Ok(committed) => {
                winners += 1;
                assert_eq!(committed.len(), 1);
                assert_eq!(committed[0].revision, 2);
            }
            Err(other) => {
                losers += 1;
                match other {
                    RepositoryError::Concurrency(ConcurrencyError::WrongExpectedRevision {
                        expected: ExpectedRevision::Exact(1),
                        actual,
                    }) => {
                        assert!(
                            actual >= 1,
                            "race loser should observe the stream at or past the seeded revision"
                        );
                    }
                    RepositoryError::Concurrency(_) => {}
                    other => panic!("expected concurrency error for race loser, got {other:?}"),
                }
            }
        }
    }

    assert_eq!(winners, 1);
    assert_eq!(losers, racers - 1);

    let stream = factory().load(&aggregate_id).unwrap();
    assert_eq!(stream.len(), 2);

    let global = factory().load_global_after(None).unwrap();
    let aggregate_id_label = format!("{aggregate_id:?}");
    let sequences: Vec<u64> = global
        .iter()
        .filter(|event| format!("{:?}", event.aggregate_id) == aggregate_id_label)
        .filter_map(|event| event.sequence)
        .collect();
    if sequences.len() >= 2 {
        for window in sequences.windows(2) {
            assert_eq!(window[1], window[0] + 1);
        }
    }
}

/// Runs the atomic idempotent append contract against a store implementation.
pub fn assert_atomic_idempotent_store_contract<A, S>(
    store: S,
    aggregate_id: A::Id,
    idempotency_key: IdempotencyKey,
    event: A::Event,
) where
    A: Aggregate,
    A::Event: PartialEq + Debug + Clone,
    A::Id: Debug,
    S: AtomicIdempotentEventStore<A>,
    S::Error: EventStoreFailure + Debug,
{
    let metadata = Metadata::new().with_correlation_id("atomic-idempotent-contract");
    let events = vec![NewEvent::new(event.clone(), metadata.clone())];

    let first = store
        .append_idempotent(
            idempotency_key.clone(),
            &aggregate_id,
            ExpectedRevision::NoStream,
            events.clone(),
        )
        .unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].payload, event);
    assert_eq!(first[0].metadata, metadata);

    let retry = store
        .append_idempotent(
            idempotency_key,
            &aggregate_id,
            ExpectedRevision::NoStream,
            events,
        )
        .unwrap();
    assert_eq!(retry, first);
    let loaded = store.load(&aggregate_id).unwrap();
    assert_eq!(loaded.len(), first.len());
    assert_eq!(loaded[0].payload, first[0].payload);
    assert_eq!(loaded[0].revision, first[0].revision);
}

/// Runs the common async event-store contract against a store implementation.
///
/// Adapter crates can call this from async integration tests to verify stream
/// loading, optimistic concurrency, metadata preservation, revision assignment,
/// and global sequencing for [`AsyncEventStore`] implementations.
#[cfg(feature = "async")]
pub async fn assert_async_event_store_contract<A, S>(
    store: S,
    aggregate_id: A::Id,
    first_event: A::Event,
    second_event: A::Event,
    third_event: A::Event,
    options: EventStoreContractOptions,
) where
    A: Aggregate + Send + Sync,
    A::Event: PartialEq + Debug,
    S: AsyncEventStore<A>,
    S::Error: EventStoreFailure + Debug,
{
    assert!(store.load(&aggregate_id).await.unwrap().is_empty());

    let first_metadata = Metadata::new().with_correlation_id("async-contract-1");
    let first = store
        .append(
            &aggregate_id,
            ExpectedRevision::NoStream,
            vec![NewEvent::new(first_event.clone(), first_metadata.clone())],
        )
        .await
        .unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].revision, 1);
    if let Some(expected) = options.expected_first_global_sequence {
        assert_eq!(first[0].sequence, Some(expected));
    }
    assert_eq!(first[0].metadata, first_metadata);

    let duplicate = store
        .append(
            &aggregate_id,
            ExpectedRevision::NoStream,
            vec![NewEvent::new(second_event.clone(), Metadata::default())],
        )
        .await;
    let Err(duplicate) = duplicate else {
        panic!("expected NoStream append to fail after stream creation");
    };
    assert!(matches!(
        duplicate.into_repository_error::<()>(),
        RepositoryError::Concurrency(ConcurrencyError::StreamAlreadyExists)
    ));

    let second = store
        .append(
            &aggregate_id,
            ExpectedRevision::Exact(1),
            vec![NewEvent::new(second_event.clone(), Metadata::default())],
        )
        .await
        .unwrap();
    assert_eq!(second[0].revision, 2);
    if let Some(expected) = options.expected_first_global_sequence {
        assert_eq!(second[0].sequence, Some(expected + 1));
    }

    let empty = store
        .append(&aggregate_id, ExpectedRevision::Exact(2), Vec::new())
        .await
        .unwrap();
    assert!(empty.is_empty());

    let batch_metadata = Metadata::new().with_correlation_id("async-contract-batch");
    let batch = store
        .append(
            &aggregate_id,
            ExpectedRevision::Exact(2),
            vec![
                NewEvent::new(second_event.clone(), batch_metadata.clone()),
                NewEvent::new(third_event.clone(), Metadata::default()),
            ],
        )
        .await
        .unwrap();
    assert_eq!(batch.len(), 2);
    assert_eq!(batch[0].revision, 3);
    assert_eq!(batch[1].revision, 4);
    assert_eq!(batch[0].metadata, batch_metadata);
    if let Some(expected) = options.expected_first_global_sequence {
        assert_eq!(batch[0].sequence, Some(expected + 2));
        assert_eq!(batch[1].sequence, Some(expected + 3));
    }

    let stale = store
        .append(
            &aggregate_id,
            ExpectedRevision::Exact(99),
            vec![NewEvent::new(third_event.clone(), Metadata::default())],
        )
        .await;
    let Err(stale) = stale else {
        panic!("expected stale Exact append to fail");
    };
    assert!(matches!(
        stale.into_repository_error::<()>(),
        RepositoryError::Concurrency(ConcurrencyError::WrongExpectedRevision {
            expected: ExpectedRevision::Exact(99),
            actual: 4,
        })
    ));

    let stream = store.load(&aggregate_id).await.unwrap();
    assert_eq!(stream.len(), 4);
    assert_eq!(stream[0].payload, first_event);
    assert_eq!(stream[1].payload, second_event);
    assert_eq!(stream[2].payload, second_event);
    assert_eq!(stream[3].payload, third_event);

    if let Some(first_sequence) = first[0].sequence {
        let global = store.load_global_after(Some(first_sequence)).await.unwrap();
        assert_eq!(global.len(), 3);
        assert_eq!(global[0].revision, 2);
        assert_eq!(global[2].revision, 4);

        // `load_global_after_limited` is the primitive projection runners page
        // with, so the limit must be honoured by the backend read and resuming
        // from the last returned sequence must yield the next event exactly
        // once.
        let one = NonZeroUsize::new(1).unwrap();
        let page = store
            .load_global_after_limited(Some(first_sequence.saturating_sub(1)), one)
            .await
            .unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].revision, 1);

        let mut cursor = page[0].sequence;
        for expected_revision in 2..=4 {
            let next = store.load_global_after_limited(cursor, one).await.unwrap();
            assert_eq!(next.len(), 1);
            assert_eq!(next[0].revision, expected_revision);
            cursor = next[0].sequence;
        }
        assert!(store
            .load_global_after_limited(cursor, one)
            .await
            .unwrap()
            .is_empty());
    }

    let tail = store.load_after_revision(&aggregate_id, 1).await.unwrap();
    assert_eq!(tail.len(), 3);
    assert_eq!(tail[0].revision, 2);
    assert_eq!(tail[2].revision, 4);
    assert_eq!(tail[0].payload, second_event);
    assert!(store
        .load_after_revision(&aggregate_id, 4)
        .await
        .unwrap()
        .is_empty());
}

/// Runs a focused checkpoint-store contract.
pub fn assert_checkpoint_store_contract<C>(store: C, projection_name: &str)
where
    C: CheckpointStore,
    C::Error: Debug,
{
    assert_eq!(store.load_checkpoint(projection_name).unwrap(), None);
    store.save_checkpoint(projection_name, 42).unwrap();
    assert_eq!(store.load_checkpoint(projection_name).unwrap(), Some(42));
    store.save_checkpoint(projection_name, 100).unwrap();
    assert_eq!(store.load_checkpoint(projection_name).unwrap(), Some(100));
    store.save_checkpoint(projection_name, 90).unwrap();
    assert_eq!(store.load_checkpoint(projection_name).unwrap(), Some(100));

    store.reset_checkpoint(projection_name).unwrap();
    assert_eq!(store.load_checkpoint(projection_name).unwrap(), None);
    store.save_checkpoint(projection_name, 7).unwrap();
    assert_eq!(store.load_checkpoint(projection_name).unwrap(), Some(7));
}

/// Runs a focused async checkpoint-store contract.
#[cfg(feature = "async")]
pub async fn assert_async_checkpoint_store_contract<C>(store: C, projection_name: &str)
where
    C: AsyncCheckpointStore,
    C::Error: Debug,
{
    assert_eq!(store.load_checkpoint(projection_name).await.unwrap(), None);
    store.save_checkpoint(projection_name, 42).await.unwrap();
    assert_eq!(
        store.load_checkpoint(projection_name).await.unwrap(),
        Some(42)
    );
    store.save_checkpoint(projection_name, 100).await.unwrap();
    assert_eq!(
        store.load_checkpoint(projection_name).await.unwrap(),
        Some(100)
    );
    store.save_checkpoint(projection_name, 90).await.unwrap();
    assert_eq!(
        store.load_checkpoint(projection_name).await.unwrap(),
        Some(100)
    );

    store.reset_checkpoint(projection_name).await.unwrap();
    assert_eq!(store.load_checkpoint(projection_name).await.unwrap(), None);
}

/// Runs a focused idempotency-store contract.
pub fn assert_idempotency_store_contract<S, V>(store: S, key: IdempotencyKey, value: V)
where
    S: IdempotencyStore<V>,
    S::Error: Debug,
    V: Clone + PartialEq + Debug,
{
    assert_eq!(store.load(&key).unwrap(), None);
    assert!(store.reserve(key.clone()).unwrap());
    assert!(matches!(
        store.load(&key).unwrap(),
        Some(IdempotencyState::Pending(_))
    ));
    assert!(!store.reserve(key.clone()).unwrap());
    store.save(key.clone(), value.clone()).unwrap();
    assert_eq!(
        store.load(&key).unwrap(),
        Some(IdempotencyState::Complete(value))
    );
    assert!(!store.reserve(key.clone()).unwrap());
    store.remove(&key).unwrap();
    assert_eq!(store.load(&key).unwrap(), None);
}

/// Runs a focused async idempotency-store contract.
#[cfg(feature = "async")]
pub async fn assert_async_idempotency_store_contract<S, V>(store: S, key: IdempotencyKey, value: V)
where
    S: AsyncIdempotencyStore<V>,
    S::Error: Debug,
    V: Clone + PartialEq + Debug + Send + Sync + 'static,
{
    assert_eq!(store.load(&key).await.unwrap(), None);
    assert!(store.reserve(key.clone()).await.unwrap());
    assert!(matches!(
        store.load(&key).await.unwrap(),
        Some(IdempotencyState::Pending(_))
    ));
    assert!(!store.reserve(key.clone()).await.unwrap());
    store.save(key.clone(), value.clone()).await.unwrap();
    assert_eq!(
        store.load(&key).await.unwrap(),
        Some(IdempotencyState::Complete(value))
    );
    assert!(!store.reserve(key.clone()).await.unwrap());
    store.remove(&key).await.unwrap();
    assert_eq!(store.load(&key).await.unwrap(), None);
}

/// Runs a focused snapshot-store contract.
pub fn assert_snapshot_store_contract<A, S>(store: S, aggregate_id: A::Id, older: A, newer: A)
where
    A: Aggregate + Clone + PartialEq + Debug,
    A::Id: Debug,
    S: SnapshotStore<A>,
    S::Error: Debug,
{
    assert_eq!(store.load_snapshot(&aggregate_id).unwrap(), None);

    store
        .save_snapshot(Snapshot::new(
            aggregate_id.clone(),
            1,
            older.clone(),
            Metadata::default(),
        ))
        .unwrap();
    assert_eq!(
        store
            .load_snapshot(&aggregate_id)
            .unwrap()
            .map(|snapshot| snapshot.state),
        Some(older.clone())
    );

    store
        .save_snapshot(Snapshot::new(
            aggregate_id.clone(),
            2,
            newer.clone(),
            Metadata::default(),
        ))
        .unwrap();
    let loaded = store.load_snapshot(&aggregate_id).unwrap().unwrap();
    assert_eq!(loaded.revision, 2);
    assert_eq!(loaded.state, newer);

    let stale = store.save_snapshot(Snapshot::new(
        aggregate_id.clone(),
        1,
        older,
        Metadata::default(),
    ));
    assert!(stale.is_err());
}

impl<A> AggregateFixture<A>
where
    A: Aggregate,
{
    /// Creates an empty fixture.
    pub fn new() -> Self {
        Self { given: Vec::new() }
    }

    /// Starts from an empty event history.
    pub fn given_no_events(mut self) -> Self {
        self.given.clear();
        self
    }

    /// Starts from a given event history.
    pub fn given(mut self, events: Vec<A::Event>) -> Self {
        self.given = events;
        self
    }

    /// Handles a command against replayed state.
    pub fn when(self, command: A::Command) -> AggregateFixtureResult<A> {
        let loaded = A::replay_raw_events_from_zero(&self.given);
        let result = loaded.state.handle(command);

        AggregateFixtureResult {
            state: loaded.state,
            revision: loaded.revision,
            result,
        }
    }
}

impl<A> Default for AggregateFixture<A>
where
    A: Aggregate,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Result of executing a command in an aggregate fixture.
#[derive(Clone, Debug)]
pub struct AggregateFixtureResult<A>
where
    A: Aggregate,
{
    state: A,
    revision: u64,
    result: Result<Vec<A::Event>, A::Error>,
}

impl<A> AggregateFixtureResult<A>
where
    A: Aggregate,
{
    /// Asserts that command handling produced exactly the expected events.
    pub fn then_expect_events(self, expected: Vec<A::Event>) -> Self
    where
        A::Event: PartialEq + Debug,
        A::Error: Debug,
    {
        assert_eq!(self.result.as_ref().unwrap(), &expected);
        self
    }

    /// Asserts that command handling produced no events.
    pub fn then_expect_no_events(self) -> Self
    where
        A::Error: Debug,
    {
        assert!(self.result.as_ref().unwrap().is_empty());
        self
    }

    /// Asserts that command handling returned the expected domain error.
    pub fn then_expect_error(self, expected: A::Error) -> Self
    where
        A::Error: PartialEq + Debug,
    {
        match &self.result {
            Ok(_) => panic!("expected aggregate error, got events"),
            Err(error) => assert_eq!(error, &expected),
        }
        self
    }

    /// Asserts against aggregate state after successful command events apply.
    pub fn then_expect_state(self, assertion: impl FnOnce(&A)) -> Self
    where
        A: Clone,
        A::Error: Debug,
    {
        let events = self.result.as_ref().unwrap();
        let mut state = self.state.clone();
        for event in events {
            state.apply(event);
        }

        assertion(&state);
        self
    }

    /// Asserts the replayed revision before the command.
    pub fn then_expect_revision(self, expected: u64) -> Self {
        assert_eq!(self.revision, expected);
        self
    }
}
