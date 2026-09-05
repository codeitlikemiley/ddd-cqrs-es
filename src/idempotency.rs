use crate::error::{ConcurrencyError, EventStoreFailure, RepositoryError};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Default time to wait for an idempotency key that is already pending.
pub const DEFAULT_IDEMPOTENCY_PENDING_TIMEOUT: Duration = Duration::from_secs(30);

/// Default polling interval while waiting for a pending idempotency key to complete.
pub const DEFAULT_IDEMPOTENCY_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Default lease duration for a pending idempotency reservation.
pub const DEFAULT_IDEMPOTENCY_LEASE: Duration = Duration::from_secs(60);

/// Suggested retention for completed idempotency rows when operators run periodic purge.
///
/// Completed rows are not removed automatically; call
/// [`IdempotencyStore::purge_completed_older_than`] on a schedule in production.
pub const DEFAULT_IDEMPOTENCY_COMPLETED_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Lease metadata attached to a pending idempotency key.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdempotencyLease {
    /// Owner token for the in-flight reservation.
    pub owner: String,
    /// Wall-clock expiry in milliseconds since the Unix epoch.
    pub expires_at_ms: u64,
}

impl IdempotencyLease {
    /// Returns `true` when the lease has expired at `now_ms`.
    pub fn is_expired(&self, now_ms: u64) -> bool {
        now_ms >= self.expires_at_ms
    }
}

/// Configuration for reserving a pending idempotency key with a lease.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdempotencyLeaseConfig {
    /// Lease duration applied to the reservation.
    pub lease_duration: Duration,
    /// Optional explicit owner token; a unique token is generated when absent.
    pub owner: Option<String>,
}

impl Default for IdempotencyLeaseConfig {
    fn default() -> Self {
        Self {
            lease_duration: DEFAULT_IDEMPOTENCY_LEASE,
            owner: None,
        }
    }
}

impl<V> IdempotencyState<V> {
    /// Returns `true` when the state is an expired pending lease at `now_ms`.
    pub fn is_expired_pending(&self, now_ms: u64) -> bool {
        matches!(self, Self::Pending(lease) if lease.is_expired(now_ms))
    }
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) fn expires_at_ms(duration: Duration) -> u64 {
    now_ms().saturating_add(duration.as_millis() as u64)
}

pub(crate) fn new_lease(config: &IdempotencyLeaseConfig) -> IdempotencyLease {
    IdempotencyLease {
        owner: config
            .owner
            .clone()
            .unwrap_or_else(|| format!("owner-{}", now_ms())),
        expires_at_ms: expires_at_ms(config.lease_duration),
    }
}

pub(crate) fn pending_state_from_row(
    owner: Option<String>,
    expires_at_ms: Option<i64>,
    now_ms: u64,
) -> Option<IdempotencyLease> {
    let lease = match (
        owner.filter(|value| !value.is_empty()),
        expires_at_ms.and_then(|value| u64::try_from(value).ok()),
    ) {
        (Some(owner), Some(expires_at_ms)) => IdempotencyLease {
            owner,
            expires_at_ms,
        },
        _ => new_lease(&IdempotencyLeaseConfig::default()),
    };
    if lease.is_expired(now_ms) {
        None
    } else {
        Some(lease)
    }
}

/// Wait policy used when an idempotency key is already pending.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdempotencyWaitConfig {
    /// Maximum time to wait for a pending key before returning a timeout error.
    pub pending_timeout: Duration,
    /// Polling interval while waiting for another caller to complete the key.
    pub poll_interval: Duration,
}

impl IdempotencyWaitConfig {
    /// Creates an idempotency wait policy.
    ///
    /// `poll_interval` must be non-zero; callers that need sub-millisecond polling
    /// should pick an explicit minimum rather than relying on a silent floor.
    pub fn new(pending_timeout: Duration, poll_interval: Duration) -> Self {
        assert!(!poll_interval.is_zero(), "poll_interval must be non-zero");
        Self {
            pending_timeout,
            poll_interval,
        }
    }

    /// Returns the next delay, capped by the remaining timeout.
    ///
    /// A small deterministic jitter derived from `jitter_key` spreads concurrent
    /// waiters so they do not poll in lockstep.
    pub(crate) fn next_delay(
        &self,
        elapsed: Duration,
        jitter_key: &IdempotencyKey,
    ) -> Option<Duration> {
        let remaining = self.pending_timeout.checked_sub(elapsed)?;
        if remaining.is_zero() || self.poll_interval.is_zero() {
            return None;
        }

        let jitter_ms = idempotency_poll_jitter_ms(jitter_key, self.poll_interval);
        let delay = self
            .poll_interval
            .saturating_add(Duration::from_millis(jitter_ms));
        Some(remaining.min(delay))
    }
}

fn idempotency_poll_jitter_ms(key: &IdempotencyKey, poll_interval: Duration) -> u64 {
    let span_ms = (poll_interval.as_millis().max(1) / 4) as u64;
    if span_ms == 0 {
        return 0;
    }
    let hash = key.as_str().bytes().fold(0u64, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(u64::from(byte))
    });
    hash % span_ms
}

impl Default for IdempotencyWaitConfig {
    fn default() -> Self {
        Self {
            pending_timeout: DEFAULT_IDEMPOTENCY_PENDING_TIMEOUT,
            poll_interval: DEFAULT_IDEMPOTENCY_POLL_INTERVAL,
        }
    }
}

/// Maximum idempotency key length supported by MySQL `VARCHAR(255)` columns.
///
/// Keys longer than this are rejected by SQL stores that persist the raw key
/// string so `INSERT IGNORE` cannot silently truncate two distinct keys into one.
pub const IDEMPOTENCY_KEY_MAX_LEN: usize = 255;

/// Error returned when an idempotency key exceeds a store's supported length.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdempotencyKeyTooLong {
    /// Actual key length in bytes.
    pub len: usize,
    /// Maximum supported length.
    pub max_len: usize,
}

impl Display for IdempotencyKeyTooLong {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "idempotency key length {} exceeds the supported maximum of {}",
            self.len, self.max_len
        )
    }
}

impl Error for IdempotencyKeyTooLong {}

/// Stable idempotency key used to deduplicate command retries.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::IdempotencyKey;
///
/// let key = IdempotencyKey::new("command-123");
/// assert_eq!(key.as_str(), "command-123");
/// assert_eq!(key.to_string(), "command-123");
/// ```
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    /// Creates a new idempotency key.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the key as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns an error when the key exceeds [`IDEMPOTENCY_KEY_MAX_LEN`].
    pub fn validate_storage_length(&self) -> Result<(), IdempotencyKeyTooLong> {
        if self.0.len() > IDEMPOTENCY_KEY_MAX_LEN {
            Err(IdempotencyKeyTooLong {
                len: self.0.len(),
                max_len: IDEMPOTENCY_KEY_MAX_LEN,
            })
        } else {
            Ok(())
        }
    }
}

impl Display for IdempotencyKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// State of a processed or in-progress command.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdempotencyState<V> {
    /// Command is currently being processed.
    Pending(IdempotencyLease),
    /// Command has completed, containing the original result.
    Complete(V),
}

/// Stores previously committed command results by idempotency key.
pub trait IdempotencyStore<V>: Clone + Send + Sync + 'static
where
    V: Clone,
{
    /// Store-specific error type.
    type Error;

    /// Loads a previous result or execution status for an idempotency key.
    fn load(&self, key: &IdempotencyKey) -> Result<Option<IdempotencyState<V>>, Self::Error>;

    /// Reserves an idempotency key, marking it as pending/in-progress.
    /// Returns `true` if the key was successfully reserved, or `false` if it was already reserved/completed.
    fn reserve(&self, key: IdempotencyKey) -> Result<bool, Self::Error> {
        self.reserve_with_lease(key, &IdempotencyLeaseConfig::default())
    }

    /// Reserves an idempotency key with an explicit lease.
    fn reserve_with_lease(
        &self,
        key: IdempotencyKey,
        config: &IdempotencyLeaseConfig,
    ) -> Result<bool, Self::Error>;

    /// Extends a pending lease owned by `owner`.
    fn heartbeat(&self, key: &IdempotencyKey, owner: &str) -> Result<bool, Self::Error>;

    /// Removes or reclaims stale pending rows whose leases have expired.
    fn expire_stale_pending(&self, now_ms: u64) -> Result<usize, Self::Error>;

    /// Deletes completed rows whose `updated_at_ms` is strictly before `cutoff_ms`.
    ///
    /// Returns the number of rows removed. The default implementation is a no-op
    /// for stores that do not persist completion timestamps.
    fn expire_completed_before(&self, cutoff_ms: u64) -> Result<usize, Self::Error> {
        let _ = cutoff_ms;
        Ok(0)
    }

    /// Deletes completed rows older than `max_age` relative to the current wall clock.
    fn purge_completed_older_than(&self, max_age: Duration) -> Result<usize, Self::Error> {
        let cutoff_ms = now_ms().saturating_sub(max_age.as_millis() as u64);
        self.expire_completed_before(cutoff_ms)
    }

    /// Saves a completed result for an idempotency key.
    fn save(&self, key: IdempotencyKey, value: V) -> Result<(), Self::Error>;

    /// Removes a reservation/entry (e.g. if execution failed).
    fn remove(&self, key: &IdempotencyKey) -> Result<(), Self::Error>;
}

/// Error returned by [`InMemoryIdempotencyStore`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InMemoryIdempotencyError {
    /// Shared state was poisoned by a panic while holding a lock.
    Poisoned,
}

impl Display for InMemoryIdempotencyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            InMemoryIdempotencyError::Poisoned => {
                f.write_str("idempotency store lock was poisoned")
            }
        }
    }
}

impl Error for InMemoryIdempotencyError {}

/// Thread-safe in-memory idempotency store.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::{IdempotencyKey, IdempotencyStore, InMemoryIdempotencyStore, IdempotencyState};
///
/// let store = InMemoryIdempotencyStore::<String>::new();
/// let key = IdempotencyKey::new("msg-1");
///
/// store.reserve(key.clone()).unwrap();
/// store.save(key.clone(), "processed".to_string()).unwrap();
/// let value = store.load(&key).unwrap();
/// assert_eq!(value, Some(IdempotencyState::Complete("processed".to_string())));
/// ```
#[derive(Clone, Debug)]
struct InMemoryIdempotencyEntry<V> {
    state: IdempotencyState<V>,
    updated_at_ms: u64,
}

#[derive(Clone, Debug)]
pub struct InMemoryIdempotencyStore<V>
where
    V: Clone,
{
    entries: Arc<RwLock<HashMap<IdempotencyKey, InMemoryIdempotencyEntry<V>>>>,
}

impl<V> Default for InMemoryIdempotencyStore<V>
where
    V: Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<V> InMemoryIdempotencyStore<V>
where
    V: Clone,
{
    /// Creates an empty in-memory idempotency store.
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Removes all stored entries.
    pub fn clear(&self) -> Result<(), InMemoryIdempotencyError> {
        self.entries
            .write()
            .map_err(|_| InMemoryIdempotencyError::Poisoned)?
            .clear();
        Ok(())
    }
}

impl<V> IdempotencyStore<V> for InMemoryIdempotencyStore<V>
where
    V: Clone + Send + Sync + 'static,
{
    type Error = InMemoryIdempotencyError;

    fn load(&self, key: &IdempotencyKey) -> Result<Option<IdempotencyState<V>>, Self::Error> {
        let entries = self
            .entries
            .read()
            .map_err(|_| InMemoryIdempotencyError::Poisoned)?;
        Ok(entries.get(key).map(|entry| entry.state.clone()))
    }

    fn reserve_with_lease(
        &self,
        key: IdempotencyKey,
        config: &IdempotencyLeaseConfig,
    ) -> Result<bool, Self::Error> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| InMemoryIdempotencyError::Poisoned)?;
        let now = now_ms();
        match entries.entry(key) {
            std::collections::hash_map::Entry::Occupied(mut occupied) => {
                match &occupied.get().state {
                    IdempotencyState::Pending(lease) if lease.is_expired(now) => {
                        occupied.insert(InMemoryIdempotencyEntry {
                            state: IdempotencyState::Pending(new_lease(config)),
                            updated_at_ms: now,
                        });
                        Ok(true)
                    }
                    _ => Ok(false),
                }
            }
            std::collections::hash_map::Entry::Vacant(vacant) => {
                vacant.insert(InMemoryIdempotencyEntry {
                    state: IdempotencyState::Pending(new_lease(config)),
                    updated_at_ms: now,
                });
                Ok(true)
            }
        }
    }

    fn heartbeat(&self, key: &IdempotencyKey, owner: &str) -> Result<bool, Self::Error> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| InMemoryIdempotencyError::Poisoned)?;
        let Some(entry) = entries.get_mut(key) else {
            return Ok(false);
        };
        let IdempotencyState::Pending(lease) = &mut entry.state else {
            return Ok(false);
        };
        if lease.owner != owner {
            return Ok(false);
        }
        lease.expires_at_ms = expires_at_ms(DEFAULT_IDEMPOTENCY_LEASE);
        entry.updated_at_ms = now_ms();
        Ok(true)
    }

    fn expire_stale_pending(&self, now_ms: u64) -> Result<usize, Self::Error> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| InMemoryIdempotencyError::Poisoned)?;
        let before = entries.len();
        entries.retain(|_, entry| !entry.state.is_expired_pending(now_ms));
        Ok(before.saturating_sub(entries.len()))
    }

    fn expire_completed_before(&self, cutoff_ms: u64) -> Result<usize, Self::Error> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| InMemoryIdempotencyError::Poisoned)?;
        let before = entries.len();
        entries.retain(|_, entry| {
            !matches!(
                entry.state,
                IdempotencyState::Complete(_) if entry.updated_at_ms < cutoff_ms
            )
        });
        Ok(before.saturating_sub(entries.len()))
    }

    fn save(&self, key: IdempotencyKey, value: V) -> Result<(), Self::Error> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| InMemoryIdempotencyError::Poisoned)?;
        entries.insert(
            key,
            InMemoryIdempotencyEntry {
                state: IdempotencyState::Complete(value),
                updated_at_ms: now_ms(),
            },
        );
        Ok(())
    }

    fn remove(&self, key: &IdempotencyKey) -> Result<(), Self::Error> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| InMemoryIdempotencyError::Poisoned)?;
        entries.remove(key);
        Ok(())
    }
}

/// Error returned by idempotent repository execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdempotentRepositoryError<DomainError, StoreError, IdempotencyError> {
    /// Aggregate command handling rejected the command.
    Domain(DomainError),
    /// Event store rejected the append due to optimistic concurrency.
    Concurrency(ConcurrencyError),
    /// Event store or infrastructure operation failed.
    Store(StoreError),
    /// Idempotency store operation failed.
    Idempotency(IdempotencyError),
    /// The idempotency key remained pending until the configured wait timeout elapsed.
    IdempotencyPendingTimeout {
        /// Key that was still pending.
        key: IdempotencyKey,
        /// Time spent waiting for the pending key.
        waited: Duration,
    },
    /// Failed to release a pending idempotency key after execution failure.
    IdempotencyReleaseFailed {
        /// Key that could not be released.
        key: IdempotencyKey,
        /// Number of release attempts performed.
        attempts: usize,
    },
}

impl<DomainError, StoreError, IdempotencyError>
    IdempotentRepositoryError<DomainError, StoreError, IdempotencyError>
where
    StoreError: EventStoreFailure,
{
    /// Converts an event store error into an idempotent repository error.
    pub fn from_store_error(error: StoreError) -> Self {
        match error.into_repository_error() {
            RepositoryError::Domain(error) => IdempotentRepositoryError::Domain(error),
            RepositoryError::Concurrency(error) => IdempotentRepositoryError::Concurrency(error),
            RepositoryError::Store(error) => IdempotentRepositoryError::Store(error),
        }
    }
}

impl<DomainError, StoreError, IdempotencyError>
    IdempotentRepositoryError<DomainError, StoreError, IdempotencyError>
{
    /// Converts a repository error into an idempotent repository error.
    pub fn from_repository_error(error: RepositoryError<DomainError, StoreError>) -> Self {
        match error {
            RepositoryError::Domain(error) => IdempotentRepositoryError::Domain(error),
            RepositoryError::Concurrency(error) => IdempotentRepositoryError::Concurrency(error),
            RepositoryError::Store(error) => IdempotentRepositoryError::Store(error),
        }
    }
}

/// Returns the next delay of the pending-key wait loop, or the fully built
/// timeout error once the wait budget is spent.
///
/// This is the single source of the wait policy shared by the sync and async
/// repositories; callers only differ in how they sleep for the returned
/// delay.
pub(crate) fn pending_wait_delay<DomainError, StoreError, IdempotencyError>(
    wait_config: &IdempotencyWaitConfig,
    started: std::time::Instant,
    idempotency_key: &IdempotencyKey,
) -> Result<Duration, IdempotentRepositoryError<DomainError, StoreError, IdempotencyError>> {
    wait_config
        .next_delay(started.elapsed(), idempotency_key)
        .ok_or_else(|| IdempotentRepositoryError::IdempotencyPendingTimeout {
            key: idempotency_key.clone(),
            waited: started.elapsed(),
        })
}

impl<DomainError, StoreError, IdempotencyError> Display
    for IdempotentRepositoryError<DomainError, StoreError, IdempotencyError>
where
    DomainError: Display,
    StoreError: Display,
    IdempotencyError: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            IdempotentRepositoryError::Domain(error) => Display::fmt(error, f),
            IdempotentRepositoryError::Concurrency(error) => Display::fmt(error, f),
            IdempotentRepositoryError::Store(error) => Display::fmt(error, f),
            IdempotentRepositoryError::Idempotency(error) => Display::fmt(error, f),
            IdempotentRepositoryError::IdempotencyPendingTimeout { key, waited } => {
                write!(
                    f,
                    "idempotency key `{key}` remained pending after {} ms",
                    waited.as_millis()
                )
            }
            IdempotentRepositoryError::IdempotencyReleaseFailed { key, attempts } => {
                write!(
                    f,
                    "failed to release idempotency key `{key}` after {attempts} attempts"
                )
            }
        }
    }
}

impl<DomainError, StoreError, IdempotencyError> Error
    for IdempotentRepositoryError<DomainError, StoreError, IdempotencyError>
where
    DomainError: Error + 'static,
    StoreError: Error + 'static,
    IdempotencyError: Error + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            IdempotentRepositoryError::Domain(error) => Some(error),
            IdempotentRepositoryError::Concurrency(error) => Some(error),
            IdempotentRepositoryError::Store(error) => Some(error),
            IdempotentRepositoryError::Idempotency(error) => Some(error),
            IdempotentRepositoryError::IdempotencyPendingTimeout { .. } => None,
            IdempotentRepositoryError::IdempotencyReleaseFailed { .. } => None,
        }
    }
}

#[cfg(feature = "async")]
#[async_trait::async_trait]
impl<V> crate::async_api::AsyncIdempotencyStore<V> for InMemoryIdempotencyStore<V>
where
    V: Clone + Send + Sync + 'static,
{
    type Error = InMemoryIdempotencyError;

    async fn load(&self, key: &IdempotencyKey) -> Result<Option<IdempotencyState<V>>, Self::Error> {
        IdempotencyStore::load(self, key)
    }

    async fn reserve(&self, key: IdempotencyKey) -> Result<bool, Self::Error> {
        IdempotencyStore::reserve(self, key)
    }

    async fn reserve_with_lease(
        &self,
        key: IdempotencyKey,
        config: &IdempotencyLeaseConfig,
    ) -> Result<bool, Self::Error> {
        IdempotencyStore::reserve_with_lease(self, key, config)
    }

    async fn heartbeat(&self, key: &IdempotencyKey, owner: &str) -> Result<bool, Self::Error> {
        IdempotencyStore::heartbeat(self, key, owner)
    }

    async fn expire_stale_pending(&self, now_ms: u64) -> Result<usize, Self::Error> {
        IdempotencyStore::expire_stale_pending(self, now_ms)
    }

    async fn expire_completed_before(&self, cutoff_ms: u64) -> Result<usize, Self::Error> {
        IdempotencyStore::expire_completed_before(self, cutoff_ms)
    }

    async fn save(&self, key: IdempotencyKey, value: V) -> Result<(), Self::Error> {
        IdempotencyStore::save(self, key, value)
    }

    async fn remove(&self, key: &IdempotencyKey) -> Result<(), Self::Error> {
        IdempotencyStore::remove(self, key)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        idempotency_poll_jitter_ms, pending_wait_delay, IdempotencyKey, IdempotencyLeaseConfig,
        IdempotencyState, IdempotencyStore, IdempotencyWaitConfig, InMemoryIdempotencyStore,
    };
    use std::time::{Duration, Instant};

    type TestError = super::IdempotentRepositoryError<(), crate::EventStoreError, ()>;

    #[test]
    fn pending_wait_delay_returns_capped_delays_within_the_budget() {
        let config = IdempotencyWaitConfig::new(Duration::from_secs(30), Duration::from_millis(50));
        let key = IdempotencyKey::new("wait-key");

        let delay: Duration =
            pending_wait_delay::<(), crate::EventStoreError, ()>(&config, Instant::now(), &key)
                .unwrap();

        assert!(delay >= Duration::from_millis(50));
        assert!(delay <= Duration::from_millis(62));
        assert!(!delay.is_zero());
    }

    #[test]
    fn pending_wait_delay_times_out_once_the_budget_is_spent() {
        let config = IdempotencyWaitConfig::new(Duration::ZERO, Duration::from_millis(50));
        let key = IdempotencyKey::new("wait-key");
        let started = Instant::now();

        let error: TestError = pending_wait_delay(&config, started, &key).unwrap_err();

        match error {
            super::IdempotentRepositoryError::IdempotencyPendingTimeout {
                key: timed_out_key,
                ..
            } => assert_eq!(timed_out_key, key),
            other => panic!("expected pending timeout, got {other:?}"),
        }
    }

    #[test]
    #[should_panic(expected = "poll_interval must be non-zero")]
    fn zero_poll_interval_is_rejected() {
        let _ = IdempotencyWaitConfig::new(Duration::from_secs(1), Duration::ZERO);
    }

    #[test]
    fn poll_jitter_spreads_waiters_with_the_same_interval() {
        let interval = Duration::from_millis(40);
        let first = idempotency_poll_jitter_ms(&IdempotencyKey::new("alpha"), interval);
        let second = idempotency_poll_jitter_ms(&IdempotencyKey::new("beta"), interval);
        assert!(first <= 10);
        assert!(second <= 10);
        assert_ne!(first, second);
    }

    #[test]
    fn next_delay_applies_jitter_without_exceeding_remaining_budget() {
        let config =
            IdempotencyWaitConfig::new(Duration::from_millis(100), Duration::from_millis(20));
        let key = IdempotencyKey::new("jitter-key");
        let delay = config
            .next_delay(Duration::from_millis(10), &key)
            .expect("delay");
        assert!(delay >= Duration::from_millis(20));
        assert!(delay <= Duration::from_millis(90));
    }

    #[test]
    fn expired_pending_lease_can_be_reclaimed() {
        let store = InMemoryIdempotencyStore::<String>::new();
        let key = IdempotencyKey::new("lease-key");
        let config = IdempotencyLeaseConfig {
            lease_duration: Duration::ZERO,
            ..Default::default()
        };

        assert!(store.reserve_with_lease(key.clone(), &config).unwrap());
        assert!(matches!(
            store.load(&key).unwrap(),
            Some(IdempotencyState::Pending(_))
        ));
        assert!(store.reserve_with_lease(key.clone(), &config).unwrap());
    }

    #[test]
    fn purge_completed_older_than_removes_stale_completed_rows() {
        let store = InMemoryIdempotencyStore::<String>::new();
        let fresh_key = IdempotencyKey::new("fresh");
        let stale_key = IdempotencyKey::new("stale");

        store.save(fresh_key.clone(), "fresh".to_string()).unwrap();
        store.save(stale_key.clone(), "stale".to_string()).unwrap();

        let removed = store
            .purge_completed_older_than(Duration::from_secs(60))
            .unwrap();
        assert_eq!(removed, 0);
        assert_eq!(
            store.load(&fresh_key).unwrap(),
            Some(IdempotencyState::Complete("fresh".to_string()))
        );

        let cutoff = super::now_ms().saturating_add(1);
        let removed = store.expire_completed_before(cutoff).unwrap();
        assert_eq!(removed, 2);
        assert!(store.load(&fresh_key).unwrap().is_none());
        assert!(store.load(&stale_key).unwrap().is_none());
    }
}
