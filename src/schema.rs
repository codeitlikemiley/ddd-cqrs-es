//! # Versioned Schema Migration System
//!
//! This module provides a lightweight, framework-owned database schema migrator
//! for events, projection checkpoints, and idempotency keys, supporting SQLite,
//! Postgres, and MySQL. It avoids external heavy migration libraries.

#[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
use crate::error::EventStoreError;
#[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
use crate::sql_common::system_time_to_millis;
#[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
use std::collections::HashSet;
#[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
use std::time::SystemTime;

/// Fixed advisory-lock key serializing Postgres schema migration runs
/// (the ASCII bytes of `ddd_sche` as a big-endian i64).
#[cfg(feature = "postgres")]
const POSTGRES_SCHEMA_LOCK_KEY: i64 = 0x6464_645f_7363_6865;

/// Named lock serializing MySQL schema migration runs.
#[cfg(feature = "mysql")]
const MYSQL_SCHEMA_LOCK_NAME: &str = "ddd_cqrs_es_schema_migrations";

/// How long a MySQL migration run waits for the named lock.
#[cfg(feature = "mysql")]
const MYSQL_SCHEMA_LOCK_TIMEOUT_SECS: i64 = 60;

/// Maps a Postgres migration error, preferring the server's detailed message
/// over the vague client Display and attaching the SQLSTATE code.
#[cfg(feature = "postgres")]
fn map_postgres_schema_error(error: postgres::Error) -> EventStoreError {
    let code = error.code().map(|state| state.code().to_owned());
    let message = error
        .as_db_error()
        .map(|db| db.message().to_owned())
        .unwrap_or_else(|| error.to_string());
    let mapped = EventStoreError::backend_with_source(message, error);
    match code {
        Some(code) => mapped.with_code(code),
        None => mapped,
    }
}

/// Maps a MySQL migration error, attaching the server errno when present.
#[cfg(feature = "mysql")]
fn map_mysql_schema_error(error: mysql::Error) -> EventStoreError {
    let code = match &error {
        mysql::Error::MySqlError(server) => Some(server.code.to_string()),
        _ => None,
    };
    let mapped = EventStoreError::backend_with_source(error.to_string(), error);
    match code {
        Some(code) => mapped.with_code(code),
        None => mapped,
    }
}

/// Refusal returned when the migrations bookkeeping table exists without the
/// composite `(version, table_name)` layout this version records against.
///
/// Older releases dropped the table here. That is destructive whenever the
/// probe was wrong (a failed probe, a same-named table in another schema, or an
/// unrelated application table), so the decision belongs to the operator.
#[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
fn legacy_migrations_table_error(table: &str) -> EventStoreError {
    EventStoreError::backend(format!(
        "`{table}` exists without the `table_name` column that schema migration \
         bookkeeping requires, and this migrator will not drop it. If it is this \
         framework's pre-0.3 bookkeeping table, run `DROP TABLE {table};` and \
         rerun the migration (framework migrations are idempotent and re-record \
         themselves). If it belongs to your application, point the migrator at \
         another name with `SqlSchemaConfig::with_migrations_table`."
    ))
}

/// Supported SQL Database Dialects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlDialect {
    Sqlite,
    Postgres,
    MySql,
}

/// Schema configuration with customizable table names.
#[derive(Debug, Clone)]
pub struct SqlSchemaConfig {
    dialect: SqlDialect,
    events_table: String,
    checkpoints_table: String,
    idempotency_table: String,
    snapshots_table: String,
    migrations_table: String,
}

impl SqlSchemaConfig {
    /// Creates a schema configuration with default table names for the given dialect.
    pub fn new(dialect: SqlDialect) -> Self {
        Self {
            dialect,
            events_table: "events".to_string(),
            checkpoints_table: "projection_checkpoints".to_string(),
            idempotency_table: "idempotency_keys".to_string(),
            snapshots_table: "snapshots".to_string(),
            migrations_table: "schema_migrations".to_string(),
        }
    }

    /// Returns the SQL dialect.
    pub fn dialect(&self) -> SqlDialect {
        self.dialect
    }

    /// Returns the configured events table name.
    pub fn events_table(&self) -> &str {
        &self.events_table
    }

    /// Returns the configured checkpoints table name.
    pub fn checkpoints_table(&self) -> &str {
        &self.checkpoints_table
    }

    /// Returns the configured idempotency table name.
    pub fn idempotency_table(&self) -> &str {
        &self.idempotency_table
    }

    /// Returns the configured snapshots table name.
    pub fn snapshots_table(&self) -> &str {
        &self.snapshots_table
    }

    /// Returns the configured migrations table name.
    pub fn migrations_table(&self) -> &str {
        &self.migrations_table
    }

    /// Validates all configured SQL table names.
    #[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
    pub fn validate(&self) -> Result<(), EventStoreError> {
        crate::sql_common::validate_table_name(&self.events_table)?;
        crate::sql_common::validate_table_name(&self.checkpoints_table)?;
        crate::sql_common::validate_table_name(&self.idempotency_table)?;
        crate::sql_common::validate_table_name(&self.snapshots_table)?;
        crate::sql_common::validate_table_name(&self.migrations_table)?;
        Ok(())
    }

    /// Sets a custom events table name.
    #[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
    pub fn with_events_table(mut self, name: impl Into<String>) -> Result<Self, EventStoreError> {
        let name = name.into();
        crate::sql_common::validate_table_name(&name)?;
        self.events_table = name;
        Ok(self)
    }

    /// Sets a custom checkpoints table name.
    #[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
    pub fn with_checkpoints_table(
        mut self,
        name: impl Into<String>,
    ) -> Result<Self, EventStoreError> {
        let name = name.into();
        crate::sql_common::validate_table_name(&name)?;
        self.checkpoints_table = name;
        Ok(self)
    }

    /// Sets a custom idempotency table name.
    #[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
    pub fn with_idempotency_table(
        mut self,
        name: impl Into<String>,
    ) -> Result<Self, EventStoreError> {
        let name = name.into();
        crate::sql_common::validate_table_name(&name)?;
        self.idempotency_table = name;
        Ok(self)
    }

    /// Sets a custom snapshots table name.
    #[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
    pub fn with_snapshots_table(
        mut self,
        name: impl Into<String>,
    ) -> Result<Self, EventStoreError> {
        let name = name.into();
        crate::sql_common::validate_table_name(&name)?;
        self.snapshots_table = name;
        Ok(self)
    }

    /// Sets a custom migrations table name.
    #[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
    pub fn with_migrations_table(
        mut self,
        name: impl Into<String>,
    ) -> Result<Self, EventStoreError> {
        let name = name.into();
        crate::sql_common::validate_table_name(&name)?;
        self.migrations_table = name;
        Ok(self)
    }

    /// Interpolates a SQL string replacing the placeholders with configured table names.
    pub fn interpolate(&self, sql: &str) -> String {
        sql.replace("{events_table}", &self.events_table)
            .replace("{checkpoints_table}", &self.checkpoints_table)
            .replace("{idempotency_table}", &self.idempotency_table)
            .replace("{snapshots_table}", &self.snapshots_table)
            .replace("{migrations_table}", &self.migrations_table)
    }
}

/// A representation of a versioned schema migration.
#[derive(Debug, Clone)]
pub struct SchemaMigration {
    /// Migration version, used for ordering and idempotent replay.
    pub version: i32,
    /// Short migration name used for observability and debugging.
    pub description: &'static str,
    /// Forward migration SQL (`up`) statement with `{..._table}` placeholders.
    pub up_sql: &'static str,
}

/// Canonical framework-owned migrations.
/// Returns the built-in migration list for a given SQL dialect in ascending version order.
pub fn get_migrations(dialect: SqlDialect) -> Vec<SchemaMigration> {
    match dialect {
        SqlDialect::Sqlite => vec![
            SchemaMigration {
                version: 1,
                description: "create_events_table",
                up_sql: r#"
                    CREATE TABLE IF NOT EXISTS {events_table} (
                        sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                        event_id TEXT NOT NULL UNIQUE,
                        aggregate_id TEXT NOT NULL,
                        aggregate_type TEXT NOT NULL,
                        revision INTEGER NOT NULL,
                        event_type TEXT NOT NULL,
                        event_version INTEGER NOT NULL,
                        payload TEXT NOT NULL,
                        metadata TEXT NOT NULL,
                        recorded_at_ms INTEGER NOT NULL,
                        UNIQUE (aggregate_type, aggregate_id, revision)
                    );
                "#,
            },
            SchemaMigration {
                version: 2,
                description: "create_checkpoints_table",
                up_sql: r#"
                    CREATE TABLE IF NOT EXISTS {checkpoints_table} (
                        projection_name TEXT PRIMARY KEY,
                        sequence INTEGER NOT NULL
                    );
                "#,
            },
            SchemaMigration {
                version: 3,
                description: "create_idempotency_table",
                up_sql: r#"
                    CREATE TABLE IF NOT EXISTS {idempotency_table} (
                        idempotency_key TEXT PRIMARY KEY,
                        state TEXT NOT NULL CHECK (state IN ('pending', 'complete')),
                        value TEXT,
                        updated_at_ms INTEGER NOT NULL
                    );
                "#,
            },
            SchemaMigration {
                version: 4,
                description: "create_snapshots_table",
                up_sql: r#"
                    CREATE TABLE IF NOT EXISTS {snapshots_table} (
                        aggregate_type TEXT NOT NULL,
                        aggregate_id TEXT NOT NULL,
                        revision INTEGER NOT NULL,
                        state TEXT NOT NULL,
                        metadata TEXT NOT NULL,
                        recorded_at_ms INTEGER NOT NULL,
                        PRIMARY KEY (aggregate_type, aggregate_id)
                    );
                "#,
            },
            SchemaMigration {
                version: 5,
                description: "create_events_global_replay_index",
                up_sql: r#"
                    CREATE INDEX IF NOT EXISTS {events_table}_global_replay_idx
                        ON {events_table} (aggregate_type, sequence);
                "#,
            },
            SchemaMigration {
                version: 6,
                description: "drop_duplicate_events_stream_index",
                up_sql: r#"
                    DROP INDEX IF EXISTS {events_table}_stream_idx;
                "#,
            },
            SchemaMigration {
                version: 7,
                description: "idempotency_lease_columns",
                up_sql: r#"
                    ALTER TABLE {idempotency_table} ADD COLUMN owner TEXT;
                    ALTER TABLE {idempotency_table} ADD COLUMN expires_at_ms INTEGER;
                    CREATE INDEX IF NOT EXISTS {idempotency_table}_pending_updated_idx
                        ON {idempotency_table} (updated_at_ms)
                        WHERE state = 'pending';
                "#,
            },
            SchemaMigration {
                version: 8,
                description: "idempotency_completed_purge_index",
                up_sql: r#"
                    CREATE INDEX IF NOT EXISTS {idempotency_table}_completed_updated_idx
                        ON {idempotency_table} (updated_at_ms)
                        WHERE state = 'complete';
                "#,
            },
        ],
        SqlDialect::Postgres => vec![
            SchemaMigration {
                version: 1,
                description: "create_events_table",
                up_sql: r#"
                    CREATE TABLE IF NOT EXISTS {events_table} (
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
                "#,
            },
            SchemaMigration {
                version: 2,
                description: "create_checkpoints_table",
                up_sql: r#"
                    CREATE TABLE IF NOT EXISTS {checkpoints_table} (
                        projection_name VARCHAR(255) PRIMARY KEY,
                        sequence BIGINT NOT NULL
                    );
                "#,
            },
            SchemaMigration {
                version: 3,
                description: "create_idempotency_table",
                up_sql: r#"
                    CREATE TABLE IF NOT EXISTS {idempotency_table} (
                        idempotency_key VARCHAR(255) PRIMARY KEY,
                        state VARCHAR(20) NOT NULL CHECK (state IN ('pending', 'complete')),
                        value JSONB,
                        updated_at_ms BIGINT NOT NULL
                    );
                "#,
            },
            SchemaMigration {
                version: 4,
                description: "create_snapshots_table",
                up_sql: r#"
                    CREATE TABLE IF NOT EXISTS {snapshots_table} (
                        aggregate_type TEXT NOT NULL,
                        aggregate_id TEXT NOT NULL,
                        revision BIGINT NOT NULL,
                        state JSONB NOT NULL,
                        metadata JSONB NOT NULL,
                        recorded_at_ms BIGINT NOT NULL,
                        PRIMARY KEY (aggregate_type, aggregate_id)
                    );
                "#,
            },
            SchemaMigration {
                version: 5,
                description: "create_events_global_replay_index",
                up_sql: r#"
                    CREATE INDEX IF NOT EXISTS {events_table}_global_replay_idx
                        ON {events_table} (aggregate_type, sequence);
                "#,
            },
            SchemaMigration {
                version: 6,
                description: "drop_duplicate_events_stream_index",
                up_sql: r#"
                    DROP INDEX IF EXISTS {events_table}_stream_idx;
                "#,
            },
            SchemaMigration {
                version: 7,
                description: "idempotency_lease_columns",
                up_sql: r#"
                    ALTER TABLE {idempotency_table} ADD COLUMN IF NOT EXISTS owner TEXT;
                    ALTER TABLE {idempotency_table} ADD COLUMN IF NOT EXISTS expires_at_ms BIGINT;
                    CREATE INDEX IF NOT EXISTS {idempotency_table}_pending_updated_idx
                        ON {idempotency_table} (updated_at_ms)
                        WHERE state = 'pending';
                "#,
            },
            SchemaMigration {
                version: 8,
                description: "idempotency_completed_purge_index",
                up_sql: r#"
                    CREATE INDEX IF NOT EXISTS {idempotency_table}_completed_updated_idx
                        ON {idempotency_table} (updated_at_ms)
                        WHERE state = 'complete';
                "#,
            },
        ],
        SqlDialect::MySql => vec![
            SchemaMigration {
                version: 1,
                description: "create_events_table",
                up_sql: r#"
                    CREATE TABLE IF NOT EXISTS {events_table} (
                        sequence BIGINT AUTO_INCREMENT PRIMARY KEY,
                        event_id VARCHAR(255) NOT NULL UNIQUE,
                        aggregate_id VARCHAR(255) NOT NULL,
                        aggregate_type VARCHAR(255) NOT NULL,
                        revision BIGINT NOT NULL,
                        event_type VARCHAR(255) NOT NULL,
                        event_version INT NOT NULL,
                        payload JSON NOT NULL,
                        metadata JSON NOT NULL,
                        recorded_at_ms BIGINT NOT NULL,
                        UNIQUE KEY (aggregate_type, aggregate_id, revision)
                    );
                "#,
            },
            SchemaMigration {
                version: 2,
                description: "create_checkpoints_table",
                up_sql: r#"
                    CREATE TABLE IF NOT EXISTS {checkpoints_table} (
                        projection_name VARCHAR(255) PRIMARY KEY,
                        sequence BIGINT NOT NULL
                    );
                "#,
            },
            SchemaMigration {
                version: 3,
                description: "create_idempotency_table",
                up_sql: r#"
                    CREATE TABLE IF NOT EXISTS {idempotency_table} (
                        idempotency_key VARCHAR(255) PRIMARY KEY,
                        state VARCHAR(20) NOT NULL CHECK (state IN ('pending', 'complete')),
                        value JSON,
                        updated_at_ms BIGINT NOT NULL
                    );
                "#,
            },
            SchemaMigration {
                version: 4,
                description: "create_snapshots_table",
                up_sql: r#"
                    CREATE TABLE IF NOT EXISTS {snapshots_table} (
                        aggregate_type VARCHAR(255) NOT NULL,
                        aggregate_id VARCHAR(255) NOT NULL,
                        revision BIGINT NOT NULL,
                        state JSON NOT NULL,
                        metadata JSON NOT NULL,
                        recorded_at_ms BIGINT NOT NULL,
                        PRIMARY KEY (aggregate_type, aggregate_id)
                    );
                "#,
            },
            SchemaMigration {
                version: 5,
                description: "create_events_global_replay_index",
                up_sql: r#"
                    CREATE INDEX {events_table}_global_replay_idx
                        ON {events_table} (aggregate_type, sequence);
                "#,
            },
            SchemaMigration {
                version: 6,
                description: "drop_duplicate_events_stream_index",
                up_sql: "SELECT 1;",
            },
            SchemaMigration {
                version: 7,
                description: "idempotency_lease_columns",
                up_sql: r#"
                    ALTER TABLE {idempotency_table}
                        ADD COLUMN owner VARCHAR(255) NULL,
                        ADD COLUMN expires_at_ms BIGINT NULL;
                    CREATE INDEX {idempotency_table}_pending_updated_idx
                        ON {idempotency_table} (updated_at_ms);
                "#,
            },
            SchemaMigration {
                version: 8,
                description: "idempotency_completed_purge_index",
                up_sql: r#"
                    CREATE INDEX {idempotency_table}_completed_idx
                        ON {idempotency_table} (updated_at_ms, state);
                "#,
            },
        ],
    }
}

#[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
fn get_target_table_name(version: i32, config: &SqlSchemaConfig) -> &str {
    match version {
        1 => &config.events_table,
        2 => &config.checkpoints_table,
        3 => &config.idempotency_table,
        4 => &config.snapshots_table,
        5 => &config.events_table,
        6 => &config.events_table,
        7 => &config.idempotency_table,
        8 => &config.idempotency_table,
        _ => "",
    }
}

/// The Versioned Schema Migrator.
///
/// # Atomicity & Transaction Limits
/// While `SchemaMigrator` runs are idempotent, schema migration is not fully transaction-wrapped
/// across all database dialects. Specifically, in **MySQL**, DDL commands (such as `CREATE TABLE` and
/// `DROP TABLE`) trigger **implicit commits**. This means any DDL operation executed during a migration
/// run commits immediately, and cannot be rolled back mid-transaction if a subsequent migration step fails.
///
/// Users should ensure they have proper database backups and verify migration files before applying
/// them to a live MySQL environment.
#[derive(Debug, Clone)]
pub struct SchemaMigrator {
    #[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
    config: SqlSchemaConfig,
    #[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
    migration_versions: Option<Vec<i32>>,
}

impl SchemaMigrator {
    /// Creates a migrator for a given configuration.
    pub fn new(config: SqlSchemaConfig) -> Self {
        #[cfg(not(any(feature = "sqlite", feature = "postgres", feature = "mysql")))]
        {
            let _ = config;
            Self {}
        }

        #[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
        {
            Self {
                config,
                migration_versions: None,
            }
        }
    }

    /// Creates a migrator that applies only checkpoint-table migrations.
    #[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
    pub fn for_checkpoints(config: SqlSchemaConfig) -> Self {
        Self::with_migration_versions(config, &[2])
    }

    /// Creates a migrator that applies only idempotency-table migrations.
    #[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
    pub fn for_idempotency(config: SqlSchemaConfig) -> Self {
        Self::with_migration_versions(config, &[3, 7])
    }

    /// Creates a migrator that applies only snapshot-table migrations.
    #[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
    pub fn for_snapshots(config: SqlSchemaConfig) -> Self {
        Self::with_migration_versions(config, &[4])
    }

    #[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
    fn with_migration_versions(config: SqlSchemaConfig, versions: &[i32]) -> Self {
        Self {
            config,
            migration_versions: Some(versions.to_vec()),
        }
    }

    #[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
    fn validate_config(&self) -> Result<(), EventStoreError> {
        self.config.validate()
    }

    #[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
    fn migrations_for(&self, dialect: SqlDialect) -> Vec<SchemaMigration> {
        let migrations = get_migrations(dialect);
        match &self.migration_versions {
            Some(versions) => migrations
                .into_iter()
                .filter(|migration| versions.contains(&migration.version))
                .collect(),
            None => migrations,
        }
    }

    #[cfg(any(feature = "sqlite", feature = "postgres", feature = "mysql"))]
    fn migration_timestamp_ms(&self) -> Result<i64, EventStoreError> {
        system_time_to_millis(SystemTime::now())
    }

    /// Runs SQLite migrations.
    #[cfg(feature = "sqlite")]
    pub fn run_sqlite(&self, conn: &rusqlite::Connection) -> Result<(), EventStoreError> {
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!("schema.migrate", dialect = "sqlite").entered();

        self.validate_config()?;

        // 1. Ensure migrations table exists (with composite key)
        let create_mig_table = self.config.interpolate(
            "CREATE TABLE IF NOT EXISTS {migrations_table} (
                version INTEGER NOT NULL,
                table_name TEXT NOT NULL,
                description TEXT NOT NULL,
                applied_at_ms INTEGER NOT NULL,
                PRIMARY KEY (version, table_name)
            );",
        );
        conn.execute(&create_mig_table, [])
            .map_err(|e| EventStoreError::backend(e.to_string()))?;

        // Check if the 'table_name' column exists in SQLite migrations table
        let pragma_query = self
            .config
            .interpolate("PRAGMA table_info({migrations_table});");
        let mut stmt = conn
            .prepare(&pragma_query)
            .map_err(|e| EventStoreError::backend(e.to_string()))?;
        let columns: HashSet<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| EventStoreError::backend(e.to_string()))?
            .collect::<Result<HashSet<String>, _>>()
            .map_err(|e| EventStoreError::backend(e.to_string()))?;

        if !columns.contains("table_name") && !columns.is_empty() {
            return Err(legacy_migrations_table_error(&self.config.migrations_table));
        }

        // 2. Fetch applied migrations
        let query_applied = self
            .config
            .interpolate("SELECT version, table_name FROM {migrations_table};");
        let mut stmt = conn
            .prepare(&query_applied)
            .map_err(|e| EventStoreError::backend(e.to_string()))?;
        let applied_pairs: HashSet<(i32, String)> = stmt
            .query_map([], |row| {
                let v: i32 = row.get(0)?;
                let t: String = row.get(1)?;
                Ok((v, t))
            })
            .map_err(|e| EventStoreError::backend(e.to_string()))?
            .collect::<Result<HashSet<(i32, String)>, _>>()
            .map_err(|e| EventStoreError::backend(e.to_string()))?;

        // 3. Execute unapplied migrations
        let migrations = self.migrations_for(SqlDialect::Sqlite);
        for m in migrations {
            let target_table = get_target_table_name(m.version, &self.config);
            if !applied_pairs.contains(&(m.version, target_table.to_string())) {
                // Execute migration SQL (exec batch to support multiple statements like CREATE INDEX)
                let sql = self.config.interpolate(m.up_sql);
                conn.execute_batch(&sql)
                    .map_err(|e| EventStoreError::backend(e.to_string()))?;

                // Version 2 compatibility copy
                if m.version == 2 {
                    let old_checkpoints_exist: bool = conn.query_row(
                        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='checkpoints')",
                        [],
                        |row| row.get(0),
                    ).unwrap_or(false);

                    if old_checkpoints_exist {
                        let copy_sql = self.config.interpolate(
                            "INSERT OR IGNORE INTO {checkpoints_table} (projection_name, sequence) \
                             SELECT projection_name, last_sequence FROM checkpoints;"
                        );
                        conn.execute(&copy_sql, [])
                            .map_err(|e| EventStoreError::backend(e.to_string()))?;
                    }
                }

                // Record applied migration
                let now_ms = self.migration_timestamp_ms()?;
                let insert_mig = self.config.interpolate(
                    "INSERT INTO {migrations_table} (version, table_name, description, applied_at_ms) VALUES (?1, ?2, ?3, ?4);"
                );
                conn.execute(
                    &insert_mig,
                    rusqlite::params![m.version, target_table, m.description, now_ms],
                )
                .map_err(|e| EventStoreError::backend(e.to_string()))?;
            }
        }

        Ok(())
    }

    /// Runs PostgreSQL migrations.
    #[cfg(feature = "postgres")]
    pub fn run_postgres(&self, client: &mut postgres::Client) -> Result<(), EventStoreError> {
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!("schema.migrate", dialect = "postgres").entered();

        // Concurrent migration runs (multiple stores initializing their
        // schemas at once) race on CREATE TABLE IF NOT EXISTS and on the
        // applied-migrations bookkeeping, so the whole run holds a
        // database-wide advisory lock.
        client
            .query("SELECT pg_advisory_lock($1);", &[&POSTGRES_SCHEMA_LOCK_KEY])
            .map_err(map_postgres_schema_error)?;
        let result = self.run_postgres_locked(client);
        let unlock = client
            .query(
                "SELECT pg_advisory_unlock($1);",
                &[&POSTGRES_SCHEMA_LOCK_KEY],
            )
            .map(drop)
            .map_err(map_postgres_schema_error);
        result.and(unlock)
    }

    #[cfg(feature = "postgres")]
    fn run_postgres_locked(&self, client: &mut postgres::Client) -> Result<(), EventStoreError> {
        self.validate_config()?;

        // 1. Ensure migrations table exists
        let create_mig_table = self.config.interpolate(
            "CREATE TABLE IF NOT EXISTS {migrations_table} (
                version INT NOT NULL,
                table_name VARCHAR(255) NOT NULL,
                description TEXT NOT NULL,
                applied_at_ms BIGINT NOT NULL,
                PRIMARY KEY (version, table_name)
            );",
        );
        client
            .batch_execute(&create_mig_table)
            .map_err(map_postgres_schema_error)?;

        // Check if table_name column exists. `to_regclass` resolves the name
        // through `search_path`, so the probe reads the same relation the
        // SELECT/INSERT statements below write to instead of matching any
        // same-named table in another schema. Dropped columns keep their
        // `pg_attribute` row, hence `NOT attisdropped`.
        let check_col = self.config.interpolate(
            "SELECT EXISTS (
                SELECT 1
                FROM pg_attribute a
                WHERE a.attrelid = to_regclass('{migrations_table}')
                  AND a.attname = 'table_name'
                  AND a.attnum > 0
                  AND NOT a.attisdropped
            );",
        );
        let has_col: bool = client
            .query_one(&check_col, &[])
            .map(|row| row.get(0))
            .map_err(map_postgres_schema_error)?;

        if !has_col {
            return Err(legacy_migrations_table_error(&self.config.migrations_table));
        }

        // 2. Fetch applied migrations
        let query_applied = self
            .config
            .interpolate("SELECT version, table_name FROM {migrations_table};");
        let rows = client
            .query(&query_applied, &[])
            .map_err(map_postgres_schema_error)?;
        let applied_pairs: HashSet<(i32, String)> = rows
            .into_iter()
            .map(|row| (row.get(0), row.get(1)))
            .collect();

        // 3. Execute unapplied migrations
        let migrations = self.migrations_for(SqlDialect::Postgres);
        for m in migrations {
            let target_table = get_target_table_name(m.version, &self.config);
            if !applied_pairs.contains(&(m.version, target_table.to_string())) {
                let sql = self.config.interpolate(m.up_sql);
                client
                    .batch_execute(&sql)
                    .map_err(map_postgres_schema_error)?;

                // Version 2 compatibility copy
                if m.version == 2 {
                    let old_checkpoints_exist: bool = client.query_one(
                        "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name = 'checkpoints')",
                        &[],
                    ).map(|row| row.get(0)).unwrap_or(false);

                    if old_checkpoints_exist {
                        let copy_sql = self.config.interpolate(
                            "INSERT INTO {checkpoints_table} (projection_name, sequence) \
                             SELECT projection_name, last_sequence FROM checkpoints ON CONFLICT DO NOTHING;"
                        );
                        client
                            .execute(&copy_sql, &[])
                            .map_err(map_postgres_schema_error)?;
                    }
                }

                // Record applied migration
                let now_ms = self.migration_timestamp_ms()?;
                let insert_mig = self.config.interpolate(
                    "INSERT INTO {migrations_table} (version, table_name, description, applied_at_ms) VALUES ($1, $2, $3, $4);"
                );
                client
                    .execute(
                        &insert_mig,
                        &[&m.version, &target_table, &m.description, &now_ms],
                    )
                    .map_err(map_postgres_schema_error)?;
            }
        }

        Ok(())
    }

    /// Runs MySQL migrations.
    #[cfg(feature = "mysql")]
    pub fn run_mysql(&self, conn: &mut mysql::Conn) -> Result<(), EventStoreError> {
        #[cfg(feature = "tracing")]
        let _span = tracing::debug_span!("schema.migrate", dialect = "mysql").entered();

        use mysql::prelude::Queryable;

        let acquired: Option<Option<i64>> = conn
            .query_first(format!(
                "SELECT GET_LOCK('{MYSQL_SCHEMA_LOCK_NAME}', {MYSQL_SCHEMA_LOCK_TIMEOUT_SECS});"
            ))
            .map_err(map_mysql_schema_error)?;
        if acquired != Some(Some(1)) {
            return Err(EventStoreError::backend(format!(
                "timed out waiting for the `{MYSQL_SCHEMA_LOCK_NAME}` schema migration lock"
            )));
        }
        let result = self.run_mysql_locked(conn);
        let unlock = conn
            .query_drop(format!("SELECT RELEASE_LOCK('{MYSQL_SCHEMA_LOCK_NAME}');"))
            .map_err(map_mysql_schema_error);
        result.and(unlock)
    }

    #[cfg(feature = "mysql")]
    fn run_mysql_locked(&self, conn: &mut mysql::Conn) -> Result<(), EventStoreError> {
        use mysql::prelude::Queryable;
        self.validate_config()?;

        // 1. Ensure migrations table exists
        let create_mig_table = self.config.interpolate(
            "CREATE TABLE IF NOT EXISTS {migrations_table} (
                version INT NOT NULL,
                table_name VARCHAR(255) NOT NULL,
                description VARCHAR(255) NOT NULL,
                applied_at_ms BIGINT NOT NULL,
                PRIMARY KEY (version, table_name)
            );",
        );
        conn.query_drop(&create_mig_table)
            .map_err(map_mysql_schema_error)?;

        // Check if table_name column exists
        let check_col = self.config.interpolate(
            "SELECT EXISTS (
                SELECT 1
                FROM information_schema.columns
                WHERE table_name = '{migrations_table}'
                  AND column_name = 'table_name'
                  AND table_schema = DATABASE()
            );",
        );
        let has_col: bool = conn
            .query_first(&check_col)
            .map_err(map_mysql_schema_error)?
            .and_then(|row: mysql::Row| row.get::<bool, _>(0))
            .unwrap_or(false);

        if !has_col {
            return Err(legacy_migrations_table_error(&self.config.migrations_table));
        }

        // 2. Fetch applied migrations
        let query_applied = self
            .config
            .interpolate("SELECT version, table_name FROM {migrations_table};");
        let rows: Vec<(i32, String)> =
            conn.query(&query_applied).map_err(map_mysql_schema_error)?;
        let applied_pairs: HashSet<(i32, String)> = rows.into_iter().collect();

        // 3. Execute unapplied migrations
        let migrations = self.migrations_for(SqlDialect::MySql);
        for m in migrations {
            let target_table = get_target_table_name(m.version, &self.config);
            if !applied_pairs.contains(&(m.version, target_table.to_string())) {
                // Execute migration SQL (using standard query_drop)
                let sql = self.config.interpolate(m.up_sql);
                if m.version == 5 {
                    let events_table = self.config.events_table.as_str();
                    let index_name = format!("{events_table}_global_replay_idx");
                    let index_exists_query = format!(
                        "SELECT COUNT(1) > 0 FROM information_schema.statistics \
                         WHERE table_schema = DATABASE() \
                         AND table_name = '{}' \
                         AND index_name = '{}';",
                        events_table, index_name
                    );
                    let index_exists = conn
                        .query_first(&index_exists_query)
                        .map(|row_opt| {
                            row_opt
                                .and_then(|row: mysql::Row| row.get::<bool, _>(0))
                                .unwrap_or(false)
                        })
                        .unwrap_or(false);
                    if !index_exists {
                        conn.query_drop(&sql).map_err(map_mysql_schema_error)?;
                    }
                } else if m.version == 6 {
                    let events_table = self.config.events_table.as_str();
                    let duplicate_indexes_query = r#"
                        SELECT index_name
                        FROM information_schema.statistics
                        WHERE table_schema = DATABASE()
                          AND table_name = ?
                          AND non_unique = 1
                        GROUP BY index_name
                        HAVING GROUP_CONCAT(column_name ORDER BY seq_in_index SEPARATOR ',') =
                            'aggregate_type,aggregate_id,revision';
                    "#;
                    let duplicate_indexes: Vec<String> = conn
                        .exec(duplicate_indexes_query, (events_table,))
                        .map_err(map_mysql_schema_error)?;

                    let quoted_events_table = format!("`{}`", events_table.replace('`', "``"));
                    for index_name in duplicate_indexes {
                        let quoted_index_name = format!("`{}`", index_name.replace('`', "``"));
                        let drop_index = format!(
                            "ALTER TABLE {quoted_events_table} DROP INDEX {quoted_index_name};"
                        );
                        conn.query_drop(drop_index)
                            .map_err(map_mysql_schema_error)?;
                    }
                } else {
                    conn.query_drop(&sql).map_err(map_mysql_schema_error)?;
                }

                // Version 2 compatibility copy
                if m.version == 2 {
                    let old_checkpoints_exist: bool = conn.query_first(
                        "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name = 'checkpoints' AND table_schema = DATABASE())"
                    ).map(|row_opt| row_opt.and_then(|r: mysql::Row| r.get::<bool, _>(0)).unwrap_or(false)).unwrap_or(false);

                    if old_checkpoints_exist {
                        let copy_sql = self.config.interpolate(
                            "INSERT IGNORE INTO {checkpoints_table} (projection_name, sequence) \
                             SELECT projection_name, last_sequence FROM checkpoints;",
                        );
                        conn.query_drop(&copy_sql).map_err(map_mysql_schema_error)?;
                    }
                }

                // Record applied migration
                let now_ms = self.migration_timestamp_ms()?;
                let insert_mig = self.config.interpolate(
                    "INSERT INTO {migrations_table} (version, table_name, description, applied_at_ms) VALUES (?, ?, ?, ?);"
                );
                conn.exec_drop(
                    &insert_mig,
                    (m.version, target_table, m.description, now_ms),
                )
                .map_err(map_mysql_schema_error)?;
            }
        }

        Ok(())
    }
}

/// A concurrent-safe async schema initializer to guarantee schemas are initialized exactly once.
#[cfg(feature = "async")]
pub struct AsyncSchemaInitializer {
    initialized: std::sync::atomic::AtomicBool,
    lock: std::sync::OnceLock<tokio::sync::Mutex<()>>,
}

#[cfg(feature = "async")]
impl AsyncSchemaInitializer {
    /// Creates a new schema initializer.
    pub const fn new() -> Self {
        Self {
            initialized: std::sync::atomic::AtomicBool::new(false),
            lock: std::sync::OnceLock::new(),
        }
    }

    /// Runs the provided asynchronous initialization function exactly once, safely handling concurrency.
    pub async fn run<F, Fut, E>(&self, init_fn: F) -> Result<(), E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<(), E>>,
    {
        if self.initialized.load(std::sync::atomic::Ordering::Acquire) {
            return Ok(());
        }

        let lock = self.lock.get_or_init(|| tokio::sync::Mutex::new(()));
        let _guard = lock.lock().await;

        if self.initialized.load(std::sync::atomic::Ordering::Acquire) {
            return Ok(());
        }

        init_fn().await?;

        self.initialized
            .store(true, std::sync::atomic::Ordering::Release);
        Ok(())
    }

    /// Resets the initialization state (primarily for testing purposes).
    pub fn reset(&self) {
        self.initialized
            .store(false, std::sync::atomic::Ordering::Release);
    }
}

#[cfg(feature = "async")]
impl Default for AsyncSchemaInitializer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "async")]
impl std::fmt::Debug for AsyncSchemaInitializer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncSchemaInitializer")
            .field("initialized", &self.initialized)
            .finish()
    }
}
