// =========================================================================
// FLAT-FILE FALLBACK STORAGE IMPLEMENTATIONS
// =========================================================================

#[cfg(feature = "json-file")]
static FILE_LOCKS: std::sync::OnceLock<
    std::sync::Mutex<
        std::collections::HashMap<std::path::PathBuf, std::sync::Arc<std::sync::Mutex<()>>>,
    >,
> = std::sync::OnceLock::new();

#[cfg(feature = "json-file")]
fn get_file_lock(
    path: &std::path::Path,
) -> Result<std::sync::Arc<std::sync::Mutex<()>>, crate::error::EventStoreError> {
    let map_lock =
        FILE_LOCKS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    let mut map = map_lock
        .lock()
        .map_err(|_| crate::error::EventStoreError::Poisoned)?;
    let canonical = if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
        if let Ok(canon_parent) = parent.canonicalize() {
            if let Some(filename) = path.file_name() {
                canon_parent.join(filename)
            } else {
                canon_parent
            }
        } else {
            path.to_path_buf()
        }
    } else {
        path.to_path_buf()
    };
    Ok(map
        .entry(canonical)
        .or_insert_with(|| std::sync::Arc::new(std::sync::Mutex::new(())))
        .clone())
}

#[cfg(feature = "json-file")]
fn write_atomic(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    use std::io::Write as _;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp_name = format!(
        "{}.tmp.{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        nanos
    );
    let tmp_path = path.with_file_name(tmp_name);
    let result = (|| {
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(content.as_bytes())?;
        // Flush to disk before the rename so a crash cannot publish an
        // empty or truncated file under the final name.
        file.sync_all()?;
        std::fs::rename(&tmp_path, path)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }
    sync_parent_dir(path);
    Ok(())
}

/// Best-effort fsync of the containing directory so a rename or newly
/// created file survives a crash. Ignored on platforms where directories
/// cannot be opened for syncing.
#[cfg(feature = "json-file")]
fn sync_parent_dir(path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
}

/// Reads every stored envelope as raw JSON, migrating legacy whole-array
/// files to the JSON Lines format on first contact.
#[cfg(feature = "json-file")]
fn read_event_values(
    path: &std::path::Path,
) -> Result<Vec<serde_json::Value>, crate::error::EventStoreError> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(path)
        .map_err(|e| crate::error::EventStoreError::backend(e.to_string()))?;

    if content.trim_start().starts_with('[') {
        // Legacy format: one JSON array holding every envelope. Parse it and
        // durably rewrite the file as JSON Lines before returning.
        let values: Vec<serde_json::Value> = serde_json::from_str(&content)
            .map_err(|e| crate::error::EventStoreError::deserialization(e.to_string()))?;
        let mut lines = String::new();
        for value in &values {
            lines.push_str(
                &serde_json::to_string(value)
                    .map_err(|e| crate::error::EventStoreError::serialization(e.to_string()))?,
            );
            lines.push('\n');
        }
        write_atomic(path, &lines)
            .map_err(|e| crate::error::EventStoreError::backend(e.to_string()))?;
        return Ok(values);
    }

    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .map_err(|e| crate::error::EventStoreError::deserialization(e.to_string()))
        })
        .collect()
}

/// Appends pre-serialized envelope lines and flushes them to disk before
/// acknowledging.
#[cfg(feature = "json-file")]
fn append_event_lines(path: &std::path::Path, lines: &[String]) -> std::io::Result<()> {
    use std::io::Write as _;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let created = !path.exists();
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    for line in lines {
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
    }
    file.sync_all()?;
    if created {
        sync_parent_dir(path);
    }
    Ok(())
}

#[cfg(feature = "json-file")]
/// A JSON file-backed event store.
///
/// Events are stored as JSON Lines (one envelope per line); files written by
/// older releases as a single JSON array are migrated in place on first
/// read. Appends add lines and fsync before acknowledging, so committed
/// events survive a crash; reads still scan the whole file (dev-tier).
///
/// > [!WARNING]
/// > This adapter is intended for **single-process development and testing purposes only**.
/// > It is not designed or certified for production use-cases where high concurrency,
/// > multi-process access, or strict reliability guarantees are required.
pub struct JsonFileEventStore<A> {
    events_path: std::path::PathBuf,
    _marker: std::marker::PhantomData<fn() -> A>,
}

#[cfg(feature = "json-file")]
impl<A> Clone for JsonFileEventStore<A> {
    fn clone(&self) -> Self {
        Self {
            events_path: self.events_path.clone(),
            _marker: std::marker::PhantomData,
        }
    }
}

#[cfg(feature = "json-file")]
impl<A> std::fmt::Debug for JsonFileEventStore<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JsonFileEventStore")
            .field("events_path", &self.events_path)
            .finish()
    }
}

#[cfg(feature = "json-file")]
impl<A> JsonFileEventStore<A> {
    /// Creates a new JSON file-backed event store.
    pub fn new(events_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            events_path: events_path.into(),
            _marker: std::marker::PhantomData,
        }
    }
}

#[cfg(feature = "json-file")]
impl<A> crate::event_store::EventStore<A> for JsonFileEventStore<A>
where
    A: crate::aggregate::Aggregate + 'static,
    A::Event: serde::Serialize + serde::de::DeserializeOwned + Clone,
    A::Id: serde::Serialize + serde::de::DeserializeOwned + Clone + PartialEq,
{
    type Error = crate::error::EventStoreError;

    fn load(
        &self,
        aggregate_id: &A::Id,
    ) -> Result<crate::event_store::EventStream<A>, Self::Error> {
        let lock = get_file_lock(&self.events_path)?;
        let _guard = lock
            .lock()
            .map_err(|_| crate::error::EventStoreError::Poisoned)?;

        let values = read_event_values(&self.events_path)?;

        let mut envelopes = Vec::new();
        for val in values {
            if let Some(agg_type_val) = val.get("aggregate_type") {
                if let Some(agg_type_str) = agg_type_val.as_str() {
                    if agg_type_str == A::aggregate_type() {
                        let id_val = val.get("aggregate_id").ok_or_else(|| {
                            crate::error::EventStoreError::deserialization(
                                "missing aggregate_id".to_string(),
                            )
                        })?;
                        let id = serde_json::from_value::<A::Id>(id_val.clone()).map_err(|e| {
                            crate::error::EventStoreError::deserialization(format!(
                                "failed to deserialize aggregate_id: {e}"
                            ))
                        })?;
                        if &id == aggregate_id {
                            let envelope = serde_json::from_value::<
                                crate::event::EventEnvelope<A::Event, A::Id>,
                            >(val)
                            .map_err(|e| {
                                crate::error::EventStoreError::deserialization(format!(
                                    "failed to deserialize event envelope: {e}"
                                ))
                            })?;
                            envelopes.push(envelope);
                        }
                    }
                }
            }
        }

        envelopes.sort_by_key(|e| e.revision);
        Ok(envelopes)
    }

    fn append(
        &self,
        aggregate_id: &A::Id,
        expected_revision: crate::event::ExpectedRevision,
        events: Vec<crate::event::NewEvent<A::Event>>,
    ) -> Result<crate::event_store::EventStream<A>, Self::Error> {
        let lock = get_file_lock(&self.events_path)?;
        let _guard = lock
            .lock()
            .map_err(|_| crate::error::EventStoreError::Poisoned)?;

        if let Some(parent) = self.events_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let all_values = read_event_values(&self.events_path)?;

        let mut current_revision = 0u64;
        let mut max_sequence = 0u64;

        for val in &all_values {
            if let Some(seq) = val.get("sequence").and_then(|s| s.as_u64()) {
                if seq > max_sequence {
                    max_sequence = seq;
                }
            }

            if let Some(agg_type_val) = val.get("aggregate_type") {
                if let Some(agg_type_str) = agg_type_val.as_str() {
                    if agg_type_str == A::aggregate_type() {
                        let id_val = val.get("aggregate_id").ok_or_else(|| {
                            crate::error::EventStoreError::deserialization(
                                "missing aggregate_id".to_string(),
                            )
                        })?;
                        let id = serde_json::from_value::<A::Id>(id_val.clone()).map_err(|e| {
                            crate::error::EventStoreError::deserialization(format!(
                                "failed to deserialize aggregate_id: {e}"
                            ))
                        })?;
                        if &id == aggregate_id {
                            let rev = val
                                .get("revision")
                                .ok_or_else(|| {
                                    crate::error::EventStoreError::deserialization(
                                        "missing revision".to_string(),
                                    )
                                })?
                                .as_u64()
                                .ok_or_else(|| {
                                    crate::error::EventStoreError::deserialization(
                                        "revision is not a valid u64".to_string(),
                                    )
                                })?;
                            if rev > current_revision {
                                current_revision = rev;
                            }
                        }
                    }
                }
            }
        }

        match expected_revision {
            crate::event::ExpectedRevision::Any => {}
            crate::event::ExpectedRevision::NoStream if current_revision == 0 => {}
            crate::event::ExpectedRevision::NoStream => {
                return Err(crate::error::EventStoreError::Concurrency(
                    crate::error::ConcurrencyError::StreamAlreadyExists,
                ));
            }
            crate::event::ExpectedRevision::Exact(expected) if expected == current_revision => {}
            crate::event::ExpectedRevision::Exact(_) => {
                return Err(crate::error::EventStoreError::Concurrency(
                    crate::error::ConcurrencyError::WrongExpectedRevision {
                        expected: expected_revision,
                        actual: current_revision,
                    },
                ));
            }
        }

        if events.is_empty() {
            return Ok(Vec::new());
        }

        let mut envelopes = Vec::new();
        let mut new_lines = Vec::new();
        let now = std::time::SystemTime::now();

        for (i, event) in events.into_iter().enumerate() {
            let revision = current_revision + i as u64 + 1;
            let sequence = max_sequence + i as u64 + 1;
            let event_id = crate::event::EventId::new();

            let envelope = crate::event::EventEnvelope::new(
                event_id,
                aggregate_id.clone(),
                A::aggregate_type().to_string(),
                revision,
                Some(sequence),
                event.event_type,
                event.event_version,
                event.payload,
                event.metadata,
                now,
            );

            let line = serde_json::to_string(&envelope)
                .map_err(|e| crate::error::EventStoreError::serialization(e.to_string()))?;

            new_lines.push(line);
            envelopes.push(envelope);
        }

        append_event_lines(&self.events_path, &new_lines)
            .map_err(|e| crate::error::EventStoreError::backend(e.to_string()))?;

        Ok(envelopes)
    }

    fn load_global_after(
        &self,
        sequence: Option<u64>,
    ) -> Result<crate::event_store::EventStream<A>, Self::Error> {
        self.load_global_range(sequence, None)
    }

    fn load_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: std::num::NonZeroUsize,
    ) -> Result<crate::event_store::EventStream<A>, Self::Error> {
        self.load_global_range(sequence, Some(limit.get()))
    }
}

#[cfg(feature = "json-file")]
impl<A> JsonFileEventStore<A>
where
    A: crate::aggregate::Aggregate,
    A::Event: serde::Serialize + serde::de::DeserializeOwned + Clone,
    A::Id: serde::Serialize + serde::de::DeserializeOwned + Clone + PartialEq,
{
    /// Reads the global feed after `sequence`, deserializing at most `limit`
    /// envelopes.
    ///
    /// The dev-only file store has no index, so the line scan itself is
    /// unavoidable; the bound applies to envelope deserialization and to the
    /// returned allocation, which is what projection replay pays for.
    fn load_global_range(
        &self,
        sequence: Option<u64>,
        limit: Option<usize>,
    ) -> Result<crate::event_store::EventStream<A>, crate::error::EventStoreError> {
        let lock = get_file_lock(&self.events_path)?;
        let _guard = lock
            .lock()
            .map_err(|_| crate::error::EventStoreError::Poisoned)?;

        let values = read_event_values(&self.events_path)?;
        let seq_num = sequence.unwrap_or(0);

        // Lines are appended in commit order, but a legacy file could be out of
        // order, so select by sequence first and only then deserialize the
        // bounded window.
        let mut selected = Vec::new();
        for value in values {
            let matches_type = value
                .get("aggregate_type")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|aggregate_type| aggregate_type == A::aggregate_type());
            if !matches_type {
                continue;
            }
            let event_sequence = value
                .get("sequence")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            if event_sequence > seq_num {
                selected.push((event_sequence, value));
            }
        }

        selected.sort_by_key(|(sequence, _)| *sequence);
        if let Some(limit) = limit {
            selected.truncate(limit);
        }

        selected
            .into_iter()
            .map(|(_, value)| {
                serde_json::from_value::<crate::event::EventEnvelope<A::Event, A::Id>>(value)
                    .map_err(|e| {
                        crate::error::EventStoreError::deserialization(format!(
                            "failed to deserialize event envelope: {e}"
                        ))
                    })
            })
            .collect()
    }
}

#[cfg(all(feature = "json-file", feature = "async"))]
#[async_trait::async_trait]
impl<A> crate::async_api::AsyncEventStore<A> for JsonFileEventStore<A>
where
    A: crate::aggregate::Aggregate + Send + Sync + 'static,
    A::Event: serde::Serialize + serde::de::DeserializeOwned + Clone + Send + Sync,
    A::Id: serde::Serialize + serde::de::DeserializeOwned + Clone + PartialEq + Send + Sync,
{
    type Error = crate::error::EventStoreError;

    async fn load(
        &self,
        aggregate_id: &A::Id,
    ) -> Result<crate::event_store::EventStream<A>, Self::Error> {
        let this = self.clone();
        let agg_id = aggregate_id.clone();
        tokio::task::spawn_blocking(move || crate::event_store::EventStore::load(&this, &agg_id))
            .await
            .map_err(|e| crate::error::EventStoreError::backend(e.to_string()))?
    }

    async fn append(
        &self,
        aggregate_id: &A::Id,
        expected_revision: crate::event::ExpectedRevision,
        events: Vec<crate::event::NewEvent<A::Event>>,
    ) -> Result<crate::event_store::EventStream<A>, Self::Error> {
        let this = self.clone();
        let agg_id = aggregate_id.clone();
        tokio::task::spawn_blocking(move || {
            crate::event_store::EventStore::append(&this, &agg_id, expected_revision, events)
        })
        .await
        .map_err(|e| crate::error::EventStoreError::backend(e.to_string()))?
    }

    async fn load_global_after(
        &self,
        sequence: Option<u64>,
    ) -> Result<crate::event_store::EventStream<A>, Self::Error> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            crate::event_store::EventStore::load_global_after(&this, sequence)
        })
        .await
        .map_err(|e| crate::error::EventStoreError::backend(e.to_string()))?
    }

    async fn load_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: std::num::NonZeroUsize,
    ) -> Result<crate::event_store::EventStream<A>, Self::Error> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            crate::event_store::EventStore::load_global_after_limited(&this, sequence, limit)
        })
        .await
        .map_err(|e| crate::error::EventStoreError::backend(e.to_string()))?
    }
}

#[cfg(feature = "json-file")]
#[derive(Clone, Debug)]
/// A JSON file-backed checkpoint store.
///
/// > [!WARNING]
/// > This adapter is intended for **single-process development and testing purposes only**.
/// > It is not designed or certified for production use-cases where high concurrency,
/// > multi-process access, or strict reliability guarantees are required.
pub struct JsonFileCheckpointStore {
    checkpoints_path: std::path::PathBuf,
}

#[cfg(feature = "json-file")]
impl JsonFileCheckpointStore {
    /// Creates a new JSON file-backed checkpoint store.
    pub fn new(checkpoints_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            checkpoints_path: checkpoints_path.into(),
        }
    }
}

#[cfg(feature = "json-file")]
impl crate::projection::CheckpointStore for JsonFileCheckpointStore {
    type Error = crate::error::EventStoreError;

    fn load_checkpoint(&self, projection_name: &str) -> Result<Option<u64>, Self::Error> {
        let lock = get_file_lock(&self.checkpoints_path)?;
        let _guard = lock
            .lock()
            .map_err(|_| crate::error::EventStoreError::Poisoned)?;

        if !self.checkpoints_path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&self.checkpoints_path)
            .map_err(|e| crate::error::EventStoreError::backend(e.to_string()))?;

        let map: std::collections::HashMap<String, u64> = serde_json::from_str(&content)
            .map_err(|e| crate::error::EventStoreError::deserialization(e.to_string()))?;

        Ok(map.get(projection_name).copied())
    }

    fn save_checkpoint(&self, projection_name: &str, sequence: u64) -> Result<(), Self::Error> {
        let lock = get_file_lock(&self.checkpoints_path)?;
        let _guard = lock
            .lock()
            .map_err(|_| crate::error::EventStoreError::Poisoned)?;

        if let Some(parent) = self.checkpoints_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let content = if self.checkpoints_path.exists() {
            std::fs::read_to_string(&self.checkpoints_path)
                .map_err(|e| crate::error::EventStoreError::backend(e.to_string()))?
        } else {
            "{}".to_string()
        };

        let mut map: std::collections::HashMap<String, u64> = serde_json::from_str(&content)
            .map_err(|e| crate::error::EventStoreError::deserialization(e.to_string()))?;

        map.insert(projection_name.to_string(), sequence);

        let new_content = serde_json::to_string(&map)
            .map_err(|e| crate::error::EventStoreError::serialization(e.to_string()))?;
        write_atomic(&self.checkpoints_path, &new_content)
            .map_err(|e| crate::error::EventStoreError::backend(e.to_string()))?;

        Ok(())
    }
}

#[cfg(all(feature = "json-file", feature = "async"))]
#[async_trait::async_trait]
impl crate::projection::AsyncCheckpointStore for JsonFileCheckpointStore {
    type Error = crate::error::EventStoreError;

    async fn load_checkpoint(&self, projection_name: &str) -> Result<Option<u64>, Self::Error> {
        let this = self.clone();
        let name = projection_name.to_owned();
        tokio::task::spawn_blocking(move || {
            crate::projection::CheckpointStore::load_checkpoint(&this, &name)
        })
        .await
        .map_err(|e| crate::error::EventStoreError::backend(e.to_string()))?
    }

    async fn save_checkpoint(
        &self,
        projection_name: &str,
        sequence: u64,
    ) -> Result<(), Self::Error> {
        let this = self.clone();
        let name = projection_name.to_owned();
        tokio::task::spawn_blocking(move || {
            crate::projection::CheckpointStore::save_checkpoint(&this, &name, sequence)
        })
        .await
        .map_err(|e| crate::error::EventStoreError::backend(e.to_string()))?
    }
}

#[cfg(feature = "json-file")]
impl<A> crate::raw_feed::RawEventFeed for JsonFileEventStore<A> {
    type Error = crate::error::EventStoreError;

    /// Serves every stored envelope of any aggregate type in global sequence
    /// order, payloads as raw JSON.
    fn load_raw_global_after_limited(
        &self,
        sequence: Option<u64>,
        limit: std::num::NonZeroUsize,
    ) -> Result<Vec<crate::raw_feed::RawEventEnvelope>, Self::Error> {
        let lock = get_file_lock(&self.events_path)?;
        let _guard = lock
            .lock()
            .map_err(|_| crate::error::EventStoreError::Poisoned)?;

        let values = read_event_values(&self.events_path)?;
        let checkpoint = sequence.unwrap_or(0);

        let mut envelopes = Vec::new();
        for val in values {
            let envelope = serde_json::from_value::<crate::raw_feed::RawEventEnvelope>(val)
                .map_err(|e| {
                    crate::error::EventStoreError::deserialization(format!(
                        "failed to deserialize event envelope: {e}"
                    ))
                })?;
            if envelope.sequence.unwrap_or(0) > checkpoint {
                envelopes.push(envelope);
            }
        }

        envelopes.sort_by_key(|e| e.sequence);
        envelopes.truncate(limit.get());
        Ok(envelopes)
    }
}
