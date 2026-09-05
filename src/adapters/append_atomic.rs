//! Portable optimistic-concurrency append for HTTP SQL transports.
//!
//! Each transport executes a single guarded `INSERT … SELECT … WHERE` (Postgres)
//! or equivalent (SQLite) so revision checks and writes happen in one round trip.

use crate::{ConcurrencyError, ExpectedRevision};
use serde_json::Value;

const GUARD_ANY: i32 = 0;
const GUARD_NOSTREAM: i32 = 1;
const GUARD_EXACT: i32 = 2;

/// One event row prepared for atomic append.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppendEventRow {
    /// Stable event identifier.
    pub event_id: String,
    /// Persisted event type name.
    pub event_type: String,
    /// Event schema version.
    pub event_version: u32,
    /// Serialized event payload.
    pub payload: Value,
    /// Serialized event metadata.
    pub metadata: Value,
    /// Wall-clock millis since UNIX epoch.
    pub recorded_at_ms: i64,
}

/// One committed row returned from an atomic append.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppendCommittedRow {
    /// Event identifier echoed from the inserted row.
    pub event_id: String,
    /// Stream revision assigned by the store.
    pub revision: u64,
    /// Global sequence assigned by the store.
    pub sequence: u64,
}

/// Outcome of an atomic append attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppendAtomicResult {
    /// All rows were inserted under the expected revision guard.
    Committed(Vec<AppendCommittedRow>),
    /// The revision guard or unique constraint rejected the append.
    Conflict(ConcurrencyError),
}

fn validate_events_table_name(table_name: &str) -> Result<(), String> {
    let mut chars = table_name.chars();
    let Some(first) = chars.next() else {
        return Err("SQL event table name cannot be empty".to_owned());
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return Err(format!("invalid SQL event table name `{table_name}`"));
    }
    if chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric()) {
        Ok(())
    } else {
        Err(format!("invalid SQL event table name `{table_name}`"))
    }
}

fn guard_mode(expected: ExpectedRevision) -> (i32, u64) {
    match expected {
        ExpectedRevision::Any => (GUARD_ANY, 0),
        ExpectedRevision::NoStream => (GUARD_NOSTREAM, 0),
        ExpectedRevision::Exact(revision) => (GUARD_EXACT, revision),
    }
}

/// Builds a guarded Postgres multi-row append statement and positional params.
pub fn build_postgres_append_statement(
    table: &str,
    aggregate_type: &str,
    aggregate_id: &str,
    expected: ExpectedRevision,
    rows: &[AppendEventRow],
) -> Result<(String, Vec<Value>), String> {
    validate_events_table_name(table)?;
    if rows.is_empty() {
        return Err("append requires at least one event row".to_owned());
    }

    let (mode, expected_revision) = guard_mode(expected);
    let mut event_ids = Vec::with_capacity(rows.len());
    let mut row_offsets = Vec::with_capacity(rows.len());
    let mut event_types = Vec::with_capacity(rows.len());
    let mut event_versions = Vec::with_capacity(rows.len());
    let mut payloads = Vec::with_capacity(rows.len());
    let mut metadata_values = Vec::with_capacity(rows.len());
    let mut recorded_at_values = Vec::with_capacity(rows.len());

    for (index, row) in rows.iter().enumerate() {
        event_ids.push(Value::String(row.event_id.clone()));
        row_offsets.push(Value::Number((index as u64 + 1).into()));
        event_types.push(Value::String(row.event_type.clone()));
        event_versions.push(Value::Number(row.event_version.into()));
        payloads.push(row.payload.clone());
        metadata_values.push(row.metadata.clone());
        recorded_at_values.push(Value::Number(row.recorded_at_ms.into()));
    }

    let sql = format!(
        "WITH stream AS ( \
            SELECT COALESCE(MAX(revision), 0)::bigint AS current_rev \
            FROM {table} \
            WHERE aggregate_type = $1 AND aggregate_id = $2 \
         ), \
         guard AS ( \
            SELECT current_rev FROM stream \
            WHERE $3 = 0 \
               OR ($3 = 1 AND current_rev = 0) \
               OR ($3 = 2 AND current_rev = $4) \
         ) \
         INSERT INTO {table} ( \
            event_id, aggregate_id, aggregate_type, revision, event_type, event_version, \
            payload, metadata, recorded_at_ms \
         ) \
         SELECT \
            u.event_id, \
            $2, \
            $1, \
            g.current_rev + u.row_offset, \
            u.event_type, \
            u.event_version, \
            u.payload, \
            u.metadata, \
            u.recorded_at_ms \
         FROM guard g \
         CROSS JOIN UNNEST( \
            $5::text[], \
            $6::bigint[], \
            $7::text[], \
            $8::int[], \
            $9::jsonb[], \
            $10::jsonb[], \
            $11::bigint[] \
         ) AS u( \
            event_id, row_offset, event_type, event_version, payload, metadata, recorded_at_ms \
         ) \
         RETURNING event_id, revision, sequence"
    );

    let params = vec![
        Value::String(aggregate_type.to_owned()),
        Value::String(aggregate_id.to_owned()),
        Value::Number(mode.into()),
        Value::Number(expected_revision.into()),
        Value::Array(event_ids),
        Value::Array(row_offsets),
        Value::Array(event_types),
        Value::Array(event_versions),
        Value::Array(payloads),
        Value::Array(metadata_values),
        Value::Array(recorded_at_values),
    ];

    Ok((sql, params))
}

/// Builds a guarded SQLite multi-row append statement and positional params.
pub fn build_sqlite_append_statement(
    table: &str,
    aggregate_type: &str,
    aggregate_id: &str,
    expected: ExpectedRevision,
    rows: &[AppendEventRow],
) -> Result<(String, Vec<Value>), String> {
    validate_events_table_name(table)?;
    if rows.is_empty() {
        return Err("append requires at least one event row".to_owned());
    }

    let (mode, expected_revision) = guard_mode(expected);
    let mut values_sql = String::new();
    let mut params = vec![
        Value::String(aggregate_type.to_owned()),
        Value::String(aggregate_id.to_owned()),
        Value::Number(mode.into()),
        Value::Number(GUARD_NOSTREAM.into()),
        Value::Number(GUARD_EXACT.into()),
        Value::Number(expected_revision.into()),
        Value::String(aggregate_id.to_owned()),
        Value::String(aggregate_type.to_owned()),
    ];

    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            values_sql.push_str(", ");
        }
        values_sql.push('(');
        for _ in 0..6 {
            values_sql.push('?');
            values_sql.push(',');
        }
        values_sql.push('?');
        values_sql.push(')');
        params.push(Value::String(row.event_id.clone()));
        params.push(Value::String(row.event_type.clone()));
        params.push(Value::Number(row.event_version.into()));
        params.push(row.payload.clone());
        params.push(row.metadata.clone());
        params.push(Value::Number(row.recorded_at_ms.into()));
        params.push(Value::Number((index as u64 + 1).into()));
    }

    let sql = format!(
        "WITH stream AS ( \
            SELECT COALESCE(MAX(revision), 0) AS current_rev \
            FROM {table} \
            WHERE aggregate_type = ? AND aggregate_id = ? \
         ), \
         guard AS ( \
            SELECT current_rev FROM stream \
            WHERE ? = 0 \
               OR (? = 1 AND current_rev = 0) \
               OR (? = 2 AND current_rev = ?) \
         ) \
         INSERT INTO {table} ( \
            event_id, aggregate_id, aggregate_type, revision, event_type, event_version, \
            payload, metadata, recorded_at_ms \
         ) \
         SELECT \
            u.event_id, \
            ?, \
            ?, \
            g.current_rev + u.row_offset, \
            u.event_type, \
            u.event_version, \
            u.payload, \
            u.metadata, \
            u.recorded_at_ms \
         FROM guard g \
         CROSS JOIN (VALUES {values_sql}) AS u( \
            event_id, event_type, event_version, payload, metadata, recorded_at_ms, row_offset \
         ) \
         RETURNING event_id, sequence, revision"
    );

    Ok((sql, params))
}

/// Postgres/SQLite query that reads the current stream revision.
pub fn current_revision_query_postgres(table: &str) -> String {
    format!(
        "SELECT COALESCE(MAX(revision), 0) AS max_rev \
         FROM {table} \
         WHERE aggregate_type = $1 AND aggregate_id = $2"
    )
}

/// SQLite query that reads the current stream revision.
pub fn current_revision_query_sqlite(table: &str) -> String {
    format!(
        "SELECT COALESCE(MAX(revision), 0) AS max_rev \
         FROM {table} \
         WHERE aggregate_type = ? AND aggregate_id = ?"
    )
}

/// Parses `RETURNING` rows into committed append rows.
pub fn parse_committed_rows(rows: &[Value]) -> Result<Vec<AppendCommittedRow>, String> {
    rows.iter().map(parse_committed_row).collect()
}

fn parse_committed_row(row: &Value) -> Result<AppendCommittedRow, String> {
    let obj = row
        .as_object()
        .ok_or_else(|| "append row is not a JSON object".to_owned())?;
    let event_id = obj
        .get("event_id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "missing `event_id` in append row".to_owned())?
        .to_owned();
    let revision = parse_u64_field(obj, "revision")?;
    let sequence = parse_u64_field(obj, "sequence")?;
    Ok(AppendCommittedRow {
        event_id,
        revision,
        sequence,
    })
}

fn parse_u64_field(obj: &serde_json::Map<String, Value>, key: &str) -> Result<u64, String> {
    let value = obj
        .get(key)
        .ok_or_else(|| format!("missing `{key}` in append row"))?;
    if let Some(number) = value.as_u64() {
        return Ok(number);
    }
    if let Some(number) = value.as_i64() {
        return u64::try_from(number).map_err(|_| format!("negative `{key}` in append row"));
    }
    if let Some(text) = value.as_str() {
        return text
            .parse::<u64>()
            .map_err(|error| format!("invalid `{key}` `{text}`: {error}"));
    }
    Err(format!("unsupported `{key}` type in append row"))
}

/// Parses a revision probe row (`max_rev`).
pub fn parse_current_revision(rows: &[Value]) -> Result<u64, String> {
    rows.first()
        .and_then(|row| row.get("max_rev"))
        .map(parse_revision_value)
        .transpose()?
        .ok_or_else(|| "revision probe returned no rows".to_owned())
}

fn parse_revision_value(value: &Value) -> Result<u64, String> {
    if let Some(number) = value.as_u64() {
        return Ok(number);
    }
    if let Some(number) = value.as_i64() {
        return u64::try_from(number)
            .map_err(|_| "revision probe returned negative value".to_owned());
    }
    if let Some(text) = value.as_str() {
        return text
            .parse::<u64>()
            .map_err(|error| format!("invalid revision probe `{text}`: {error}"));
    }
    Err("unsupported revision probe value".to_owned())
}

/// Maps a revision guard miss to the appropriate concurrency error.
pub fn conflict_for_revision(expected: ExpectedRevision, actual: u64) -> AppendAtomicResult {
    AppendAtomicResult::Conflict(match expected {
        ExpectedRevision::NoStream => ConcurrencyError::StreamAlreadyExists,
        _ => ConcurrencyError::WrongExpectedRevision { expected, actual },
    })
}

/// Returns `true` when a transport error looks like a stream-revision unique violation.
pub fn is_revision_unique_violation(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    (lower.contains("unique") || lower.contains("23505") || lower.contains("duplicate"))
        && (lower.contains("revision") || lower.contains("aggregate"))
}

/// Resolves append rows, zero-row guard misses, and unique violations.
pub fn resolve_append_outcome(
    expected: ExpectedRevision,
    rows: &[Value],
    actual_revision: u64,
    transport_error: Option<&str>,
) -> Result<AppendAtomicResult, String> {
    if let Some(error) = transport_error {
        if is_revision_unique_violation(error) {
            return Ok(conflict_for_revision(expected, actual_revision));
        }
        return Err(error.to_owned());
    }
    if rows.is_empty() {
        return Ok(conflict_for_revision(expected, actual_revision));
    }
    let committed = parse_committed_rows(rows)?;
    Ok(AppendAtomicResult::Committed(committed))
}

#[cfg(feature = "wasi-neon")]
/// Atomically appends events through the Neon HTTP SQL endpoint.
pub async fn append_atomic_neon(
    url: &str,
    table: &str,
    aggregate_type: &str,
    aggregate_id: &str,
    expected: ExpectedRevision,
    rows: &[AppendEventRow],
) -> Result<AppendAtomicResult, String> {
    use super::execute_neon_query;

    if rows.is_empty() {
        return Ok(AppendAtomicResult::Committed(Vec::new()));
    }

    let (sql, params) =
        build_postgres_append_statement(table, aggregate_type, aggregate_id, expected, rows)?;
    let inserted = match execute_neon_query(url, &sql, params).await {
        Ok(rows) => rows,
        Err(error) if is_revision_unique_violation(&error) => {
            let actual =
                read_current_revision_neon(url, table, aggregate_type, aggregate_id).await?;
            return Ok(conflict_for_revision(expected, actual));
        }
        Err(error) => return Err(error),
    };
    if !inserted.is_empty() {
        return parse_committed_rows(&inserted).map(AppendAtomicResult::Committed);
    }

    let actual = read_current_revision_neon(url, table, aggregate_type, aggregate_id).await?;
    Ok(conflict_for_revision(expected, actual))
}

#[cfg(feature = "wasi-neon")]
async fn read_current_revision_neon(
    url: &str,
    table: &str,
    aggregate_type: &str,
    aggregate_id: &str,
) -> Result<u64, String> {
    use super::execute_neon_query;

    let sql = current_revision_query_postgres(table);
    let params = vec![
        Value::String(aggregate_type.to_owned()),
        Value::String(aggregate_id.to_owned()),
    ];
    let rows = execute_neon_query(url, &sql, params).await?;
    parse_current_revision(&rows)
}

#[cfg(feature = "wasi-libsql")]
/// Atomically appends events through the LibSQL Hrana pipeline endpoint.
pub async fn append_atomic_libsql(
    url: &str,
    auth_token: Option<&str>,
    table: &str,
    aggregate_type: &str,
    aggregate_id: &str,
    expected: ExpectedRevision,
    rows: &[AppendEventRow],
) -> Result<AppendAtomicResult, String> {
    use super::execute_libsql_query;

    if rows.is_empty() {
        return Ok(AppendAtomicResult::Committed(Vec::new()));
    }

    let (sql, params) =
        build_sqlite_append_statement(table, aggregate_type, aggregate_id, expected, rows)?;
    let inserted = match execute_libsql_query(url, auth_token, &sql, params).await {
        Ok(result) => result.rows,
        Err(error) if is_revision_unique_violation(&error) => {
            let actual =
                read_current_revision_libsql(url, auth_token, table, aggregate_type, aggregate_id)
                    .await?;
            return Ok(conflict_for_revision(expected, actual));
        }
        Err(error) => return Err(error),
    };
    if !inserted.is_empty() {
        return parse_committed_rows(&inserted).map(AppendAtomicResult::Committed);
    }

    let actual =
        read_current_revision_libsql(url, auth_token, table, aggregate_type, aggregate_id).await?;
    Ok(conflict_for_revision(expected, actual))
}

#[cfg(feature = "wasi-libsql")]
async fn read_current_revision_libsql(
    url: &str,
    auth_token: Option<&str>,
    table: &str,
    aggregate_type: &str,
    aggregate_id: &str,
) -> Result<u64, String> {
    use super::execute_libsql_query;

    let sql = current_revision_query_sqlite(table);
    let params = vec![
        Value::String(aggregate_type.to_owned()),
        Value::String(aggregate_id.to_owned()),
    ];
    let result = execute_libsql_query(url, auth_token, &sql, params).await?;
    parse_current_revision(&result.rows)
}

#[cfg(feature = "wasi-supabase-rpc")]
/// Atomically appends events through the Supabase `execute_sql` RPC.
pub async fn append_atomic_supabase(
    url: &str,
    secret_key: Option<&str>,
    table: &str,
    aggregate_type: &str,
    aggregate_id: &str,
    expected: ExpectedRevision,
    rows: &[AppendEventRow],
) -> Result<AppendAtomicResult, String> {
    use super::execute_supabase_query;

    if rows.is_empty() {
        return Ok(AppendAtomicResult::Committed(Vec::new()));
    }

    let (sql, params) =
        build_postgres_append_statement(table, aggregate_type, aggregate_id, expected, rows)?;
    let inserted = match execute_supabase_query(url, secret_key, &sql, params).await {
        Ok(rows) => rows,
        Err(error) if is_revision_unique_violation(&error) => {
            let actual = read_current_revision_supabase(
                url,
                secret_key,
                table,
                aggregate_type,
                aggregate_id,
            )
            .await?;
            return Ok(conflict_for_revision(expected, actual));
        }
        Err(error) => return Err(error),
    };
    if !inserted.is_empty() {
        return parse_committed_rows(&inserted).map(AppendAtomicResult::Committed);
    }

    let actual =
        read_current_revision_supabase(url, secret_key, table, aggregate_type, aggregate_id)
            .await?;
    Ok(conflict_for_revision(expected, actual))
}

#[cfg(feature = "wasi-supabase-rpc")]
async fn read_current_revision_supabase(
    url: &str,
    secret_key: Option<&str>,
    table: &str,
    aggregate_type: &str,
    aggregate_id: &str,
) -> Result<u64, String> {
    use super::execute_supabase_query;

    let sql = current_revision_query_postgres(table);
    let params = vec![
        Value::String(aggregate_type.to_owned()),
        Value::String(aggregate_id.to_owned()),
    ];
    let rows = execute_supabase_query(url, secret_key, &sql, params).await?;
    parse_current_revision(&rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_rows() -> Vec<AppendEventRow> {
        vec![
            AppendEventRow {
                event_id: "evt-1".to_owned(),
                event_type: "Incremented".to_owned(),
                event_version: 1,
                payload: json!({"amount": 1}),
                metadata: json!({}),
                recorded_at_ms: 1_700_000_000_000,
            },
            AppendEventRow {
                event_id: "evt-2".to_owned(),
                event_type: "Incremented".to_owned(),
                event_version: 1,
                payload: json!({"amount": 2}),
                metadata: json!({}),
                recorded_at_ms: 1_700_000_000_001,
            },
        ]
    }

    #[test]
    fn postgres_statement_includes_revision_guard_and_unnest() {
        let (sql, params) = build_postgres_append_statement(
            "events",
            "Counter",
            "\"counter-1\"",
            ExpectedRevision::Exact(2),
            &sample_rows(),
        )
        .unwrap();

        assert!(sql.contains("WITH stream AS"));
        assert!(sql.contains("guard AS"));
        assert!(sql.contains("UNNEST"));
        assert!(sql.contains("RETURNING event_id, revision, sequence"));
        assert_eq!(params.len(), 11);
        assert_eq!(params[2], json!(GUARD_EXACT));
        assert_eq!(params[3], json!(2));
    }

    #[test]
    fn sqlite_statement_uses_values_and_returning() {
        let (sql, params) = build_sqlite_append_statement(
            "events",
            "Counter",
            "\"counter-1\"",
            ExpectedRevision::NoStream,
            &sample_rows(),
        )
        .unwrap();

        assert!(sql.contains("WITH stream AS"));
        assert!(sql.contains("VALUES"));
        assert!(sql.contains("RETURNING event_id, sequence, revision"));
        assert_eq!(params[2], json!(GUARD_NOSTREAM));
        assert_eq!(params.len(), 8 + sample_rows().len() * 7);
    }

    #[test]
    fn zero_rows_map_to_concurrency_errors() {
        assert_eq!(
            conflict_for_revision(ExpectedRevision::NoStream, 1),
            AppendAtomicResult::Conflict(ConcurrencyError::StreamAlreadyExists)
        );
        assert_eq!(
            conflict_for_revision(ExpectedRevision::Exact(2), 4),
            AppendAtomicResult::Conflict(ConcurrencyError::WrongExpectedRevision {
                expected: ExpectedRevision::Exact(2),
                actual: 4,
            })
        );
    }

    #[test]
    fn parse_committed_and_revision_rows() {
        let committed = parse_committed_rows(&[json!({
            "event_id": "evt-1",
            "revision": "3",
            "sequence": 42
        })])
        .unwrap();
        assert_eq!(
            committed,
            vec![AppendCommittedRow {
                event_id: "evt-1".to_owned(),
                revision: 3,
                sequence: 42
            }]
        );

        assert_eq!(parse_current_revision(&[json!({"max_rev": 7})]).unwrap(), 7);
    }

    #[test]
    fn resolve_append_outcome_commits_and_conflicts() {
        let committed = resolve_append_outcome(
            ExpectedRevision::Exact(1),
            &[json!({"event_id": "evt-1", "revision": 2, "sequence": 9})],
            1,
            None,
        )
        .unwrap();
        assert!(matches!(committed, AppendAtomicResult::Committed(_)));

        let conflict = resolve_append_outcome(ExpectedRevision::Exact(1), &[], 3, None).unwrap();
        assert_eq!(
            conflict,
            AppendAtomicResult::Conflict(ConcurrencyError::WrongExpectedRevision {
                expected: ExpectedRevision::Exact(1),
                actual: 3,
            })
        );

        let unique = resolve_append_outcome(
            ExpectedRevision::NoStream,
            &[],
            2,
            Some("duplicate key value violates unique constraint \"events_aggregate_revision\""),
        )
        .unwrap();
        assert_eq!(
            unique,
            AppendAtomicResult::Conflict(ConcurrencyError::StreamAlreadyExists)
        );
    }

    #[test]
    fn rejects_invalid_table_names() {
        assert!(build_postgres_append_statement(
            "events;drop",
            "Counter",
            "id",
            ExpectedRevision::Any,
            &sample_rows()
        )
        .is_err());
    }
}
