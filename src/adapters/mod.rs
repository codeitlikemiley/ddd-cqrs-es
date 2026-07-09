//! # ddd_cqrs_es adapters
//!
//! Stable built-in persistence adapters are exposed as `SqliteEventStore`,
//! `PostgresEventStore`, `SqliteCheckpointStore`, and
//! `PostgresCheckpointStore`, plus the SQL idempotency stores, when the
//! corresponding SQL feature is enabled.
//!
//! This module contains shared schema snippets and experimental WASI/Spin
//! query helpers for runtime-specific transports such as Neon, Supabase,
//! LibSQL, raw PostgreSQL TCP, and Spin host calls. These helpers are not
//! general-purpose SQL parameterization APIs and are not full event-store or
//! checkpoint-store backends until they implement the reusable library traits.

#[cfg(feature = "json-file")]
mod json_file;
mod runtime;

#[cfg(feature = "json-file")]
pub use json_file::{JsonFileCheckpointStore, JsonFileEventStore};
pub use runtime::*;

/// SQL schema for the Postgres `events` table used by framework-owned migrations.
pub const EVENTS_TABLE_SCHEMA_POSTGRES: &str = r#"
CREATE TABLE IF NOT EXISTS events (
    sequence BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    aggregate_id TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    revision BIGINT NOT NULL,
    event_type TEXT NOT NULL,
    event_version INT NOT NULL,
    payload JSONB NOT NULL,
    metadata JSONB NOT NULL,
    recorded_at_ms BIGINT NOT NULL,
    UNIQUE (aggregate_type, aggregate_id, revision)
);
"#;

/// SQL schema for the Postgres `checkpoints` table used by framework-owned migrations.
pub const CHECKPOINTS_TABLE_SCHEMA_POSTGRES: &str = r#"
CREATE TABLE IF NOT EXISTS checkpoints (
    projection_name VARCHAR(255) PRIMARY KEY,
    last_sequence BIGINT NOT NULL
);
"#;

/// SQL schema for the SQLite `events` table used by framework-owned migrations.
pub const EVENTS_TABLE_SCHEMA_SQLITE: &str = r#"
CREATE TABLE IF NOT EXISTS events (
    event_id TEXT NOT NULL UNIQUE,
    aggregate_id TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    revision INTEGER NOT NULL,
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type TEXT NOT NULL,
    event_version INTEGER NOT NULL,
    payload TEXT NOT NULL,
    metadata TEXT NOT NULL,
    recorded_at_ms INTEGER NOT NULL,
    UNIQUE (aggregate_id, aggregate_type, revision)
);
"#;

/// SQL schema for the SQLite `checkpoints` table used by framework-owned migrations.
pub const CHECKPOINTS_TABLE_SCHEMA_SQLITE: &str = r#"
CREATE TABLE IF NOT EXISTS checkpoints (
    projection_name TEXT PRIMARY KEY,
    last_sequence INTEGER NOT NULL
);
"#;

/// Decode a database JSON row into a standard [`crate::event::EventEnvelope`].
pub fn row_to_envelope<E, Id>(
    row: &serde_json::Value,
) -> Result<crate::event::EventEnvelope<E, Id>, String>
where
    E: serde::de::DeserializeOwned,
    Id: serde::de::DeserializeOwned,
{
    let obj = row
        .as_object()
        .ok_or_else(|| "Row is not a JSON object".to_string())?;

    let event_id_str = obj
        .get("event_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing or invalid event_id".to_string())?;

    let aggregate_id_str = obj
        .get("aggregate_id")
        .and_then(|v| {
            if let Some(s) = v.as_str() {
                Some(s.to_string())
            } else {
                serde_json::to_string(v).ok()
            }
        })
        .ok_or_else(|| "Missing or invalid aggregate_id".to_string())?;

    let aggregate_type = obj
        .get("aggregate_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing or invalid aggregate_type".to_string())?
        .to_string();

    let revision_val = obj
        .get("revision")
        .ok_or_else(|| "Missing revision".to_string())?;
    let revision = if let Some(s) = revision_val.as_str() {
        s.parse::<u64>().map_err(|e| e.to_string())?
    } else {
        revision_val
            .as_u64()
            .ok_or_else(|| "Invalid revision type".to_string())?
    };

    let sequence_val = obj.get("sequence");
    let sequence = match sequence_val {
        Some(v) if !v.is_null() => {
            if let Some(s) = v.as_str() {
                s.parse::<u64>().ok()
            } else {
                v.as_u64()
            }
        }
        _ => None,
    };

    let event_type = obj
        .get("event_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing or invalid event_type".to_string())?
        .to_string();

    let event_version_val = obj
        .get("event_version")
        .ok_or_else(|| "Missing event_version".to_string())?;
    let event_version = if let Some(s) = event_version_val.as_str() {
        s.parse::<u32>().map_err(|e| e.to_string())?
    } else {
        event_version_val
            .as_u64()
            .ok_or_else(|| "Invalid event_version type".to_string())? as u32
    };

    let payload_val = obj
        .get("payload")
        .ok_or_else(|| "Missing payload".to_string())?;
    let payload: E = match payload_val {
        serde_json::Value::String(s) => {
            serde_json::from_str(s).map_err(|e| format!("payload string deserialize: {}", e))?
        }
        other => serde_json::from_value(other.clone())
            .map_err(|e| format!("payload value deserialize: {}", e))?,
    };

    let metadata_val = obj
        .get("metadata")
        .ok_or_else(|| "Missing metadata".to_string())?;
    let metadata: crate::metadata::Metadata = match metadata_val {
        serde_json::Value::String(s) => {
            serde_json::from_str(s).map_err(|e| format!("metadata string deserialize: {}", e))?
        }
        other => serde_json::from_value(other.clone())
            .map_err(|e| format!("metadata value deserialize: {}", e))?,
    };

    let recorded_at_ms_val = obj
        .get("recorded_at_ms")
        .ok_or_else(|| "Missing recorded_at_ms".to_string())?;
    let recorded_at_ms = if let Some(s) = recorded_at_ms_val.as_str() {
        s.parse::<i64>().map_err(|e| e.to_string())?
    } else {
        recorded_at_ms_val
            .as_i64()
            .ok_or_else(|| "Invalid recorded_at_ms type".to_string())?
    };

    let duration = std::time::Duration::from_millis(recorded_at_ms as u64);
    let recorded_at = std::time::UNIX_EPOCH + duration;

    let aggregate_id: Id = serde_json::from_str(&aggregate_id_str)
        .or_else(|_| serde_json::from_value(serde_json::Value::String(aggregate_id_str.clone())))
        .map_err(|e| format!("aggregate_id deserialization failure: {}", e))?;

    Ok(crate::event::EventEnvelope::new(
        crate::event::EventId::from_string(event_id_str.to_string()),
        aggregate_id,
        aggregate_type,
        revision,
        sequence,
        event_type,
        event_version,
        payload,
        metadata,
        recorded_at,
    ))
}
