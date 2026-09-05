use crate::aggregate::{Aggregate, LoadedAggregate};
use crate::event::{EventEnvelope, NewEvent};
use crate::metadata::Metadata;

/// How many times a failed idempotency-key release is retried. Shared by the
/// sync and async repositories so the cleanup policy cannot drift between
/// them.
pub(crate) const RELEASE_MAX_ATTEMPTS: usize = 3;

/// Delay between idempotency-key release attempts.
pub(crate) const RELEASE_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(50);

/// How many times a failed idempotency save is retried after a successful append.
pub(crate) const SAVE_MAX_ATTEMPTS: usize = 3;

/// Delay between idempotency save attempts.
pub(crate) const SAVE_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(50);

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

use crate::idempotency::{IdempotencyKey, IdempotencyStore, IdempotentRepositoryError};

/// Saves a completed idempotency result with bounded retries.
pub(crate) fn save_idempotency_result_with_retry<I, V, DomainError, StoreError>(
    idempotency_store: &I,
    idempotency_key: IdempotencyKey,
    value: V,
) -> Result<(), IdempotentRepositoryError<DomainError, StoreError, I::Error>>
where
    I: IdempotencyStore<V>,
    V: Clone,
{
    for attempt in 1..=SAVE_MAX_ATTEMPTS {
        match idempotency_store.save(idempotency_key.clone(), value.clone()) {
            Ok(()) => return Ok(()),
            Err(error) if attempt == SAVE_MAX_ATTEMPTS => {
                return Err(IdempotentRepositoryError::Idempotency(error));
            }
            Err(_) => std::thread::sleep(SAVE_RETRY_DELAY),
        }
    }
    unreachable!("save retry loop must return")
}

/// Releases a reserved idempotency key and surfaces failure as a repository error.
pub(crate) fn release_idempotency_key_with_result<I, V, DomainError, StoreError>(
    idempotency_store: &I,
    idempotency_key: &IdempotencyKey,
) -> Result<(), IdempotentRepositoryError<DomainError, StoreError, I::Error>>
where
    I: IdempotencyStore<V>,
    V: Clone,
{
    for attempt in 1..=RELEASE_MAX_ATTEMPTS {
        match idempotency_store.remove(idempotency_key) {
            Ok(()) => return Ok(()),
            Err(error) => {
                #[cfg(feature = "tracing")]
                if attempt == RELEASE_MAX_ATTEMPTS {
                    tracing::warn!(
                        key = %idempotency_key,
                        "{}",
                        RELEASE_FAILED_MESSAGE
                    );
                } else {
                    tracing::debug!(
                        key = %idempotency_key,
                        attempt,
                        "{}",
                        RELEASE_RETRY_MESSAGE
                    );
                }
                if attempt == RELEASE_MAX_ATTEMPTS {
                    return Err(IdempotentRepositoryError::IdempotencyReleaseFailed {
                        key: idempotency_key.clone(),
                        attempts: RELEASE_MAX_ATTEMPTS,
                    });
                }
                if attempt < RELEASE_MAX_ATTEMPTS {
                    std::thread::sleep(RELEASE_RETRY_DELAY);
                }
                let _ = error;
            }
        }
    }
    Err(IdempotentRepositoryError::IdempotencyReleaseFailed {
        key: idempotency_key.clone(),
        attempts: RELEASE_MAX_ATTEMPTS,
    })
}

#[cfg(feature = "async")]
pub(crate) async fn async_save_idempotency_result_with_retry<I, V, DomainError, StoreError>(
    idempotency_store: &I,
    idempotency_key: IdempotencyKey,
    value: V,
) -> Result<(), IdempotentRepositoryError<DomainError, StoreError, I::Error>>
where
    I: crate::async_api::AsyncIdempotencyStore<V>,
    V: Clone + Send + Sync + 'static,
{
    for attempt in 1..=SAVE_MAX_ATTEMPTS {
        match idempotency_store
            .save(idempotency_key.clone(), value.clone())
            .await
        {
            Ok(()) => return Ok(()),
            Err(error) if attempt == SAVE_MAX_ATTEMPTS => {
                return Err(IdempotentRepositoryError::Idempotency(error));
            }
            Err(_) => tokio::time::sleep(SAVE_RETRY_DELAY).await,
        }
    }
    unreachable!("async save retry loop must return")
}

#[cfg(feature = "async")]
pub(crate) async fn async_release_idempotency_key_with_result<I, V, DomainError, StoreError>(
    idempotency_store: &I,
    idempotency_key: &IdempotencyKey,
) -> Result<(), IdempotentRepositoryError<DomainError, StoreError, I::Error>>
where
    I: crate::async_api::AsyncIdempotencyStore<V>,
    V: Clone + Send + Sync + 'static,
{
    for attempt in 1..=RELEASE_MAX_ATTEMPTS {
        match idempotency_store.remove(idempotency_key).await {
            Ok(()) => return Ok(()),
            Err(_) => {
                #[cfg(feature = "tracing")]
                if attempt == RELEASE_MAX_ATTEMPTS {
                    tracing::warn!(
                        key = %idempotency_key,
                        "{}",
                        RELEASE_FAILED_MESSAGE
                    );
                } else {
                    tracing::debug!(
                        key = %idempotency_key,
                        attempt,
                        "{}",
                        RELEASE_RETRY_MESSAGE
                    );
                }
                if attempt == RELEASE_MAX_ATTEMPTS {
                    return Err(IdempotentRepositoryError::IdempotencyReleaseFailed {
                        key: idempotency_key.clone(),
                        attempts: RELEASE_MAX_ATTEMPTS,
                    });
                }
                tokio::time::sleep(RELEASE_RETRY_DELAY).await;
            }
        }
    }
    Err(IdempotentRepositoryError::IdempotencyReleaseFailed {
        key: idempotency_key.clone(),
        attempts: RELEASE_MAX_ATTEMPTS,
    })
}
