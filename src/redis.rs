//! Experimental Redis event store, checkpoint store, and pub/sub helpers.
//!
//! Redis support is async-only in this crate. The event store uses Redis as a
//! persistence backend with optimistic concurrency enforced by one Lua `EVAL`
//! append script. Pub/sub publishing is intentionally separate from event
//! durability; Redis messages are notifications and must not be treated as the
//! source of truth.

use crate::aggregate::Aggregate;
use crate::async_api::AsyncEventStore;
use crate::error::EventStoreError;
use crate::event::{EventEnvelope, EventId, ExpectedRevision, NewEvent};
use crate::event_store::EventStream;
use crate::projection::AsyncCheckpointStore;
use crate::sql_common::{
    check_expected_revision, deserialize_id, deserialize_metadata, deserialize_payload,
    millis_to_system_time, serialize_id, serialize_metadata, serialize_payload,
    system_time_to_millis,
};
use crate::upcast::UpcasterRegistry;
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
#[cfg(feature = "wasi-redis")]
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::marker::PhantomData;
#[cfg(feature = "wasi-redis")]
use std::net::TcpStream;
use std::num::NonZeroUsize;
#[cfg(any(feature = "wasi-redis", feature = "spin-redis"))]
use std::sync::{Arc, Mutex as StdMutex};
#[cfg(feature = "wasi-redis")]
use std::time::Duration;
use std::time::SystemTime;

const DEFAULT_PREFIX: &str = "ddd_cqrs_es";
const DEFAULT_CHECKPOINT_PREFIX: &str = "ddd_cqrs_es";

/// Members read from a sorted-set index in one `ZRANGEBYSCORE` round trip.
///
/// Every index read is paged at this size so no single reply scales with the
/// stream or backlog length. The reply carries two elements per member
/// (`WITHSCORES`), so a page stays three orders of magnitude below the raw RESP
/// client's `MAX_RESP_ARRAY_LEN` ceiling.
const SEQUENCE_PAGE_SIZE: usize = 500;

/// Event hashes fetched in one `EVAL` round trip.
///
/// A stored event has ten fields, so the flat length-prefixed reply for a full
/// chunk is `256 * 21 = 5_376` elements. Fetching an unchunked backlog instead
/// crossed the `MAX_RESP_ARRAY_LEN` element ceiling at roughly 47_600 events
/// and made the stream or feed permanently unreadable.
const HASH_FETCH_CHUNK_SIZE: usize = 256;

const APPEND_LUA: &str = r#"
local current = tonumber(redis.call('GET', KEYS[1]) or '0')
local expected_kind = ARGV[1]
local expected_revision = tonumber(ARGV[2])
local count = tonumber(ARGV[3])
local event_key_prefix = ARGV[4]

if expected_kind == 'no_stream' and current ~= 0 then
    return {'ERR', 'stream_exists', current}
end

if expected_kind == 'exact' and current ~= expected_revision then
    return {'ERR', 'wrong_revision', current}
end

if count == 0 then
    return {'OK', 0, 0, current}
end

local last_sequence = redis.call('INCRBY', KEYS[2], count)
local first_sequence = last_sequence - count + 1

for i = 0, count - 1 do
    local base = 5 + (i * 8)
    local revision = current + i + 1
    local sequence = first_sequence + i
    local event_key = event_key_prefix .. tostring(sequence)

    redis.call(
        'HSET',
        event_key,
        'event_id', ARGV[base],
        'aggregate_id', ARGV[base + 1],
        'aggregate_type', ARGV[base + 2],
        'revision', tostring(revision),
        'sequence', tostring(sequence),
        'event_type', ARGV[base + 3],
        'event_version', ARGV[base + 4],
        'payload', ARGV[base + 5],
        'metadata', ARGV[base + 6],
        'recorded_at_ms', ARGV[base + 7]
    )
    redis.call('ZADD', KEYS[3], revision, tostring(sequence))
    redis.call('ZADD', KEYS[4], sequence, tostring(sequence))
end

redis.call('SET', KEYS[1], current + count)
return {'OK', first_sequence, last_sequence, current + count}
"#;

/// Fetches every key's hash in one round trip as a flat scalar list: each
/// hash is emitted as its item count followed by that many field/value
/// entries. A flat reply avoids nested RESP arrays, which some executor
/// backends (notably the Spin `redis-result` WIT variant) cannot represent.
const FETCH_HASHES_LUA: &str = r#"
local out = {}
for i = 1, #KEYS do
    local hash = redis.call('HGETALL', KEYS[i])
    out[#out + 1] = #hash
    for j = 1, #hash do
        out[#out + 1] = hash[j]
    end
end
return out
"#;

/// Atomically advances a projection checkpoint only when the new sequence is
/// greater than the stored value.
const CHECKPOINT_SAVE_LUA: &str = r#"
local current = redis.call('GET', KEYS[1])
if current == false or tonumber(current) < tonumber(ARGV[1]) then
    redis.call('SET', KEYS[1], ARGV[1])
end
return 1
"#;

/// Redis protocol value returned by [`RedisCommandExecutor`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RedisValue {
    /// Redis null/nil value.
    Nil,
    /// Simple string status value.
    Status(String),
    /// Integer value.
    Int(i64),
    /// Bulk byte value.
    Bytes(Vec<u8>),
    /// RESP array value.
    Array(Vec<RedisValue>),
}

/// Minimal async Redis command abstraction used by the experimental Redis
/// event store.
#[async_trait]
pub trait RedisCommandExecutor: Clone + Send + Sync + 'static {
    /// Executor-specific error type.
    type Error: Display + Send + Sync + 'static;

    /// Executes one Redis command with already encoded binary arguments.
    async fn execute(&self, command: &str, args: Vec<Vec<u8>>) -> Result<RedisValue, Self::Error>;

    /// Publishes a notification payload to a Redis channel.
    async fn publish(&self, channel: &str, payload: &[u8]) -> Result<(), Self::Error> {
        let _ = self
            .execute(
                "PUBLISH",
                vec![channel.as_bytes().to_vec(), payload.to_vec()],
            )
            .await?;
        Ok(())
    }
}

/// Experimental Redis-backed async event store.
///
/// This adapter is intentionally not a sync [`crate::EventStore`]
/// implementation. Redis host APIs used by Spin and the WASI example are
/// async, so the stable surface for this backend is [`AsyncEventStore`].
pub struct RedisEventStore<A, C>
where
    A: Aggregate + Send + Sync,
    C: RedisCommandExecutor,
{
    client: C,
    prefix: String,
    upcasters: UpcasterRegistry,
    _marker: PhantomData<fn() -> A>,
}

impl<A, C> Clone for RedisEventStore<A, C>
where
    A: Aggregate + Send + Sync,
    C: RedisCommandExecutor,
{
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            prefix: self.prefix.clone(),
            upcasters: self.upcasters.clone(),
            _marker: PhantomData,
        }
    }
}

impl<A, C> std::fmt::Debug for RedisEventStore<A, C>
where
    A: Aggregate + Send + Sync,
    C: RedisCommandExecutor,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisEventStore")
            .field("prefix", &self.prefix)
            .finish_non_exhaustive()
    }
}

impl<A, C> RedisEventStore<A, C>
where
    A: Aggregate + Send + Sync,
    C: RedisCommandExecutor,
{
    /// Creates a Redis event store with the default `ddd_cqrs_es` key prefix.
    pub fn new(client: C) -> Self {
        Self {
            client,
            prefix: DEFAULT_PREFIX.to_owned(),
            upcasters: UpcasterRegistry::new(),
            _marker: PhantomData,
        }
    }

    /// Creates a Redis event store with a custom key prefix.
    pub fn with_prefix(client: C, prefix: impl Into<String>) -> Result<Self, EventStoreError> {
        let prefix = prefix.into();
        validate_redis_prefix(&prefix)?;

        Ok(Self {
            client,
            prefix,
            upcasters: UpcasterRegistry::new(),
            _marker: PhantomData,
        })
    }

    /// Returns the Redis command executor.
    pub fn client(&self) -> &C {
        &self.client
    }

    /// Returns the key prefix used by this store.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Returns the upcaster registry.
    pub fn upcasters(&self) -> &UpcasterRegistry {
        &self.upcasters
    }

    /// Registers a sequential schema version upcaster for a specific event type.
    pub fn register_upcaster<U>(&self, event_type: impl Into<String>, upcaster: U)
    where
        U: crate::upcast::EventUpcaster + Send + Sync + 'static,
        U::Error: std::fmt::Debug + Display + Send + Sync + 'static,
    {
        self.upcasters.register(event_type, upcaster);
    }

    fn event_key_prefix(&self) -> String {
        format!("{}:event:", self.prefix)
    }

    fn sequence_key(&self) -> String {
        format!("{}:seq", self.prefix)
    }

    fn global_key(&self) -> String {
        format!("{}:global", self.prefix)
    }

    fn event_key(&self, sequence: u64) -> String {
        format!("{}{}", self.event_key_prefix(), sequence)
    }

    fn stream_keys(&self, aggregate_id: &A::Id) -> Result<RedisStreamKeys, EventStoreError>
    where
        A::Id: serde::Serialize,
    {
        let aggregate_id_json = serialize_id(aggregate_id)?;
        let aggregate_type_key = hex_encode(A::aggregate_type().as_bytes());
        let aggregate_id_key = hex_encode(aggregate_id_json.as_bytes());

        Ok(RedisStreamKeys {
            aggregate_id_json,
            revision_key: format!(
                "{}:revision:{}:{}",
                self.prefix, aggregate_type_key, aggregate_id_key
            ),
            stream_key: format!(
                "{}:stream:{}:{}",
                self.prefix, aggregate_type_key, aggregate_id_key
            ),
        })
    }

    async fn current_revision(&self, revision_key: &str) -> Result<u64, EventStoreError> {
        let value = self
            .client
            .execute("GET", vec![revision_key.as_bytes().to_vec()])
            .await
            .map_err(map_executor_error)?;
        redis_optional_u64(&value, "stream revision")
    }

    /// Fetches the event hashes for `sequences`, at most
    /// [`HASH_FETCH_CHUNK_SIZE`] keys per round trip.
    ///
    /// The reply order matches `sequences`; every sequence must exist or the
    /// load fails (an indexed-but-missing event is store corruption).
    ///
    /// Chunking bounds each RESP reply so a long stream or replay backlog stays
    /// readable; an unchunked fetch grew the reply with the event count and
    /// tripped the raw client's array-length ceiling past roughly 47_600
    /// events.
    ///
    /// The batched multi-key script requires all event keys to hash to one
    /// Redis Cluster slot (or a non-cluster deployment); CROSSSLOT errors
    /// surface from clustered proxies where per-key HGETALL previously worked.
    async fn load_sequence_hashes(
        &self,
        sequences: &[u64],
    ) -> Result<Vec<BTreeMap<String, Vec<u8>>>, EventStoreError> {
        let mut hashes = Vec::with_capacity(sequences.len());

        for chunk in sequences.chunks(HASH_FETCH_CHUNK_SIZE) {
            let mut args = Vec::with_capacity(chunk.len() + 2);
            args.push(FETCH_HASHES_LUA.as_bytes().to_vec());
            args.push(chunk.len().to_string().into_bytes());
            for sequence in chunk {
                args.push(self.event_key(*sequence).into_bytes());
            }
            let value = self
                .client
                .execute("EVAL", args)
                .await
                .map_err(map_executor_error)?;

            hashes.extend(unpack_flat_hash_batch(&value, chunk)?);
        }

        Ok(hashes)
    }

    /// Reads member sequences from a sorted-set index in ascending score order,
    /// paging at [`SEQUENCE_PAGE_SIZE`] members per round trip.
    ///
    /// `after_score` is exclusive, and `limit` caps the total member count;
    /// `None` drains the index. Paging advances by score, which is exact here
    /// because both indexes this store writes score members uniquely: the
    /// global index scores each sequence by itself, and a stream index scores
    /// each of its sequences by that event's stream revision.
    ///
    /// Paging is not a consistent snapshot — appends committed between pages
    /// are picked up, which is what a replay cursor wants.
    async fn load_indexed_sequences(
        &self,
        index_key: &str,
        after_score: u64,
        limit: Option<usize>,
    ) -> Result<Vec<u64>, EventStoreError> {
        let mut sequences = Vec::new();
        let mut cursor = after_score;

        loop {
            let page_size = match limit {
                Some(limit) => match limit.saturating_sub(sequences.len()) {
                    0 => return Ok(sequences),
                    remaining => remaining.min(SEQUENCE_PAGE_SIZE),
                },
                None => SEQUENCE_PAGE_SIZE,
            };

            let value = self
                .client
                .execute(
                    "ZRANGEBYSCORE",
                    vec![
                        index_key.as_bytes().to_vec(),
                        format!("({cursor}").into_bytes(),
                        b"+inf".to_vec(),
                        b"WITHSCORES".to_vec(),
                        b"LIMIT".to_vec(),
                        b"0".to_vec(),
                        page_size.to_string().into_bytes(),
                    ],
                )
                .await
                .map_err(map_executor_error)?;

            let page = redis_scored_sequence_page(&value)?;
            let page_len = page.len();
            let highest_score = page.last().map(|(_, score)| *score);
            sequences.extend(page.into_iter().map(|(member, _)| member));

            if page_len < page_size {
                return Ok(sequences);
            }
            // A full page that did not move the score cursor would be re-read
            // forever; stop instead of looping on it.
            match highest_score {
                Some(score) if score > cursor => cursor = score,
                _ => return Ok(sequences),
            }
        }
    }
}

#[async_trait]
impl<A, C> AsyncEventStore<A> for RedisEventStore<A, C>
where
    A: Aggregate + Send + Sync + 'static,
    A::Event: serde::Serialize + serde::de::DeserializeOwned + Send + Sync,
    A::Id: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + Clone,
    C: RedisCommandExecutor,
{
    type Error = EventStoreError;

    async fn load(&self, aggregate_id: &A::Id) -> Result<EventStream<A>, Self::Error> {
        let keys = self.stream_keys(aggregate_id)?;
        // Stream members are scored by revision, which starts at 1, so an
        // exclusive floor of 0 covers the whole stream.
        let sequences = self
            .load_indexed_sequences(&keys.stream_key, 0, None)
            .await?;
        let hashes = self.load_sequence_hashes(&sequences).await?;
        let mut events = Vec::with_capacity(hashes.len());
        for hash in hashes {
            if hash_field_string(&hash, "aggregate_type")? == A::aggregate_type() {
                events.push(hash_to_envelope::<A>(&self.upcasters, hash)?);
            }
        }

        Ok(events)
    }

    async fn load_after_revision(
        &self,
        aggregate_id: &A::Id,
        revision: u64,
    ) -> Result<EventStream<A>, Self::Error> {
        let keys = self.stream_keys(aggregate_id)?;
        let sequences = self
            .load_indexed_sequences(&keys.stream_key, revision, None)
            .await?;
        let hashes = self.load_sequence_hashes(&sequences).await?;
        let mut events = Vec::with_capacity(hashes.len());
        for hash in hashes {
            if hash_field_string(&hash, "aggregate_type")? == A::aggregate_type() {
                events.push(hash_to_envelope::<A>(&self.upcasters, hash)?);
            }
        }

        Ok(events)
    }

    async fn append(
        &self,
        aggregate_id: &A::Id,
        expected_revision: ExpectedRevision,
        events: Vec<NewEvent<A::Event>>,
    ) -> Result<EventStream<A>, Self::Error> {
        let keys = self.stream_keys(aggregate_id)?;
        let prepared = events
            .into_iter()
            .map(PreparedRedisEvent::new)
            .collect::<Result<Vec<_>, _>>()?;

        if prepared.is_empty() {
            let actual = self.current_revision(&keys.revision_key).await?;
            check_expected_revision(expected_revision, actual)?;
            return Ok(Vec::new());
        }

        let sequence_key = self.sequence_key();
        let global_key = self.global_key();
        let event_key_prefix = self.event_key_prefix();
        let args = build_append_eval_args(AppendEvalArgs {
            script: APPEND_LUA,
            aggregate_type: A::aggregate_type(),
            keys: &keys,
            sequence_key: &sequence_key,
            global_key: &global_key,
            event_key_prefix: &event_key_prefix,
            expected_revision,
            events: &prepared,
        });
        let value = self
            .client
            .execute("EVAL", args)
            .await
            .map_err(map_executor_error)?;
        let AppendEvalResult {
            first_sequence,
            next_revision,
            ..
        } = parse_append_eval_result(&value, expected_revision)?;
        let base_revision = next_revision
            .checked_sub(prepared.len() as u64)
            .ok_or_else(|| {
                EventStoreError::deserialization(
                    "Redis append script returned revision smaller than event count".to_owned(),
                )
            })?;

        let mut committed = Vec::with_capacity(prepared.len());
        for (index, event) in prepared.into_iter().enumerate() {
            let revision = base_revision + index as u64 + 1;
            let sequence = first_sequence + index as u64;
            committed.push(EventEnvelope::new(
                event.event_id,
                aggregate_id.clone(),
                A::aggregate_type(),
                revision,
                Some(sequence),
                event.event_type,
                event.event_version,
                event.payload,
                event.metadata,
                event.recorded_at,
            ));
        }

        Ok(committed)
    }

    async fn load_global_after(
        &self,
        sequence: Option<u64>,
    ) -> Result<EventStream<A>, Self::Error> {
        let sequences = self
            .load_indexed_sequences(&self.global_key(), sequence.unwrap_or_default(), None)
            .await?;
        let hashes = self.load_sequence_hashes(&sequences).await?;
        let mut events = Vec::with_capacity(hashes.len());
        for hash in hashes {
            events.push(hash_to_envelope::<A>(&self.upcasters, hash)?);
        }

        Ok(events)
    }

    async fn load_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: NonZeroUsize,
    ) -> Result<EventStream<A>, Self::Error> {
        let sequences = self
            .load_indexed_sequences(
                &self.global_key(),
                sequence.unwrap_or_default(),
                Some(limit.get()),
            )
            .await?;
        let hashes = self.load_sequence_hashes(&sequences).await?;
        let mut events = Vec::with_capacity(hashes.len());
        for hash in hashes {
            events.push(hash_to_envelope::<A>(&self.upcasters, hash)?);
        }

        Ok(events)
    }
}

#[async_trait]
impl<A, C> crate::raw_feed::AsyncRawEventFeed for RedisEventStore<A, C>
where
    A: Aggregate + Send + Sync + 'static,
    A::Event: serde::Serialize + serde::de::DeserializeOwned + Send + Sync,
    A::Id: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + Clone,
    C: RedisCommandExecutor,
{
    type Error = EventStoreError;

    /// Serves every event under this store's key prefix in global sequence
    /// order. Stores sharing one prefix share the global feed, so this
    /// interleaves aggregate types persisted under that prefix.
    async fn load_raw_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: NonZeroUsize,
    ) -> Result<Vec<crate::raw_feed::RawEventEnvelope>, Self::Error> {
        let sequences = self
            .load_indexed_sequences(
                &self.global_key(),
                sequence.unwrap_or_default(),
                Some(limit.get()),
            )
            .await?;
        let hashes = self.load_sequence_hashes(&sequences).await?;
        let mut events = Vec::with_capacity(hashes.len());
        for hash in hashes {
            events.push(hash_to_raw_envelope(&self.upcasters, hash)?);
        }

        Ok(events)
    }
}

/// Experimental Redis-backed async checkpoint store.
#[derive(Clone, Debug)]
pub struct RedisCheckpointStore<C>
where
    C: RedisCommandExecutor,
{
    client: C,
    prefix: String,
}

impl<C> RedisCheckpointStore<C>
where
    C: RedisCommandExecutor,
{
    /// Creates a Redis checkpoint store with the default `ddd_cqrs_es` prefix.
    pub fn new(client: C) -> Self {
        Self {
            client,
            prefix: DEFAULT_CHECKPOINT_PREFIX.to_owned(),
        }
    }

    /// Creates a Redis checkpoint store with a custom key prefix.
    pub fn with_prefix(client: C, prefix: impl Into<String>) -> Result<Self, EventStoreError> {
        let prefix = prefix.into();
        validate_redis_prefix(&prefix)?;

        Ok(Self { client, prefix })
    }

    fn checkpoint_key(&self, projection_name: &str) -> String {
        format!(
            "{}:checkpoint:{}",
            self.prefix,
            hex_encode(projection_name.as_bytes())
        )
    }
}

#[async_trait]
impl<C> AsyncCheckpointStore for RedisCheckpointStore<C>
where
    C: RedisCommandExecutor,
{
    type Error = EventStoreError;

    async fn load_checkpoint(&self, projection_name: &str) -> Result<Option<u64>, Self::Error> {
        let value = self
            .client
            .execute(
                "GET",
                vec![self.checkpoint_key(projection_name).into_bytes()],
            )
            .await
            .map_err(map_executor_error)?;
        let sequence = redis_optional_u64(&value, "projection checkpoint")?;
        Ok((sequence != 0).then_some(sequence))
    }

    async fn save_checkpoint(
        &self,
        projection_name: &str,
        sequence: u64,
    ) -> Result<(), Self::Error> {
        let _ = self
            .client
            .execute(
                "EVAL",
                vec![
                    CHECKPOINT_SAVE_LUA.as_bytes().to_vec(),
                    "1".into(),
                    self.checkpoint_key(projection_name).into_bytes(),
                    sequence.to_string().into_bytes(),
                ],
            )
            .await
            .map_err(map_executor_error)?;
        Ok(())
    }
}

/// Redis notification publisher for read-model/realtime invalidation.
///
/// Publishing is best-effort notification only. Callers should commit events
/// and update projections first, then publish a message to wake clients.
#[derive(Clone, Debug)]
pub struct RedisPubSubPublisher<C>
where
    C: RedisCommandExecutor,
{
    client: C,
    channel: String,
}

impl<C> RedisPubSubPublisher<C>
where
    C: RedisCommandExecutor,
{
    /// Creates a publisher for one Redis channel.
    pub fn new(client: C, channel: impl Into<String>) -> Self {
        Self {
            client,
            channel: channel.into(),
        }
    }

    /// Returns the configured Redis channel.
    pub fn channel(&self) -> &str {
        &self.channel
    }

    /// Publishes a raw notification payload.
    pub async fn publish(&self, payload: &[u8]) -> Result<(), EventStoreError> {
        self.client
            .publish(&self.channel, payload)
            .await
            .map_err(map_executor_error)
    }

    /// Publishes a JSON-serialized notification payload.
    pub async fn publish_json<T>(&self, value: &T) -> Result<(), EventStoreError>
    where
        T: serde::Serialize + Sync,
    {
        let payload = serde_json::to_vec(value)
            .map_err(|error| EventStoreError::serialization(error.to_string()))?;
        self.publish(&payload).await
    }
}

/// Spin SDK Redis command executor.
///
/// The established host connection is cached per client (clones share it), so
/// repeated commands skip the open handshake. A command that fails drops the
/// cached connection and surfaces the error; the next command reopens.
#[cfg(feature = "spin-redis")]
#[derive(Clone)]
pub struct SpinRedisClient {
    url: String,
    cached_connection: Arc<StdMutex<Option<spin_sdk::redis::Connection>>>,
}

#[cfg(feature = "spin-redis")]
impl std::fmt::Debug for SpinRedisClient {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpinRedisClient")
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "spin-redis")]
impl SpinRedisClient {
    /// Creates a Spin Redis client for a Redis URL.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            cached_connection: Arc::new(StdMutex::new(None)),
        }
    }

    /// Returns the Redis URL.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Takes the cached connection or opens a new one.
    async fn connection(&self) -> Result<spin_sdk::redis::Connection, SpinRedisError> {
        let cached = self
            .cached_connection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        match cached {
            Some(connection) => Ok(connection),
            None => Ok(spin_sdk::redis::Connection::open(&self.url).await?),
        }
    }

    /// Returns a connection to the cache after a successful command.
    fn store_connection(&self, connection: spin_sdk::redis::Connection) {
        *self
            .cached_connection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(connection);
    }
}

/// Error returned by [`SpinRedisClient`].
#[cfg(feature = "spin-redis")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpinRedisError(String);

#[cfg(feature = "spin-redis")]
impl Display for SpinRedisError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(feature = "spin-redis")]
impl std::error::Error for SpinRedisError {}

#[cfg(feature = "spin-redis")]
impl From<spin_sdk::redis::Error> for SpinRedisError {
    fn from(value: spin_sdk::redis::Error) -> Self {
        Self(format!("{value:?}"))
    }
}

#[cfg(feature = "spin-redis")]
#[async_trait]
impl RedisCommandExecutor for SpinRedisClient {
    type Error = SpinRedisError;

    async fn execute(&self, command: &str, args: Vec<Vec<u8>>) -> Result<RedisValue, Self::Error> {
        let connection = self.connection().await?;
        let args = args
            .into_iter()
            .map(spin_sdk::redis::RedisParameter::Binary)
            .collect::<Vec<_>>();
        let values = connection.execute(command, args).await?;
        self.store_connection(connection);
        Ok(RedisValue::Array(
            values.into_iter().map(spin_result_to_value).collect(),
        ))
    }

    async fn publish(&self, channel: &str, payload: &[u8]) -> Result<(), Self::Error> {
        let connection = self.connection().await?;
        connection.publish(channel, payload).await?;
        self.store_connection(connection);
        Ok(())
    }
}

/// Minimal raw RESP Redis command executor for generic WASI/Wasmtime.
///
/// This client supports plain `redis://` TCP URLs. It is deliberately small and
/// does not implement TLS, Sentinel, Cluster, or RESP3-specific behavior.
///
/// # Connection reuse
///
/// Commands reuse one cached TCP connection per client (clones share it), so
/// repeated commands skip the connect/AUTH/SELECT handshake. Reuse is guarded
/// conservatively:
///
/// - before reuse, a non-blocking probe discards connections the server has
///   closed while idle;
/// - a command whose write fails on the cached connection is retried once on a
///   fresh connection (the incomplete command was never executable);
/// - any failure after the command was fully written surfaces to the caller
///   and discards the connection, because the server may have executed it —
///   retrying is left to the caller's idempotency/concurrency handling;
/// - a connection is returned to the cache only after a complete
///   command/reply cycle with no trailing buffered bytes.
///
/// Commands on one client are serialized; use separately constructed clients
/// for independent parallel connections.
#[cfg(feature = "wasi-redis")]
#[derive(Clone, Debug)]
pub struct WasiRedisClient {
    url: String,
    read_timeout: Option<Duration>,
    nonblocking_subscription_reads: bool,
    cached_connection: Arc<StdMutex<Option<BufReader<TcpStream>>>>,
}

#[cfg(feature = "wasi-redis")]
impl WasiRedisClient {
    /// Creates a raw RESP Redis client for a `redis://` URL.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            read_timeout: Some(Duration::from_secs(5)),
            nonblocking_subscription_reads: false,
            cached_connection: Arc::new(StdMutex::new(None)),
        }
    }

    /// Sets the read timeout used by newly opened TCP connections.
    ///
    /// The cached connection is discarded so the next command opens a
    /// connection with the new timeout.
    pub fn with_read_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.read_timeout = timeout;
        self.clear_cached_connection();
        self
    }

    /// Configures subscriptions opened by this client to use nonblocking socket
    /// reads after the initial `SUBSCRIBE` acknowledgement is received.
    pub fn with_nonblocking_subscription_reads(mut self, enabled: bool) -> Self {
        self.nonblocking_subscription_reads = enabled;
        self
    }

    /// Returns the Redis URL.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Subscribes to one Redis channel using a blocking raw RESP connection.
    pub fn subscribe(&self, channel: &str) -> Result<WasiRedisSubscription, RedisClientError> {
        let mut reader = self.open_reader()?;
        write_command(
            reader.get_mut(),
            "SUBSCRIBE",
            &[channel.as_bytes().to_vec()],
        )?;
        let _ = read_resp_value(&mut reader)?;
        if self.nonblocking_subscription_reads {
            reader.get_mut().set_nonblocking(true)?;
        }

        Ok(WasiRedisSubscription {
            channel: channel.to_owned(),
            reader,
        })
    }

    fn open_reader(&self) -> Result<BufReader<TcpStream>, RedisClientError> {
        let address = RedisAddress::parse(&self.url)?;
        let stream = TcpStream::connect((address.host.as_str(), address.port))?;
        stream.set_read_timeout(self.read_timeout)?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let mut reader = BufReader::new(stream);

        if let Some(password) = &address.password {
            let mut args = Vec::new();
            if let Some(username) = &address.username {
                args.push(username.as_bytes().to_vec());
            }
            args.push(password.as_bytes().to_vec());
            write_command(reader.get_mut(), "AUTH", &args)?;
            expect_ok(read_resp_value(&mut reader)?, "AUTH")?;
        }

        if let Some(db) = address.db {
            write_command(reader.get_mut(), "SELECT", &[db.to_string().into_bytes()])?;
            expect_ok(read_resp_value(&mut reader)?, "SELECT")?;
        }

        Ok(reader)
    }

    fn clear_cached_connection(&self) {
        *self
            .cached_connection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }

    /// Takes the cached connection when it still looks healthy.
    fn take_cached_connection(&self) -> Option<BufReader<TcpStream>> {
        let reader = self
            .cached_connection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()?;
        cached_connection_is_reusable(&reader).then_some(reader)
    }

    /// Returns a connection to the cache after a complete command/reply cycle.
    ///
    /// A reader with buffered bytes left over is out of protocol sync and is
    /// dropped instead of cached.
    fn store_cached_connection(&self, reader: BufReader<TcpStream>) {
        if !reader.buffer().is_empty() {
            return;
        }
        *self
            .cached_connection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(reader);
    }
}

/// Probes an idle cached connection without blocking.
///
/// A healthy idle connection has nothing to read, so a readable socket means
/// the server closed it (EOF) or left desync bytes behind; both are discarded.
/// Runtimes without non-blocking or peek support skip the probe and reuse the
/// connection — write failures still fall back to a fresh connection.
#[cfg(feature = "wasi-redis")]
fn cached_connection_is_reusable(reader: &BufReader<TcpStream>) -> bool {
    let stream = reader.get_ref();
    if let Err(error) = stream.set_nonblocking(true) {
        return error.kind() == ErrorKind::Unsupported;
    }
    let mut probe = [0_u8; 1];
    let healthy = match stream.peek(&mut probe) {
        // EOF or stray reply bytes: the connection is dead or desynced.
        Ok(_) => false,
        Err(error) => matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Unsupported),
    };
    healthy && stream.set_nonblocking(false).is_ok()
}

#[cfg(feature = "wasi-redis")]
#[async_trait]
impl RedisCommandExecutor for WasiRedisClient {
    type Error = RedisClientError;

    async fn execute(&self, command: &str, args: Vec<Vec<u8>>) -> Result<RedisValue, Self::Error> {
        if let Some(mut reader) = self.take_cached_connection() {
            match write_command(reader.get_mut(), command, &args) {
                Ok(()) => {
                    let result = read_resp_value(&mut reader);
                    if result.is_ok() {
                        self.store_cached_connection(reader);
                    }
                    // A failed read is not retried: the fully written command
                    // may have executed server-side. The connection is dropped
                    // and the error surfaces to the caller.
                    return result;
                }
                Err(RedisClientError::Io(_) | RedisClientError::Timeout) => {
                    // The cached connection went stale before a complete
                    // command reached the server; retry once on a fresh one.
                }
                Err(error) => return Err(error),
            }
        }

        let mut reader = self.open_reader()?;
        write_command(reader.get_mut(), command, &args)?;
        let result = read_resp_value(&mut reader);
        if result.is_ok() {
            self.store_cached_connection(reader);
        }
        result
    }
}

/// Blocking Redis subscription reader returned by [`WasiRedisClient::subscribe`].
#[cfg(feature = "wasi-redis")]
#[derive(Debug)]
pub struct WasiRedisSubscription {
    channel: String,
    reader: BufReader<TcpStream>,
}

#[cfg(feature = "wasi-redis")]
impl WasiRedisSubscription {
    /// Returns the subscribed channel.
    pub fn channel(&self) -> &str {
        &self.channel
    }

    /// Reads the next published message payload.
    pub fn next_message(&mut self) -> Result<Vec<u8>, RedisClientError> {
        loop {
            let value = read_resp_value(&mut self.reader)?;
            let RedisValue::Array(items) = value else {
                continue;
            };
            if items.len() < 3 {
                continue;
            }
            let kind = redis_value_string(&items[0], "subscription message kind")
                .map_err(|error| RedisClientError::Protocol(error.to_string()))?;
            if kind == "message" {
                return redis_value_bytes(&items[2], "subscription payload")
                    .map_err(|error| RedisClientError::Protocol(error.to_string()));
            }
        }
    }

    /// Reads the next message, returning `Ok(None)` when the configured socket
    /// timeout expires before a message arrives.
    pub fn try_next_message(&mut self) -> Result<Option<Vec<u8>>, RedisClientError> {
        match self.next_message() {
            Ok(message) => Ok(Some(message)),
            Err(RedisClientError::Timeout) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

/// Error returned by the raw RESP Redis client.
#[cfg(feature = "wasi-redis")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RedisClientError {
    /// Redis URL could not be parsed.
    InvalidUrl(String),
    /// TCP or stream I/O failed.
    Io(String),
    /// A blocking socket read timed out before Redis produced a response.
    Timeout,
    /// Redis returned an error response.
    Redis(String),
    /// RESP protocol data was malformed or unexpected.
    Protocol(String),
}

#[cfg(feature = "wasi-redis")]
impl Display for RedisClientError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RedisClientError::InvalidUrl(message) => write!(f, "invalid Redis URL: {message}"),
            RedisClientError::Io(message) => write!(f, "Redis I/O error: {message}"),
            RedisClientError::Timeout => f.write_str("Redis read timed out"),
            RedisClientError::Redis(message) => write!(f, "Redis error: {message}"),
            RedisClientError::Protocol(message) => write!(f, "Redis protocol error: {message}"),
        }
    }
}

#[cfg(feature = "wasi-redis")]
impl std::error::Error for RedisClientError {}

#[cfg(feature = "wasi-redis")]
impl From<std::io::Error> for RedisClientError {
    fn from(value: std::io::Error) -> Self {
        match value.kind() {
            ErrorKind::TimedOut | ErrorKind::WouldBlock => RedisClientError::Timeout,
            _ => RedisClientError::Io(value.to_string()),
        }
    }
}

#[derive(Clone, Debug)]
struct RedisStreamKeys {
    aggregate_id_json: String,
    revision_key: String,
    stream_key: String,
}

#[derive(Clone, Debug)]
struct PreparedRedisEvent<E> {
    event_id: EventId,
    event_type: String,
    event_version: u32,
    payload: E,
    payload_json: Vec<u8>,
    metadata: crate::Metadata,
    metadata_json: Vec<u8>,
    recorded_at: SystemTime,
    recorded_at_ms: i64,
}

impl<E> PreparedRedisEvent<E>
where
    E: serde::Serialize,
{
    fn new(event: NewEvent<E>) -> Result<Self, EventStoreError> {
        let event_id = EventId::new();
        let recorded_at = SystemTime::now();
        let recorded_at_ms = system_time_to_millis(recorded_at)?;
        let payload_json = serde_json::to_vec(&serialize_payload(&event.payload)?)
            .map_err(|error| EventStoreError::serialization(format!("event payload: {error}")))?;
        let metadata_json = serde_json::to_vec(&serialize_metadata(&event.metadata)?)
            .map_err(|error| EventStoreError::serialization(format!("metadata: {error}")))?;

        Ok(Self {
            event_id,
            event_type: event.event_type.into_string(),
            event_version: event.event_version,
            payload: event.payload,
            payload_json,
            metadata: event.metadata,
            metadata_json,
            recorded_at,
            recorded_at_ms,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AppendEvalResult {
    first_sequence: u64,
    last_sequence: u64,
    next_revision: u64,
}

struct AppendEvalArgs<'a, E> {
    script: &'a str,
    aggregate_type: &'a str,
    keys: &'a RedisStreamKeys,
    sequence_key: &'a str,
    global_key: &'a str,
    event_key_prefix: &'a str,
    expected_revision: ExpectedRevision,
    events: &'a [PreparedRedisEvent<E>],
}

fn build_append_eval_args<E>(input: AppendEvalArgs<'_, E>) -> Vec<Vec<u8>> {
    let (expected_kind, expected_value) = expected_revision_arg(input.expected_revision);
    let mut args = vec![
        input.script.as_bytes().to_vec(),
        b"4".to_vec(),
        input.keys.revision_key.as_bytes().to_vec(),
        input.sequence_key.as_bytes().to_vec(),
        input.keys.stream_key.as_bytes().to_vec(),
        input.global_key.as_bytes().to_vec(),
        expected_kind.as_bytes().to_vec(),
        expected_value.to_string().into_bytes(),
        input.events.len().to_string().into_bytes(),
        input.event_key_prefix.as_bytes().to_vec(),
    ];

    for event in input.events {
        args.push(event.event_id.as_str().as_bytes().to_vec());
        args.push(input.keys.aggregate_id_json.as_bytes().to_vec());
        args.push(input.aggregate_type.as_bytes().to_vec());
        args.push(event.event_type.as_bytes().to_vec());
        args.push(event.event_version.to_string().into_bytes());
        args.push(event.payload_json.clone());
        args.push(event.metadata_json.clone());
        args.push(event.recorded_at_ms.to_string().into_bytes());
    }

    args
}

fn expected_revision_arg(expected_revision: ExpectedRevision) -> (&'static str, u64) {
    match expected_revision {
        ExpectedRevision::Any => ("any", 0),
        ExpectedRevision::NoStream => ("no_stream", 0),
        ExpectedRevision::Exact(revision) => ("exact", revision),
    }
}

fn parse_append_eval_result(
    value: &RedisValue,
    expected: ExpectedRevision,
) -> Result<AppendEvalResult, EventStoreError> {
    let items = redis_array_items(value)?;
    if items.len() < 3 {
        return Err(EventStoreError::deserialization(
            "Redis append script returned too few fields".to_owned(),
        ));
    }

    let status = redis_value_string(&items[0], "append script status")?;
    match status.as_str() {
        "OK" => {
            if items.len() < 4 {
                return Err(EventStoreError::deserialization(
                    "Redis append script returned too few success fields".to_owned(),
                ));
            }
            Ok(AppendEvalResult {
                first_sequence: redis_value_u64(&items[1], "append first sequence")?,
                last_sequence: redis_value_u64(&items[2], "append last sequence")?,
                next_revision: redis_value_u64(&items[3], "append next revision")?,
            })
        }
        "ERR" => {
            let reason = redis_value_string(&items[1], "append error reason")?;
            let actual = redis_value_u64(&items[2], "append actual revision")?;
            match reason.as_str() {
                "stream_exists" => Err(EventStoreError::Concurrency(
                    crate::ConcurrencyError::StreamAlreadyExists,
                )),
                "wrong_revision" => Err(EventStoreError::Concurrency(
                    crate::ConcurrencyError::WrongExpectedRevision { expected, actual },
                )),
                _ => Err(EventStoreError::backend(format!(
                    "Redis append script failed: {reason}"
                ))),
            }
        }
        _ => Err(EventStoreError::deserialization(format!(
            "unknown Redis append status `{status}`"
        ))),
    }
}

fn hash_to_envelope<A>(
    upcasters: &UpcasterRegistry,
    hash: BTreeMap<String, Vec<u8>>,
) -> Result<EventEnvelope<A::Event, A::Id>, EventStoreError>
where
    A: Aggregate,
    A::Event: serde::de::DeserializeOwned,
    A::Id: serde::de::DeserializeOwned,
{
    let event_id = hash_field_string(&hash, "event_id")?;
    let aggregate_id_json = hash_field_string(&hash, "aggregate_id")?;
    let aggregate_type = hash_field_string(&hash, "aggregate_type")?;
    let revision = hash_field_u64(&hash, "revision")?;
    let sequence = hash_field_u64(&hash, "sequence")?;
    let event_type = hash_field_string(&hash, "event_type")?;
    let event_version = hash_field_u32(&hash, "event_version")?;
    let payload_bytes = hash_field_bytes(&hash, "payload")?;
    let metadata_bytes = hash_field_bytes(&hash, "metadata")?;
    let recorded_at_ms = hash_field_i64(&hash, "recorded_at_ms")?;

    let aggregate_id = deserialize_id(&aggregate_id_json)?;
    let (event_version, upcasted_bytes) = upcasters
        .prepare_payload(&event_type, event_version, payload_bytes)
        .map_err(|error| EventStoreError::deserialization(error.to_string()))?;
    let payload_value = serde_json::from_slice(&upcasted_bytes)
        .map_err(|error| EventStoreError::deserialization(format!("payload JSON: {error}")))?;
    let payload = deserialize_payload(&event_id, &event_type, payload_value)?;
    let metadata_value = serde_json::from_slice(&metadata_bytes)
        .map_err(|error| EventStoreError::deserialization(format!("metadata JSON: {error}")))?;
    let metadata = deserialize_metadata(&event_id, metadata_value)?;
    let recorded_at = millis_to_system_time(recorded_at_ms)?;

    Ok(EventEnvelope::new(
        EventId::from_string(event_id),
        aggregate_id,
        aggregate_type,
        revision,
        Some(sequence),
        event_type,
        event_version,
        payload,
        metadata,
        recorded_at,
    ))
}

/// Maps a stored event hash into an untyped envelope, applying upcasters but
/// keeping the payload as raw JSON and the aggregate id as its stored string.
fn hash_to_raw_envelope(
    upcasters: &UpcasterRegistry,
    hash: BTreeMap<String, Vec<u8>>,
) -> Result<crate::raw_feed::RawEventEnvelope, EventStoreError> {
    let event_id = hash_field_string(&hash, "event_id")?;
    let aggregate_id_json = hash_field_string(&hash, "aggregate_id")?;
    let aggregate_type = hash_field_string(&hash, "aggregate_type")?;
    let revision = hash_field_u64(&hash, "revision")?;
    let sequence = hash_field_u64(&hash, "sequence")?;
    let event_type = hash_field_string(&hash, "event_type")?;
    let event_version = hash_field_u32(&hash, "event_version")?;
    let payload_bytes = hash_field_bytes(&hash, "payload")?;
    let metadata_bytes = hash_field_bytes(&hash, "metadata")?;
    let recorded_at_ms = hash_field_i64(&hash, "recorded_at_ms")?;

    let (event_version, upcasted_bytes) = upcasters
        .prepare_payload(&event_type, event_version, payload_bytes)
        .map_err(|error| EventStoreError::deserialization(error.to_string()))?;
    let payload: serde_json::Value = serde_json::from_slice(&upcasted_bytes)
        .map_err(|error| EventStoreError::deserialization(format!("payload JSON: {error}")))?;
    let metadata_value = serde_json::from_slice(&metadata_bytes)
        .map_err(|error| EventStoreError::deserialization(format!("metadata JSON: {error}")))?;
    let metadata = deserialize_metadata(&event_id, metadata_value)?;
    let recorded_at = millis_to_system_time(recorded_at_ms)?;

    Ok(EventEnvelope::new(
        EventId::from_string(event_id),
        aggregate_id_json,
        aggregate_type,
        revision,
        Some(sequence),
        event_type,
        event_version,
        payload,
        metadata,
        recorded_at,
    ))
}

fn validate_redis_prefix(prefix: &str) -> Result<(), EventStoreError> {
    if prefix.is_empty() {
        return Err(EventStoreError::backend(
            "Redis key prefix cannot be empty".to_owned(),
        ));
    }

    if prefix
        .chars()
        .all(|ch| ch == ':' || ch == '_' || ch == '-' || ch.is_ascii_alphanumeric())
    {
        Ok(())
    } else {
        Err(EventStoreError::backend(format!(
            "invalid Redis key prefix `{prefix}`"
        )))
    }
}

fn map_executor_error<E>(error: E) -> EventStoreError
where
    E: Display,
{
    EventStoreError::backend(error.to_string())
}

fn redis_array_items(value: &RedisValue) -> Result<&[RedisValue], EventStoreError> {
    match value {
        RedisValue::Array(items) => Ok(items),
        RedisValue::Nil => Ok(&[]),
        _ => Err(EventStoreError::deserialization(format!(
            "expected Redis array, got {value:?}"
        ))),
    }
}

fn redis_scalar(value: &RedisValue) -> &RedisValue {
    match value {
        RedisValue::Array(items) if items.len() == 1 => &items[0],
        _ => value,
    }
}

fn redis_optional_u64(value: &RedisValue, label: &str) -> Result<u64, EventStoreError> {
    match redis_scalar(value) {
        RedisValue::Nil => Ok(0),
        value => redis_value_u64(value, label),
    }
}

/// Parses one `ZRANGEBYSCORE ... WITHSCORES` page into `(member, score)` pairs.
///
/// Redis returns scores as strings; both indexes this store writes use integer
/// scores (a global sequence or a stream revision), so a fractional score is
/// rejected rather than silently rounded into a paging cursor.
fn redis_scored_sequence_page(value: &RedisValue) -> Result<Vec<(u64, u64)>, EventStoreError> {
    let items = redis_array_items(value)?;
    if !items.len().is_multiple_of(2) {
        return Err(EventStoreError::deserialization(
            "Redis scored index page has an odd element count".to_owned(),
        ));
    }

    items
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            Ok((
                redis_value_u64(&pair[0], "Redis sequence")?,
                redis_value_u64(&pair[1], "Redis index score")?,
            ))
        })
        .collect()
}

fn redis_hash_from_items(
    items: &[RedisValue],
) -> Result<BTreeMap<String, Vec<u8>>, EventStoreError> {
    if !items.len().is_multiple_of(2) {
        return Err(EventStoreError::deserialization(
            "Redis hash reply has odd field count".to_owned(),
        ));
    }

    let mut hash = BTreeMap::new();
    for pair in items.as_chunks::<2>().0 {
        let field = redis_value_string(&pair[0], "Redis hash field")?;
        let value = redis_value_bytes(&pair[1], "Redis hash value")?;
        hash.insert(field, value);
    }

    Ok(hash)
}

/// Unpacks the flat length-prefixed reply of [`FETCH_HASHES_LUA`]: one item
/// count followed by that many field/value entries per requested sequence.
fn unpack_flat_hash_batch(
    value: &RedisValue,
    sequences: &[u64],
) -> Result<Vec<BTreeMap<String, Vec<u8>>>, EventStoreError> {
    let items = redis_array_items(value)?;
    let mut cursor = 0_usize;
    let mut hashes = Vec::with_capacity(sequences.len());

    for sequence in sequences {
        let Some(count) = items.get(cursor) else {
            return Err(EventStoreError::deserialization(format!(
                "Redis hash batch ended after {} of {} events",
                hashes.len(),
                sequences.len()
            )));
        };
        let RedisValue::Int(count) = count else {
            return Err(EventStoreError::deserialization(
                "Redis hash batch length prefix is not an integer".to_owned(),
            ));
        };
        let count = usize::try_from(*count).map_err(|_| {
            EventStoreError::deserialization(
                "Redis hash batch length prefix is negative".to_owned(),
            )
        })?;
        cursor += 1;

        let Some(entries) = items.get(cursor..cursor + count) else {
            return Err(EventStoreError::deserialization(format!(
                "Redis hash batch is truncated at event sequence {sequence}"
            )));
        };
        cursor += count;

        let hash = redis_hash_from_items(entries)?;
        if hash.is_empty() {
            return Err(EventStoreError::deserialization(format!(
                "Redis event sequence {sequence} is indexed but missing"
            )));
        }
        hashes.push(hash);
    }

    if cursor != items.len() {
        return Err(EventStoreError::deserialization(format!(
            "Redis hash batch has {} trailing items after {} events",
            items.len() - cursor,
            sequences.len()
        )));
    }

    Ok(hashes)
}

fn redis_value_string(value: &RedisValue, label: &str) -> Result<String, EventStoreError> {
    let bytes = redis_value_bytes(value, label)?;
    String::from_utf8(bytes).map_err(|error| {
        EventStoreError::deserialization(format!("{label} is not valid UTF-8: {error}"))
    })
}

fn redis_value_bytes(value: &RedisValue, label: &str) -> Result<Vec<u8>, EventStoreError> {
    match value {
        RedisValue::Bytes(bytes) => Ok(bytes.clone()),
        RedisValue::Status(value) => Ok(value.as_bytes().to_vec()),
        RedisValue::Int(value) => Ok(value.to_string().into_bytes()),
        _ => Err(EventStoreError::deserialization(format!(
            "{label}: expected Redis scalar, got {value:?}"
        ))),
    }
}

fn redis_value_u64(value: &RedisValue, label: &str) -> Result<u64, EventStoreError> {
    match value {
        RedisValue::Int(value) => u64::try_from(*value)
            .map_err(|_| EventStoreError::deserialization(format!("{label} cannot be negative"))),
        RedisValue::Bytes(bytes) => {
            let text = std::str::from_utf8(bytes).map_err(|error| {
                EventStoreError::deserialization(format!("{label} is not valid UTF-8: {error}"))
            })?;
            text.parse::<u64>().map_err(|error| {
                EventStoreError::deserialization(format!("{label} is not a u64: {error}"))
            })
        }
        RedisValue::Status(text) => text.parse::<u64>().map_err(|error| {
            EventStoreError::deserialization(format!("{label} is not a u64: {error}"))
        }),
        _ => Err(EventStoreError::deserialization(format!(
            "{label}: expected Redis integer scalar, got {value:?}"
        ))),
    }
}

fn hash_field_bytes(
    hash: &BTreeMap<String, Vec<u8>>,
    field: &str,
) -> Result<Vec<u8>, EventStoreError> {
    hash.get(field).cloned().ok_or_else(|| {
        EventStoreError::deserialization(format!("Redis event hash missing `{field}`"))
    })
}

fn hash_field_string(
    hash: &BTreeMap<String, Vec<u8>>,
    field: &str,
) -> Result<String, EventStoreError> {
    let value = hash_field_bytes(hash, field)?;
    String::from_utf8(value).map_err(|error| {
        EventStoreError::deserialization(format!("Redis event hash `{field}` UTF-8: {error}"))
    })
}

fn hash_field_u64(hash: &BTreeMap<String, Vec<u8>>, field: &str) -> Result<u64, EventStoreError> {
    let value = hash_field_string(hash, field)?;
    value.parse::<u64>().map_err(|error| {
        EventStoreError::deserialization(format!("Redis event hash `{field}` u64: {error}"))
    })
}

fn hash_field_i64(hash: &BTreeMap<String, Vec<u8>>, field: &str) -> Result<i64, EventStoreError> {
    let value = hash_field_string(hash, field)?;
    value.parse::<i64>().map_err(|error| {
        EventStoreError::deserialization(format!("Redis event hash `{field}` i64: {error}"))
    })
}

fn hash_field_u32(hash: &BTreeMap<String, Vec<u8>>, field: &str) -> Result<u32, EventStoreError> {
    let value = hash_field_u64(hash, field)?;
    u32::try_from(value).map_err(|_| {
        EventStoreError::deserialization(format!("Redis event hash `{field}` exceeds u32"))
    })
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(feature = "spin-redis")]
fn spin_result_to_value(value: spin_sdk::redis::RedisResult) -> RedisValue {
    match value {
        spin_sdk::redis::RedisResult::Nil => RedisValue::Nil,
        spin_sdk::redis::RedisResult::Status(value) => RedisValue::Status(value),
        spin_sdk::redis::RedisResult::Int64(value) => RedisValue::Int(value),
        spin_sdk::redis::RedisResult::Binary(value) => RedisValue::Bytes(value),
    }
}

#[cfg(feature = "wasi-redis")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct RedisAddress {
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    db: Option<u32>,
}

#[cfg(feature = "wasi-redis")]
impl RedisAddress {
    fn parse(url: &str) -> Result<Self, RedisClientError> {
        let Some(rest) = url.strip_prefix("redis://") else {
            return Err(RedisClientError::InvalidUrl(
                "only redis:// URLs are supported".to_owned(),
            ));
        };
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let (auth, host_port) = authority
            .rsplit_once('@')
            .map_or((None, authority), |(auth, host_port)| {
                (Some(auth), host_port)
            });

        if host_port.is_empty() {
            return Err(RedisClientError::InvalidUrl("host is required".to_owned()));
        }

        let (host, port) = match host_port.rsplit_once(':') {
            Some((host, port)) => {
                let port = port.parse::<u16>().map_err(|error| {
                    RedisClientError::InvalidUrl(format!("invalid port `{port}`: {error}"))
                })?;
                (host.to_owned(), port)
            }
            None => (host_port.to_owned(), 6379),
        };

        let (username, password) = auth
            .map(|auth| {
                let (username, password) = auth.split_once(':').unwrap_or(("", auth));
                (
                    (!username.is_empty()).then(|| username.to_owned()),
                    (!password.is_empty()).then(|| password.to_owned()),
                )
            })
            .unwrap_or((None, None));
        let db = if path.is_empty() {
            None
        } else {
            Some(path.parse::<u32>().map_err(|error| {
                RedisClientError::InvalidUrl(format!("invalid database `{path}`: {error}"))
            })?)
        };

        Ok(Self {
            host,
            port,
            username,
            password,
            db,
        })
    }
}

#[cfg(feature = "wasi-redis")]
fn write_command(
    stream: &mut TcpStream,
    command: &str,
    args: &[Vec<u8>],
) -> Result<(), RedisClientError> {
    let encoded = encode_resp_command(command, args);
    stream.write_all(&encoded)?;
    stream.flush()?;
    Ok(())
}

#[cfg(feature = "wasi-redis")]
fn encode_resp_command(command: &str, args: &[Vec<u8>]) -> Vec<u8> {
    let mut output = Vec::new();
    output.extend_from_slice(format!("*{}\r\n", args.len() + 1).as_bytes());
    push_bulk(&mut output, command.as_bytes());
    for arg in args {
        push_bulk(&mut output, arg);
    }
    output
}

#[cfg(feature = "wasi-redis")]
fn push_bulk(output: &mut Vec<u8>, bytes: &[u8]) {
    output.extend_from_slice(format!("${}\r\n", bytes.len()).as_bytes());
    output.extend_from_slice(bytes);
    output.extend_from_slice(b"\r\n");
}

#[cfg(feature = "wasi-redis")]
const MAX_RESP_BULK_BYTES: usize = 64 * 1024 * 1024;
#[cfg(feature = "wasi-redis")]
const MAX_RESP_LINE_BYTES: usize = 1024 * 1024;
#[cfg(feature = "wasi-redis")]
const MAX_RESP_ARRAY_LEN: usize = 1_000_000;
#[cfg(feature = "wasi-redis")]
const MAX_RESP_DEPTH: usize = 32;

#[cfg(feature = "wasi-redis")]
fn read_resp_value(reader: &mut impl BufRead) -> Result<RedisValue, RedisClientError> {
    read_resp_value_at_depth(reader, 0)
}

#[cfg(feature = "wasi-redis")]
fn read_resp_value_at_depth(
    reader: &mut impl BufRead,
    depth: usize,
) -> Result<RedisValue, RedisClientError> {
    if depth >= MAX_RESP_DEPTH {
        return Err(RedisClientError::Protocol(format!(
            "RESP nesting exceeds {MAX_RESP_DEPTH} levels"
        )));
    }
    let mut prefix = [0_u8; 1];
    reader.read_exact(&mut prefix)?;
    match prefix[0] {
        b'+' => Ok(RedisValue::Status(read_resp_line(reader)?)),
        b'-' => Err(RedisClientError::Redis(read_resp_line(reader)?)),
        b':' => {
            let line = read_resp_line(reader)?;
            let value = line.parse::<i64>().map_err(|error| {
                RedisClientError::Protocol(format!("invalid integer `{line}`: {error}"))
            })?;
            Ok(RedisValue::Int(value))
        }
        b'$' => {
            let line = read_resp_line(reader)?;
            let len = line.parse::<i64>().map_err(|error| {
                RedisClientError::Protocol(format!("invalid bulk length `{line}`: {error}"))
            })?;
            if len < 0 {
                return Ok(RedisValue::Nil);
            }
            let len = usize::try_from(len)
                .map_err(|_| RedisClientError::Protocol("bulk length exceeds usize".to_owned()))?;
            if len > MAX_RESP_BULK_BYTES {
                return Err(RedisClientError::Protocol(format!(
                    "bulk reply of {len} bytes exceeds the {MAX_RESP_BULK_BYTES} byte limit"
                )));
            }
            let mut bytes = vec![0_u8; len];
            reader.read_exact(&mut bytes)?;
            read_expected_crlf(reader)?;
            Ok(RedisValue::Bytes(bytes))
        }
        b'*' => {
            let line = read_resp_line(reader)?;
            let len = line.parse::<i64>().map_err(|error| {
                RedisClientError::Protocol(format!("invalid array length `{line}`: {error}"))
            })?;
            if len < 0 {
                return Ok(RedisValue::Nil);
            }
            let len = usize::try_from(len)
                .map_err(|_| RedisClientError::Protocol("array length exceeds usize".to_owned()))?;
            if len > MAX_RESP_ARRAY_LEN {
                return Err(RedisClientError::Protocol(format!(
                    "array reply of {len} elements exceeds the {MAX_RESP_ARRAY_LEN} element limit"
                )));
            }
            let mut values = Vec::with_capacity(len.min(1024));
            for _ in 0..len {
                values.push(read_resp_value_at_depth(reader, depth + 1)?);
            }
            Ok(RedisValue::Array(values))
        }
        other => Err(RedisClientError::Protocol(format!(
            "unknown RESP prefix byte `{other}`"
        ))),
    }
}

#[cfg(feature = "wasi-redis")]
fn read_resp_line(reader: &mut impl BufRead) -> Result<String, RedisClientError> {
    // A bounded `read_until` keeps the buffered memchr fast path while still
    // refusing hostile over-long lines: the limit is one past the maximum so
    // a line that fills it is detectably too long.
    let mut line = Vec::new();
    let limit = MAX_RESP_LINE_BYTES as u64 + 1;
    reader.by_ref().take(limit).read_until(b'\n', &mut line)?;
    if line.len() > MAX_RESP_LINE_BYTES {
        return Err(RedisClientError::Protocol(
            "RESP line exceeds the maximum length".to_owned(),
        ));
    }
    if !line.ends_with(b"\r\n") {
        return Err(RedisClientError::Protocol(
            "RESP line did not end with CRLF".to_owned(),
        ));
    }
    line.truncate(line.len() - 2);
    String::from_utf8(line)
        .map_err(|error| RedisClientError::Protocol(format!("RESP line UTF-8: {error}")))
}

#[cfg(feature = "wasi-redis")]
fn read_expected_crlf(reader: &mut impl Read) -> Result<(), RedisClientError> {
    let mut crlf = [0_u8; 2];
    reader.read_exact(&mut crlf)?;
    if crlf == *b"\r\n" {
        Ok(())
    } else {
        Err(RedisClientError::Protocol(
            "bulk string did not end with CRLF".to_owned(),
        ))
    }
}

#[cfg(feature = "wasi-redis")]
fn expect_ok(value: RedisValue, command: &str) -> Result<(), RedisClientError> {
    match value {
        RedisValue::Status(status) if status.eq_ignore_ascii_case("OK") => Ok(()),
        other => Err(RedisClientError::Protocol(format!(
            "{command} returned unexpected value {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Aggregate, DomainEvent, Metadata};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    enum TestEvent {
        Created { value: i32 },
        Updated { value: i32 },
    }

    impl DomainEvent for TestEvent {
        fn event_type(&self) -> &'static str {
            match self {
                TestEvent::Created { .. } => "created",
                TestEvent::Updated { .. } => "updated",
            }
        }
    }

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    struct TestAggregate {
        value: i32,
        revision: u64,
    }

    impl Aggregate for TestAggregate {
        type Id = String;
        type Command = ();
        type Event = TestEvent;
        type Error = String;

        fn aggregate_type() -> &'static str {
            "test_aggregate"
        }

        fn new() -> Self {
            Self::default()
        }

        fn apply(&mut self, event: &Self::Event) {
            self.value = match event {
                TestEvent::Created { value } | TestEvent::Updated { value } => *value,
            };
        }

        fn handle(&self, _command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
            Ok(Vec::new())
        }
    }

    #[derive(Clone, Default)]
    struct RecordingRedisClient {
        calls: RecordedCalls,
    }

    type RecordedCalls = Arc<Mutex<Vec<(String, Vec<Vec<u8>>)>>>;

    #[async_trait]
    impl RedisCommandExecutor for RecordingRedisClient {
        type Error = String;

        async fn execute(
            &self,
            command: &str,
            args: Vec<Vec<u8>>,
        ) -> Result<RedisValue, Self::Error> {
            self.calls
                .lock()
                .map_err(|_| "poisoned".to_owned())?
                .push((command.to_owned(), args));
            Ok(RedisValue::Array(vec![
                RedisValue::Status("OK".to_owned()),
                RedisValue::Int(1),
                RedisValue::Int(1),
                RedisValue::Int(1),
            ]))
        }
    }

    fn bytes(value: &str) -> RedisValue {
        RedisValue::Bytes(value.as_bytes().to_vec())
    }

    /// Fields in a stored event hash. The flat batch reply carries two
    /// elements per field plus one length prefix per event.
    const EVENT_HASH_FIELD_COUNT: usize = 10;

    /// Elements the flat batch reply spends on one event.
    const REPLY_ELEMENTS_PER_EVENT: usize = EVENT_HASH_FIELD_COUNT * 2 + 1;

    /// Element ceiling the raw RESP client enforces on an array reply. Kept as
    /// a literal so the chunking tests still describe the limit when the
    /// `wasi-redis` client is compiled out.
    const RESP_ARRAY_CEILING: usize = 1_000_000;

    /// Backlog length at which an unchunked hash fetch first exceeded
    /// [`RESP_ARRAY_CEILING`] and made a stream or feed permanently unreadable.
    const LEGACY_UNREADABLE_BACKLOG: usize = RESP_ARRAY_CEILING / REPLY_ELEMENTS_PER_EVENT;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum FakeRedisCommand {
        RangeByScore,
        FetchHashes,
    }

    #[derive(Clone, Copy, Debug)]
    struct FakeRedisCall {
        kind: FakeRedisCommand,
        /// Index members or event keys the call asked for.
        requested: usize,
        /// Elements in the RESP array the call was answered with.
        reply_len: usize,
    }

    /// Fake Redis serving a synthetic global feed of `event_count` events and
    /// recording the shape of every command it answers, so the read paths can
    /// be checked for unbounded index reads and unbounded batch fetches
    /// without a live server.
    #[derive(Clone)]
    struct FakeRedisBackend {
        prefix: String,
        event_count: u64,
        calls: Arc<Mutex<Vec<FakeRedisCall>>>,
    }

    impl FakeRedisBackend {
        fn new(prefix: &str, event_count: u64) -> Self {
            Self {
                prefix: prefix.to_owned(),
                event_count,
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn record(&self, kind: FakeRedisCommand, requested: usize, reply_len: usize) {
            self.calls.lock().unwrap().push(FakeRedisCall {
                kind,
                requested,
                reply_len,
            });
        }

        fn calls_of(&self, kind: FakeRedisCommand) -> Vec<FakeRedisCall> {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .copied()
                .filter(|call| call.kind == kind)
                .collect()
        }

        fn widest_reply(&self) -> usize {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .map(|call| call.reply_len)
                .max()
                .unwrap_or_default()
        }
    }

    fn fake_event_hash_items(sequence: u64) -> Vec<RedisValue> {
        static METADATA_JSON: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        let metadata = METADATA_JSON
            .get_or_init(|| serde_json::to_string(&Metadata::default()).unwrap())
            .as_str();
        let sequence_text = sequence.to_string();

        [
            ("event_id", format!("evt-{sequence}")),
            ("aggregate_id", "\"stream-1\"".to_owned()),
            ("aggregate_type", TestAggregate::aggregate_type().to_owned()),
            ("revision", sequence_text.clone()),
            ("sequence", sequence_text),
            ("event_type", "created".to_owned()),
            ("event_version", "1".to_owned()),
            ("payload", "{\"Created\":{\"value\":1}}".to_owned()),
            ("metadata", metadata.to_owned()),
            ("recorded_at_ms", "1700000000000".to_owned()),
        ]
        .into_iter()
        .flat_map(|(field, value)| [bytes(field), RedisValue::Bytes(value.into_bytes())])
        .collect()
    }

    #[async_trait]
    impl RedisCommandExecutor for FakeRedisBackend {
        type Error = String;

        async fn execute(
            &self,
            command: &str,
            args: Vec<Vec<u8>>,
        ) -> Result<RedisValue, Self::Error> {
            let arg = |index: usize| String::from_utf8(args[index].clone()).unwrap();

            match command {
                "ZRANGEBYSCORE" => {
                    assert_eq!(arg(0), format!("{}:global", self.prefix));
                    assert_eq!(arg(2), "+inf");
                    assert_eq!(arg(3), "WITHSCORES");
                    assert_eq!(arg(4), "LIMIT");
                    assert_eq!(arg(5), "0");
                    let after: u64 = arg(1).trim_start_matches('(').parse().unwrap();
                    let requested: usize = arg(6).parse().unwrap();
                    let last = self.event_count.min(after.saturating_add(requested as u64));

                    let mut items = Vec::new();
                    for sequence in (after + 1)..=last {
                        // The global index scores each member by its own
                        // sequence.
                        items.push(RedisValue::Bytes(sequence.to_string().into_bytes()));
                        items.push(RedisValue::Bytes(sequence.to_string().into_bytes()));
                    }

                    self.record(FakeRedisCommand::RangeByScore, requested, items.len());
                    Ok(RedisValue::Array(items))
                }
                "EVAL" => {
                    let key_count: usize = arg(1).parse().unwrap();
                    let mut items = Vec::new();
                    for index in 0..key_count {
                        let key = arg(2 + index);
                        let sequence: u64 = key.rsplit(':').next().unwrap().parse().unwrap();
                        items.push(RedisValue::Int((EVENT_HASH_FIELD_COUNT * 2) as i64));
                        items.extend(fake_event_hash_items(sequence));
                    }

                    self.record(FakeRedisCommand::FetchHashes, key_count, items.len());
                    Ok(RedisValue::Array(items))
                }
                other => Err(format!("unexpected command `{other}`")),
            }
        }
    }

    fn fake_backed_store(
        prefix: &str,
        event_count: u64,
    ) -> (
        FakeRedisBackend,
        RedisEventStore<TestAggregate, FakeRedisBackend>,
    ) {
        let backend = FakeRedisBackend::new(prefix, event_count);
        let store =
            RedisEventStore::<TestAggregate, _>::with_prefix(backend.clone(), prefix).unwrap();
        (backend, store)
    }

    #[tokio::test]
    async fn global_replay_pages_the_index_and_chunks_hash_fetches() {
        const EVENTS: u64 = 1_200;
        let (backend, store) = fake_backed_store("ddd:paging", EVENTS);

        let events = store.load_global_after(None).await.unwrap();

        assert_eq!(events.len(), EVENTS as usize);
        assert_eq!(events.first().unwrap().sequence, Some(1));
        assert_eq!(events.last().unwrap().sequence, Some(EVENTS));

        let index_pages = backend.calls_of(FakeRedisCommand::RangeByScore);
        let hash_fetches = backend.calls_of(FakeRedisCommand::FetchHashes);

        // 500 + 500 + 200: the short page ends the scan.
        assert_eq!(index_pages.len(), 3);
        assert!(index_pages
            .iter()
            .all(|call| call.requested == SEQUENCE_PAGE_SIZE));
        assert_eq!(
            hash_fetches.len(),
            (EVENTS as usize).div_ceil(HASH_FETCH_CHUNK_SIZE)
        );
        assert!(hash_fetches
            .iter()
            .all(|call| call.requested <= HASH_FETCH_CHUNK_SIZE));
    }

    #[tokio::test]
    async fn limited_global_replay_asks_the_index_for_no_more_than_the_limit() {
        let (backend, store) = fake_backed_store("ddd:limited", 1_000);

        let events = store
            .load_global_after_limited(Some(10), NonZeroUsize::new(30).unwrap())
            .await
            .unwrap();

        assert_eq!(events.len(), 30);
        assert_eq!(events.first().unwrap().sequence, Some(11));
        assert_eq!(events.last().unwrap().sequence, Some(40));

        let index_pages = backend.calls_of(FakeRedisCommand::RangeByScore);
        assert_eq!(index_pages.len(), 1);
        assert_eq!(index_pages[0].requested, 30);
        assert_eq!(backend.calls_of(FakeRedisCommand::FetchHashes).len(), 1);
    }

    /// A backlog past the point where the previous single-`EVAL` fetch produced
    /// an array reply the raw RESP client refused, permanently blocking replay.
    #[tokio::test]
    async fn backlog_past_the_legacy_resp_ceiling_stays_readable() {
        let events_count = LEGACY_UNREADABLE_BACKLOG as u64 + 2_000;
        let (backend, store) = fake_backed_store("ddd:ceiling", events_count);

        let events = store.load_global_after(None).await.unwrap();

        assert_eq!(events.len(), events_count as usize);
        assert!(
            events_count as usize * REPLY_ELEMENTS_PER_EVENT > RESP_ARRAY_CEILING,
            "fixture must exceed what one unchunked reply could carry"
        );
        assert!(
            backend.widest_reply() < RESP_ARRAY_CEILING,
            "widest reply {} must stay under the {RESP_ARRAY_CEILING} element ceiling",
            backend.widest_reply()
        );
    }

    #[cfg(feature = "wasi-redis")]
    #[test]
    fn resp_array_ceiling_constant_matches_the_client() {
        assert_eq!(RESP_ARRAY_CEILING, MAX_RESP_ARRAY_LEN);
    }

    #[test]
    fn scored_index_page_parses_member_and_score_pairs() {
        let page = RedisValue::Array(vec![bytes("7"), bytes("3"), bytes("9"), bytes("4")]);

        assert_eq!(
            redis_scored_sequence_page(&page).unwrap(),
            vec![(7, 3), (9, 4)]
        );
    }

    #[test]
    fn scored_index_page_rejects_an_odd_element_count() {
        let page = RedisValue::Array(vec![bytes("7"), bytes("3"), bytes("9")]);

        assert!(redis_scored_sequence_page(&page).is_err());
    }

    #[test]
    fn flat_hash_batch_unpacks_length_prefixed_hashes() {
        let reply = RedisValue::Array(vec![
            RedisValue::Int(4),
            bytes("event_id"),
            bytes("e-1"),
            bytes("payload"),
            bytes("{}"),
            RedisValue::Int(2),
            bytes("event_id"),
            bytes("e-2"),
        ]);

        let hashes = unpack_flat_hash_batch(&reply, &[1, 2]).unwrap();

        assert_eq!(hashes.len(), 2);
        assert_eq!(hashes[0].get("event_id"), Some(&b"e-1".to_vec()));
        assert_eq!(hashes[0].get("payload"), Some(&b"{}".to_vec()));
        assert_eq!(hashes[1].get("event_id"), Some(&b"e-2".to_vec()));
    }

    #[test]
    fn flat_hash_batch_rejects_missing_indexed_event() {
        let reply = RedisValue::Array(vec![RedisValue::Int(0)]);

        let error = unpack_flat_hash_batch(&reply, &[7]).unwrap_err();

        assert!(error
            .to_string()
            .contains("sequence 7 is indexed but missing"));
    }

    #[test]
    fn flat_hash_batch_rejects_truncated_reply() {
        let reply = RedisValue::Array(vec![RedisValue::Int(4), bytes("event_id"), bytes("e-1")]);

        assert!(unpack_flat_hash_batch(&reply, &[1]).is_err());
    }

    #[test]
    fn flat_hash_batch_rejects_short_batch_and_trailing_items() {
        let empty = RedisValue::Array(Vec::new());
        assert!(unpack_flat_hash_batch(&empty, &[1]).is_err());

        let trailing = RedisValue::Array(vec![
            RedisValue::Int(2),
            bytes("event_id"),
            bytes("e-1"),
            bytes("junk"),
        ]);
        assert!(unpack_flat_hash_batch(&trailing, &[1]).is_err());
    }

    #[test]
    fn flat_hash_batch_rejects_non_integer_length_prefix() {
        let reply = RedisValue::Array(vec![bytes("2"), bytes("event_id"), bytes("e-1")]);

        assert!(unpack_flat_hash_batch(&reply, &[1]).is_err());
    }

    #[test]
    fn raw_hash_mapping_keeps_payload_and_id_untyped() {
        let mut hash = BTreeMap::new();
        let mut put = |k: &str, v: &str| hash.insert(k.to_owned(), v.as_bytes().to_vec());
        put("event_id", "evt-1");
        put("aggregate_id", "\"counter-1\"");
        put("aggregate_type", "test_aggregate");
        put("revision", "3");
        put("sequence", "7");
        put("event_type", "created");
        put("event_version", "1");
        put("payload", "{\"Created\":{\"value\":9}}");
        let metadata_json = serde_json::to_string(&Metadata::default()).unwrap();
        put("metadata", &metadata_json);
        put("recorded_at_ms", "1700000000000");

        let raw = hash_to_raw_envelope(&UpcasterRegistry::new(), hash).unwrap();

        assert_eq!(raw.aggregate_id, "\"counter-1\"");
        assert_eq!(raw.aggregate_type, "test_aggregate");
        assert_eq!(raw.sequence, Some(7));
        assert_eq!(raw.payload["Created"]["value"], 9);
    }

    #[test]
    fn redis_prefix_validation_accepts_key_safe_names() {
        assert!(validate_redis_prefix("ddd:tenant-1_events").is_ok());
    }

    #[test]
    fn redis_prefix_validation_rejects_whitespace() {
        assert!(validate_redis_prefix("ddd tenant").is_err());
    }

    #[test]
    fn key_names_hex_encode_aggregate_identity() {
        let client = RecordingRedisClient::default();
        let store = RedisEventStore::<TestAggregate, _>::with_prefix(client, "ddd:test").unwrap();

        let keys = store.stream_keys(&"counter:1".to_owned()).unwrap();

        assert!(keys.revision_key.starts_with("ddd:test:revision:"));
        assert!(!keys.revision_key.contains("\"counter:1\""));
        assert!(keys.stream_key.starts_with("ddd:test:stream:"));
    }

    #[test]
    fn append_lua_arguments_include_atomic_eval_shape() {
        let keys = RedisStreamKeys {
            aggregate_id_json: "\"stream-1\"".to_owned(),
            revision_key: "ddd:revision".to_owned(),
            stream_key: "ddd:stream".to_owned(),
        };
        let event = PreparedRedisEvent::new(NewEvent::new(
            TestEvent::Created { value: 7 },
            Metadata::new().with_correlation_id("corr-1"),
        ))
        .unwrap();

        let args = build_append_eval_args(AppendEvalArgs {
            script: "return 1",
            aggregate_type: TestAggregate::aggregate_type(),
            keys: &keys,
            sequence_key: "ddd:seq",
            global_key: "ddd:global",
            event_key_prefix: "ddd:event:",
            expected_revision: ExpectedRevision::NoStream,
            events: &[event],
        });

        assert_eq!(args[0], b"return 1");
        assert_eq!(args[1], b"4");
        assert_eq!(args[2], b"ddd:revision");
        assert_eq!(args[6], b"no_stream");
        assert_eq!(args[8], b"1");
        assert_eq!(args[12], b"test_aggregate");
    }

    /// Fake single-threaded RESP server: serves `replies_per_connection`
    /// command/`+PONG` cycles per accepted connection, then closes it, for up
    /// to `max_connections` connections. Returns the URL and an accepted-
    /// connection counter.
    #[cfg(feature = "wasi-redis")]
    fn spawn_fake_redis_server(
        replies_per_connection: usize,
        max_connections: usize,
    ) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
        use std::io::Read as _;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("redis://{}", listener.local_addr().unwrap());
        let accepted = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&accepted);

        std::thread::spawn(move || {
            for stream in listener.incoming().take(max_connections) {
                let Ok(mut stream) = stream else { break };
                counter.fetch_add(1, Ordering::SeqCst);
                for _ in 0..replies_per_connection {
                    // Read one full RESP command (terminated by the trailing
                    // CRLF of its last bulk argument).
                    let mut request = Vec::new();
                    let mut byte = [0_u8; 1];
                    while !request.ends_with(b"PING\r\n") {
                        match stream.read(&mut byte) {
                            Ok(1) => request.push(byte[0]),
                            _ => return,
                        }
                    }
                    if stream.write_all(b"+PONG\r\n").is_err() {
                        return;
                    }
                }
                // Dropping the stream closes the connection.
            }
        });

        (url, accepted)
    }

    #[cfg(feature = "wasi-redis")]
    #[tokio::test]
    async fn wasi_client_reuses_the_connection_across_commands() {
        use std::sync::atomic::Ordering;

        let (url, accepted) = spawn_fake_redis_server(3, 4);
        let client = WasiRedisClient::new(url).with_read_timeout(Some(Duration::from_secs(2)));

        for _ in 0..3 {
            let reply = client.execute("PING", Vec::new()).await.unwrap();
            assert_eq!(reply, RedisValue::Status("PONG".to_owned()));
        }

        assert_eq!(accepted.load(Ordering::SeqCst), 1);
    }

    #[cfg(feature = "wasi-redis")]
    #[tokio::test]
    async fn wasi_client_reconnects_after_the_server_closes_the_idle_connection() {
        use std::sync::atomic::Ordering;

        let (url, accepted) = spawn_fake_redis_server(1, 4);
        let client = WasiRedisClient::new(url).with_read_timeout(Some(Duration::from_secs(2)));

        let reply = client.execute("PING", Vec::new()).await.unwrap();
        assert_eq!(reply, RedisValue::Status("PONG".to_owned()));

        // Let the server-side close (FIN) reach the cached connection so the
        // reuse probe can observe it.
        std::thread::sleep(Duration::from_millis(100));

        let reply = client.execute("PING", Vec::new()).await.unwrap();
        assert_eq!(reply, RedisValue::Status("PONG".to_owned()));

        assert_eq!(accepted.load(Ordering::SeqCst), 2);
    }

    #[cfg(feature = "wasi-redis")]
    #[test]
    fn resp_encoder_writes_binary_safe_bulk_arguments() {
        let encoded = encode_resp_command("SET", &[b"k\r\n1".to_vec(), b"v".to_vec()]);

        assert_eq!(
            encoded,
            b"*3\r\n$3\r\nSET\r\n$4\r\nk\r\n1\r\n$1\r\nv\r\n".to_vec()
        );
    }

    #[cfg(feature = "wasi-redis")]
    #[test]
    fn resp_decoder_reads_arrays_and_bulk_values() {
        let input = b"*2\r\n$3\r\nfoo\r\n:42\r\n";
        let mut reader = BufReader::new(&input[..]);

        let value = read_resp_value(&mut reader).unwrap();

        assert_eq!(
            value,
            RedisValue::Array(vec![
                RedisValue::Bytes(b"foo".to_vec()),
                RedisValue::Int(42)
            ])
        );
    }

    #[cfg(feature = "wasi-redis")]
    #[test]
    fn redis_url_parser_supports_password_and_database() {
        let parsed = RedisAddress::parse("redis://:secret@localhost:6380/2").unwrap();

        assert_eq!(
            parsed,
            RedisAddress {
                host: "localhost".to_owned(),
                port: 6380,
                username: None,
                password: Some("secret".to_owned()),
                db: Some(2),
            }
        );
    }

    #[cfg(feature = "wasi-redis")]
    #[test]
    fn redis_url_parser_supports_acl_username_and_password() {
        let parsed = RedisAddress::parse("redis://app:secret@localhost/0").unwrap();

        assert_eq!(
            parsed,
            RedisAddress {
                host: "localhost".to_owned(),
                port: 6379,
                username: Some("app".to_owned()),
                password: Some("secret".to_owned()),
                db: Some(0),
            }
        );
    }

    #[cfg(feature = "wasi-redis")]
    fn live_client() -> Option<WasiRedisClient> {
        std::env::var("DDD_CQRS_ES_REDIS_URL")
            .ok()
            .or_else(|| std::env::var("REDIS_URL").ok())
            .map(WasiRedisClient::new)
    }

    /// Returns a live Redis client, or `None` when no Redis URL is configured
    /// and the caller should return early.
    ///
    /// Locally this only prints a skip notice. Under CI it panics: the workflow
    /// starts a Redis service and exports the URL, so an unset variable means
    /// the service is missing and a silent skip would hide backend regressions.
    #[cfg(feature = "wasi-redis")]
    fn live_client_or_skip(test_name: &str) -> Option<WasiRedisClient> {
        if let Some(client) = live_client() {
            return Some(client);
        }
        let running_in_ci = std::env::var("CI").is_ok_and(|value| {
            let value = value.trim();
            !value.is_empty() && !value.eq_ignore_ascii_case("false") && value != "0"
        });
        assert!(
            !running_in_ci,
            "live Redis {test_name} cannot be skipped in CI: \
             neither DDD_CQRS_ES_REDIS_URL nor REDIS_URL is set"
        );
        eprintln!("skipping live Redis {test_name}: DDD_CQRS_ES_REDIS_URL or REDIS_URL is not set");
        None
    }

    #[cfg(feature = "wasi-redis")]
    async fn cleanup_prefix(client: &WasiRedisClient, prefix: &str) {
        let Ok(keys) = client
            .execute("KEYS", vec![format!("{prefix}:*").into_bytes()])
            .await
        else {
            return;
        };
        let Ok(keys) = redis_array_items(&keys) else {
            return;
        };
        let keys = keys
            .iter()
            .filter_map(|value| redis_value_bytes(value, "cleanup key").ok())
            .collect::<Vec<_>>();
        if !keys.is_empty() {
            let _ = client.execute("DEL", keys).await;
        }
    }

    #[cfg(feature = "wasi-redis")]
    fn unique_prefix(test_name: &str) -> String {
        format!(
            "ddd:test:{}:{}",
            test_name,
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        )
    }

    #[cfg(feature = "wasi-redis")]
    #[tokio::test]
    async fn live_redis_async_event_store_passes_reusable_contract() {
        let Some(client) = live_client_or_skip("async contract test") else {
            return;
        };
        let prefix = unique_prefix("async_contract");
        cleanup_prefix(&client, &prefix).await;
        let store =
            RedisEventStore::<TestAggregate, _>::with_prefix(client.clone(), prefix.clone())
                .unwrap();

        crate::testing::assert_async_event_store_contract::<TestAggregate, _>(
            store,
            "stream-1".to_owned(),
            TestEvent::Created { value: 11 },
            TestEvent::Updated { value: 12 },
            crate::testing::EventStoreContractOptions::default(),
        )
        .await;
        cleanup_prefix(&client, &prefix).await;
    }

    #[cfg(feature = "wasi-redis")]
    #[tokio::test]
    async fn live_redis_expected_revision_conflicts() {
        let Some(client) = live_client_or_skip("expected-revision test") else {
            return;
        };
        let prefix = unique_prefix("expected_revision");
        cleanup_prefix(&client, &prefix).await;
        let store =
            RedisEventStore::<TestAggregate, _>::with_prefix(client.clone(), prefix.clone())
                .unwrap();

        store
            .append(
                &"stream-1".to_owned(),
                ExpectedRevision::NoStream,
                vec![NewEvent::new(
                    TestEvent::Created { value: 1 },
                    Metadata::default(),
                )],
            )
            .await
            .unwrap();
        let duplicate = store
            .append(
                &"stream-1".to_owned(),
                ExpectedRevision::NoStream,
                vec![NewEvent::new(
                    TestEvent::Updated { value: 2 },
                    Metadata::default(),
                )],
            )
            .await
            .unwrap_err();

        assert_eq!(
            duplicate,
            EventStoreError::Concurrency(crate::ConcurrencyError::StreamAlreadyExists)
        );
        cleanup_prefix(&client, &prefix).await;
    }

    #[cfg(feature = "wasi-redis")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn live_redis_concurrent_same_stream_append_has_one_revision_winner() {
        let Some(client) = live_client_or_skip("concurrent append test") else {
            return;
        };
        let prefix = unique_prefix("concurrent");
        cleanup_prefix(&client, &prefix).await;
        let store =
            RedisEventStore::<TestAggregate, _>::with_prefix(client.clone(), prefix.clone())
                .unwrap();
        let first = store.clone();
        let second = store.clone();
        let left_id = "stream-1".to_owned();
        let right_id = left_id.clone();

        let (left, right) = tokio::join!(
            async {
                first
                    .append(
                        &left_id,
                        ExpectedRevision::NoStream,
                        vec![NewEvent::new(
                            TestEvent::Created { value: 1 },
                            Metadata::default(),
                        )],
                    )
                    .await
            },
            async {
                second
                    .append(
                        &right_id,
                        ExpectedRevision::NoStream,
                        vec![NewEvent::new(
                            TestEvent::Created { value: 2 },
                            Metadata::default(),
                        )],
                    )
                    .await
            },
        );
        let winners = usize::from(left.is_ok()) + usize::from(right.is_ok());

        assert_eq!(winners, 1);
        cleanup_prefix(&client, &prefix).await;
    }

    #[cfg(feature = "wasi-redis")]
    #[tokio::test]
    async fn live_redis_global_ordering_and_checkpoint_update() {
        let Some(client) = live_client_or_skip("global ordering test") else {
            return;
        };
        let prefix = unique_prefix("global_checkpoint");
        cleanup_prefix(&client, &prefix).await;
        let store =
            RedisEventStore::<TestAggregate, _>::with_prefix(client.clone(), prefix.clone())
                .unwrap();
        let checkpoint = RedisCheckpointStore::with_prefix(client.clone(), prefix.clone()).unwrap();

        crate::testing::assert_async_checkpoint_store_contract(
            checkpoint.clone(),
            "projection_contract",
        )
        .await;

        store
            .append(
                &"stream-1".to_owned(),
                ExpectedRevision::NoStream,
                vec![NewEvent::new(
                    TestEvent::Created { value: 1 },
                    Metadata::default(),
                )],
            )
            .await
            .unwrap();
        store
            .append(
                &"stream-2".to_owned(),
                ExpectedRevision::NoStream,
                vec![NewEvent::new(
                    TestEvent::Created { value: 2 },
                    Metadata::default(),
                )],
            )
            .await
            .unwrap();
        checkpoint.save_checkpoint("projection", 1).await.unwrap();

        let global = store.load_global_after(Some(1)).await.unwrap();
        let loaded_checkpoint = checkpoint.load_checkpoint("projection").await.unwrap();

        assert_eq!((global[0].sequence, loaded_checkpoint), (Some(2), Some(1)));
        cleanup_prefix(&client, &prefix).await;
    }

    #[cfg(feature = "wasi-redis")]
    #[tokio::test]
    async fn live_redis_publish_and_subscribe_round_trip() {
        let Some(client) = live_client_or_skip("pub/sub test") else {
            return;
        };
        let channel = unique_prefix("pubsub");
        let mut subscription = client.subscribe(&channel).unwrap();
        let handle = std::thread::spawn(move || subscription.next_message());
        std::thread::sleep(Duration::from_millis(50));

        client.publish(&channel, b"{\"ok\":true}").await.unwrap();
        let message = handle.join().unwrap().unwrap();

        assert_eq!(message, b"{\"ok\":true}");
    }
}
