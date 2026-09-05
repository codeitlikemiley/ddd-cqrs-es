#[cfg(any(
    feature = "sqlite",
    feature = "postgres",
    feature = "mysql",
    feature = "redis"
))]
use crate::error::EventStoreError;
#[cfg(any(
    feature = "sqlite",
    feature = "postgres",
    feature = "mysql",
    feature = "redis"
))]
use crate::{ConcurrencyError, ExpectedRevision};
#[cfg(any(
    feature = "sqlite",
    feature = "postgres",
    feature = "mysql",
    feature = "redis"
))]
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
pub(crate) fn validate_table_name(table_name: &str) -> Result<(), EventStoreError> {
    let mut chars = table_name.chars();
    let Some(first) = chars.next() else {
        return Err(EventStoreError::backend(
            "SQL event table name cannot be empty".to_owned(),
        ));
    };

    if !(first == '_' || first.is_ascii_alphabetic()) {
        return Err(EventStoreError::backend(format!(
            "invalid SQL event table name `{table_name}`"
        )));
    }

    if chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric()) {
        Ok(())
    } else {
        Err(EventStoreError::backend(format!(
            "invalid SQL event table name `{table_name}`"
        )))
    }
}

#[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
/// Returns true when a database unique-violation message refers to the stream
/// revision constraint, not the global `event_id` uniqueness guard.
pub(crate) fn is_stream_revision_unique_violation_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    if lower.contains("event_id") {
        return false;
    }
    lower.contains("revision")
        && (lower.contains("aggregate_id") || lower.contains("aggregate_type"))
}

/// MySQL duplicate-key messages often omit `revision` and name composite uniques
/// after the first column only; keep this broader than [`is_stream_revision_unique_violation_message`].
pub(crate) fn is_mysql_stream_revision_unique_violation_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    if lower.contains("event_id") {
        return false;
    }
    lower.contains("revision") || lower.contains("aggregate_type") || lower.contains("aggregate_id")
}

#[cfg(any(
    feature = "sqlite",
    feature = "postgres",
    feature = "mysql",
    feature = "redis"
))]
/// Maps a stream-revision unique violation into the appropriate concurrency error.
pub(crate) fn map_stream_unique_violation(
    expected: ExpectedRevision,
    current_revision: u64,
) -> EventStoreError {
    match expected {
        ExpectedRevision::NoStream => {
            EventStoreError::Concurrency(ConcurrencyError::StreamAlreadyExists)
        }
        _ => EventStoreError::Concurrency(ConcurrencyError::WrongExpectedRevision {
            expected,
            actual: current_revision,
        }),
    }
}

#[cfg(any(
    feature = "sqlite",
    feature = "postgres",
    feature = "mysql",
    feature = "redis"
))]
/// Returns an error when a snapshot save did not advance stored revision.
pub(crate) fn stale_snapshot_revision_error(offered: u64, current: u64) -> EventStoreError {
    EventStoreError::Concurrency(ConcurrencyError::StaleSnapshotRevision { offered, current })
}

#[cfg(any(
    feature = "sqlite",
    feature = "postgres",
    feature = "mysql",
    feature = "redis"
))]
pub(crate) fn check_expected_revision(
    expected: ExpectedRevision,
    actual: u64,
) -> Result<(), EventStoreError> {
    match expected {
        ExpectedRevision::Any => Ok(()),
        ExpectedRevision::NoStream if actual == 0 => Ok(()),
        ExpectedRevision::NoStream => Err(EventStoreError::Concurrency(
            ConcurrencyError::StreamAlreadyExists,
        )),
        ExpectedRevision::Exact(expected) if expected == actual => Ok(()),
        ExpectedRevision::Exact(_) => Err(EventStoreError::Concurrency(
            ConcurrencyError::WrongExpectedRevision { expected, actual },
        )),
    }
}

#[cfg(any(
    feature = "sqlite",
    feature = "postgres",
    feature = "mysql",
    feature = "redis"
))]
pub(crate) fn system_time_to_millis(recorded_at: SystemTime) -> Result<i64, EventStoreError> {
    let duration = recorded_at.duration_since(UNIX_EPOCH).map_err(|error| {
        EventStoreError::serialization_with_source(
            format!("recorded_at is before UNIX_EPOCH: {error}"),
            error,
        )
    })?;

    i64::try_from(duration.as_millis()).map_err(|_| {
        EventStoreError::serialization("recorded_at timestamp exceeds i64 millis".to_owned())
    })
}

#[cfg(any(
    feature = "sqlite",
    feature = "postgres",
    feature = "mysql",
    feature = "redis"
))]
pub(crate) fn millis_to_system_time(millis: i64) -> Result<SystemTime, EventStoreError> {
    let millis = u64::try_from(millis).map_err(|_| {
        EventStoreError::deserialization("recorded_at_ms cannot be negative".to_owned())
    })?;

    Ok(UNIX_EPOCH + Duration::from_millis(millis))
}

#[cfg(all(
    feature = "json",
    any(
        feature = "sqlite",
        feature = "postgres",
        feature = "mysql",
        feature = "redis"
    )
))]
pub(crate) fn serialize_id<Id>(id: &Id) -> Result<String, EventStoreError>
where
    Id: serde::Serialize,
{
    match serde_json::to_value(id).map_err(|error| {
        EventStoreError::serialization_with_source(format!("aggregate_id: {error}"), error)
    })? {
        serde_json::Value::String(value) => Ok(value),
        other => serde_json::to_string(&other).map_err(|error| {
            EventStoreError::serialization_with_source(format!("aggregate_id: {error}"), error)
        }),
    }
}

#[cfg(all(
    feature = "json",
    any(
        feature = "sqlite",
        feature = "postgres",
        feature = "mysql",
        feature = "redis"
    )
))]
/// Returns the storage keys that may identify one aggregate stream.
///
/// New rows store plain string ids; legacy rows used `serde_json::to_string`
/// which persisted JSON quote characters in the column.
pub(crate) fn aggregate_id_lookup_keys<Id>(id: &Id) -> Result<Vec<String>, EventStoreError>
where
    Id: serde::Serialize,
{
    let current = serialize_id(id)?;
    let mut keys = vec![current.clone()];
    if let Ok(legacy) = serde_json::to_string(id) {
        if legacy != current {
            keys.push(legacy);
        }
    }
    Ok(keys)
}

/// Returns the highest stream revision observed under any legacy or current key.
pub(crate) fn max_revision_for_lookup_keys<E>(
    keys: &[String],
    mut read: impl FnMut(&str) -> Result<u64, E>,
) -> Result<u64, E> {
    keys.iter()
        .try_fold(0u64, |max, key| read(key).map(|revision| max.max(revision)))
}

#[cfg(all(
    feature = "json",
    any(
        feature = "sqlite",
        feature = "postgres",
        feature = "mysql",
        feature = "redis"
    )
))]
pub(crate) fn deserialize_id<Id>(value: &str) -> Result<Id, EventStoreError>
where
    Id: serde::de::DeserializeOwned,
{
    match serde_json::from_str(value) {
        Ok(id) => Ok(id),
        Err(first) => {
            let quoted = serde_json::to_string(value).map_err(|error| {
                EventStoreError::deserialization_with_source(
                    format!("aggregate_id: {error}"),
                    error,
                )
            })?;
            serde_json::from_str(&quoted).map_err(|error| {
                EventStoreError::deserialization_with_source(
                    format!("aggregate_id: {error} (also tried legacy quoted form after {first})"),
                    error,
                )
            })
        }
    }
}

#[cfg(all(
    feature = "json",
    any(
        feature = "sqlite",
        feature = "postgres",
        feature = "mysql",
        feature = "redis"
    )
))]
pub(crate) fn serialize_payload<E>(event: &E) -> Result<serde_json::Value, EventStoreError>
where
    E: serde::Serialize,
{
    serde_json::to_value(event).map_err(|error| {
        EventStoreError::serialization_with_source(format!("event payload: {error}"), error)
    })
}

#[cfg(all(
    feature = "json",
    any(
        feature = "sqlite",
        feature = "postgres",
        feature = "mysql",
        feature = "redis"
    )
))]
pub(crate) fn deserialize_payload<E>(
    event_id: &str,
    event_type: &str,
    value: serde_json::Value,
) -> Result<E, EventStoreError>
where
    E: serde::de::DeserializeOwned,
{
    serde_json::from_value(value).map_err(|error| {
        EventStoreError::deserialization_with_source(
            format!("event_id `{event_id}` event_type `{event_type}` payload: {error}"),
            error,
        )
    })
}

#[cfg(all(
    feature = "json",
    any(
        feature = "sqlite",
        feature = "postgres",
        feature = "mysql",
        feature = "redis"
    )
))]
pub(crate) fn serialize_metadata(
    metadata: &crate::Metadata,
) -> Result<serde_json::Value, EventStoreError> {
    serde_json::to_value(metadata).map_err(|error| {
        EventStoreError::serialization_with_source(format!("metadata: {error}"), error)
    })
}

#[cfg(all(
    feature = "json",
    any(
        feature = "sqlite",
        feature = "postgres",
        feature = "mysql",
        feature = "redis"
    )
))]
pub(crate) fn deserialize_metadata(
    event_id: &str,
    value: serde_json::Value,
) -> Result<crate::Metadata, EventStoreError> {
    serde_json::from_value(value).map_err(|error| {
        EventStoreError::deserialization_with_source(
            format!("event_id `{event_id}` metadata: {error}"),
            error,
        )
    })
}

#[cfg(all(
    test,
    any(
        feature = "sqlite",
        feature = "postgres",
        feature = "mysql",
        feature = "redis"
    )
))]
mod tests {
    use super::*;
    use crate::error::EventStoreError;

    #[test]
    fn check_expected_revision_accepts_any_and_exact_matches() {
        assert!(check_expected_revision(ExpectedRevision::Any, 10).is_ok());
        assert!(check_expected_revision(ExpectedRevision::NoStream, 0).is_ok());
        assert!(check_expected_revision(ExpectedRevision::Exact(4), 4).is_ok());
    }

    #[test]
    fn check_expected_revision_rejects_stale_exact_and_existing_stream() {
        assert!(matches!(
            check_expected_revision(ExpectedRevision::NoStream, 1),
            Err(EventStoreError::Concurrency(
                ConcurrencyError::StreamAlreadyExists
            ))
        ));
        assert!(matches!(
            check_expected_revision(ExpectedRevision::Exact(2), 4),
            Err(EventStoreError::Concurrency(
                ConcurrencyError::WrongExpectedRevision {
                    expected: ExpectedRevision::Exact(2),
                    actual: 4,
                }
            ))
        ));
    }

    #[test]
    fn stream_revision_unique_violation_message_excludes_event_id_collisions() {
        assert!(is_stream_revision_unique_violation_message(
            "duplicate key value violates unique constraint \"events_aggregate_type_aggregate_id_revision_key\""
        ));
        assert!(!is_stream_revision_unique_violation_message(
            "duplicate key value violates unique constraint \"events_event_id_key\""
        ));
        assert!(is_mysql_stream_revision_unique_violation_message(
            "Duplicate entry 'acct-1-2' for key 'events.aggregate_type'"
        ));
        assert!(!is_mysql_stream_revision_unique_violation_message(
            "Duplicate entry 'evt-1' for key 'events.event_id'"
        ));
    }

    #[test]
    fn map_stream_unique_violation_maps_nostream_and_exact() {
        assert!(matches!(
            map_stream_unique_violation(ExpectedRevision::NoStream, 0),
            EventStoreError::Concurrency(ConcurrencyError::StreamAlreadyExists)
        ));
        assert!(matches!(
            map_stream_unique_violation(ExpectedRevision::Exact(1), 2),
            EventStoreError::Concurrency(ConcurrencyError::WrongExpectedRevision {
                expected: ExpectedRevision::Exact(1),
                actual: 2,
            })
        ));
        assert!(matches!(
            map_stream_unique_violation(ExpectedRevision::Any, 2),
            EventStoreError::Concurrency(ConcurrencyError::WrongExpectedRevision {
                expected: ExpectedRevision::Any,
                actual: 2,
            })
        ));
    }

    #[test]
    fn serialize_id_stores_string_ids_without_json_quotes() {
        let id = "counter-1".to_string();
        assert_eq!(serialize_id(&id).unwrap(), "counter-1");
    }

    #[test]
    fn deserialize_id_accepts_legacy_json_quoted_string_ids() {
        let id: String = deserialize_id("\"counter-1\"").unwrap();
        assert_eq!(id, "counter-1");
        let id: String = deserialize_id("counter-1").unwrap();
        assert_eq!(id, "counter-1");
    }

    #[test]
    fn stale_snapshot_revision_error_reports_current_revision() {
        assert!(matches!(
            stale_snapshot_revision_error(1, 3),
            EventStoreError::Concurrency(ConcurrencyError::StaleSnapshotRevision {
                offered: 1,
                current: 3,
            })
        ));
    }
}
