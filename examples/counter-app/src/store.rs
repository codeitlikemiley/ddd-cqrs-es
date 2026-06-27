use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use ddd_cqrs_es::{Aggregate, EventEnvelope, EventId, ExpectedRevision, NewEvent};
use ddd_cqrs_es::error::EventStoreError;
use ddd_cqrs_es::async_api::AsyncEventStore;
use async_trait::async_trait;

// #[cfg(feature = "postgres")]
// pub use ddd_cqrs_es::{PostgresEventStore, PostgresCheckpointStore};

static SCHEMA_INITIALIZED: AtomicBool = AtomicBool::new(false);
static SCHEMA_INIT_LOCK: OnceLock<futures::lock::Mutex<()>> = OnceLock::new();

// =========================================================================
// ENVIRONMENT CONFIGURATION HELPERS
// =========================================================================

pub fn get_backend() -> String {
    std::env::var("DATABASE_BACKEND").unwrap_or_else(|_| "sqlite".to_string())
}

fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

pub fn get_postgres_url() -> String {
    let backend = get_backend();
    match backend.as_str() {
        "supabase" => {
            return env_non_empty("SUPABASE_URL")
                .or_else(|| env_non_empty("DATABASE_URL"))
                .unwrap_or_default();
        }
        "neon" => {
            return env_non_empty("DATABASE_URL")
                .or_else(|| env_non_empty("NEON_DB_URL"))
                .unwrap_or_default();
        }
        _ => {}
    }

    env_non_empty("DATABASE_URL")
        .or_else(|| env_non_empty("POSTGRES_URL"))
        .unwrap_or_else(|| "postgresql://postgres:postgres@localhost:5432/postgres".to_string())
}

pub fn get_supabase_secret_key() -> Option<String> {
    env_non_empty("SUPABASE_SECRET_KEY")
        .or_else(|| env_non_empty("DATABASE_AUTH_TOKEN"))
}

pub fn get_turso_url() -> String {
    env_non_empty("DATABASE_URL")
        .or_else(|| env_non_empty("TURSO_URL"))
        .unwrap_or_else(|| "http://127.0.0.1:8080".to_string())
}

pub fn get_turso_auth_token() -> Option<String> {
    env_non_empty("DATABASE_AUTH_TOKEN").or_else(|| env_non_empty("TURSO_AUTH_TOKEN"))
}

// -------------------------------------------------------------------------
// ROUTED QUERY EXECUTOR
// -------------------------------------------------------------------------
#[allow(unused_variables)]
async fn execute_query_routed(
    sql_sqlite: &str,
    sql_postgres: &str,
    params: Vec<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, String> {
    let backend = get_backend();

    if backend == "libsql" || backend == "turso" {
        #[cfg(feature = "libsql")]
        {
            let url = get_turso_url();
            let auth = get_turso_auth_token();
            let res = ddd_cqrs_es::adapters::execute_libsql_query(&url, auth.as_deref(), sql_sqlite, params).await?;
            Ok(res.rows)
        }
        #[cfg(not(feature = "libsql"))]
        {
            Err("libsql feature is not enabled".to_string())
        }
    } else if backend == "supabase" {
        #[cfg(feature = "supabase")]
        {
            let url = get_postgres_url();
            let secret = get_supabase_secret_key();
            ddd_cqrs_es::adapters::execute_supabase_query(&url, secret.as_deref(), sql_postgres, params).await
        }
        #[cfg(not(feature = "supabase"))]
        {
            Err("supabase feature is not enabled".to_string())
        }
    } else if backend == "neon" {
        #[cfg(feature = "neon")]
        {
            let url = get_postgres_url();
            ddd_cqrs_es::adapters::execute_neon_query(&url, sql_postgres, params).await
        }
        #[cfg(not(feature = "neon"))]
        {
            Err("neon feature is not enabled".to_string())
        }
    } else {
        // Postgres TCP or Spin PG
        #[cfg(runtime_spin)]
        {
            #[cfg(feature = "postgres")]
            {
                let url = get_postgres_url();
                ddd_cqrs_es::adapters::execute_spin_pg(&url, sql_postgres, params).await
            }
            #[cfg(not(feature = "postgres"))]
            {
                Err("postgres feature is not enabled".to_string())
            }
        }
        #[cfg(runtime_wasmtime)]
        {
            #[cfg(feature = "postgres")]
            {
                let url = get_postgres_url();
                ddd_cqrs_es::adapters::execute_raw_tcp_postgres(&url, sql_postgres, params)
            }
            #[cfg(not(feature = "postgres"))]
            {
                Err("postgres feature is not enabled".to_string())
            }
        }
    }
}

// =========================================================================
// WASMTIME FLAT FILE PERSISTENCE FALLBACK
// =========================================================================
#[cfg(runtime_wasmtime)]
mod fs_fallback {
    use std::fs;
    use std::path::Path;
    use super::*;

    pub fn initialize_schema() -> Result<(), String> {
        fs::create_dir_all("/data").map_err(|e| e.to_string())?;
        
        let events_path = Path::new("/data/events.json");
        if !events_path.exists() {
            fs::write(events_path, "[]").map_err(|e| e.to_string())?;
        }

        let checkpoints_path = Path::new("/data/checkpoints.json");
        if !checkpoints_path.exists() {
            fs::write(checkpoints_path, "{}").map_err(|e| e.to_string())?;
        }

        let rm_path = Path::new("/data/counter_read_model.json");
        if !rm_path.exists() {
            fs::write(rm_path, "{}").map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    pub fn load_events<E, Id>(aggregate_id: &Id) -> Result<Vec<EventEnvelope<E, Id>>, EventStoreError>
    where
        E: serde::de::DeserializeOwned + Clone,
        Id: serde::Serialize + serde::de::DeserializeOwned + Clone + PartialEq,
    {
        let events_path = Path::new("/data/events.json");
        if !events_path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(events_path)
            .map_err(|e| EventStoreError::Backend(e.to_string()))?;
        
        let values: Vec<serde_json::Value> = serde_json::from_str(&content)
            .map_err(|e| EventStoreError::Deserialization(e.to_string()))?;

        let mut envelopes = Vec::new();
        for val in values {
            if let Some(id_val) = val.get("aggregate_id") {
                if let Ok(id) = serde_json::from_value::<Id>(id_val.clone()) {
                    if &id == aggregate_id {
                        if let Ok(envelope) = serde_json::from_value::<EventEnvelope<E, Id>>(val) {
                            envelopes.push(envelope);
                        }
                    }
                }
            }
        }

        envelopes.sort_by_key(|e| e.revision);
        Ok(envelopes)
    }

    pub fn append_events<E, Id>(
        aggregate_id: &Id,
        expected_revision: ExpectedRevision,
        events: Vec<NewEvent<E>>,
        aggregate_type: &str,
    ) -> Result<Vec<EventEnvelope<E, Id>>, EventStoreError>
    where
        E: serde::Serialize + serde::de::DeserializeOwned + Clone,
        Id: serde::Serialize + serde::de::DeserializeOwned + Clone + PartialEq,
    {
        let events_path = Path::new("/data/events.json");
        let content = if events_path.exists() {
            fs::read_to_string(events_path).map_err(|e| EventStoreError::Backend(e.to_string()))?
        } else {
            "[]".to_string()
        };

        let mut all_values: Vec<serde_json::Value> = serde_json::from_str(&content)
            .map_err(|e| EventStoreError::Deserialization(e.to_string()))?;

        let mut current_revision = 0u64;
        let mut max_sequence = 0u64;

        for val in &all_values {
            if let Some(seq) = val.get("sequence").and_then(|s| s.as_u64()) {
                if seq > max_sequence {
                    max_sequence = seq;
                }
            }

            if let Some(id_val) = val.get("aggregate_id") {
                if let Ok(id) = serde_json::from_value::<Id>(id_val.clone()) {
                    if &id == aggregate_id {
                        if let Some(rev) = val.get("revision").and_then(|r| r.as_u64()) {
                            if rev > current_revision {
                                current_revision = rev;
                            }
                        }
                    }
                }
            }
        }

        match expected_revision {
            ExpectedRevision::Any => {}
            ExpectedRevision::NoStream if current_revision == 0 => {}
            ExpectedRevision::NoStream => {
                return Err(EventStoreError::Concurrency(
                    ddd_cqrs_es::ConcurrencyError::StreamAlreadyExists,
                ));
            }
            ExpectedRevision::Exact(expected) if expected == current_revision => {}
            ExpectedRevision::Exact(_) => {
                return Err(EventStoreError::Concurrency(
                    ddd_cqrs_es::ConcurrencyError::WrongExpectedRevision {
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
        let now = std::time::SystemTime::now();

        for (i, event) in events.into_iter().enumerate() {
            let revision = current_revision + i as u64 + 1;
            let sequence = max_sequence + i as u64 + 1;
            let event_id = EventId::new();

            let envelope = EventEnvelope::new(
                event_id,
                aggregate_id.clone(),
                aggregate_type.to_string(),
                revision,
                Some(sequence),
                event.event_type,
                event.event_version,
                event.payload,
                event.metadata,
                now,
            );

            let val = serde_json::to_value(&envelope)
                .map_err(|e| EventStoreError::Serialization(e.to_string()))?;
            
            all_values.push(val);
            envelopes.push(envelope);
        }

        let new_content = serde_json::to_string(&all_values)
            .map_err(|e| EventStoreError::Serialization(e.to_string()))?;
        fs::write(events_path, new_content)
            .map_err(|e| EventStoreError::Backend(e.to_string()))?;

        Ok(envelopes)
    }

    pub fn load_global_after<E, Id>(sequence: u64) -> Result<Vec<EventEnvelope<E, Id>>, EventStoreError>
    where
        E: serde::de::DeserializeOwned + Clone,
        Id: serde::de::DeserializeOwned + Clone,
    {
        let events_path = Path::new("/data/events.json");
        if !events_path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(events_path)
            .map_err(|e| EventStoreError::Backend(e.to_string()))?;
        
        let values: Vec<serde_json::Value> = serde_json::from_str(&content)
            .map_err(|e| EventStoreError::Deserialization(e.to_string()))?;

        let mut envelopes = Vec::new();
        for val in values {
            if let Ok(envelope) = serde_json::from_value::<EventEnvelope<E, Id>>(val) {
                if envelope.sequence.unwrap_or(0) > sequence {
                    envelopes.push(envelope);
                }
            }
        }

        envelopes.sort_by_key(|e| e.sequence);
        Ok(envelopes)
    }

    pub fn load_checkpoint(projection_name: &str) -> Result<Option<u64>, EventStoreError> {
        let path = Path::new("/data/checkpoints.json");
        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(path)
            .map_err(|e| EventStoreError::Backend(e.to_string()))?;
        
        let map: std::collections::HashMap<String, u64> = serde_json::from_str(&content)
            .map_err(|e| EventStoreError::Deserialization(e.to_string()))?;

        Ok(map.get(projection_name).copied())
    }

    pub fn save_checkpoint(projection_name: &str, sequence: u64) -> Result<(), EventStoreError> {
        let path = Path::new("/data/checkpoints.json");
        let content = if path.exists() {
            fs::read_to_string(path).map_err(|e| EventStoreError::Backend(e.to_string()))?
        } else {
            "{}".to_string()
        };

        let mut map: std::collections::HashMap<String, u64> = serde_json::from_str(&content)
            .map_err(|e| EventStoreError::Deserialization(e.to_string()))?;

        map.insert(projection_name.to_string(), sequence);

        let new_content = serde_json::to_string(&map)
            .map_err(|e| EventStoreError::Serialization(e.to_string()))?;
        fs::write(path, new_content)
            .map_err(|e| EventStoreError::Backend(e.to_string()))?;
        
        Ok(())
    }
}

// =========================================================================
// MIGRATIONS AT BOOT (ONCE)
// =========================================================================

pub async fn initialize_schema_async() -> Result<(), String> {
    if SCHEMA_INITIALIZED.load(Ordering::Acquire) {
        return Ok(());
    }

    let lock = SCHEMA_INIT_LOCK.get_or_init(|| futures::lock::Mutex::new(()));
    let _guard = lock.lock().await;

    if SCHEMA_INITIALIZED.load(Ordering::Acquire) {
        return Ok(());
    }

    let backend = get_backend();

    if backend == "sqlite" {
        #[cfg(runtime_spin)]
        {
            #[cfg(feature = "sqlite")]
            {
                let sql_events = ddd_cqrs_es::adapters::EVENTS_TABLE_SCHEMA_SQLITE;
                let sql_checkpoints = ddd_cqrs_es::adapters::CHECKPOINTS_TABLE_SCHEMA_SQLITE;
                let sql_read_model = "CREATE TABLE IF NOT EXISTS counter_read_model (id TEXT PRIMARY KEY, value INTEGER NOT NULL);";
                ddd_cqrs_es::adapters::execute_spin_sqlite(sql_events, Vec::new()).await.map_err(|e| e.to_string())?;
                ddd_cqrs_es::adapters::execute_spin_sqlite(sql_checkpoints, Vec::new()).await.map_err(|e| e.to_string())?;
                ddd_cqrs_es::adapters::execute_spin_sqlite(sql_read_model, Vec::new()).await.map_err(|e| e.to_string())?;
            }
            #[cfg(not(feature = "sqlite"))]
            {
                return Err("sqlite feature not enabled".to_string());
            }
        }
        #[cfg(runtime_wasmtime)]
        {
            fs_fallback::initialize_schema()?;
        }
    } else {
        // Postgres or LibSQL
        let (sql_events, sql_checkpoints, sql_read_model) = if backend == "libsql" || backend == "turso" {
            (
                ddd_cqrs_es::adapters::EVENTS_TABLE_SCHEMA_SQLITE,
                ddd_cqrs_es::adapters::CHECKPOINTS_TABLE_SCHEMA_SQLITE,
                "CREATE TABLE IF NOT EXISTS counter_read_model (id TEXT PRIMARY KEY, value INTEGER NOT NULL);",
            )
        } else {
            (
                ddd_cqrs_es::adapters::EVENTS_TABLE_SCHEMA_POSTGRES,
                ddd_cqrs_es::adapters::CHECKPOINTS_TABLE_SCHEMA_POSTGRES,
                "CREATE TABLE IF NOT EXISTS counter_read_model (id VARCHAR(255) PRIMARY KEY, value BIGINT NOT NULL);",
            )
        };

        execute_query_routed(sql_events, sql_events, Vec::new()).await?;
        execute_query_routed(sql_checkpoints, sql_checkpoints, Vec::new()).await?;
        execute_query_routed(sql_read_model, sql_read_model, Vec::new()).await?;
    }

    SCHEMA_INITIALIZED.store(true, Ordering::Release);
    Ok(())
}

// =========================================================================
// MULTI-BACKEND EVENT STORE
// =========================================================================

pub struct MultiBackendEventStore<A> {
    _phantom: PhantomData<fn() -> A>,
}

impl<A> Clone for MultiBackendEventStore<A> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<A> MultiBackendEventStore<A> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

#[async_trait]
impl<A> AsyncEventStore<A> for MultiBackendEventStore<A>
where
    A: Aggregate + Send + Sync + 'static,
    A::Event: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + Clone,
    A::Id: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + Clone + PartialEq + std::fmt::Display,
{
    type Error = EventStoreError;

    async fn load(&self, aggregate_id: &A::Id) -> Result<Vec<EventEnvelope<A::Event, A::Id>>, Self::Error> {
        let backend = get_backend();

        if backend == "sqlite" {
            #[cfg(runtime_spin)]
            {
                #[cfg(feature = "sqlite")]
                {
                    let query = "SELECT sequence, event_id, aggregate_id, aggregate_type, revision, event_type, event_version, payload, metadata, recorded_at_ms FROM events WHERE aggregate_type = ? AND aggregate_id = ? ORDER BY revision ASC";
                    let agg_id_str = serde_json::to_string(aggregate_id).map_err(|e| EventStoreError::Serialization(e.to_string()))?;
                    let params = vec![
                        serde_json::Value::String(A::aggregate_type().to_string()),
                        serde_json::Value::String(agg_id_str),
                    ];
                    let rows = ddd_cqrs_es::adapters::execute_spin_sqlite(query, params).await
                        .map_err(|e| EventStoreError::Backend(e))?;
                    let mut envelopes = Vec::new();
                    for r in rows {
                        envelopes.push(ddd_cqrs_es::adapters::row_to_envelope::<A::Event, A::Id>(&r).map_err(|e| EventStoreError::Deserialization(e))?);
                    }
                    return Ok(envelopes);
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    return Err(EventStoreError::Backend("sqlite feature not enabled".to_string()));
                }
            }
            #[cfg(runtime_wasmtime)]
            {
                return fs_fallback::load_events(aggregate_id);
            }
        }

        let query_sqlite = "SELECT sequence, event_id, aggregate_id, aggregate_type, revision, event_type, event_version, payload, metadata, recorded_at_ms FROM events WHERE aggregate_type = ? AND aggregate_id = ? ORDER BY revision ASC";
        let query_postgres = "SELECT sequence, event_id, aggregate_id, aggregate_type, revision, event_type, event_version, payload, metadata, recorded_at_ms FROM events WHERE aggregate_type = $1 AND aggregate_id = $2 ORDER BY revision ASC";

        let agg_id_str = serde_json::to_string(aggregate_id).map_err(|e| EventStoreError::Serialization(e.to_string()))?;
        let params = vec![
            serde_json::Value::String(A::aggregate_type().to_string()),
            serde_json::Value::String(agg_id_str),
        ];

        let rows = execute_query_routed(query_sqlite, query_postgres, params).await
            .map_err(|e| EventStoreError::Backend(e))?;

        let mut envelopes = Vec::new();
        for r in rows {
            envelopes.push(ddd_cqrs_es::adapters::row_to_envelope::<A::Event, A::Id>(&r).map_err(|e| EventStoreError::Deserialization(e))?);
        }
        Ok(envelopes)
    }

    async fn append(
        &self,
        aggregate_id: &A::Id,
        expected_revision: ExpectedRevision,
        events: Vec<NewEvent<A::Event>>,
    ) -> Result<Vec<EventEnvelope<A::Event, A::Id>>, Self::Error> {
        let backend = get_backend();

        if backend == "sqlite" {
            #[cfg(runtime_spin)]
            {
                #[cfg(feature = "sqlite")]
                {
                    // In spin SQLite, we query current revision first
                    let query_rev = "SELECT COALESCE(MAX(revision), 0) as max_rev FROM events WHERE aggregate_type = ? AND aggregate_id = ?";
                    let agg_id_str = serde_json::to_string(aggregate_id).map_err(|e| EventStoreError::Serialization(e.to_string()))?;
                    let params_rev = vec![
                        serde_json::Value::String(A::aggregate_type().to_string()),
                        serde_json::Value::String(agg_id_str.clone()),
                    ];
                    let rows_rev = ddd_cqrs_es::adapters::execute_spin_sqlite(query_rev, params_rev).await
                        .map_err(|e| EventStoreError::Backend(e))?;
                    
                    let current_revision = rows_rev.first()
                        .and_then(|r| r.get("max_rev"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);

                    match expected_revision {
                        ExpectedRevision::Any => {}
                        ExpectedRevision::NoStream if current_revision == 0 => {}
                        ExpectedRevision::NoStream => {
                            return Err(EventStoreError::Concurrency(ddd_cqrs_es::ConcurrencyError::StreamAlreadyExists));
                        }
                        ExpectedRevision::Exact(expected) if expected == current_revision => {}
                        ExpectedRevision::Exact(_) => {
                            return Err(EventStoreError::Concurrency(ddd_cqrs_es::ConcurrencyError::WrongExpectedRevision {
                                expected: expected_revision,
                                actual: current_revision,
                            }));
                        }
                    }

                    let mut envelopes = Vec::new();
                    let now = std::time::SystemTime::now();
                    let now_ms = now.duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;

                    for (i, event) in events.into_iter().enumerate() {
                        let revision = current_revision + i as u64 + 1;
                        let event_id = EventId::new();

                        let insert_query = "INSERT INTO events (event_id, aggregate_id, aggregate_type, revision, event_type, event_version, payload, metadata, recorded_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING sequence";
                        let payload_str = serde_json::to_string(&event.payload).map_err(|e| EventStoreError::Serialization(e.to_string()))?;
                        let metadata_str = serde_json::to_string(&event.metadata).map_err(|e| EventStoreError::Serialization(e.to_string()))?;
                        let params_insert = vec![
                            serde_json::Value::String(event_id.to_string()),
                            serde_json::Value::String(agg_id_str.clone()),
                            serde_json::Value::String(A::aggregate_type().to_string()),
                            serde_json::Value::Number(revision.into()),
                            serde_json::Value::String(event.event_type.clone()),
                            serde_json::Value::Number(event.event_version.into()),
                            serde_json::Value::String(payload_str),
                            serde_json::Value::String(metadata_str),
                            serde_json::Value::Number(now_ms.into()),
                        ];

                        let insert_rows = ddd_cqrs_es::adapters::execute_spin_sqlite(insert_query, params_insert).await
                            .map_err(|e| EventStoreError::Backend(e))?;

                        let sequence = insert_rows.first()
                            .and_then(|r| r.get("sequence"))
                            .and_then(|v| {
                                if let Some(u) = v.as_u64() {
                                    Some(u)
                                } else if let Some(i) = v.as_i64() {
                                    Some(i as u64)
                                } else {
                                    None
                                }
                            });

                        let envelope = EventEnvelope::new(
                            event_id.clone(),
                            aggregate_id.clone(),
                            A::aggregate_type().to_string(),
                            revision,
                            sequence,
                            event.event_type,
                            event.event_version,
                            event.payload,
                            event.metadata,
                            now,
                        );

                        envelopes.push(envelope);
                    }
                    return Ok(envelopes);
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    return Err(EventStoreError::Backend("sqlite feature not enabled".to_string()));
                }
            }
            #[cfg(runtime_wasmtime)]
            {
                return fs_fallback::append_events(aggregate_id, expected_revision, events, A::aggregate_type());
            }
        }

        let query_sqlite_rev = "SELECT COALESCE(MAX(revision), 0) as max_rev FROM events WHERE aggregate_type = ? AND aggregate_id = ?";
        let query_postgres_rev = "SELECT COALESCE(MAX(revision), 0) as max_rev FROM events WHERE aggregate_type = $1 AND aggregate_id = $2";

        let agg_id_str = serde_json::to_string(aggregate_id).map_err(|e| EventStoreError::Serialization(e.to_string()))?;
        let params_rev = vec![
            serde_json::Value::String(A::aggregate_type().to_string()),
            serde_json::Value::String(agg_id_str.clone()),
        ];

        let rows_rev = execute_query_routed(query_sqlite_rev, query_postgres_rev, params_rev).await
            .map_err(|e| EventStoreError::Backend(e))?;

        let current_revision = rows_rev.first()
            .and_then(|r| r.get("max_rev"))
            .and_then(|v| {
                if let Some(u) = v.as_u64() {
                    Some(u)
                } else if let Some(i) = v.as_i64() {
                    Some(i as u64)
                } else if let Some(s) = v.as_str() {
                    s.parse::<u64>().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);

        match expected_revision {
            ExpectedRevision::Any => {}
            ExpectedRevision::NoStream if current_revision == 0 => {}
            ExpectedRevision::NoStream => {
                return Err(EventStoreError::Concurrency(ddd_cqrs_es::ConcurrencyError::StreamAlreadyExists));
            }
            ExpectedRevision::Exact(expected) if expected == current_revision => {}
            ExpectedRevision::Exact(_) => {
                return Err(EventStoreError::Concurrency(ddd_cqrs_es::ConcurrencyError::WrongExpectedRevision {
                    expected: expected_revision,
                    actual: current_revision,
                }));
            }
        }

        let mut envelopes = Vec::new();
        let now = std::time::SystemTime::now();
        let now_ms = now.duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;

        for (i, event) in events.into_iter().enumerate() {
            let revision = current_revision + i as u64 + 1;
            let event_id = EventId::new();

            let sql_sqlite_insert = "INSERT INTO events (event_id, aggregate_id, aggregate_type, revision, event_type, event_version, payload, metadata, recorded_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING sequence";
            let sql_postgres_insert = "INSERT INTO events (event_id, aggregate_id, aggregate_type, revision, event_type, event_version, payload, metadata, recorded_at_ms) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING sequence";

            let payload_val = serde_json::to_value(&event.payload).map_err(|e| EventStoreError::Serialization(e.to_string()))?;
            let metadata_val = serde_json::to_value(&event.metadata).map_err(|e| EventStoreError::Serialization(e.to_string()))?;

            let params_insert = vec![
                serde_json::Value::String(event_id.to_string()),
                serde_json::Value::String(agg_id_str.clone()),
                serde_json::Value::String(A::aggregate_type().to_string()),
                serde_json::Value::Number(revision.into()),
                serde_json::Value::String(event.event_type.clone()),
                serde_json::Value::Number(event.event_version.into()),
                payload_val,
                metadata_val,
                serde_json::Value::Number(now_ms.into()),
            ];

            let insert_rows = execute_query_routed(sql_sqlite_insert, sql_postgres_insert, params_insert).await
                .map_err(|e| EventStoreError::Backend(e))?;

            let sequence = insert_rows.first()
                .and_then(|r| r.get("sequence"))
                .and_then(|v| {
                    if let Some(u) = v.as_u64() {
                        Some(u)
                    } else if let Some(i) = v.as_i64() {
                        Some(i as u64)
                    } else if let Some(s) = v.as_str() {
                        s.parse::<u64>().ok()
                    } else {
                        None
                    }
                });

            let envelope = EventEnvelope::new(
                event_id.clone(),
                aggregate_id.clone(),
                A::aggregate_type().to_string(),
                revision,
                sequence,
                event.event_type,
                event.event_version,
                event.payload,
                event.metadata,
                now,
            );

            envelopes.push(envelope);
        }

        Ok(envelopes)
    }

    async fn load_global_after(&self, sequence: Option<u64>) -> Result<Vec<EventEnvelope<A::Event, A::Id>>, Self::Error> {
        let backend = get_backend();
        let seq = sequence.unwrap_or(0);

        if backend == "sqlite" {
            #[cfg(runtime_spin)]
            {
                #[cfg(feature = "sqlite")]
                {
                    let query = "SELECT sequence, event_id, aggregate_id, aggregate_type, revision, event_type, event_version, payload, metadata, recorded_at_ms FROM events WHERE sequence > ? ORDER BY sequence ASC";
                    let params = vec![serde_json::Value::Number(seq.into())];
                    let rows = ddd_cqrs_es::adapters::execute_spin_sqlite(query, params).await
                        .map_err(|e| EventStoreError::Backend(e))?;
                    let mut envelopes = Vec::new();
                    for r in rows {
                        envelopes.push(ddd_cqrs_es::adapters::row_to_envelope::<A::Event, A::Id>(&r).map_err(|e| EventStoreError::Deserialization(e))?);
                    }
                    return Ok(envelopes);
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    return Err(EventStoreError::Backend("sqlite feature not enabled".to_string()));
                }
            }
            #[cfg(runtime_wasmtime)]
            {
                return fs_fallback::load_global_after(seq);
            }
        }

        let query_sqlite = "SELECT sequence, event_id, aggregate_id, aggregate_type, revision, event_type, event_version, payload, metadata, recorded_at_ms FROM events WHERE sequence > ? ORDER BY sequence ASC";
        let query_postgres = "SELECT sequence, event_id, aggregate_id, aggregate_type, revision, event_type, event_version, payload, metadata, recorded_at_ms FROM events WHERE sequence > $1 ORDER BY sequence ASC";

        let params = vec![serde_json::Value::Number(seq.into())];
        let rows = execute_query_routed(query_sqlite, query_postgres, params).await
            .map_err(|e| EventStoreError::Backend(e))?;

        let mut envelopes = Vec::new();
        for r in rows {
            envelopes.push(ddd_cqrs_es::adapters::row_to_envelope::<A::Event, A::Id>(&r).map_err(|e| EventStoreError::Deserialization(e))?);
        }
        Ok(envelopes)
    }
}

// =========================================================================
// CHECKPOINT STORE
// =========================================================================

#[derive(Clone)]
pub struct MultiBackendCheckpointStore;

impl MultiBackendCheckpointStore {
    pub fn new() -> Self {
        Self
    }

    pub async fn load_checkpoint_async(&self, projection_name: &str) -> Result<Option<u64>, EventStoreError> {
        let backend = get_backend();

        if backend == "sqlite" {
            #[cfg(runtime_spin)]
            {
                #[cfg(feature = "sqlite")]
                {
                    let query = "SELECT last_sequence FROM checkpoints WHERE projection_name = ?";
                    let params = vec![serde_json::Value::String(projection_name.to_string())];
                    let rows = ddd_cqrs_es::adapters::execute_spin_sqlite(query, params).await
                        .map_err(|e| EventStoreError::Backend(e))?;
                    if let Some(r) = rows.first() {
                        if let Some(val) = r.get("last_sequence") {
                            if let Some(u) = val.as_u64() {
                                return Ok(Some(u));
                            }
                        }
                    }
                    return Ok(None);
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    return Err(EventStoreError::Backend("sqlite feature not enabled".to_string()));
                }
            }
            #[cfg(runtime_wasmtime)]
            {
                return fs_fallback::load_checkpoint(projection_name);
            }
        }

        let query_sqlite = "SELECT last_sequence FROM checkpoints WHERE projection_name = ?";
        let query_postgres = "SELECT last_sequence FROM checkpoints WHERE projection_name = $1";

        let params = vec![serde_json::Value::String(projection_name.to_string())];
        let rows = execute_query_routed(query_sqlite, query_postgres, params).await
            .map_err(|e| EventStoreError::Backend(e))?;

        if let Some(r) = rows.first() {
            if let Some(val) = r.get("last_sequence") {
                if let Some(u) = val.as_u64() {
                    return Ok(Some(u));
                } else if let Some(i) = val.as_i64() {
                    return Ok(Some(i as u64));
                } else if let Some(s) = val.as_str() {
                    if let Ok(u) = s.parse::<u64>() {
                        return Ok(Some(u));
                    }
                }
            }
        }

        Ok(None)
    }

    pub async fn save_checkpoint_async(&self, projection_name: &str, sequence: u64) -> Result<(), EventStoreError> {
        let backend = get_backend();

        if backend == "sqlite" {
            #[cfg(runtime_spin)]
            {
                #[cfg(feature = "sqlite")]
                {
                    let query = "INSERT INTO checkpoints (projection_name, last_sequence) VALUES (?, ?) ON CONFLICT(projection_name) DO UPDATE SET last_sequence = excluded.last_sequence";
                    let params = vec![
                        serde_json::Value::String(projection_name.to_string()),
                        serde_json::Value::Number(sequence.into()),
                    ];
                    ddd_cqrs_es::adapters::execute_spin_sqlite(query, params).await
                        .map_err(|e| EventStoreError::Backend(e))?;
                    return Ok(());
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    return Err(EventStoreError::Backend("sqlite feature not enabled".to_string()));
                }
            }
            #[cfg(runtime_wasmtime)]
            {
                return fs_fallback::save_checkpoint(projection_name, sequence);
            }
        }

        let sql_sqlite = "INSERT INTO checkpoints (projection_name, last_sequence) VALUES (?, ?) ON CONFLICT(projection_name) DO UPDATE SET last_sequence = excluded.last_sequence";
        let sql_postgres = "INSERT INTO checkpoints (projection_name, last_sequence) VALUES ($1, $2) ON CONFLICT(projection_name) DO UPDATE SET last_sequence = EXCLUDED.last_sequence";

        let params = vec![
            serde_json::Value::String(projection_name.to_string()),
            serde_json::Value::Number(sequence.into()),
        ];

        execute_query_routed(sql_sqlite, sql_postgres, params).await
            .map_err(|e| EventStoreError::Backend(e))?;

        Ok(())
    }
}

// =========================================================================
// COUNTER-SPECIFIC READ MODEL & PROJECTION
// =========================================================================

pub struct MultiBackendCounterProjection;

impl MultiBackendCounterProjection {
    pub fn new() -> Self {
        Self
    }

    pub async fn apply_async(&mut self, envelope: &EventEnvelope<crate::domain::CounterEvent, crate::domain::CounterId>) -> Result<(), EventStoreError> {
        let aggregate_id_str = serde_json::to_string(&envelope.aggregate_id)
            .map_err(|e| EventStoreError::Serialization(e.to_string()))?;
            
        let backend = get_backend();
        
        if backend == "sqlite" {
            #[cfg(runtime_spin)]
            {
                #[cfg(feature = "sqlite")]
                {
                    let (sql, param_val) = match envelope.payload {
                        crate::domain::CounterEvent::Incremented { amount } => (
                            "INSERT INTO counter_read_model (id, value) VALUES (?, ?) ON CONFLICT(id) DO UPDATE SET value = value + excluded.value;",
                            amount,
                        ),
                        crate::domain::CounterEvent::Decremented { amount } => (
                            "INSERT INTO counter_read_model (id, value) VALUES (?, ?) ON CONFLICT(id) DO UPDATE SET value = value + excluded.value;",
                            -amount,
                        ),
                        crate::domain::CounterEvent::ResetPerformed { value } => (
                            "INSERT INTO counter_read_model (id, value) VALUES (?, ?) ON CONFLICT(id) DO UPDATE SET value = excluded.value;",
                            value,
                        ),
                    };
                    let params = vec![
                        serde_json::Value::String(aggregate_id_str),
                        serde_json::Value::Number(param_val.into()),
                    ];
                    ddd_cqrs_es::adapters::execute_spin_sqlite(sql, params).await
                        .map_err(|e| EventStoreError::Backend(e))?;
                    return Ok(());
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    return Err(EventStoreError::Backend("sqlite feature not enabled".to_string()));
                }
            }
            #[cfg(runtime_wasmtime)]
            {
                // Local fs read model update
                use std::fs;
                use std::path::Path;
                let path = Path::new("/data/counter_read_model.json");
                let content = if path.exists() {
                    fs::read_to_string(path).map_err(|e| EventStoreError::Backend(e.to_string()))?
                } else {
                    "{}".to_string()
                };
                let mut map: std::collections::HashMap<String, i32> = serde_json::from_str(&content)
                    .map_err(|e| EventStoreError::Deserialization(e.to_string()))?;
                let current = map.get(&aggregate_id_str).copied().unwrap_or(0);
                let updated = match envelope.payload {
                    crate::domain::CounterEvent::Incremented { amount } => current + amount,
                    crate::domain::CounterEvent::Decremented { amount } => current - amount,
                    crate::domain::CounterEvent::ResetPerformed { value } => value,
                };
                map.insert(aggregate_id_str, updated);
                let new_content = serde_json::to_string(&map)
                    .map_err(|e| EventStoreError::Serialization(e.to_string()))?;
                fs::write(path, new_content).map_err(|e| EventStoreError::Backend(e.to_string()))?;
                return Ok(());
            }
        }
        
        let (sql_sqlite, sql_postgres, param_val) = match envelope.payload {
            crate::domain::CounterEvent::Incremented { amount } => (
                "INSERT INTO counter_read_model (id, value) VALUES (?, ?) ON CONFLICT(id) DO UPDATE SET value = value + excluded.value;",
                "INSERT INTO counter_read_model (id, value) VALUES ($1, $2) ON CONFLICT(id) DO UPDATE SET value = counter_read_model.value + EXCLUDED.value;",
                amount,
            ),
            crate::domain::CounterEvent::Decremented { amount } => (
                "INSERT INTO counter_read_model (id, value) VALUES (?, ?) ON CONFLICT(id) DO UPDATE SET value = value + excluded.value;",
                "INSERT INTO counter_read_model (id, value) VALUES ($1, $2) ON CONFLICT(id) DO UPDATE SET value = counter_read_model.value + EXCLUDED.value;",
                -amount,
            ),
            crate::domain::CounterEvent::ResetPerformed { value } => (
                "INSERT INTO counter_read_model (id, value) VALUES (?, ?) ON CONFLICT(id) DO UPDATE SET value = excluded.value;",
                "INSERT INTO counter_read_model (id, value) VALUES ($1, $2) ON CONFLICT(id) DO UPDATE SET value = EXCLUDED.value;",
                value,
            ),
        };
        
        let params_upsert = vec![
            serde_json::Value::String(aggregate_id_str),
            serde_json::Value::Number(param_val.into()),
        ];
        
        execute_query_routed(sql_sqlite, sql_postgres, params_upsert).await
            .map_err(|e| EventStoreError::Backend(e))?;
        
        Ok(())
    }
}

// -------------------------------------------------------------------------
// QUERY APIS
// -------------------------------------------------------------------------

fn value_as_i64(value: &serde_json::Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|v| i64::try_from(v).ok()))
        .or_else(|| value.as_str().and_then(|v| v.parse::<i64>().ok()))
}

fn value_as_u64(value: &serde_json::Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|v| u64::try_from(v).ok()))
        .or_else(|| value.as_str().and_then(|v| v.parse::<u64>().ok()))
}

fn row_count(row: &serde_json::Value) -> i32 {
    row.get("count")
        .or_else(|| row.get("value"))
        .and_then(value_as_i64)
        .and_then(|v| i32::try_from(v).ok())
        .unwrap_or(0)
}

fn event_log_from_row(row: &serde_json::Value) -> crate::app::EventLogDto {
    let sequence = row.get("sequence").and_then(value_as_u64).unwrap_or(0);
    let event_type = row
        .get("event_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let revision = row.get("revision").and_then(value_as_u64).unwrap_or(0);
    let payload = row
        .get("payload")
        .map(|v| {
            if v.is_string() {
                v.as_str().unwrap_or("").to_string()
            } else {
                v.to_string()
            }
        })
        .unwrap_or_default();
    let recorded_at_ms = row
        .get("recorded_at_ms")
        .and_then(value_as_i64)
        .unwrap_or(0);
    let recorded_at = format!("+{}ms", recorded_at_ms % 100000);

    crate::app::EventLogDto {
        sequence,
        event_type,
        revision,
        payload,
        recorded_at,
    }
}

fn event_logs_from_value(value: Option<&serde_json::Value>) -> Result<Vec<crate::app::EventLogDto>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };

    let parsed;
    let value = if let Some(s) = value.as_str() {
        parsed = serde_json::from_str::<serde_json::Value>(s)
            .map_err(|e| format!("Failed to parse latest_events JSON: {}", e))?;
        &parsed
    } else {
        value
    };

    let Some(rows) = value.as_array() else {
        return Ok(Vec::new());
    };

    Ok(rows.iter().map(event_log_from_row).collect())
}

pub async fn get_counter_view_db() -> Result<crate::app::CounterViewDto, String> {
    let backend = get_backend();

    if backend == "sqlite" {
        let count = get_count_db().await?;
        let latest_events = get_latest_events_db().await?;
        return Ok(crate::app::CounterViewDto {
            count,
            latest_events,
        });
    }

    let aggregate_id = crate::domain::CounterId("global".to_string());
    let aggregate_id_str = serde_json::to_string(&aggregate_id).map_err(|e| e.to_string())?;
    let params = vec![serde_json::Value::String(aggregate_id_str)];

    let query_sqlite = r#"
        SELECT
            COALESCE((SELECT value FROM counter_read_model WHERE id = ?), 0) AS count,
            COALESCE((
                SELECT json_group_array(json_object(
                    'sequence', sequence,
                    'event_type', event_type,
                    'revision', revision,
                    'payload', payload,
                    'recorded_at_ms', recorded_at_ms
                ))
                FROM (
                    SELECT sequence, event_type, revision, payload, recorded_at_ms
                    FROM events
                    ORDER BY sequence DESC
                    LIMIT 5
                )
            ), '[]') AS latest_events
    "#;
    let query_postgres = r#"
        SELECT
            COALESCE((SELECT value FROM counter_read_model WHERE id = $1), 0) AS count,
            COALESCE((
                SELECT json_agg(json_build_object(
                    'sequence', sequence,
                    'event_type', event_type,
                    'revision', revision,
                    'payload', payload,
                    'recorded_at_ms', recorded_at_ms
                ) ORDER BY sequence DESC)
                FROM (
                    SELECT sequence, event_type, revision, payload, recorded_at_ms
                    FROM events
                    ORDER BY sequence DESC
                    LIMIT 5
                ) latest
            ), '[]'::json) AS latest_events
    "#;

    let rows = execute_query_routed(query_sqlite, query_postgres, params).await?;
    let Some(row) = rows.first() else {
        return Ok(crate::app::CounterViewDto {
            count: 0,
            latest_events: Vec::new(),
        });
    };

    Ok(crate::app::CounterViewDto {
        count: row_count(row),
        latest_events: event_logs_from_value(row.get("latest_events"))?,
    })
}

pub async fn get_count_db() -> Result<i32, String> {
    let backend = get_backend();
    
    if backend == "sqlite" {
        #[cfg(runtime_spin)]
        {
            #[cfg(feature = "sqlite")]
            {
                let query = "SELECT value FROM counter_read_model WHERE id = ?";
                let aggregate_id = crate::domain::CounterId("global".to_string());
                let aggregate_id_str = serde_json::to_string(&aggregate_id).map_err(|e| e.to_string())?;
                let params = vec![serde_json::Value::String(aggregate_id_str)];
                let rows = ddd_cqrs_es::adapters::execute_spin_sqlite(query, params).await.map_err(|e| e.to_string())?;
                return Ok(rows.first().map(row_count).unwrap_or(0));
            }
            #[cfg(not(feature = "sqlite"))]
            {
                return Err("sqlite feature not enabled".to_string());
            }
        }
        #[cfg(runtime_wasmtime)]
        {
            use std::fs;
            use std::path::Path;
            let path = Path::new("/data/counter_read_model.json");
            if !path.exists() {
                return Ok(0);
            }
            let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
            let map: std::collections::HashMap<String, i32> = serde_json::from_str(&content).map_err(|e| e.to_string())?;
            let aggregate_id = crate::domain::CounterId("global".to_string());
            let aggregate_id_str = serde_json::to_string(&aggregate_id).map_err(|e| e.to_string())?;
            return Ok(map.get(&aggregate_id_str).copied().unwrap_or(0));
        }
    }
    
    let query_sqlite = "SELECT value FROM counter_read_model WHERE id = ?";
    let query_postgres = "SELECT value FROM counter_read_model WHERE id = $1";
    
    let aggregate_id = crate::domain::CounterId("global".to_string());
    let aggregate_id_str = serde_json::to_string(&aggregate_id).map_err(|e| e.to_string())?;
    let params = vec![serde_json::Value::String(aggregate_id_str)];
    
    let rows = execute_query_routed(query_sqlite, query_postgres, params).await?;
    
    Ok(rows.first().map(row_count).unwrap_or(0))
}

pub async fn get_latest_events_db() -> Result<Vec<crate::app::EventLogDto>, String> {
    let backend = get_backend();
    
    if backend == "sqlite" {
        #[cfg(runtime_spin)]
        {
            #[cfg(feature = "sqlite")]
            {
                let query = "SELECT sequence, event_type, revision, payload, recorded_at_ms FROM events ORDER BY sequence DESC LIMIT 5";
                let rows = ddd_cqrs_es::adapters::execute_spin_sqlite(query, Vec::new()).await.map_err(|e| e.to_string())?;
                return Ok(rows.iter().map(event_log_from_row).collect());
            }
            #[cfg(not(feature = "sqlite"))]
            {
                return Err("sqlite feature not enabled".to_string());
            }
        }
        #[cfg(runtime_wasmtime)]
        {
            use std::fs;
            use std::path::Path;
            let path = Path::new("/data/events.json");
            if !path.exists() {
                return Ok(Vec::new());
            }
            let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
            let values: Vec<serde_json::Value> = serde_json::from_str(&content).map_err(|e| e.to_string())?;
            let mut matching_vals: Vec<serde_json::Value> = values.into_iter()
                .filter(|val| {
                    use ddd_cqrs_es::Aggregate;
                    val.get("aggregate_type").and_then(|t| t.as_str()) == Some(crate::domain::Counter::aggregate_type())
                })
                .collect();
            matching_vals.sort_by_key(|val| val.get("sequence").and_then(|s| s.as_u64()).unwrap_or(0));
            matching_vals.reverse();
            let mut events = Vec::new();
            for val in matching_vals.into_iter().take(5) {
                events.push(event_log_from_row(&val));
            }
            return Ok(events);
        }
    }
    
    let query_sqlite = "SELECT sequence, event_type, revision, payload, recorded_at_ms FROM events ORDER BY sequence DESC LIMIT 5";
    let query_postgres = "SELECT sequence, event_type, revision, payload, recorded_at_ms FROM events ORDER BY sequence DESC LIMIT 5";
    
    let rows = execute_query_routed(query_sqlite, query_postgres, Vec::new()).await?;
    
    Ok(rows.iter().map(event_log_from_row).collect())
}

// -------------------------------------------------------------------------
// ASYNC COORDINATOR FOR PROJECTIONS RUNNER
// -------------------------------------------------------------------------
pub async fn run_projections_async(
    event_store: &MultiBackendEventStore<crate::domain::Counter>,
    checkpoint_store: &MultiBackendCheckpointStore,
    projection: &mut MultiBackendCounterProjection,
) -> Result<usize, String> {
    use ddd_cqrs_es::async_api::AsyncEventStore;
    
    let last_sequence = checkpoint_store.load_checkpoint_async("counter_projection").await
        .map_err(|e| e.to_string())?;
        
    let envelopes = event_store.load_global_after(last_sequence).await
        .map_err(|e| e.to_string())?;
        
    let count = envelopes.len();
    let mut last_sequence_processed = None;
    for envelope in envelopes {
        projection.apply_async(&envelope).await
            .map_err(|e| e.to_string())?;
            
        let sequence = envelope.sequence
            .ok_or_else(|| "Event envelope is missing global sequence".to_string())?;
            
        last_sequence_processed = Some(sequence);
    }
    
    if let Some(seq) = last_sequence_processed {
        checkpoint_store.save_checkpoint_async("counter_projection", seq).await
            .map_err(|e| e.to_string())?;
    }
    
    Ok(count)
}
