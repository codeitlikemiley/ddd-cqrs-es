//! Product storage adapters (KV, SQL, vault, dashboard data).
#![allow(unused_imports)]
#![allow(dead_code)]

use std::borrow::Cow;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "mail-capture")]
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use futures::lock::Mutex;
use serde_json::Value;
#[cfg(feature = "mail-capture")]
use serde_json::json;
use sha2::{Digest, Sha256};
#[cfg(all(feature = "spicedb", runtime_spin))]
use wasi_auth::authorization::{
    AccessRequest as SpiceDbAccessRequest, ActionName as SpiceDbActionName,
    Authorizer as SpiceDbAuthorizer, ConsistencyRequirement, Decision as AuthorizationDecision,
    Resource as SpiceDbResource, ResourceType as SpiceDbResourceType,
};
#[cfg(all(feature = "spicedb", runtime_spin))]
use wasi_auth::context::OrganizationId;
#[cfg(all(feature = "spicedb", runtime_spin))]
use wasi_auth::postgres::outbox::load_relationship_consistency;
use wasi_auth::schema::{AppliedSchemaMigration, plan_schema};
#[cfg(all(feature = "spicedb", runtime_spin))]
use wasi_auth::spicedb::{
    PermissionMap, SpiceDbBearerToken, SpiceDbEndpoint, SpiceDbProvider, SpiceDbTransport,
};

use crate::contracts::{
    HealthStatusResponse, StorageEventTypeCount, StorageProjectionRunResponse,
    StorageStatusResponse,
};
use crate::error::{AuthStackError, AuthStackResult};

use super::*;

pub(crate) const AUTH_PRODUCTION_MODE: &str = "AUTH_PRODUCTION_MODE";
pub(crate) const MFA_VAULT_KEY: &str = "AUTH_VAULT_KEY_BASE64";
pub(crate) const MFA_RECOVERY_PEPPER: &str = "AUTH_RECOVERY_CODE_PEPPER_BASE64";
#[cfg(all(feature = "spicedb", runtime_spin))]
pub(crate) const MAX_SPICEDB_RESPONSE_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StorageBackend {
    Postgres,
}

#[cfg(all(feature = "spicedb", runtime_spin))]
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Spin outbound HTTP transport failed")]
pub(crate) struct SpinOutboundHttpTransportError;

#[cfg(all(feature = "spicedb", runtime_spin))]
#[derive(Clone, Copy, Debug)]
pub(crate) struct SpinOutboundHttpTransport;

#[cfg(all(feature = "spicedb", runtime_spin))]
impl SpiceDbTransport for SpinOutboundHttpTransport {
    type Error = SpinOutboundHttpTransportError;

    async fn send(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Vec<u8>>, Self::Error> {
        spin_outbound_http_send(request, MAX_SPICEDB_RESPONSE_BYTES).await
    }
}

#[cfg(all(feature = "spicedb", runtime_spin))]
pub(crate) async fn spin_outbound_http_send(
    request: http::Request<Vec<u8>>,
    max_response_bytes: usize,
) -> Result<http::Response<Vec<u8>>, SpinOutboundHttpTransportError> {
    use bytes::Bytes;
    use http_body_util::BodyExt as _;
    use spin_sdk::http::{FullBody, send};

    let (parts, body) = request.into_parts();
    let request = http::Request::from_parts(parts, FullBody::new(Bytes::from(body)));
    let response = send(request)
        .await
        .map_err(|_| SpinOutboundHttpTransportError)?;
    if response
        .headers()
        .get(http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|length| length > max_response_bytes)
    {
        return Err(SpinOutboundHttpTransportError);
    }
    let (parts, body) = response.into_parts();
    let body = body
        .collect()
        .await
        .map_err(|_| SpinOutboundHttpTransportError)?
        .to_bytes();
    if body.len() > max_response_bytes {
        return Err(SpinOutboundHttpTransportError);
    }
    Ok(http::Response::from_parts(parts, body.to_vec()))
}

#[derive(Clone, Debug)]
#[cfg(feature = "mail-capture")]
#[cfg_attr(not(runtime_spin), allow(dead_code))]
pub(crate) struct AtomicSqlStatement {
    sql: String,
    params: Vec<Value>,
    returns_rows: bool,
    minimum_rows: usize,
}

#[cfg(feature = "mail-capture")]
impl AtomicSqlStatement {
    fn execute(sql: impl Into<String>, params: Vec<Value>) -> Self {
        Self {
            sql: sql.into(),
            params,
            returns_rows: false,
            minimum_rows: 0,
        }
    }

    fn guard(sql: impl Into<String>, params: Vec<Value>) -> Self {
        Self {
            sql: sql.into(),
            params,
            returns_rows: true,
            minimum_rows: 1,
        }
    }
}

pub(crate) static SCHEMA_INITIALIZED: AtomicBool = AtomicBool::new(false);
pub(crate) static SCHEMA_INIT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
pub(crate) static RUNTIME_SECURITY_VALIDATED: AtomicBool = AtomicBool::new(false);
pub(crate) static RUNTIME_SECURITY_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub async fn initialize_schema_async() -> AuthStackResult<()> {
    if SCHEMA_INITIALIZED.load(Ordering::Acquire) {
        return Ok(());
    }

    let lock = SCHEMA_INIT_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().await;

    if SCHEMA_INITIALIZED.load(Ordering::Acquire) {
        return Ok(());
    }

    validate_runtime_security_config().await?;
    let applied = execute_postgres(
        "SELECT version, checksum FROM auth_schema_migrations ORDER BY version",
        Vec::new(),
    )
    .await?
    .into_iter()
    .map(|row| {
        Ok(AppliedSchemaMigration {
            version: required_string(&row, "version")?,
            checksum: required_string(&row, "checksum")?,
        })
    })
    .collect::<AuthStackResult<Vec<_>>>()?;
    let pending =
        plan_schema(&applied).map_err(|error| AuthStackError::configuration(error.to_string()))?;
    if !pending.is_empty() {
        return Err(AuthStackError::configuration(format!(
            "wasi-auth schema has pending migrations: {}",
            pending
                .iter()
                .map(|migration| migration.version())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }

    SCHEMA_INITIALIZED.store(true, Ordering::Release);
    tracing::info!("wasi-auth PostgreSQL schema verified");

    Ok(())
}

#[cfg(feature = "mail-capture")]
pub async fn verify_atomic_rollback_probe() -> AuthStackResult<Value> {
    initialize_schema_async().await?;
    let probe_id = secure_storage_id("rollback-probe")?;
    let now = now_ms();
    let result = execute_sql_atomic(vec![
        AtomicSqlStatement::execute(
            "INSERT INTO auth_rate_limit_buckets \
             (bucket_key, attempt_count, window_expires_at_ms, updated_at_ms) \
             VALUES (?1, 1, ?2, ?3)",
            vec![
                json!(&probe_id),
                json!(now.saturating_add(60_000)),
                json!(now),
            ],
        ),
        AtomicSqlStatement::guard(
            "SELECT bucket_key FROM auth_rate_limit_buckets WHERE bucket_key = ?1 AND FALSE",
            vec![json!(&probe_id)],
        ),
    ])
    .await;
    if result.is_ok() {
        return Err(AuthStackError::store(
            "atomic rollback probe unexpectedly committed",
        ));
    }
    let rows = execute_sql(
        "SELECT COUNT(*) AS remaining FROM auth_rate_limit_buckets WHERE bucket_key = ?1",
        vec![json!(&probe_id)],
    )
    .await?;
    let remaining = rows
        .first()
        .and_then(|row| row_i64(row, "remaining"))
        .unwrap_or(1);
    if remaining != 0 {
        return Err(AuthStackError::store("atomic rollback left partial state"));
    }
    Ok(json!({"rolled_back": true, "verified_categories": ["rate_limit_bucket"]}))
}

pub async fn csrf_token_for_session(session_id: &str) -> AuthStackResult<String> {
    let secret = if let Some(value) = store_config_value("AUTH_CSRF_SECRET")
        .await
        .filter(|value| !value.trim().is_empty())
    {
        value
    } else {
        "dev-fullstack-csrf-secret-change-me".to_owned()
    };
    let digest = Sha256::digest(format!("csrf:{secret}:{session_id}").as_bytes());
    Ok(URL_SAFE_NO_PAD.encode(digest))
}

pub async fn direct_spicedb_enabled() -> bool {
    config_bool("AUTH_SPICEDB_ENABLED", false).await
}

pub async fn check_direct_spicedb_membership(
    context: wasi_auth::context::VerifiedAuthContext,
    organization_id: &str,
) -> AuthStackResult<(wasi_auth::authorization::Decision, Option<u64>)> {
    #[cfg(all(feature = "spicedb", runtime_spin))]
    {
        let organization = OrganizationId::new(organization_id.to_owned())
            .map_err(|_| AuthStackError::validation("organization_id is invalid"))?;
        let synchronization = load_relationship_consistency(
            &crate::auth_product::store().await?,
            "organization",
            organization_id,
        )
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "resource relationship consistency lookup failed");
            AuthStackError::store("relationship consistency lookup failed")
        })?;
        if synchronization.has_unsettled() {
            return Ok((
                AuthorizationDecision::deny(
                    wasi_auth::context::PolicyRevision::new("spicedb-pending-v1").map_err(
                        |_| AuthStackError::configuration("SpiceDB policy revision is invalid"),
                    )?,
                    "spicedb.pending_relationship",
                ),
                synchronization.resource_revision(),
            ));
        }

        let provider = direct_spicedb_provider().await?;
        let resource = SpiceDbResource::new(
            SpiceDbResourceType::new("organization")
                .map_err(|_| AuthStackError::configuration("SpiceDB resource type is invalid"))?,
            organization_id,
            Some(organization),
        )
        .map_err(|_| AuthStackError::validation("organization resource is invalid"))?;
        let consistency = synchronization.consistency_token().map_or(
            ConsistencyRequirement::MinimizeLatency,
            |token| ConsistencyRequirement::AtLeastAsFresh {
                token: token.to_owned(),
            },
        );
        let request = SpiceDbAccessRequest::new(
            context,
            SpiceDbActionName::new("authorization.check")
                .map_err(|_| AuthStackError::configuration("SpiceDB action is invalid"))?,
            resource,
        )
        .map_err(|_| AuthStackError::Forbidden)?
        .with_consistency(consistency);
        let decision = SpiceDbAuthorizer::new(provider)
            .check(&request)
            .await
            .map_err(|error| {
                tracing::error!(error = %error, "direct SpiceDB failed closed");
                AuthStackError::Forbidden
            })?;
        Ok((decision, synchronization.resource_revision()))
    }
    #[cfg(not(all(feature = "spicedb", runtime_spin)))]
    {
        let _ = (context, organization_id);
        Err(AuthStackError::configuration(
            "direct SpiceDB requires the spicedb feature on Spin",
        ))
    }
}

#[cfg(all(feature = "spicedb", runtime_spin))]
pub(crate) async fn direct_spicedb_provider()
-> AuthStackResult<&'static SpiceDbProvider<SpinOutboundHttpTransport>> {
    static PROVIDER: OnceLock<SpiceDbProvider<SpinOutboundHttpTransport>> = OnceLock::new();
    if let Some(provider) = PROVIDER.get() {
        return Ok(provider);
    }
    let check_url = store_config_value("AUTH_SPICEDB_CHECK_URL")
        .await
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AuthStackError::configuration("AUTH_SPICEDB_CHECK_URL is required"))?;
    let token = store_config_value("AUTH_SPICEDB_CHECK_TOKEN")
        .await
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AuthStackError::configuration("AUTH_SPICEDB_CHECK_TOKEN is required"))?;
    let permission = store_config_value("AUTH_SPICEDB_MEMBERSHIP_PERMISSION")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "member".to_owned());
    let provider = SpiceDbProvider::new(
        SpiceDbEndpoint::new(check_url.trim())
            .map_err(|_| AuthStackError::configuration("AUTH_SPICEDB_CHECK_URL is invalid"))?,
        SpiceDbBearerToken::new(token)
            .map_err(|_| AuthStackError::configuration("AUTH_SPICEDB_CHECK_TOKEN is invalid"))?,
        SpinOutboundHttpTransport,
        PermissionMap::new([("authorization.check", permission)]).map_err(|_| {
            AuthStackError::configuration("AUTH_SPICEDB_MEMBERSHIP_PERMISSION is invalid")
        })?,
        "spicedb-membership-v1",
    )
    .map_err(|_| AuthStackError::configuration("SpiceDB provider config is invalid"))?;
    let _ = PROVIDER.set(provider);
    PROVIDER
        .get()
        .ok_or_else(|| AuthStackError::configuration("SpiceDB provider initialization failed"))
}

pub(crate) async fn storage_backend() -> AuthStackResult<StorageBackend> {
    let value = runtime_config_value("DATABASE_BACKEND")
        .await
        .unwrap_or_else(|| default_storage_backend().into());
    match value.trim().to_ascii_lowercase().as_str() {
        "postgres" | "postgresql" => Ok(StorageBackend::Postgres),
        other => Err(AuthStackError::configuration(format!(
            "unsupported DATABASE_BACKEND={other}; production fullstack requires postgres"
        ))),
    }
}

pub(crate) fn default_storage_backend() -> &'static str {
    "postgres"
}

pub(crate) fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

pub(crate) async fn runtime_config_value(name: &str) -> Option<String> {
    #[cfg(all(runtime_spin, not(test)))]
    {
        let variable_name = name.to_ascii_lowercase();
        if let Ok(value) = spin_sdk::variables::get(&variable_name).await
            && !value.is_empty()
        {
            return Some(value);
        }
    }

    env_non_empty(name)
}

#[allow(dead_code)]
pub(crate) async fn database_url(backend_name: &str) -> AuthStackResult<String> {
    runtime_config_value("DATABASE_URL").await.ok_or_else(|| {
        AuthStackError::configuration(format!(
            "DATABASE_URL is required for DATABASE_BACKEND={backend_name}"
        ))
    })
}

pub(crate) async fn execute_sql(sql: &str, params: Vec<Value>) -> AuthStackResult<Vec<Value>> {
    storage_backend().await?;
    execute_postgres(sql, params).await
}

#[cfg(feature = "mail-capture")]
pub(crate) async fn execute_sql_atomic(
    statements: Vec<AtomicSqlStatement>,
) -> AuthStackResult<Vec<Vec<Value>>> {
    storage_backend().await?;

    #[cfg(all(runtime_spin, feature = "postgres"))]
    {
        let statements = statements
            .into_iter()
            .map(|statement| {
                let sql = postgres_sql(&statement.sql).into_owned();
                if statement.minimum_rows > 0 {
                    ddd_cqrs_es::adapters::SpinSqlStatement::guard(sql, statement.params)
                } else if statement.returns_rows {
                    ddd_cqrs_es::adapters::SpinSqlStatement::query(sql, statement.params)
                } else {
                    ddd_cqrs_es::adapters::SpinSqlStatement::execute(sql, statement.params)
                }
            })
            .collect();
        let url = database_url("postgres").await?;
        ddd_cqrs_es::adapters::execute_spin_pg_atomic(&url, statements)
            .await
            .map_err(AuthStackError::store)
    }

    #[cfg(not(all(runtime_spin, feature = "postgres")))]
    {
        let _ = statements;
        Err(AuthStackError::configuration(
            "atomic SQL storage requires Spin PostgreSQL",
        ))
    }
}

pub(crate) async fn execute_postgres(sql: &str, params: Vec<Value>) -> AuthStackResult<Vec<Value>> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let url = database_url("postgres").await?;
        let sql = postgres_sql(sql);
        ddd_cqrs_es::adapters::execute_spin_pg(&url, sql.as_ref(), params)
            .await
            .map_err(AuthStackError::store)
    }

    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (sql, params);
        Err(AuthStackError::configuration(
            "postgres storage requires the postgres feature on Spin",
        ))
    }
}

#[allow(dead_code)]
pub(crate) fn postgres_sql(sql: &str) -> Cow<'_, str> {
    let mut converted = postgres_placeholders(sql);
    if converted.contains("INTEGER PRIMARY KEY AUTOINCREMENT") {
        converted = converted.replace("INTEGER PRIMARY KEY AUTOINCREMENT", "BIGSERIAL PRIMARY KEY");
    }
    if converted.contains(" INTEGER") {
        converted = converted
            .replace(" INTEGER NOT NULL", " BIGINT NOT NULL")
            .replace(" INTEGER PRIMARY KEY", " BIGINT PRIMARY KEY")
            .replace(" INTEGER,", " BIGINT,")
            .replace(" INTEGER\n", " BIGINT\n");
    }
    if converted.trim_start().starts_with("INSERT OR IGNORE INTO") {
        converted = converted.replacen("INSERT OR IGNORE INTO", "INSERT INTO", 1);
        converted.push_str(" ON CONFLICT DO NOTHING");
    }
    if converted == sql {
        Cow::Borrowed(sql)
    } else {
        Cow::Owned(converted)
    }
}

#[allow(dead_code)]
pub(crate) fn postgres_placeholders(sql: &str) -> String {
    let mut output = String::with_capacity(sql.len());
    let mut chars = sql.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '?' {
            output.push(ch);
            continue;
        }

        let mut digits = String::new();
        while let Some(next) = chars.peek() {
            if next.is_ascii_digit() {
                digits.push(*next);
                chars.next();
            } else {
                break;
            }
        }

        if digits.is_empty() {
            output.push('?');
        } else {
            output.push('$');
            output.push_str(&digits);
        }
    }

    output
}

// ── App-owned profile store (Spin key-value; not wasi-auth schema) ──────────

