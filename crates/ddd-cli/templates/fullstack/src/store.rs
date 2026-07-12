use std::borrow::Cow;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use futures::lock::Mutex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
#[cfg(all(feature = "spicedb", runtime_spin))]
use wasi_auth::authentication::{RelationshipOperation, RelationshipOutboxIntent};
#[cfg(all(feature = "spicedb", runtime_spin))]
use wasi_auth::authorization::{
    AccessRequest as SpiceDbAccessRequest, ActionName as SpiceDbActionName,
    Authorizer as SpiceDbAuthorizer, ConsistencyRequirement, Decision as AuthorizationDecision,
    Resource as SpiceDbResource, ResourceType as SpiceDbResourceType,
};
#[cfg(all(feature = "spicedb", runtime_spin))]
use wasi_auth::context::{OrganizationId, PolicyRevision};
#[cfg(all(feature = "mail-http", runtime_spin))]
use wasi_auth::mail::{
    EmailKind, EmailMessage, HttpMailBearerToken, HttpMailEndpoint, HttpMailTransport, HttpMailer,
    Mailer, Recipient,
};
#[cfg(all(feature = "spicedb", runtime_spin))]
use wasi_auth::spicedb::{
    PermissionMap, SpiceDbBearerToken, SpiceDbEndpoint, SpiceDbProvider, SpiceDbRelationshipWriter,
    SpiceDbTransport, SpiceDbWriteEndpoint,
};
use wasi_auth::schema::{AppliedSchemaMigration, plan_schema};

use crate::contracts::{
    HealthStatusResponse, StorageEventTypeCount, StorageProjectionRunResponse, StorageStatusResponse,
};
use crate::error::{AuthStackError, AuthStackResult};

const AUTH_PRODUCTION_MODE: &str = "AUTH_PRODUCTION_MODE";
const MFA_VAULT_KEY: &str = "AUTH_VAULT_KEY_BASE64";
const MFA_RECOVERY_PEPPER: &str = "AUTH_RECOVERY_CODE_PEPPER_BASE64";
#[cfg(all(feature = "mail-http", runtime_spin))]
const MAX_MAIL_DISPATCH_BATCH: usize = 25;
#[cfg(all(feature = "mail-http", runtime_spin))]
const MAIL_LEASE_MS: u64 = 30_000;
#[cfg(all(feature = "mail-http", runtime_spin))]
const MAX_MAIL_WEBHOOK_RESPONSE_BYTES: usize = 16 * 1024;
#[cfg(all(feature = "spicedb", runtime_spin))]
const MAX_SPICEDB_RESPONSE_BYTES: usize = 256 * 1024;
#[cfg(all(feature = "spicedb", runtime_spin))]
const MAX_RELATIONSHIP_DISPATCH_BATCH: usize = 100;
#[cfg(all(feature = "spicedb", runtime_spin))]
const RELATIONSHIP_LEASE_MS: u64 = 30_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StorageBackend {
    Postgres,
}

#[cfg(all(any(feature = "mail-http", feature = "spicedb"), runtime_spin))]
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Spin outbound HTTP transport failed")]
struct SpinOutboundHttpTransportError;

#[cfg(all(any(feature = "mail-http", feature = "spicedb"), runtime_spin))]
#[derive(Clone, Copy, Debug)]
struct SpinOutboundHttpTransport;

#[cfg(all(feature = "mail-http", runtime_spin))]
impl HttpMailTransport for SpinOutboundHttpTransport {
    type Error = SpinOutboundHttpTransportError;

    async fn send(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Vec<u8>>, Self::Error> {
        spin_outbound_http_send(request, MAX_MAIL_WEBHOOK_RESPONSE_BYTES).await
    }
}

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

#[cfg(all(any(feature = "mail-http", feature = "spicedb"), runtime_spin))]
async fn spin_outbound_http_send(
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
#[cfg_attr(not(runtime_spin), allow(dead_code))]
struct AtomicSqlStatement {
    sql: String,
    params: Vec<Value>,
    returns_rows: bool,
    minimum_rows: usize,
}

impl AtomicSqlStatement {
    fn execute(sql: impl Into<String>, params: Vec<Value>) -> Self {
        Self {
            sql: sql.into(),
            params,
            returns_rows: false,
            minimum_rows: 0,
        }
    }

    #[allow(dead_code)]
    fn query(sql: impl Into<String>, params: Vec<Value>) -> Self {
        Self {
            sql: sql.into(),
            params,
            returns_rows: true,
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

static SCHEMA_INITIALIZED: AtomicBool = AtomicBool::new(false);
static SCHEMA_INIT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

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
    let pending = plan_schema(&applied)
        .map_err(|error| AuthStackError::configuration(error.to_string()))?;
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
            vec![json!(&probe_id), json!(now.saturating_add(60_000)), json!(now)],
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

pub async fn dispatch_pending_relationships() -> AuthStackResult<usize> {
    if !config_bool("AUTH_SPICEDB_ENABLED", false).await {
        return Ok(0);
    }
    #[cfg(all(feature = "spicedb", runtime_spin))]
    {
        dispatch_pending_spicedb_relationships().await
    }
    #[cfg(not(all(feature = "spicedb", runtime_spin)))]
    {
        Err(AuthStackError::configuration(
            "AUTH_SPICEDB_ENABLED requires the spicedb feature on Spin",
        ))
    }
}

#[cfg(all(feature = "spicedb", runtime_spin))]
async fn dispatch_pending_spicedb_relationships() -> AuthStackResult<usize> {
    let endpoint = store_config_value("AUTH_SPICEDB_WRITE_URL")
        .await
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AuthStackError::configuration("AUTH_SPICEDB_WRITE_URL is required"))?;
    let bearer_token = store_config_value("AUTH_SPICEDB_TOKEN")
        .await
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AuthStackError::configuration("AUTH_SPICEDB_TOKEN is required"))?;
    let writer = SpiceDbRelationshipWriter::new(
        SpiceDbWriteEndpoint::new(endpoint.trim())
            .map_err(|_| AuthStackError::configuration("AUTH_SPICEDB_WRITE_URL is invalid"))?,
        SpiceDbBearerToken::new(bearer_token)
            .map_err(|_| AuthStackError::configuration("AUTH_SPICEDB_TOKEN is invalid"))?,
        SpinOutboundHttpTransport,
    );
    let now = now_ms();
    let rows = execute_sql(
        "SELECT intent_id, operation, resource, relation_name, subject, resource_revision, \
                consistency_token, attempt_count \
         FROM auth_relationship_outbox \
         WHERE status = 'pending' AND available_at_ms <= ?1 \
           AND (lease_id IS NULL OR leased_until_ms < ?1) \
         ORDER BY available_at_ms ASC, created_at_ms ASC LIMIT ?2",
        vec![json!(now), json!(MAX_RELATIONSHIP_DISPATCH_BATCH)],
    )
    .await?;

    let mut claimed = Vec::with_capacity(rows.len());
    for row in rows {
        let intent_id = required_string(&row, "intent_id")?;
        let lease_id = secure_storage_id("relationship_lease")?;
        if execute_sql_atomic(vec![AtomicSqlStatement::guard(
            "UPDATE auth_relationship_outbox SET lease_id = ?1, leased_until_ms = ?2 \
             WHERE intent_id = ?3 AND status = 'pending' AND available_at_ms <= ?4 \
               AND (lease_id IS NULL OR leased_until_ms < ?4) RETURNING intent_id",
            vec![
                json!(&lease_id),
                json!(now.saturating_add(RELATIONSHIP_LEASE_MS)),
                json!(&intent_id),
                json!(now),
            ],
        )])
        .await
        .is_err()
        {
            continue;
        }
        let operation = match required_string(&row, "operation")?.as_str() {
            "grant" => RelationshipOperation::Grant,
            "revoke" => RelationshipOperation::Revoke,
            _ => {
                release_invalid_relationship_intent(&intent_id, &lease_id).await?;
                continue;
            }
        };
        let resource_revision = row_i64(&row, "resource_revision")
            .filter(|value| *value >= 0)
            .ok_or_else(|| AuthStackError::store("relationship revision is invalid"))?
            as u64;
        claimed.push((
            intent_id,
            lease_id,
            row_i64(&row, "attempt_count").unwrap_or_default().max(0) as u32,
            RelationshipOutboxIntent {
                operation,
                resource: required_string(&row, "resource")?,
                relation: required_string(&row, "relation_name")?,
                subject: required_string(&row, "subject")?,
                resource_revision,
                consistency_token: row_string(&row, "consistency_token"),
            },
        ));
    }
    if claimed.is_empty() {
        return Ok(0);
    }

    let intents = claimed
        .iter()
        .map(|(_, _, _, intent)| intent.clone())
        .collect::<Vec<_>>();
    match writer.write(&intents).await {
        Ok(receipt) => {
            let completed_at = now_ms();
            let statements = claimed
                .iter()
                .map(|(intent_id, lease_id, _, _)| {
                    AtomicSqlStatement::guard(
                        "UPDATE auth_relationship_outbox \
                         SET status = 'completed', consistency_token = ?1, updated_at_ms = ?2, \
                             lease_id = NULL, leased_until_ms = NULL, last_error = NULL \
                         WHERE intent_id = ?3 AND lease_id = ?4 AND status = 'pending' \
                         RETURNING intent_id",
                        vec![
                            json!(receipt.consistency_token()),
                            json!(completed_at),
                            json!(intent_id),
                            json!(lease_id),
                        ],
                    )
                })
                .collect();
            execute_sql_atomic(statements).await?;
            Ok(claimed.len())
        }
        Err(error) => {
            let failed_at = now_ms();
            let statements = claimed
                .iter()
                .map(|(intent_id, lease_id, attempts, _)| {
                    AtomicSqlStatement::guard(
                        "UPDATE auth_relationship_outbox \
                         SET attempt_count = attempt_count + 1, last_error = 'provider_failure', \
                             available_at_ms = ?1, updated_at_ms = ?2, lease_id = NULL, \
                             leased_until_ms = NULL \
                         WHERE intent_id = ?3 AND lease_id = ?4 AND status = 'pending' \
                         RETURNING intent_id",
                        vec![
                            json!(failed_at.saturating_add(relationship_retry_delay_ms(*attempts))),
                            json!(failed_at),
                            json!(intent_id),
                            json!(lease_id),
                        ],
                    )
                })
                .collect();
            execute_sql_atomic(statements).await?;
            tracing::warn!(error = %error, intent_count = claimed.len(), "SpiceDB outbox batch deferred");
            Ok(0)
        }
    }
}

#[cfg(all(feature = "spicedb", runtime_spin))]
async fn release_invalid_relationship_intent(
    intent_id: &str,
    lease_id: &str,
) -> AuthStackResult<()> {
    execute_sql_atomic(vec![AtomicSqlStatement::guard(
        "UPDATE auth_relationship_outbox \
         SET status = 'failed', last_error = 'invalid_intent', updated_at_ms = ?1, \
             lease_id = NULL, leased_until_ms = NULL \
         WHERE intent_id = ?2 AND lease_id = ?3 RETURNING intent_id",
        vec![json!(now_ms()), json!(intent_id), json!(lease_id)],
    )])
    .await?;
    Ok(())
}

#[cfg(all(feature = "spicedb", runtime_spin))]
fn relationship_retry_delay_ms(attempts: u32) -> u64 {
    let exponent = attempts.min(6);
    100_u64.saturating_mul(1_u64 << exponent).min(10_000)
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
        let issuer = context.principal().issuer();
        let user_id = context.principal().user_id().as_str();
        let mut identity = Vec::with_capacity(issuer.len() + user_id.len() + 1);
        identity.extend_from_slice(issuer.as_bytes());
        identity.push(0);
        identity.extend_from_slice(user_id.as_bytes());
        let subject = format!("user:v1_{}", URL_SAFE_NO_PAD.encode(identity));
        let resource = format!("organization:{organization_id}");
        let rows = execute_sql(
            "SELECT status, operation, consistency_token, resource_revision \
             FROM auth_relationship_outbox WHERE resource = ?1 AND subject = ?2 \
             ORDER BY resource_revision DESC, created_at_ms DESC LIMIT 1",
            vec![json!(&resource), json!(&subject)],
        )
        .await?;
        let latest = rows.first();
        let resource_revision = latest
            .and_then(|row| row_i64(row, "resource_revision"))
            .filter(|revision| *revision >= 0)
            .map(|revision| revision as u64);
        if latest.is_some_and(|row| row_string(row, "status").as_deref() != Some("completed")) {
            let revision = PolicyRevision::new("spicedb-pending-v1")
                .map_err(|_| AuthStackError::configuration("SpiceDB policy revision is invalid"))?;
            return Ok((
                AuthorizationDecision::deny(revision, "spicedb.pending_relationship"),
                resource_revision,
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
        let consistency = latest
            .and_then(|row| row_string(row, "consistency_token"))
            .filter(|value| !value.is_empty())
            .map_or(ConsistencyRequirement::MinimizeLatency, |token| {
                ConsistencyRequirement::AtLeastAsFresh { token }
            });
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
        Ok((decision, resource_revision))
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
async fn direct_spicedb_provider()
-> AuthStackResult<&'static SpiceDbProvider<SpinOutboundHttpTransport>> {
    static PROVIDER: OnceLock<SpiceDbProvider<SpinOutboundHttpTransport>> = OnceLock::new();
    if let Some(provider) = PROVIDER.get() {
        return Ok(provider);
    }
    let check_url = store_config_value("AUTH_SPICEDB_CHECK_URL")
        .await
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AuthStackError::configuration("AUTH_SPICEDB_CHECK_URL is required"))?;
    let token = store_config_value("AUTH_SPICEDB_TOKEN")
        .await
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AuthStackError::configuration("AUTH_SPICEDB_TOKEN is required"))?;
    let permission = store_config_value("AUTH_SPICEDB_MEMBERSHIP_PERMISSION")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "member".to_owned());
    let provider = SpiceDbProvider::new(
        SpiceDbEndpoint::new(check_url.trim())
            .map_err(|_| AuthStackError::configuration("AUTH_SPICEDB_CHECK_URL is invalid"))?,
        SpiceDbBearerToken::new(token)
            .map_err(|_| AuthStackError::configuration("AUTH_SPICEDB_TOKEN is invalid"))?,
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

async fn storage_backend() -> AuthStackResult<StorageBackend> {
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

fn default_storage_backend() -> &'static str {
    "postgres"
}

fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

async fn runtime_config_value(name: &str) -> Option<String> {
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
async fn database_url(backend_name: &str) -> AuthStackResult<String> {
    runtime_config_value("DATABASE_URL").await.ok_or_else(|| {
        AuthStackError::configuration(format!(
            "DATABASE_URL is required for DATABASE_BACKEND={backend_name}"
        ))
    })
}

async fn execute_sql(sql: &str, params: Vec<Value>) -> AuthStackResult<Vec<Value>> {
    storage_backend().await?;
    execute_postgres(sql, params).await
}

async fn execute_sql_atomic(
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

async fn execute_postgres(sql: &str, params: Vec<Value>) -> AuthStackResult<Vec<Value>> {
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
fn postgres_sql(sql: &str) -> Cow<'_, str> {
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
fn postgres_placeholders(sql: &str) -> String {
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

pub async fn storage_status() -> AuthStackResult<StorageStatusResponse> {
    initialize_schema_async().await?;
    let summary_rows = execute_sql(
        "SELECT COUNT(*) AS event_count, COALESCE(MAX(sequence), 0) AS latest_sequence FROM auth_audit_log",
        Vec::new(),
    )
    .await?;
    let summary = summary_rows.first();
    let event_count = summary
        .and_then(|row| row_i64(row, "event_count"))
        .unwrap_or_default()
        .max(0) as u64;
    let latest_sequence = summary
        .and_then(|row| row_i64(row, "latest_sequence"))
        .unwrap_or_default()
        .max(0) as u64;
    let event_types = execute_sql(
        "SELECT action AS event_type, COUNT(*) AS count \
         FROM auth_audit_log GROUP BY action ORDER BY action",
        Vec::new(),
    )
    .await?
    .into_iter()
    .map(|row| {
        Ok(StorageEventTypeCount {
            event_type: required_string(&row, "event_type")?,
            count: row_i64(&row, "count").unwrap_or_default().max(0) as u64,
        })
    })
    .collect::<AuthStackResult<Vec<_>>>()?;
    Ok(StorageStatusResponse {
        event_count,
        latest_sequence,
        event_types,
        checkpoints: Vec::new(),
    })
}

pub async fn catch_up_storage_projections(
    batch_limit: Option<usize>,
) -> AuthStackResult<Vec<StorageProjectionRunResponse>> {
    initialize_schema_async().await?;
    if batch_limit.is_some_and(|limit| limit == 0 || limit > 10_000) {
        return Err(AuthStackError::validation("projection batch limit is invalid"));
    }
    Ok(Vec::new())
}

fn required_string(row: &Value, key: &str) -> AuthStackResult<String> {
    row_string(row, key).ok_or_else(|| AuthStackError::store(format!("missing column '{key}'")))
}

fn row_string(row: &Value, key: &str) -> Option<String> {
    row.get(key).and_then(|value| match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).ok(),
        _ => None,
    })
}

fn row_i64(row: &Value, key: &str) -> Option<i64> {
    row.get(key).and_then(Value::as_i64)
}

fn random_bytes(len: usize) -> AuthStackResult<Vec<u8>> {
    #[cfg(all(target_arch = "wasm32", feature = "ssr"))]
    {
        Ok(wasip3::random::random::get_random_bytes(len as u64))
    }

    #[cfg(not(all(target_arch = "wasm32", feature = "ssr")))]
    {
        let mut bytes = vec![0_u8; len];
        getrandom::getrandom(&mut bytes).map_err(|error| {
            AuthStackError::store(format!("secure randomness unavailable: {error}"))
        })?;
        Ok(bytes)
    }
}

async fn mail_transport() -> String {
    store_config_value("AUTH_MAIL_TRANSPORT")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "capture".to_string())
        .trim()
        .to_ascii_lowercase()
}

async fn validate_runtime_security_config() -> AuthStackResult<()> {
    validate_spicedb_runtime_config().await?;
    if !config_bool(AUTH_PRODUCTION_MODE, false).await {
        return Ok(());
    }
    if !config_bool("AUTH_COOKIE_SECURE", false).await {
        return Err(AuthStackError::configuration(
            "production requires AUTH_COOKIE_SECURE=true",
        ));
    }
    if config_bool("AUTH_DEV_TOOLS", false).await {
        return Err(AuthStackError::configuration(
            "production forbids AUTH_DEV_TOOLS",
        ));
    }
    let public_base_url = store_config_value("AUTH_PUBLIC_BASE_URL")
        .await
        .unwrap_or_default();
    if !public_base_url.starts_with("https://") {
        return Err(AuthStackError::configuration(
            "production requires an https AUTH_PUBLIC_BASE_URL",
        ));
    }
    if store_config_value("AUTH_CSRF_SECRET")
        .await
        .is_none_or(|value| value.trim().len() < 32)
    {
        return Err(AuthStackError::configuration(
            "production requires AUTH_CSRF_SECRET with at least 32 characters",
        ));
    }
    require_production_secret(MFA_VAULT_KEY).await?;
    require_production_secret(MFA_RECOVERY_PEPPER).await?;
    match mail_transport().await.as_str() {
        "smtp" => {
            require_production_secret("AUTH_SMTP_URL").await?;
            #[cfg(runtime_spin)]
            return Err(AuthStackError::configuration(
                "Spin production uses AUTH_MAIL_TRANSPORT=http; SMTP requires an external native SmtpMailer worker",
            ));
        }
        "http" => {
            require_production_secret("AUTH_MAIL_HTTP_URL").await?;
            require_production_secret("AUTH_MAIL_HTTP_TOKEN").await?;
        }
        "capture" => {
            return Err(AuthStackError::configuration(
                "production forbids capture mail; configure smtp or http",
            ));
        }
        _ => {
            return Err(AuthStackError::configuration(
                "AUTH_MAIL_TRANSPORT must be capture, smtp, or http",
            ));
        }
    }
    Ok(())
}

async fn validate_spicedb_runtime_config() -> AuthStackResult<()> {
    if !config_bool("AUTH_SPICEDB_ENABLED", false).await {
        return Ok(());
    }
    #[cfg(all(feature = "spicedb", runtime_spin))]
    {
        let check_endpoint = store_config_value("AUTH_SPICEDB_CHECK_URL")
            .await
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AuthStackError::configuration("AUTH_SPICEDB_CHECK_URL is required"))?;
        SpiceDbEndpoint::new(check_endpoint.trim())
            .map_err(|_| AuthStackError::configuration("AUTH_SPICEDB_CHECK_URL is invalid"))?;
        let endpoint = store_config_value("AUTH_SPICEDB_WRITE_URL")
            .await
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AuthStackError::configuration("AUTH_SPICEDB_WRITE_URL is required"))?;
        SpiceDbWriteEndpoint::new(endpoint.trim())
            .map_err(|_| AuthStackError::configuration("AUTH_SPICEDB_WRITE_URL is invalid"))?;
        require_production_secret("AUTH_SPICEDB_TOKEN").await?;
        Ok(())
    }
    #[cfg(not(all(feature = "spicedb", runtime_spin)))]
    {
        Err(AuthStackError::configuration(
            "AUTH_SPICEDB_ENABLED requires the spicedb feature on Spin",
        ))
    }
}

async fn require_production_secret(name: &str) -> AuthStackResult<()> {
    if store_config_value(name)
        .await
        .is_some_and(|value| !value.trim().is_empty())
    {
        Ok(())
    } else {
        Err(AuthStackError::configuration(format!(
            "production requires {name}"
        )))
    }
}

async fn config_bool(name: &str, default: bool) -> bool {
    store_config_value(name)
        .await
        .map(|value| truthy(&value))
        .unwrap_or(default)
}

async fn store_config_value(name: &str) -> Option<String> {
    #[cfg(all(runtime_spin, not(test)))]
    {
        let variable_name = name.to_ascii_lowercase();
        if let Ok(value) = spin_sdk::variables::get(&variable_name).await {
            return Some(value);
        }
    }

    std::env::var(name).ok()
}

fn truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on" | "enabled"
    )
}

fn secure_storage_id(kind: &str) -> AuthStackResult<String> {
    Ok(format!(
        "{kind}_{}",
        URL_SAFE_NO_PAD.encode(random_bytes(32)?)
    ))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

pub async fn health_status() -> AuthStackResult<HealthStatusResponse> {
    initialize_schema_async().await?;
    Ok(HealthStatusResponse {
        status: "ok".to_owned(),
        storage_backend: match storage_backend().await? {
            StorageBackend::Postgres => "postgres",
        }
        .to_owned(),
        mail_transport: mail_transport().await,
        authorization_provider: "embedded-cedar".to_owned(),
        production_mode: config_bool(AUTH_PRODUCTION_MODE, false).await,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_sql_rewrites_indexed_placeholders() {
        assert_eq!(
            postgres_sql("SELECT * FROM auth_users WHERE user_id = ?1 AND status = ?2"),
            "SELECT * FROM auth_users WHERE user_id = $1 AND status = $2"
        );
    }

    #[test]
    fn postgres_sql_rewrites_insert_or_ignore() {
        assert_eq!(
            postgres_sql("INSERT OR IGNORE INTO auth_users (user_id) VALUES (?1)"),
            "INSERT INTO auth_users (user_id) VALUES ($1) ON CONFLICT DO NOTHING"
        );
    }
}
