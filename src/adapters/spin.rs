// -------------------------------------------------------------------------
// Spin SQLite adapter
// -------------------------------------------------------------------------
#[cfg(any(
    feature = "spin-sqlite",
    feature = "spin-postgres",
    feature = "spin-mysql"
))]
/// One parameterized statement in a bounded Spin host transaction.
///
/// Parameters are kept separate from SQL so callers never interpolate secret
/// material. Use [`Self::query`] only when returned rows are required.
#[derive(Clone, PartialEq)]
pub struct SpinSqlStatement {
    sql: String,
    params: Vec<serde_json::Value>,
    returns_rows: bool,
    minimum_rows: usize,
}

#[cfg(any(
    feature = "spin-sqlite",
    feature = "spin-postgres",
    feature = "spin-mysql"
))]
impl std::fmt::Debug for SpinSqlStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpinSqlStatement")
            .field("sql", &self.sql)
            .field("params", &format!("[{} redacted]", self.params.len()))
            .field("returns_rows", &self.returns_rows)
            .field("minimum_rows", &self.minimum_rows)
            .finish()
    }
}

#[cfg(any(
    feature = "spin-sqlite",
    feature = "spin-postgres",
    feature = "spin-mysql"
))]
impl SpinSqlStatement {
    /// Creates a write statement.
    #[must_use]
    pub fn execute(sql: impl Into<String>, params: Vec<serde_json::Value>) -> Self {
        Self {
            sql: sql.into(),
            params,
            returns_rows: false,
            minimum_rows: 0,
        }
    }

    /// Creates a query whose rows are included in the transaction result.
    #[must_use]
    pub fn query(sql: impl Into<String>, params: Vec<serde_json::Value>) -> Self {
        Self {
            sql: sql.into(),
            params,
            returns_rows: true,
            minimum_rows: 0,
        }
    }

    /// Creates a transactional guard query that must return at least one row.
    /// An empty rowset causes the complete transaction to roll back.
    #[must_use]
    pub fn guard(sql: impl Into<String>, params: Vec<serde_json::Value>) -> Self {
        Self {
            sql: sql.into(),
            params,
            returns_rows: true,
            minimum_rows: 1,
        }
    }

    /// Creates a Postgres transaction-scoped advisory lock for append serialization.
    ///
    /// Include this as the first statement in [`execute_spin_pg_atomic`] when
    /// appending to an events table so concurrent writers observe the same
    /// ordering guarantees as the native Postgres event store.
    #[cfg(feature = "spin-postgres")]
    #[must_use]
    pub fn postgres_append_advisory_lock(events_table: impl Into<String>) -> Self {
        Self::execute(
            "SELECT pg_advisory_xact_lock($1, hashtext($2::text))",
            vec![
                serde_json::json!(0x6464_6465_i64),
                serde_json::Value::String(events_table.into()),
            ],
        )
    }

    /// Returns the parameterized SQL text.
    #[must_use]
    pub fn sql(&self) -> &str {
        &self.sql
    }

    /// Returns non-interpolated statement parameters.
    #[must_use]
    pub fn params(&self) -> &[serde_json::Value] {
        &self.params
    }

    /// Returns whether rows must be materialized.
    #[must_use]
    pub const fn returns_rows(&self) -> bool {
        self.returns_rows
    }

    /// Returns the minimum row count required before commit.
    #[must_use]
    pub const fn minimum_rows(&self) -> usize {
        self.minimum_rows
    }
}

#[cfg(any(
    feature = "spin-sqlite",
    feature = "spin-postgres",
    feature = "spin-mysql"
))]
const MAX_SPIN_TRANSACTION_STATEMENTS: usize = 1_024;

#[cfg(feature = "spin-sqlite")]
/// Execute a Spin SQLite query against the default connection and return JSON rows.
///
/// Statement parameters are converted into Spin value types and each returned
/// column is materialized as a JSON object keyed by column name.
pub async fn execute_spin_sqlite(
    sql: &str,
    params: Vec<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, String> {
    let conn = spin_sdk::sqlite::Connection::open_default()
        .await
        .map_err(|e| format!("SQLite open connection error: {:?}", e))?;

    let spin_params: Vec<spin_sdk::sqlite::Value> = params
        .into_iter()
        .map(|v| match v {
            serde_json::Value::Null => spin_sdk::sqlite::Value::Null,
            serde_json::Value::Bool(b) => spin_sdk::sqlite::Value::Integer(if b { 1 } else { 0 }),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    spin_sdk::sqlite::Value::Integer(i)
                } else if let Some(f) = n.as_f64() {
                    spin_sdk::sqlite::Value::Real(f)
                } else {
                    spin_sdk::sqlite::Value::Null
                }
            }
            serde_json::Value::String(s) => spin_sdk::sqlite::Value::Text(s),
            other => spin_sdk::sqlite::Value::Text(other.to_string()),
        })
        .collect();

    let rowset = conn
        .execute(sql, spin_params)
        .await
        .map_err(|e| format!("SQLite query error: {:?}", e))?;

    let columns = rowset.columns().to_vec();
    let rows_list = rowset
        .collect()
        .await
        .map_err(|e| format!("SQLite rows collection error: {:?}", e))?;

    let mut rows = Vec::new();
    for r in rows_list {
        let mut row_obj = serde_json::Map::new();
        for (i, col_name) in columns.iter().enumerate() {
            let val = match &r.values[i] {
                spin_sdk::sqlite::Value::Null => serde_json::Value::Null,
                spin_sdk::sqlite::Value::Integer(i) => {
                    serde_json::Value::Number(serde_json::Number::from(*i))
                }
                spin_sdk::sqlite::Value::Real(f) => {
                    if let Some(num) = serde_json::Number::from_f64(*f) {
                        serde_json::Value::Number(num)
                    } else {
                        serde_json::Value::Null
                    }
                }
                spin_sdk::sqlite::Value::Text(s) => serde_json::Value::String(s.clone()),
                spin_sdk::sqlite::Value::Blob(b) => {
                    serde_json::Value::String(String::from_utf8_lossy(b).into_owned())
                }
            };
            row_obj.insert(col_name.clone(), val);
        }
        rows.push(serde_json::Value::Object(row_obj));
    }

    Ok(rows)
}

#[cfg(feature = "spin-sqlite")]
/// Executes parameterized SQLite statements on one host connection inside
/// `BEGIN IMMEDIATE` and `COMMIT`.
///
/// A failure attempts `ROLLBACK` on the same connection and returns no partial
/// result. Transactions are bounded to 1,024 statements. The outer vector has
/// one rowset per supplied statement; write rowsets are empty.
pub async fn execute_spin_sqlite_atomic(
    statements: Vec<SpinSqlStatement>,
) -> Result<Vec<Vec<serde_json::Value>>, String> {
    use spin_sdk::sqlite::Connection;

    validate_spin_transaction(&statements)?;
    let connection = Connection::open_default()
        .await
        .map_err(|error| format!("SQLite open connection error: {error:?}"))?;
    finish_sqlite_statement(&connection, "BEGIN IMMEDIATE", Vec::new())
        .await
        .map_err(|error| format!("SQLite begin transaction error: {error}"))?;

    let mut output = Vec::with_capacity(statements.len());
    for (index, statement) in statements.into_iter().enumerate() {
        let params = sqlite_parameters(statement.params);
        match collect_sqlite_statement(&connection, &statement.sql, params).await {
            Ok(rows) if rows.len() >= statement.minimum_rows => {
                output.push(if statement.returns_rows {
                    rows
                } else {
                    Vec::new()
                })
            }
            Ok(_) => {
                let rollback = finish_sqlite_statement(&connection, "ROLLBACK", Vec::new()).await;
                return Err(transaction_failure(
                    "SQLite",
                    index,
                    "transaction guard returned too few rows".to_owned(),
                    rollback,
                ));
            }
            Err(error) => {
                let rollback = finish_sqlite_statement(&connection, "ROLLBACK", Vec::new()).await;
                return Err(transaction_failure("SQLite", index, error, rollback));
            }
        }
    }

    if let Err(error) = finish_sqlite_statement(&connection, "COMMIT", Vec::new()).await {
        let rollback = finish_sqlite_statement(&connection, "ROLLBACK", Vec::new()).await;
        return Err(transaction_failure(
            "SQLite commit",
            output.len(),
            error,
            rollback,
        ));
    }
    Ok(output)
}

#[cfg(feature = "spin-sqlite")]
fn sqlite_parameters(params: Vec<serde_json::Value>) -> Vec<spin_sdk::sqlite::Value> {
    params
        .into_iter()
        .map(|value| match value {
            serde_json::Value::Null => spin_sdk::sqlite::Value::Null,
            serde_json::Value::Bool(value) => spin_sdk::sqlite::Value::Integer(i64::from(value)),
            serde_json::Value::Number(value) => value.as_i64().map_or_else(
                || {
                    value
                        .as_f64()
                        .map_or(spin_sdk::sqlite::Value::Null, spin_sdk::sqlite::Value::Real)
                },
                spin_sdk::sqlite::Value::Integer,
            ),
            serde_json::Value::String(value) => spin_sdk::sqlite::Value::Text(value),
            other => spin_sdk::sqlite::Value::Text(other.to_string()),
        })
        .collect()
}

#[cfg(feature = "spin-sqlite")]
async fn collect_sqlite_statement(
    connection: &spin_sdk::sqlite::Connection,
    sql: &str,
    params: Vec<spin_sdk::sqlite::Value>,
) -> Result<Vec<serde_json::Value>, String> {
    let rowset = connection
        .execute(sql, params)
        .await
        .map_err(|error| format!("host execute failed: {error:?}"))?;
    let columns = rowset.columns().to_vec();
    let rows = rowset
        .collect()
        .await
        .map_err(|error| format!("host row stream failed: {error:?}"))?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let values = columns
                .iter()
                .zip(row.values)
                .map(|(column, value)| (column.clone(), sqlite_value_json(value)))
                .collect();
            serde_json::Value::Object(values)
        })
        .collect())
}

#[cfg(feature = "spin-sqlite")]
async fn finish_sqlite_statement(
    connection: &spin_sdk::sqlite::Connection,
    sql: &str,
    params: Vec<spin_sdk::sqlite::Value>,
) -> Result<(), String> {
    connection
        .execute(sql, params)
        .await
        .map_err(|error| format!("host execute failed: {error:?}"))?
        .result()
        .await
        .map_err(|error| format!("host completion failed: {error:?}"))
}

#[cfg(feature = "spin-sqlite")]
fn sqlite_value_json(value: spin_sdk::sqlite::Value) -> serde_json::Value {
    match value {
        spin_sdk::sqlite::Value::Null => serde_json::Value::Null,
        spin_sdk::sqlite::Value::Integer(value) => value.into(),
        spin_sdk::sqlite::Value::Real(value) => serde_json::Number::from_f64(value)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        spin_sdk::sqlite::Value::Text(value) => serde_json::Value::String(value),
        spin_sdk::sqlite::Value::Blob(value) => {
            serde_json::Value::String(String::from_utf8_lossy(&value).into_owned())
        }
    }
}

#[cfg(feature = "spin-postgres")]
fn spin_postgres_db_value_json(value: &spin_sdk::pg::DbValue) -> serde_json::Value {
    use spin_sdk::pg::DbValue;

    match value {
        DbValue::DbNull => serde_json::Value::Null,
        DbValue::Boolean(value) => serde_json::Value::Bool(*value),
        DbValue::Int8(value) => i64::from(*value).into(),
        DbValue::Int16(value) => i64::from(*value).into(),
        DbValue::Int32(value) => i64::from(*value).into(),
        DbValue::Int64(value) => (*value).into(),
        DbValue::Floating32(value) => serde_json::Number::from_f64(f64::from(*value))
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        DbValue::Floating64(value) => serde_json::Number::from_f64(*value)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        DbValue::Str(value) => serde_json::Value::String(value.clone()),
        DbValue::Binary(value) => {
            serde_json::Value::String(String::from_utf8_lossy(value).into_owned())
        }
        DbValue::Jsonb(value) => serde_json::from_slice(value).unwrap_or_else(|_| {
            serde_json::Value::String(String::from_utf8_lossy(value).into_owned())
        }),
        DbValue::Unsupported(value) => {
            serde_json::Value::String(String::from_utf8_lossy(value).into_owned())
        }
        other => serde_json::Value::String(format!("{other:?}")),
    }
}

#[cfg(any(feature = "spin-postgres", feature = "spin-mysql"))]
fn spin_sql_returns_rows(sql: &str) -> bool {
    let upper = sql.trim_start().to_ascii_uppercase();
    if upper.starts_with("WITH ") {
        return upper.contains("SELECT") || upper.contains("RETURNING");
    }
    upper.starts_with("SELECT") || upper.contains("RETURNING")
}

/// Execute a Spin Postgres query and return JSON rows for read operations.
///
/// For write commands this returns an empty rowset after successful execution.
/// Prefer [`execute_spin_pg_with_returns_rows`] when the caller knows whether
/// rows are expected (for example CTE reads).
pub async fn execute_spin_pg(
    db_url: &str,
    sql: &str,
    params: Vec<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, String> {
    execute_spin_pg_with_returns_rows(db_url, sql, params, spin_sql_returns_rows(sql)).await
}

#[cfg(feature = "spin-postgres")]
/// Execute a Spin Postgres query with an explicit row-return expectation.
pub async fn execute_spin_pg_with_returns_rows(
    db_url: &str,
    sql: &str,
    params: Vec<serde_json::Value>,
    returns_rows: bool,
) -> Result<Vec<serde_json::Value>, String> {
    use spin_sdk::pg::{Connection as SpinPgConn, ParameterValue as SpinPgParam};

    let pg_params: Vec<SpinPgParam> = params
        .into_iter()
        .map(|v| match v {
            serde_json::Value::Null => SpinPgParam::DbNull,
            serde_json::Value::Bool(b) => SpinPgParam::Boolean(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    SpinPgParam::Int64(i)
                } else if let Some(f) = n.as_f64() {
                    SpinPgParam::Floating64(f)
                } else {
                    SpinPgParam::DbNull
                }
            }
            serde_json::Value::String(s) => SpinPgParam::Str(s),
            other => SpinPgParam::Str(other.to_string()),
        })
        .collect();

    let conn = SpinPgConn::open(db_url)
        .await
        .map_err(|e| format!("Pg connection error: {:?}", e))?;

    if returns_rows {
        let mut rowset = conn
            .query(sql, pg_params)
            .await
            .map_err(|e| format!("Pg query error: {:?}", e))?;
        let col_names: Vec<String> = rowset.columns().iter().map(|c| c.name.clone()).collect();

        let mut rows = Vec::new();
        let rows_reader = rowset.rows();
        while let Some(row) = rows_reader.next().await {
            let mut row_obj = serde_json::Map::new();
            for (i, val) in row.iter().enumerate() {
                let col_name = col_names
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| format!("col_{}", i));
                row_obj.insert(col_name, spin_postgres_db_value_json(val));
            }
            rows.push(serde_json::Value::Object(row_obj));
        }
        Ok(rows)
    } else {
        conn.execute(sql, pg_params)
            .await
            .map_err(|e| format!("Pg execute error: {:?}", e))?;
        Ok(Vec::new())
    }
}

#[cfg(feature = "spin-postgres")]
/// Executes parameterized PostgreSQL statements on one Spin host connection
/// inside `BEGIN` and `COMMIT`.
///
/// A failure attempts `ROLLBACK` on the same connection. Transactions are
/// bounded to 1,024 statements and return one rowset per statement.
pub async fn execute_spin_pg_atomic(
    db_url: &str,
    statements: Vec<SpinSqlStatement>,
) -> Result<Vec<Vec<serde_json::Value>>, String> {
    use spin_sdk::pg::Connection;

    validate_spin_transaction(&statements)?;
    let connection = Connection::open(db_url)
        .await
        .map_err(|error| format!("Pg connection error: {error:?}"))?;
    connection
        .execute("BEGIN", Vec::new())
        .await
        .map_err(|error| format!("Pg begin transaction error: {error:?}"))?;

    let mut output = Vec::with_capacity(statements.len());
    for (index, statement) in statements.into_iter().enumerate() {
        let params = postgres_parameters(statement.params);
        let result = if statement.returns_rows {
            collect_postgres_statement(&connection, &statement.sql, params).await
        } else {
            connection
                .execute(&statement.sql, params)
                .await
                .map(|_| Vec::new())
                .map_err(|error| format!("host execute failed: {error:?}"))
        };
        match result {
            Ok(rows) if rows.len() >= statement.minimum_rows => output.push(rows),
            Ok(_) => {
                let rollback = connection
                    .execute("ROLLBACK", Vec::new())
                    .await
                    .map(|_| ())
                    .map_err(|rollback_error| format!("{rollback_error:?}"));
                return Err(transaction_failure(
                    "Postgres",
                    index,
                    "transaction guard returned too few rows".to_owned(),
                    rollback,
                ));
            }
            Err(error) => {
                let rollback = connection
                    .execute("ROLLBACK", Vec::new())
                    .await
                    .map(|_| ())
                    .map_err(|rollback_error| format!("{rollback_error:?}"));
                return Err(transaction_failure("Postgres", index, error, rollback));
            }
        }
    }

    if let Err(error) = connection.execute("COMMIT", Vec::new()).await {
        let rollback = connection
            .execute("ROLLBACK", Vec::new())
            .await
            .map(|_| ())
            .map_err(|rollback_error| format!("{rollback_error:?}"));
        return Err(transaction_failure(
            "Postgres commit",
            output.len(),
            format!("{error:?}"),
            rollback,
        ));
    }
    Ok(output)
}

#[cfg(feature = "spin-postgres")]
fn postgres_parameters(params: Vec<serde_json::Value>) -> Vec<spin_sdk::pg::ParameterValue> {
    params
        .into_iter()
        .map(|value| match value {
            serde_json::Value::Null => spin_sdk::pg::ParameterValue::DbNull,
            serde_json::Value::Bool(value) => spin_sdk::pg::ParameterValue::Boolean(value),
            serde_json::Value::Number(value) => value.as_i64().map_or_else(
                || {
                    value.as_f64().map_or(
                        spin_sdk::pg::ParameterValue::DbNull,
                        spin_sdk::pg::ParameterValue::Floating64,
                    )
                },
                spin_sdk::pg::ParameterValue::Int64,
            ),
            serde_json::Value::String(value) => spin_sdk::pg::ParameterValue::Str(value),
            other => spin_sdk::pg::ParameterValue::Str(other.to_string()),
        })
        .collect()
}

#[cfg(feature = "spin-postgres")]
async fn collect_postgres_statement(
    connection: &spin_sdk::pg::Connection,
    sql: &str,
    params: Vec<spin_sdk::pg::ParameterValue>,
) -> Result<Vec<serde_json::Value>, String> {
    use spin_sdk::pg::DbValue;

    let mut rowset = connection
        .query(sql, params)
        .await
        .map_err(|error| format!("host query failed: {error:?}"))?;
    let columns = rowset
        .columns()
        .iter()
        .map(|column| column.name.clone())
        .collect::<Vec<_>>();
    let mut rows = Vec::new();
    while let Some(row) = rowset.rows().next().await {
        let values = columns
            .iter()
            .zip(&row)
            .map(|(column, value)| (column.clone(), spin_postgres_db_value_json(value)))
            .collect();
        rows.push(serde_json::Value::Object(values));
    }
    rowset
        .result()
        .await
        .map_err(|error| format!("host row stream failed: {error:?}"))?;
    Ok(rows)
}

#[cfg(any(
    feature = "spin-sqlite",
    feature = "spin-postgres",
    feature = "spin-mysql"
))]
fn validate_spin_transaction(statements: &[SpinSqlStatement]) -> Result<(), String> {
    if statements.is_empty() {
        return Err("Spin SQL transaction requires at least one statement".to_owned());
    }
    if statements.len() > MAX_SPIN_TRANSACTION_STATEMENTS {
        return Err(format!(
            "Spin SQL transaction exceeds {MAX_SPIN_TRANSACTION_STATEMENTS} statements"
        ));
    }
    if statements
        .iter()
        .any(|statement| statement.sql.trim().is_empty())
    {
        return Err("Spin SQL transaction contains an empty statement".to_owned());
    }
    Ok(())
}

#[cfg(any(
    feature = "spin-sqlite",
    feature = "spin-postgres",
    feature = "spin-mysql"
))]
fn transaction_failure(
    backend: &str,
    statement_index: usize,
    operation_error: String,
    rollback: Result<(), String>,
) -> String {
    match rollback {
        Ok(()) => format!(
            "{backend} transaction statement {statement_index} failed and rolled back: {operation_error}"
        ),
        Err(rollback_error) => format!(
            "{backend} transaction statement {statement_index} failed: {operation_error}; rollback also failed: {rollback_error}"
        ),
    }
}

#[cfg(all(
    test,
    any(
        feature = "spin-sqlite",
        feature = "spin-postgres",
        feature = "spin-mysql"
    )
))]
mod spin_transaction_tests {
    use super::*;

    #[test]
    fn statement_modes_preserve_parameters_without_interpolation() {
        let execute = SpinSqlStatement::execute(
            "INSERT INTO secrets(value) VALUES (?1)",
            vec![serde_json::json!("sensitive")],
        );
        let query = SpinSqlStatement::query("SELECT value FROM secrets", Vec::new());
        let guard = SpinSqlStatement::guard(
            "UPDATE secrets SET used = 1 WHERE id = ?1 RETURNING id",
            vec![serde_json::json!(7)],
        );

        assert!(!execute.returns_rows());
        assert_eq!(execute.minimum_rows(), 0);
        assert_eq!(execute.params(), &[serde_json::json!("sensitive")]);
        assert!(query.returns_rows());
        assert_eq!(query.minimum_rows(), 0);
        assert!(guard.returns_rows());
        assert_eq!(guard.minimum_rows(), 1);
    }

    #[test]
    fn transaction_bounds_reject_empty_blank_and_oversized_batches() {
        assert!(validate_spin_transaction(&[]).is_err());
        assert!(validate_spin_transaction(&[SpinSqlStatement::execute(" ", Vec::new())]).is_err());
        let oversized = vec![
            SpinSqlStatement::execute("SELECT 1", Vec::new());
            MAX_SPIN_TRANSACTION_STATEMENTS + 1
        ];
        assert!(validate_spin_transaction(&oversized).is_err());
    }

    #[test]
    fn failure_message_never_formats_statement_parameters() {
        let error = transaction_failure("SQLite", 2, "guard rejected".to_owned(), Ok(()));
        assert_eq!(
            error,
            "SQLite transaction statement 2 failed and rolled back: guard rejected"
        );
        assert!(!error.contains("sensitive"));
    }

    #[test]
    fn debug_redacts_statement_parameters() {
        let statement = SpinSqlStatement::execute(
            "INSERT INTO secrets(value) VALUES (?1)",
            vec![serde_json::json!("sensitive")],
        );
        let debug = format!("{statement:?}");
        assert!(!debug.contains("sensitive"));
        assert!(debug.contains("redacted"));
    }

    #[test]
    fn cte_select_queries_are_treated_as_returning_rows() {
        assert!(spin_sql_returns_rows(
            "WITH totals AS (SELECT 1) SELECT * FROM totals"
        ));
        assert!(!spin_sql_returns_rows("INSERT INTO t VALUES (1)"));
        assert!(!spin_sql_returns_rows("UPDATE t SET x = 1"));
    }

    #[test]
    fn postgres_text_columns_are_not_retyped_as_json() {
        let value = spin_postgres_db_value_json(&spin_sdk::pg::DbValue::Str("123".to_owned()));
        assert_eq!(value, serde_json::Value::String("123".to_owned()));
    }
}

// -------------------------------------------------------------------------
// Spin MySQL adapter
// -------------------------------------------------------------------------
#[cfg(feature = "spin-mysql")]
fn mysql_parameters(params: Vec<serde_json::Value>) -> Vec<spin_sdk::mysql::ParameterValue> {
    params
        .into_iter()
        .map(|value| match value {
            serde_json::Value::Null => spin_sdk::mysql::ParameterValue::DbNull,
            serde_json::Value::Bool(value) => {
                spin_sdk::mysql::ParameterValue::Int8(if value { 1 } else { 0 })
            }
            serde_json::Value::Number(value) => {
                if let Some(value) = value.as_i64() {
                    spin_sdk::mysql::ParameterValue::Int64(value)
                } else if let Some(value) = value.as_u64() {
                    spin_sdk::mysql::ParameterValue::Uint64(value)
                } else if let Some(value) = value.as_f64() {
                    spin_sdk::mysql::ParameterValue::Floating64(value)
                } else {
                    spin_sdk::mysql::ParameterValue::DbNull
                }
            }
            serde_json::Value::String(value) => spin_sdk::mysql::ParameterValue::Str(value),
            value => spin_sdk::mysql::ParameterValue::Str(value.to_string()),
        })
        .collect()
}

#[cfg(feature = "spin-mysql")]
fn spin_mysql_returns_rows(sql: &str) -> bool {
    spin_sql_returns_rows(sql)
}

#[cfg(feature = "spin-mysql")]
fn mysql_insert_values_row_count(insert_sql: &str) -> u64 {
    let upper = insert_sql.to_ascii_uppercase();
    let Some(values_start) = upper.rfind(" VALUES ") else {
        return 1;
    };
    let values_clause = insert_sql[values_start + " VALUES ".len()..].trim();
    let values_clause = values_clause
        .split(" RETURNING ")
        .next()
        .unwrap_or(values_clause);

    let mut rows = 0u64;
    let mut depth = 0i32;
    for ch in values_clause.chars() {
        match ch {
            '(' if depth == 0 => {
                rows += 1;
                depth = 1;
            }
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
    }
    rows.max(1)
}

#[cfg(feature = "spin-mysql")]
fn spin_mysql_insert_returning_limit(sql: &str) -> Option<(String, u64)> {
    let trimmed = sql.trim();
    let upper = trimmed.to_ascii_uppercase();
    if !upper.starts_with("INSERT") || !upper.contains(" RETURNING SEQUENCE") {
        return None;
    }
    let insert_sql = trimmed
        .replace(" RETURNING sequence", "")
        .replace(" RETURNING SEQUENCE", "");
    let limit = mysql_insert_values_row_count(&insert_sql);
    Some((insert_sql, limit))
}

#[cfg(feature = "spin-mysql")]
/// Execute a Spin MySQL query and return JSON rows for read operations.
///
/// `?` placeholders are bound by the Spin host. Callers that need JSON columns
/// decoded as text must include an explicit `CAST(... AS CHAR CHARACTER SET utf8mb4)`
/// in their SELECT list; this helper does not rewrite SQL.
pub async fn execute_spin_mysql(
    db_url: &str,
    sql: &str,
    params: Vec<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, String> {
    use spin_sdk::mysql::Connection as SpinMysqlConn;

    let mysql_params = mysql_parameters(params);
    let conn = SpinMysqlConn::open(db_url)
        .await
        .map_err(|error| format!("MySQL connection error: {error:?}"))?;

    if let Some((insert_sql, limit)) = spin_mysql_insert_returning_limit(sql) {
        return spin_mysql_insert_and_read_sequences(&conn, &insert_sql, &mysql_params, limit)
            .await;
    }

    if spin_mysql_returns_rows(sql) {
        spin_mysql_query_rows(&conn, sql, &mysql_params).await
    } else {
        conn.execute(sql, mysql_params.as_slice())
            .await
            .map_err(|error| format!("MySQL execute error: {error:?}"))?;
        Ok(Vec::new())
    }
}

#[cfg(feature = "spin-mysql")]
/// Executes parameterized MySQL statements on one Spin host connection inside
/// `START TRANSACTION` and `COMMIT`.
pub async fn execute_spin_mysql_atomic(
    db_url: &str,
    statements: Vec<SpinSqlStatement>,
) -> Result<Vec<Vec<serde_json::Value>>, String> {
    use spin_sdk::mysql::Connection;

    validate_spin_transaction(&statements)?;
    let connection = Connection::open(db_url)
        .await
        .map_err(|error| format!("MySQL connection error: {error:?}"))?;
    connection
        .execute("START TRANSACTION", &[])
        .await
        .map_err(|error| format!("MySQL begin transaction error: {error:?}"))?;

    let mut output = Vec::with_capacity(statements.len());
    for (index, statement) in statements.into_iter().enumerate() {
        let params = mysql_parameters(statement.params);
        let result = if statement.returns_rows {
            spin_mysql_query_rows(&connection, &statement.sql, &params).await
        } else {
            connection
                .execute(&statement.sql, params.as_slice())
                .await
                .map(|_| Vec::new())
                .map_err(|error| format!("host execute failed: {error:?}"))
        };
        match result {
            Ok(rows) if rows.len() >= statement.minimum_rows => output.push(rows),
            Ok(_) => {
                let rollback = connection
                    .execute("ROLLBACK", &[])
                    .await
                    .map(|_| ())
                    .map_err(|rollback_error| format!("{rollback_error:?}"));
                return Err(transaction_failure(
                    "MySQL",
                    index,
                    "transaction guard returned too few rows".to_owned(),
                    rollback,
                ));
            }
            Err(error) => {
                let rollback = connection
                    .execute("ROLLBACK", &[])
                    .await
                    .map(|_| ())
                    .map_err(|rollback_error| format!("{rollback_error:?}"));
                return Err(transaction_failure("MySQL", index, error, rollback));
            }
        }
    }

    if let Err(error) = connection.execute("COMMIT", &[]).await {
        let rollback = connection
            .execute("ROLLBACK", &[])
            .await
            .map(|_| ())
            .map_err(|rollback_error| format!("{rollback_error:?}"));
        return Err(transaction_failure(
            "MySQL commit",
            output.len(),
            format!("{error:?}"),
            rollback,
        ));
    }
    Ok(output)
}

#[cfg(feature = "spin-mysql")]
async fn spin_mysql_insert_and_read_sequences(
    conn: &spin_sdk::mysql::Connection,
    insert_sql: &str,
    params: &[spin_sdk::mysql::ParameterValue],
    limit: u64,
) -> Result<Vec<serde_json::Value>, String> {
    conn.execute("START TRANSACTION", &[])
        .await
        .map_err(|error| format!("MySQL begin transaction error: {error:?}"))?;

    let insert_result = conn.execute(insert_sql, params).await;
    if let Err(error) = insert_result {
        let _ = conn.execute("ROLLBACK", &[]).await;
        return Err(format!("MySQL execute error: {error:?}"));
    }

    let table = mysql_insert_target_table(insert_sql).unwrap_or("events");
    let read_back = format!(
        "SELECT sequence FROM {table} WHERE sequence >= LAST_INSERT_ID() \
         ORDER BY sequence ASC LIMIT {limit}"
    );
    let rows = spin_mysql_query_rows(conn, &read_back, &[]).await;
    if let Err(error) = &rows {
        let _ = conn.execute("ROLLBACK", &[]).await;
        return Err(error.clone());
    }

    conn.execute("COMMIT", &[])
        .await
        .map_err(|error| format!("MySQL commit error: {error:?}"))?;
    rows
}

#[cfg(feature = "spin-mysql")]
fn mysql_insert_target_table(insert_sql: &str) -> Option<&str> {
    let upper = insert_sql.to_ascii_uppercase();
    let into_idx = upper.find("INTO ")? + 5;
    let rest = insert_sql[into_idx..].trim_start();
    rest.split_whitespace().next()
}

#[cfg(feature = "spin-mysql")]
async fn spin_mysql_query_rows(
    conn: &spin_sdk::mysql::Connection,
    sql: &str,
    params: &[spin_sdk::mysql::ParameterValue],
) -> Result<Vec<serde_json::Value>, String> {
    let mut rowset = conn
        .query(sql, params)
        .await
        .map_err(|error| format!("MySQL query error: {error:?}"))?;
    let col_names: Vec<String> = rowset
        .columns()
        .iter()
        .map(|col| col.name.clone())
        .collect();
    let mut rows = Vec::new();

    while let Some(row) = rowset.rows().next().await {
        let mut row_obj = serde_json::Map::new();
        for (index, value) in row.iter().enumerate() {
            let col_name = col_names
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("col_{index}"));
            row_obj.insert(col_name, spin_mysql_value_to_json(value));
        }
        rows.push(serde_json::Value::Object(row_obj));
    }
    rowset
        .result()
        .await
        .map_err(|error| format!("MySQL query stream error: {error:?}"))?;

    Ok(rows)
}

#[cfg(feature = "spin-mysql")]
fn spin_mysql_value_to_json(value: &spin_sdk::mysql::DbValue) -> serde_json::Value {
    use spin_sdk::mysql::DbValue as SpinMysqlDbVal;

    match value {
        SpinMysqlDbVal::DbNull => serde_json::Value::Null,
        SpinMysqlDbVal::Int8(value) => serde_json::Value::Number((*value as i32).into()),
        SpinMysqlDbVal::Int16(value) => serde_json::Value::Number((*value as i32).into()),
        SpinMysqlDbVal::Int32(value) => serde_json::Value::Number((*value).into()),
        SpinMysqlDbVal::Int64(value) => serde_json::Value::Number((*value).into()),
        SpinMysqlDbVal::Uint8(value) => serde_json::Value::Number((*value as u32).into()),
        SpinMysqlDbVal::Uint16(value) => serde_json::Value::Number((*value as u32).into()),
        SpinMysqlDbVal::Uint32(value) => serde_json::Value::Number((*value).into()),
        SpinMysqlDbVal::Uint64(value) => serde_json::Value::Number((*value).into()),
        SpinMysqlDbVal::Floating32(value) => serde_json::Number::from_f64(*value as f64)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        SpinMysqlDbVal::Floating64(value) => serde_json::Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        SpinMysqlDbVal::Str(value) => serde_json::Value::String(value.clone()),
        SpinMysqlDbVal::Binary(value) => {
            serde_json::Value::String(String::from_utf8_lossy(value).into_owned())
        }
        other => serde_json::Value::String(format!("{other:?}")),
    }
}

#[cfg(all(test, feature = "spin-mysql"))]
mod spin_mysql_tests {
    use super::*;

    #[test]
    fn insert_returning_is_detected_without_sql_rewrite() {
        let sql = "INSERT INTO events (event_id) VALUES (?) RETURNING sequence";
        let (insert_sql, limit) = spin_mysql_insert_returning_limit(sql).unwrap();
        assert!(!insert_sql.contains("RETURNING"));
        assert_eq!(limit, 1);
    }

    #[test]
    fn insert_returning_counts_multi_row_values() {
        let sql = "INSERT INTO events (event_id) VALUES (?), (?), (?) RETURNING sequence";
        let (_, limit) = spin_mysql_insert_returning_limit(sql).unwrap();
        assert_eq!(limit, 3);
    }
}
