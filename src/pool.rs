//! Internal connection pooling for the native SQL event stores.
//!
//! The pool hands out [`PoolLease`] guards that return their connection on
//! drop, so panics and early returns cannot leak connections. Leases marked
//! broken via [`PoolLease::mark_broken`] are discarded instead of recycled.
//!
//! Sizing resolves in order: explicit constructor argument, then
//! `DDD_CQRS_ES_POOL_SIZE`, then the CPU count clamped to `[2, 8]`.

use crate::error::EventStoreError;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

pub(crate) const POOL_SIZE_ENV_VAR: &str = "DDD_CQRS_ES_POOL_SIZE";

const DEFAULT_MIN_SIZE: usize = 2;
const DEFAULT_MAX_SIZE: usize = 8;
const OVERRIDE_MIN_SIZE: usize = 1;
const OVERRIDE_MAX_SIZE: usize = 128;

/// How long an acquisition waits on an exhausted pool before failing with a
/// connection error instead of blocking indefinitely.
const DEFAULT_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(30);

/// Resolves the pool size from an explicit override, the environment, or the
/// CPU-count default.
pub(crate) fn resolve_pool_size(explicit: Option<usize>) -> usize {
    let requested = explicit.or_else(|| {
        std::env::var(POOL_SIZE_ENV_VAR)
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok())
    });
    match requested {
        Some(size) => size.clamp(OVERRIDE_MIN_SIZE, OVERRIDE_MAX_SIZE),
        None => std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(DEFAULT_MIN_SIZE)
            .clamp(DEFAULT_MIN_SIZE, DEFAULT_MAX_SIZE),
    }
}

type Connect<C> = Box<dyn Fn() -> Result<C, EventStoreError> + Send + Sync>;

/// Transport-level failures justify discarding a pooled connection; domain
/// outcomes (concurrency conflicts, serialization, deserialization) do not.
///
/// Backend errors carrying a machine-readable code are evicted only when the
/// code is connection-related; a statement-level code (unique violation,
/// syntax error, constraint failure) proves the connection delivered a round
/// trip and stays pooled. Code-less backend errors are IO-level failures and
/// keep the conservative eviction.
fn is_transport_error(error: &EventStoreError) -> bool {
    match error {
        EventStoreError::Backend {
            code: Some(code), ..
        } => is_transport_code(code),
        EventStoreError::Backend { code: None, .. } => true,
        _ => false,
    }
}

/// Connection-related backend codes: SQLSTATE class 08 (connection
/// exception), Postgres server-shutdown states, and MySQL connection errnos
/// (server shutdown/gone, aborted connections, and net read/write timeouts).
fn is_transport_code(code: &str) -> bool {
    const MYSQL_TRANSPORT_ERRNOS: &[&str] = &[
        "1053", "1152", "1159", "1160", "1161", "2002", "2003", "2006", "2013", "2055",
    ];
    code.starts_with("08")
        || matches!(code, "57P01" | "57P02" | "57P03")
        || MYSQL_TRANSPORT_ERRNOS.contains(&code)
}

struct PoolState<C> {
    idle: Vec<C>,
    leased: usize,
}

struct PoolShared<C> {
    max_size: usize,
    acquire_timeout: Duration,
    state: Mutex<PoolState<C>>,
    available: Condvar,
    connect: Option<Connect<C>>,
}

impl<C> PoolShared<C> {
    fn give_back(&self, connection: Option<C>) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.leased -= 1;
        if let Some(connection) = connection {
            state.idle.push(connection);
        }
        self.available.notify_one();
    }
}

/// A rented connection that returns to its pool on drop unless marked broken.
pub(crate) struct PoolLease<C> {
    shared: Arc<PoolShared<C>>,
    connection: Option<C>,
    broken: bool,
}

impl<C> PoolLease<C> {
    /// Evicts the connection instead of returning it to the idle set.
    pub fn mark_broken(&mut self) {
        self.broken = true;
    }
}

impl<C> Deref for PoolLease<C> {
    type Target = C;

    fn deref(&self) -> &C {
        self.connection
            .as_ref()
            .expect("lease holds its connection")
    }
}

impl<C> DerefMut for PoolLease<C> {
    fn deref_mut(&mut self) -> &mut C {
        self.connection
            .as_mut()
            .expect("lease holds its connection")
    }
}

impl<C> Drop for PoolLease<C> {
    fn drop(&mut self) {
        // A pool without a reconnect factory cannot replace a discarded
        // connection, so broken leases are retained (matching the previous
        // mutex-guarded behaviour of keeping one client forever).
        let connection = if self.broken && self.shared.connect.is_some() {
            None
        } else {
            self.connection.take()
        };
        self.shared.give_back(connection);
    }
}

/// Bounded connection pool over an arbitrary synchronous connection type.
pub(crate) struct ConnectionPool<C> {
    shared: Arc<PoolShared<C>>,
}

impl<C> Clone for ConnectionPool<C> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<C> std::fmt::Debug for ConnectionPool<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionPool")
            .field("max_size", &self.shared.max_size)
            .finish_non_exhaustive()
    }
}

impl<C> ConnectionPool<C> {
    /// Creates a pool that wraps a single pre-established connection.
    ///
    /// Without a connect factory the pool cannot grow; acquisitions queue on
    /// the one connection, matching the previous mutex-guarded behaviour.
    pub(crate) fn single(connection: C) -> Self {
        Self::build(Some(connection), 1, None)
    }

    /// Creates a growable pool that opens connections through `connect`.
    pub(crate) fn pooled(
        max_size: usize,
        connect: impl Fn() -> Result<C, EventStoreError> + Send + Sync + 'static,
    ) -> Self {
        Self::build(None, max_size, Some(Box::new(connect)))
    }

    fn build(seed: Option<C>, max_size: usize, connect: Option<Connect<C>>) -> Self {
        let idle = seed.into_iter().collect();
        Self {
            shared: Arc::new(PoolShared {
                max_size,
                acquire_timeout: DEFAULT_ACQUIRE_TIMEOUT,
                state: Mutex::new(PoolState { idle, leased: 0 }),
                available: Condvar::new(),
                connect,
            }),
        }
    }

    /// Overrides how long an acquisition waits for a connection. Test-only:
    /// production pools use [`DEFAULT_ACQUIRE_TIMEOUT`].
    #[cfg(test)]
    fn with_acquire_timeout(mut self, timeout: Duration) -> Self {
        Arc::get_mut(&mut self.shared)
            .expect("acquire timeout must be configured before the pool is shared")
            .acquire_timeout = timeout;
        self
    }

    /// Rents a connection, opening a new one when capacity allows.
    ///
    /// Waits at most the pool's acquire timeout for a connection to be
    /// returned, then fails with a connection error instead of blocking the
    /// caller indefinitely on an exhausted pool.
    pub(crate) fn acquire(&self) -> Result<PoolLease<C>, EventStoreError> {
        let deadline = std::time::Instant::now() + self.shared.acquire_timeout;
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if let Some(connection) = state.idle.pop() {
                state.leased += 1;
                return Ok(self.lease(connection));
            }
            if state.leased < self.shared.max_size {
                let Some(connect) = self.shared.connect.as_ref() else {
                    return Err(EventStoreError::backend(
                        "no pooled connection is available and no reconnect factory is configured"
                            .to_owned(),
                    ));
                };
                state.leased += 1;
                drop(state);
                let connected = connect();
                let mut state = self
                    .shared
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                match connected {
                    Ok(connection) => return Ok(self.lease(connection)),
                    Err(error) => {
                        state.leased -= 1;
                        self.shared.available.notify_one();
                        return Err(error);
                    }
                }
            }
            let now = std::time::Instant::now();
            let Some(remaining) = deadline
                .checked_duration_since(now)
                .filter(|d| !d.is_zero())
            else {
                return Err(EventStoreError::connection(format!(
                    "no pooled connection became available within {:?} \
                     ({} of {} connections leased)",
                    self.shared.acquire_timeout, state.leased, self.shared.max_size
                )));
            };
            state = self
                .shared
                .available
                .wait_timeout(state, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }

    /// Runs a read-only operation, retrying once on a fresh connection when
    /// the first attempt fails on a possibly stale pooled connection.
    pub(crate) fn read<F, T>(&self, operation: F) -> Result<T, EventStoreError>
    where
        F: Fn(&mut C) -> Result<T, EventStoreError>,
    {
        let mut lease = self.acquire()?;
        let first = operation(&mut lease);
        match first {
            Ok(value) => return Ok(value),
            Err(error) => {
                if !is_transport_error(&error) {
                    return Err(error);
                }
            }
        }
        lease.mark_broken();
        drop(lease);
        let mut fresh = self.acquire()?;
        let result = operation(&mut fresh);
        if let Err(error) = &result {
            if is_transport_error(error) {
                fresh.mark_broken();
            }
        }
        result
    }

    /// Runs an operation exactly once. The connection is discarded only when
    /// the failure looks transport-level; domain outcomes such as optimistic
    /// concurrency conflicts leave the connection healthy and pooled.
    pub(crate) fn write<F, T>(&self, operation: F) -> Result<T, EventStoreError>
    where
        F: FnOnce(&mut C) -> Result<T, EventStoreError>,
    {
        let mut lease = self.acquire()?;
        let result = operation(&mut lease);
        if let Err(error) = &result {
            if is_transport_error(error) {
                lease.mark_broken();
            }
        }
        result
    }

    fn lease(&self, connection: C) -> PoolLease<C> {
        PoolLease {
            shared: Arc::clone(&self.shared),
            connection: Some(connection),
            broken: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_pool_size, ConnectionPool};
    use crate::error::EventStoreError;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};

    #[test]
    fn resolve_prefers_explicit_then_env_then_default() {
        assert_eq!(resolve_pool_size(Some(5)), 5);
        assert_eq!(resolve_pool_size(Some(0)), 1);
        assert_eq!(resolve_pool_size(Some(10_000)), 128);
        std::env::set_var(super::POOL_SIZE_ENV_VAR, "3");
        assert_eq!(resolve_pool_size(None), 3);
        std::env::set_var(super::POOL_SIZE_ENV_VAR, "not-a-number");
        let fallback = resolve_pool_size(None);
        assert!((2..=8).contains(&fallback));
        std::env::remove_var(super::POOL_SIZE_ENV_VAR);
    }

    #[test]
    fn leases_are_returned_and_reused_without_new_connects() {
        let connects = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&connects);
        let pool = ConnectionPool::pooled(2, move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });

        for _ in 0..5 {
            let lease = pool.acquire().unwrap();
            drop(lease);
        }

        assert_eq!(connects.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn concurrent_acquires_respect_max_size_and_share_idle_connections() {
        let connects = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&connects);
        let live = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let pool = ConnectionPool::<()>::pooled(3, move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
        let barrier = Arc::new(Barrier::new(8));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let pool = pool.clone();
            let barrier = Arc::clone(&barrier);
            let live = Arc::clone(&live);
            let peak = Arc::clone(&peak);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..20 {
                    let lease = pool.acquire().unwrap();
                    let now = live.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    live.fetch_sub(1, Ordering::SeqCst);
                    drop(lease);
                }
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }

        assert!(connects.load(Ordering::SeqCst) <= 3);
        assert!(peak.load(Ordering::SeqCst) <= 3);
    }

    #[test]
    fn broken_leases_are_discarded_not_recycled() {
        let connects = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&connects);
        let pool = ConnectionPool::pooled(2, move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });

        {
            let mut lease = pool.acquire().unwrap();
            lease.mark_broken();
        }
        assert!(pool.acquire().is_ok());

        assert_eq!(connects.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn read_retries_once_on_a_fresh_connection() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&attempts);
        let pool = ConnectionPool::<()>::pooled(2, || Ok(()));

        let result = pool.read(|_| {
            let attempt = counter.fetch_add(1, Ordering::SeqCst);
            if attempt == 0 {
                Err(EventStoreError::backend("broken pipe".to_owned()))
            } else {
                Ok(attempt)
            }
        });

        assert_eq!(result.unwrap(), 1);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn acquire_fails_with_a_connection_error_when_the_pool_stays_exhausted() {
        let pool = ConnectionPool::<()>::pooled(1, || Ok(()))
            .with_acquire_timeout(std::time::Duration::from_millis(50));

        let held = pool.acquire().unwrap();
        let Err(error) = pool.acquire() else {
            panic!("expected the exhausted pool to time out");
        };

        assert!(matches!(error, EventStoreError::Connection { .. }));
        assert!(error.to_string().contains("1 of 1 connections leased"));

        // Returning the lease makes the pool usable again.
        drop(held);
        pool.acquire().unwrap();
    }

    #[test]
    fn read_evicts_the_fresh_connection_when_the_retry_also_fails() {
        let connects = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&connects);
        let pool = ConnectionPool::<()>::pooled(1, move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });

        let result: Result<(), _> =
            pool.read(|_| Err(EventStoreError::backend("broken pipe".to_owned())));
        assert!(result.is_err());
        assert_eq!(connects.load(Ordering::SeqCst), 2);

        // Both failed connections must have been evicted, so the next read
        // opens a third connection instead of recycling a broken one.
        pool.read(|_| Ok(())).unwrap();
        assert_eq!(connects.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn write_runs_exactly_once_even_on_failure() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&attempts);
        let pool = ConnectionPool::<()>::pooled(2, || Ok(()));

        let result: Result<(), EventStoreError> = pool.write(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            Err(EventStoreError::backend("conflict".to_owned()))
        });

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn factory_failures_surface_and_release_capacity() {
        let pool: ConnectionPool<()> =
            ConnectionPool::pooled(2, || Err(EventStoreError::backend("down".to_owned())));

        match pool.acquire() {
            Ok(_) => panic!("expected factory failure to surface"),
            Err(error) => assert!(matches!(error, EventStoreError::Backend { .. })),
        }
        assert!(pool.acquire().is_err());
    }

    #[test]
    fn single_pool_keeps_serving_after_marked_broken_lease() {
        let pool = ConnectionPool::single(7_u32);
        {
            let mut lease = pool.acquire().unwrap();
            lease.mark_broken();
        }

        let mut lease = pool.acquire().unwrap();
        *lease += 1;
        assert_eq!(*lease, 8);
    }

    #[test]
    fn single_pool_survives_failed_writes_and_stays_usable() {
        let pool = ConnectionPool::<()>::single(());

        for _ in 0..3 {
            let result: Result<(), EventStoreError> =
                pool.write(|_| Err(EventStoreError::backend("connection reset".to_owned())));
            assert!(result.is_err());

            // Must never hang or brick; each write acquires again.
        }
    }

    #[test]
    fn domain_errors_do_not_discard_pooled_connections() {
        let connects = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&connects);
        let pool = ConnectionPool::<()>::pooled(2, move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });

        let conflict = Err(EventStoreError::Concurrency(
            crate::ConcurrencyError::StreamAlreadyExists,
        ));
        let result: Result<(), EventStoreError> = pool.write(move |_| conflict);
        assert!(result.is_err());

        // The healthy connection must be recycled, not reconnected.
        let second: Result<(), EventStoreError> = pool.write(|_| Ok(()));
        assert!(second.is_ok());
        assert_eq!(connects.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn statement_level_backend_codes_keep_the_connection_pooled() {
        let connects = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&connects);
        let pool = ConnectionPool::<()>::pooled(2, move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });

        // A unique violation proves the server processed the statement, so
        // the connection is healthy and must be recycled.
        let result: Result<(), EventStoreError> =
            pool.write(|_| Err(EventStoreError::backend("duplicate key").with_code("23505")));
        assert!(result.is_err());
        pool.write(|_| Ok(())).unwrap();
        assert_eq!(connects.load(Ordering::SeqCst), 1);

        // A connection-exception SQLSTATE evicts as before.
        let result: Result<(), EventStoreError> =
            pool.write(|_| Err(EventStoreError::backend("connection failure").with_code("08006")));
        assert!(result.is_err());
        pool.write(|_| Ok(())).unwrap();
        assert_eq!(connects.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn transport_code_classification_covers_both_backends() {
        for code in ["08000", "08006", "57P01", "2006", "2013", "1053"] {
            assert!(super::is_transport_code(code), "{code} should evict");
        }
        for code in ["23505", "40001", "1062", "42601", "1213"] {
            assert!(!super::is_transport_code(code), "{code} should stay pooled");
        }
    }
}
