// -------------------------------------------------------------------------
// Spin SQLite adapter
// -------------------------------------------------------------------------
#[cfg(any(feature = "spin-sqlite", feature = "spin-postgres"))]
/// One parameterized statement in a bounded Spin host transaction.
///
/// Parameters are kept separate from SQL so callers never interpolate secret
/// material. Use [`Self::query`] only when returned rows are required.
#[derive(Clone, Debug, PartialEq)]
pub struct SpinSqlStatement {
    sql: String,
    params: Vec<serde_json::Value>,
    returns_rows: bool,
    minimum_rows: usize,
}

#[cfg(any(feature = "spin-sqlite", feature = "spin-postgres"))]
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

#[cfg(any(feature = "spin-sqlite", feature = "spin-postgres"))]
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

// -------------------------------------------------------------------------
// Spin Postgres adapter
// -------------------------------------------------------------------------
#[cfg(feature = "spin-postgres")]
/// Execute a Spin Postgres query and return JSON rows for read operations.
///
/// For write commands this returns an empty rowset after successful execution.
pub async fn execute_spin_pg(
    db_url: &str,
    sql: &str,
    params: Vec<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, String> {
    use spin_sdk::pg::{
        Connection as SpinPgConn, DbValue as SpinPgDbVal, ParameterValue as SpinPgParam,
    };

    // Check if query is SELECT or modifying command that returns rows (e.g. contains RETURNING)
    let sql_upper = sql.trim_start().to_ascii_uppercase();
    let is_select = sql_upper.starts_with("SELECT") || sql_upper.contains("RETURNING");

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

    if is_select {
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
                let json_val = match val {
                    SpinPgDbVal::DbNull => serde_json::Value::Null,
                    SpinPgDbVal::Boolean(b) => serde_json::Value::Bool(*b),
                    SpinPgDbVal::Int8(i) => serde_json::Value::Number((*i as i32).into()),
                    SpinPgDbVal::Int16(i) => serde_json::Value::Number((*i as i32).into()),
                    SpinPgDbVal::Int32(i) => serde_json::Value::Number((*i).into()),
                    SpinPgDbVal::Int64(i) => serde_json::Value::Number((*i).into()),
                    SpinPgDbVal::Floating32(f) => {
                        if let Some(num) = serde_json::Number::from_f64(*f as f64) {
                            serde_json::Value::Number(num)
                        } else {
                            serde_json::Value::Null
                        }
                    }
                    SpinPgDbVal::Floating64(f) => {
                        if let Some(num) = serde_json::Number::from_f64(*f) {
                            serde_json::Value::Number(num)
                        } else {
                            serde_json::Value::Null
                        }
                    }
                    SpinPgDbVal::Str(s) => {
                        if let Ok(jv) = serde_json::from_str::<serde_json::Value>(s) {
                            jv
                        } else {
                            serde_json::Value::String(s.clone())
                        }
                    }
                    SpinPgDbVal::Binary(b) => {
                        serde_json::Value::String(String::from_utf8_lossy(b).into_owned())
                    }
                    SpinPgDbVal::Jsonb(j) => {
                        if let Ok(jv) = serde_json::from_slice::<serde_json::Value>(j) {
                            jv
                        } else {
                            serde_json::Value::String(String::from_utf8_lossy(j).into_owned())
                        }
                    }
                    SpinPgDbVal::Unsupported(b) => {
                        if let Ok(jv) = serde_json::from_slice::<serde_json::Value>(b) {
                            jv
                        } else {
                            serde_json::Value::String(String::from_utf8_lossy(b).into_owned())
                        }
                    }
                    other => serde_json::Value::String(format!("{:?}", other)),
                };
                row_obj.insert(col_name, json_val);
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
            .map(|(column, value)| {
                let value = match value {
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
                    DbValue::Str(value) => serde_json::from_str(value)
                        .unwrap_or_else(|_| serde_json::Value::String(value.clone())),
                    DbValue::Binary(value) | DbValue::Unsupported(value) => {
                        serde_json::Value::String(String::from_utf8_lossy(value).into_owned())
                    }
                    DbValue::Jsonb(value) => serde_json::from_slice(value).unwrap_or_else(|_| {
                        serde_json::Value::String(String::from_utf8_lossy(value).into_owned())
                    }),
                    other => serde_json::Value::String(format!("{other:?}")),
                };
                (column.clone(), value)
            })
            .collect();
        rows.push(serde_json::Value::Object(values));
    }
    rowset
        .result()
        .await
        .map_err(|error| format!("host row stream failed: {error:?}"))?;
    Ok(rows)
}

#[cfg(any(feature = "spin-sqlite", feature = "spin-postgres"))]
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

#[cfg(any(feature = "spin-sqlite", feature = "spin-postgres"))]
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

#[cfg(all(test, any(feature = "spin-sqlite", feature = "spin-postgres")))]
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
}

// -------------------------------------------------------------------------
// Spin MySQL adapter
// -------------------------------------------------------------------------
#[cfg(feature = "spin-mysql")]
/// Execute a Spin MySQL query and return JSON rows for read operations.
///
/// `?` placeholders are interpolated into SQL-safe values before execution.
pub async fn execute_spin_mysql(
    db_url: &str,
    sql: &str,
    params: Vec<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, String> {
    use spin_sdk::mysql::{Connection as SpinMysqlConn, ParameterValue as SpinMysqlParam};

    let mysql_params: Vec<SpinMysqlParam> = params
        .into_iter()
        .map(|value| match value {
            serde_json::Value::Null => SpinMysqlParam::DbNull,
            serde_json::Value::Bool(value) => SpinMysqlParam::Int8(if value { 1 } else { 0 }),
            serde_json::Value::Number(value) => {
                if let Some(value) = value.as_i64() {
                    SpinMysqlParam::Int64(value)
                } else if let Some(value) = value.as_u64() {
                    SpinMysqlParam::Uint64(value)
                } else if let Some(value) = value.as_f64() {
                    SpinMysqlParam::Floating64(value)
                } else {
                    SpinMysqlParam::DbNull
                }
            }
            serde_json::Value::String(value) => SpinMysqlParam::Str(value),
            value => SpinMysqlParam::Str(value.to_string()),
        })
        .collect();

    let sql_upper = sql.trim_start().to_ascii_uppercase();
    let returns_rows = sql_upper.starts_with("SELECT") || sql_upper.contains("RETURNING");
    let conn = SpinMysqlConn::open(db_url)
        .await
        .map_err(|error| format!("MySQL connection error: {error:?}"))?;

    if returns_rows {
        let query_sql = sql.replace(" RETURNING sequence", "");
        if query_sql != sql {
            conn.execute(&query_sql, mysql_params.as_slice())
                .await
                .map_err(|error| format!("MySQL execute error: {error:?}"))?;
            return spin_mysql_query_rows(&conn, "SELECT LAST_INSERT_ID() AS sequence", &[]).await;
        }

        let query_sql = spin_mysql_select_sql(sql);
        spin_mysql_query_rows(&conn, &query_sql, &mysql_params).await
    } else {
        conn.execute(sql, mysql_params.as_slice())
            .await
            .map_err(|error| format!("MySQL execute error: {error:?}"))?;
        Ok(Vec::new())
    }
}

#[cfg(feature = "spin-mysql")]
fn spin_mysql_select_sql(sql: &str) -> String {
    // Spin's MySQL SDK cannot currently convert MySQL JSON columns directly;
    // cast event JSON columns to strings before RowSet decoding.
    if sql.contains(" AS payload") {
        return sql.to_string();
    }

    sql.replace(
        "payload, metadata",
        "CAST(payload AS CHAR(10000) CHARACTER SET utf8mb4) AS payload, CAST(metadata AS CHAR(10000) CHARACTER SET utf8mb4) AS metadata",
    )
    .replace(
        "payload, recorded_at_ms",
        "CAST(payload AS CHAR(10000) CHARACTER SET utf8mb4) AS payload, recorded_at_ms",
    )
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
        SpinMysqlDbVal::Str(value) => serde_json::from_str::<serde_json::Value>(value)
            .unwrap_or_else(|_| serde_json::Value::String(value.clone())),
        SpinMysqlDbVal::Binary(value) => {
            serde_json::Value::String(String::from_utf8_lossy(value).into_owned())
        }
        other => serde_json::Value::String(format!("{other:?}")),
    }
}
