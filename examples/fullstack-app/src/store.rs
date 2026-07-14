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

const AUTH_PRODUCTION_MODE: &str = "AUTH_PRODUCTION_MODE";
const MFA_VAULT_KEY: &str = "AUTH_VAULT_KEY_BASE64";
const MFA_RECOVERY_PEPPER: &str = "AUTH_RECOVERY_CODE_PEPPER_BASE64";
#[cfg(all(feature = "spicedb", runtime_spin))]
const MAX_SPICEDB_RESPONSE_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StorageBackend {
    Postgres,
}

#[cfg(all(feature = "spicedb", runtime_spin))]
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Spin outbound HTTP transport failed")]
struct SpinOutboundHttpTransportError;

#[cfg(all(feature = "spicedb", runtime_spin))]
#[derive(Clone, Copy, Debug)]
struct SpinOutboundHttpTransport;

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
#[cfg(feature = "mail-capture")]
#[cfg_attr(not(runtime_spin), allow(dead_code))]
struct AtomicSqlStatement {
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

static SCHEMA_INITIALIZED: AtomicBool = AtomicBool::new(false);
static SCHEMA_INIT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static RUNTIME_SECURITY_VALIDATED: AtomicBool = AtomicBool::new(false);
static RUNTIME_SECURITY_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

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

#[cfg(feature = "mail-capture")]
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

// ── App-owned profile store (Spin key-value; not wasi-auth schema) ──────────

const PROFILE_USER_PREFIX: &str = "app_profile:user:";
const PROFILE_HANDLE_PREFIX: &str = "app_profile:handle:";
const MAX_AVATAR_DATA_URL_BYTES: usize = 350_000;
const MAX_NAME_LEN: usize = 60;
const MAX_DISPLAY_NAME_LEN: usize = 80;
const USERNAME_MIN: usize = 3;
const USERNAME_MAX: usize = 30;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
struct StoredProfile {
    user_id: String,
    first_name: String,
    last_name: String,
    display_name: String,
    username: String,
    is_public: bool,
    #[serde(default)]
    avatar_data_url: Option<String>,
}

impl StoredProfile {
    fn empty(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            first_name: String::new(),
            last_name: String::new(),
            display_name: String::new(),
            username: String::new(),
            is_public: false,
            avatar_data_url: None,
        }
    }

    fn to_view(&self, email: Option<String>) -> crate::contracts::ProfileView {
        let public_path = if self.username.is_empty() {
            None
        } else {
            Some(format!("/u/{}", self.username))
        };
        crate::contracts::ProfileView {
            email,
            first_name: self.first_name.clone(),
            last_name: self.last_name.clone(),
            display_name: self.display_name.clone(),
            username: self.username.clone(),
            is_public: self.is_public,
            avatar_data_url: self.avatar_data_url.clone(),
            public_path,
        }
    }

    fn to_public_view(&self) -> crate::contracts::PublicProfileView {
        crate::contracts::PublicProfileView {
            username: self.username.clone(),
            display_name: self.display_name.clone(),
            first_name: self.first_name.clone(),
            last_name: self.last_name.clone(),
            avatar_data_url: self.avatar_data_url.clone(),
        }
    }
}

#[cfg(all(feature = "postgres", runtime_spin))]
async fn profile_kv() -> AuthStackResult<spin_sdk::key_value::Store> {
    spin_sdk::key_value::Store::open_default()
        .await
        .map_err(|error| AuthStackError::store(format!("profile store unavailable: {error}")))
}

fn profile_user_key(user_id: &str) -> String {
    format!("{PROFILE_USER_PREFIX}{user_id}")
}

fn profile_handle_key(username: &str) -> String {
    format!("{PROFILE_HANDLE_PREFIX}{}", username.to_ascii_lowercase())
}

fn normalize_name(label: &str, value: &str, max: usize) -> AuthStackResult<String> {
    let trimmed = value.trim();
    if trimmed.chars().count() > max {
        return Err(AuthStackError::validation(format!(
            "{label} must be at most {max} characters"
        )));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(AuthStackError::validation(format!(
            "{label} contains invalid characters"
        )));
    }
    Ok(trimmed.to_owned())
}

fn normalize_username(value: &str) -> AuthStackResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let lower = trimmed.to_ascii_lowercase();
    let len = lower.chars().count();
    if len < USERNAME_MIN || len > USERNAME_MAX {
        return Err(AuthStackError::validation(format!(
            "username must be {USERNAME_MIN}–{USERNAME_MAX} characters"
        )));
    }
    if !lower
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(AuthStackError::validation(
            "username may only use letters, numbers, and underscores",
        ));
    }
    if lower.starts_with('_') || lower.ends_with('_') {
        return Err(AuthStackError::validation(
            "username cannot start or end with an underscore",
        ));
    }
    const RESERVED: &[&str] = &[
        "admin",
        "api",
        "account",
        "auth",
        "dashboard",
        "login",
        "logout",
        "register",
        "settings",
        "support",
        "system",
        "me",
        "null",
        "undefined",
        "u",
        "www",
        "root",
        "help",
        "about",
    ];
    if RESERVED.contains(&lower.as_str()) {
        return Err(AuthStackError::validation("that username is reserved"));
    }
    Ok(lower)
}

fn validate_avatar_data_url(value: &str) -> AuthStackResult<Option<String>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > MAX_AVATAR_DATA_URL_BYTES {
        return Err(AuthStackError::validation(
            "avatar is too large (use an image under ~250 KB)",
        ));
    }
    let ok = trimmed.starts_with("data:image/png;base64,")
        || trimmed.starts_with("data:image/jpeg;base64,")
        || trimmed.starts_with("data:image/jpg;base64,")
        || trimmed.starts_with("data:image/webp;base64,")
        || trimmed.starts_with("data:image/gif;base64,");
    if !ok {
        return Err(AuthStackError::validation(
            "avatar must be a PNG, JPEG, WebP, or GIF data URL",
        ));
    }
    Ok(Some(trimmed.to_owned()))
}

#[cfg(all(feature = "postgres", runtime_spin))]
async fn load_stored_profile(user_id: &str) -> AuthStackResult<StoredProfile> {
    let store = profile_kv().await?;
    let key = profile_user_key(user_id);
    let Some(bytes) = store
        .get(&key)
        .await
        .map_err(|error| AuthStackError::store(format!("profile read failed: {error}")))?
    else {
        return Ok(StoredProfile::empty(user_id));
    };
    serde_json::from_slice(&bytes)
        .map_err(|error| AuthStackError::store(format!("profile payload is corrupt: {error}")))
}

#[cfg(all(feature = "postgres", runtime_spin))]
async fn save_stored_profile(profile: &StoredProfile) -> AuthStackResult<()> {
    let store = profile_kv().await?;
    let bytes = serde_json::to_vec(profile)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;
    store
        .set(profile_user_key(&profile.user_id), bytes)
        .await
        .map_err(|error| AuthStackError::store(format!("profile write failed: {error}")))
}

pub async fn get_profile_for_user(
    user_id: &str,
    email: Option<String>,
) -> AuthStackResult<crate::contracts::ProfileView> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        Ok(load_stored_profile(user_id).await?.to_view(email))
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = user_id;
        Ok(StoredProfile::empty("").to_view(email))
    }
}

pub async fn update_profile_for_user(
    user_id: &str,
    email: Option<String>,
    request: crate::contracts::ProfileUpdateRequest,
) -> AuthStackResult<crate::contracts::ProfileView> {
    let first_name = normalize_name("first name", &request.first_name, MAX_NAME_LEN)?;
    let last_name = normalize_name("last name", &request.last_name, MAX_NAME_LEN)?;
    let display_name = normalize_name("display name", &request.display_name, MAX_DISPLAY_NAME_LEN)?;
    let username = normalize_username(&request.username)?;

    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (user_id, email, first_name, last_name, display_name, username, request);
        return Err(AuthStackError::configuration(
            "profile storage requires Spin key-value",
        ));
    }

    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let mut profile = load_stored_profile(user_id).await?;
        let previous_username = profile.username.clone();

        if let Some(avatar) = request.avatar_data_url.as_ref() {
            profile.avatar_data_url = validate_avatar_data_url(avatar)?;
        }

        // Username uniqueness (handle index).
        if !username.is_empty() && username != previous_username {
            let store = profile_kv().await?;
            let handle_key = profile_handle_key(&username);
            if let Some(existing) = store
                .get(&handle_key)
                .await
                .map_err(|error| AuthStackError::store(format!("handle lookup failed: {error}")))?
            {
                let owner = String::from_utf8_lossy(&existing);
                if owner != user_id {
                    return Err(AuthStackError::conflict("that username is already taken"));
                }
            }
        }

        profile.first_name = first_name;
        profile.last_name = last_name;
        profile.display_name = display_name;
        profile.username = username.clone();
        profile.is_public = request.is_public;

        let store = profile_kv().await?;
        if !previous_username.is_empty() && previous_username != username {
            let _ = store.delete(profile_handle_key(&previous_username)).await;
        }
        if !username.is_empty() {
            store
                .set(profile_handle_key(&username), user_id.as_bytes())
                .await
                .map_err(|error| AuthStackError::store(format!("handle write failed: {error}")))?;
        }

        save_stored_profile(&profile).await?;
        Ok(profile.to_view(email))
    }
}

pub async fn get_public_profile_by_username(
    username: &str,
) -> AuthStackResult<crate::contracts::PublicProfileView> {
    let username = normalize_username(username)?;
    if username.is_empty() {
        return Err(AuthStackError::not_found("profile not found"));
    }

    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = username;
        return Err(AuthStackError::not_found("profile not found"));
    }

    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let handle_key = profile_handle_key(&username);
        let Some(user_bytes) = store
            .get(&handle_key)
            .await
            .map_err(|error| AuthStackError::store(format!("handle lookup failed: {error}")))?
        else {
            return Err(AuthStackError::not_found("profile not found"));
        };
        let user_id = String::from_utf8_lossy(&user_bytes).into_owned();
        let profile = load_stored_profile(&user_id).await?;
        if !profile.is_public || profile.username != username {
            return Err(AuthStackError::not_found("profile not found"));
        }
        Ok(profile.to_public_view())
    }
}

// ── Dashboard board (Spin KV) ─────────────────────────────────────────────
// Layout / resources / queries / secrets are **workspace (org) scoped**.
// Notifications + legacy HTTP sources remain per-user.

const DASHBOARD_LAYOUT_ORG_PREFIX: &str = "app_dashboard:layout:org:";
const DASHBOARD_LAYOUT_LEGACY_PREFIX: &str = "app_dashboard:layout:";
const DASHBOARD_NOTIFS_PREFIX: &str = "app_dashboard:notifs:";
const DASHBOARD_SOURCES_PREFIX: &str = "app_dashboard:sources:";
/// Org-scoped vault: `app_dashboard:secrets:org:{organization_id}`
const DASHBOARD_SECRETS_ORG_PREFIX: &str = "app_dashboard:secrets:org:";
/// Legacy per-user vault (migrated on read when possible).
const DASHBOARD_SECRETS_LEGACY_PREFIX: &str = "app_dashboard:secrets:";
const DASHBOARD_RESOURCES_ORG_PREFIX: &str = "app_dashboard:resources:org:";
const DASHBOARD_RESOURCES_LEGACY_PREFIX: &str = "app_dashboard:resources:";
const DASHBOARD_QUERIES_ORG_PREFIX: &str = "app_dashboard:queries:org:";
const DASHBOARD_QUERIES_LEGACY_PREFIX: &str = "app_dashboard:queries:";
const ORG_SLUG_PREFIX: &str = "app_org:slug:";
const ORG_ID_SLUG_PREFIX: &str = "app_org:id:";
const MAX_BOARD_NODES: usize = 48;
const MAX_HTTP_SOURCES: usize = 16;
const MAX_RESOURCES: usize = 32;
const MAX_QUERIES: usize = 48;
const MAX_HTTP_RESPONSE_BYTES: usize = 256 * 1024;

fn dashboard_layout_key(org_id: &str) -> String {
    format!("{DASHBOARD_LAYOUT_ORG_PREFIX}{org_id}")
}

fn dashboard_layout_legacy_user_key(user_id: &str) -> String {
    format!("{DASHBOARD_LAYOUT_LEGACY_PREFIX}{user_id}")
}

fn dashboard_notifs_key(user_id: &str) -> String {
    format!("{DASHBOARD_NOTIFS_PREFIX}{user_id}")
}

fn dashboard_sources_key(user_id: &str) -> String {
    format!("{DASHBOARD_SOURCES_PREFIX}{user_id}")
}

fn dashboard_secrets_key(org_id: &str) -> String {
    format!("{DASHBOARD_SECRETS_ORG_PREFIX}{org_id}")
}

fn dashboard_secrets_legacy_user_key(user_id: &str) -> String {
    format!("{DASHBOARD_SECRETS_LEGACY_PREFIX}{user_id}")
}

fn dashboard_resources_key(org_id: &str) -> String {
    format!("{DASHBOARD_RESOURCES_ORG_PREFIX}{org_id}")
}

fn dashboard_resources_legacy_user_key(user_id: &str) -> String {
    format!("{DASHBOARD_RESOURCES_LEGACY_PREFIX}{user_id}")
}

fn dashboard_queries_key(org_id: &str) -> String {
    format!("{DASHBOARD_QUERIES_ORG_PREFIX}{org_id}")
}

fn dashboard_queries_legacy_user_key(user_id: &str) -> String {
    format!("{DASHBOARD_QUERIES_LEGACY_PREFIX}{user_id}")
}

fn org_slug_key(slug: &str) -> String {
    format!("{ORG_SLUG_PREFIX}{}", slug.trim().to_ascii_lowercase())
}

fn org_id_slug_key(org_id: &str) -> String {
    format!("{ORG_ID_SLUG_PREFIX}{org_id}:slug")
}

/// Reserved workspace URL segments (not usable as org slugs).
const RESERVED_ORG_SLUGS: &[&str] = &[
    "new",
    "settings",
    "admin",
    "api",
    "account",
    "org",
    "orgs",
    "organizations",
    "onboarding",
    "dashboard",
    "login",
    "register",
    "auth",
    "u",
    "invitations",
    "vault",
    "www",
    "app",
    "static",
    "assets",
];

/// Suggest a URL slug from a display name.
pub fn suggest_org_slug(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in name.trim().chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_owned();
    if trimmed.is_empty() {
        "workspace".to_owned()
    } else if trimmed.len() > 48 {
        trimmed.chars().take(48).collect::<String>().trim_end_matches('-').to_owned()
    } else {
        trimmed
    }
}

/// Validate org URL slug: `^[a-z][a-z0-9-]{1,47}$`.
pub fn validate_org_slug(slug: &str) -> AuthStackResult<()> {
    let slug = slug.trim();
    if slug.len() < 2 || slug.len() > 48 {
        return Err(AuthStackError::validation(
            "workspace URL must be 2–48 characters",
        ));
    }
    let mut chars = slug.chars();
    let Some(first) = chars.next() else {
        return Err(AuthStackError::validation("workspace URL is required"));
    };
    if !first.is_ascii_lowercase() {
        return Err(AuthStackError::validation(
            "workspace URL must start with a lowercase letter",
        ));
    }
    if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(AuthStackError::validation(
            "workspace URL may only contain a–z, 0–9, and hyphens",
        ));
    }
    if slug.starts_with('-') || slug.ends_with('-') || slug.contains("--") {
        return Err(AuthStackError::validation(
            "workspace URL cannot start/end with a hyphen or contain double hyphens",
        ));
    }
    if RESERVED_ORG_SLUGS.contains(&slug) {
        return Err(AuthStackError::validation(format!(
            "workspace URL “{slug}” is reserved"
        )));
    }
    Ok(())
}

pub async fn resolve_org_id_for_slug(slug: &str) -> AuthStackResult<String> {
    validate_org_slug(slug)?;
    // Prefer Postgres (via org list cache is done in application); KV is fallback.
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        // Direct SQL via app DATABASE_URL when available would be ideal; for now
        // KV remains the resolve path and is dual-written on create/list.
        let store = profile_kv().await?;
        if let Some(bytes) = store
            .get(org_slug_key(slug))
            .await
            .map_err(|e| AuthStackError::store(format!("org slug read failed: {e}")))?
        {
            let id = String::from_utf8(bytes)
                .map_err(|_| AuthStackError::store("org slug mapping is corrupt"))?;
            if !id.trim().is_empty() {
                return Ok(id);
            }
        }
        // Fallback: scan is not available; try postgres lookup.
        if let Ok(id) = resolve_org_id_for_slug_postgres(slug).await {
            let _ = register_org_slug(&id, slug).await;
            return Ok(id);
        }
        Err(AuthStackError::not_found("workspace not found"))
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = slug;
        Err(AuthStackError::configuration(
            "org slug storage requires Spin key-value",
        ))
    }
}

#[cfg(all(feature = "postgres", runtime_spin))]
async fn resolve_org_id_for_slug_postgres(slug: &str) -> AuthStackResult<String> {
    let rows = execute_sql(
        "SELECT organization_id::text AS organization_id \
         FROM auth_organizations \
         WHERE slug = $1 AND status = 'active' \
         LIMIT 1",
        vec![serde_json::Value::String(slug.to_owned())],
    )
    .await?;
    rows.first()
        .and_then(|row| {
            row.get("organization_id")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        })
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AuthStackError::not_found("workspace not found"))
}

pub async fn slug_for_organization(org_id: &str) -> AuthStackResult<Option<String>> {
    let org_id = org_id.trim();
    if org_id.is_empty() {
        return Ok(None);
    }
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let Some(bytes) = store
            .get(org_id_slug_key(org_id))
            .await
            .map_err(|e| AuthStackError::store(format!("org id slug read failed: {e}")))?
        else {
            return Ok(None);
        };
        Ok(String::from_utf8(bytes).ok().filter(|s| !s.trim().is_empty()))
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = org_id;
        Ok(None)
    }
}

/// Register a unique slug for an organization. Fails if slug is taken by another org.
pub async fn register_org_slug(org_id: &str, slug: &str) -> AuthStackResult<()> {
    validate_org_slug(slug)?;
    let org_id = org_id.trim();
    if org_id.is_empty() {
        return Err(AuthStackError::validation("organization_id is required"));
    }
    let slug = slug.trim().to_ascii_lowercase();
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        if let Some(existing) = store
            .get(org_slug_key(&slug))
            .await
            .map_err(|e| AuthStackError::store(format!("org slug read failed: {e}")))?
        {
            let existing_id = String::from_utf8(existing).unwrap_or_default();
            if existing_id != org_id {
                return Err(AuthStackError::validation(format!(
                    "workspace URL “{slug}” is already taken"
                )));
            }
            // Same org re-registering — ok.
            return Ok(());
        }
        // Clear previous slug for this org if any.
        if let Some(prev) = store
            .get(org_id_slug_key(org_id))
            .await
            .map_err(|e| AuthStackError::store(format!("org id slug read failed: {e}")))?
        {
            if let Ok(prev_slug) = String::from_utf8(prev) {
                let _ = store.delete(org_slug_key(&prev_slug)).await;
            }
        }
        store
            .set(org_slug_key(&slug), org_id.as_bytes())
            .await
            .map_err(|e| AuthStackError::store(format!("org slug write failed: {e}")))?;
        store
            .set(org_id_slug_key(org_id), slug.as_bytes())
            .await
            .map_err(|e| AuthStackError::store(format!("org id slug write failed: {e}")))?;
        Ok(())
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (org_id, slug);
        Err(AuthStackError::configuration(
            "org slug storage requires Spin key-value",
        ))
    }
}

/// Ensure each org has a slug; auto-register from name when missing (best-effort unique).
pub async fn ensure_org_slug(org_id: &str, name: &str) -> AuthStackResult<String> {
    if let Some(existing) = slug_for_organization(org_id).await? {
        return Ok(existing);
    }
    let base = suggest_org_slug(name);
    let mut candidate = base.clone();
    for attempt in 0..32u32 {
        if attempt > 0 {
            candidate = format!("{base}-{}", attempt + 1);
            if candidate.len() > 48 {
                candidate = format!("ws-{}", &org_id.chars().take(8).collect::<String>());
            }
        }
        if validate_org_slug(&candidate).is_err() {
            continue;
        }
        if resolve_org_id_for_slug(&candidate).await.is_ok() {
            continue;
        }
        match register_org_slug(org_id, &candidate).await {
            Ok(()) => return Ok(candidate),
            Err(_) => continue,
        }
    }
    Err(AuthStackError::validation(
        "could not allocate a unique workspace URL",
    ))
}

fn new_id(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{prefix}{ms:x}")
}

pub fn default_dashboard_layout_public() -> crate::contracts::DashboardLayout {
    default_dashboard_layout()
}

fn default_dashboard_layout() -> crate::contracts::DashboardLayout {
    use crate::contracts::{
        BoardContainerKind, BoardNode, DashboardLayout, DashboardWidgetKind, HttpDisplayMode,
        WidgetBind,
    };
    let widget = |index: usize, kind: DashboardWidgetKind| BoardNode::Widget {
        id: format!("w{index}"),
        kind: kind.clone(),
        col_span: kind.default_span(),
        note_text: if matches!(kind, DashboardWidgetKind::Notes) {
            Some(String::new())
        } else {
            None
        },
        source_id: None,
        bind: WidgetBind::default(),
        http_mode: HttpDisplayMode::List,
    };
    // Metrics row in a container; remaining tiles at root.
    let metrics = BoardNode::Container {
        id: "c-metrics".to_owned(),
        kind: BoardContainerKind::Row,
        col_span: 12,
        children: vec![
            widget(0, DashboardWidgetKind::MetricSession),
            widget(1, DashboardWidgetKind::MetricDevices),
            widget(2, DashboardWidgetKind::MetricOrgs),
            widget(3, DashboardWidgetKind::MetricSecurity),
        ],
    };
    let activity_row = BoardNode::Container {
        id: "c-main".to_owned(),
        kind: BoardContainerKind::Row,
        col_span: 12,
        children: vec![
            widget(4, DashboardWidgetKind::Activity),
            widget(5, DashboardWidgetKind::Notifications),
        ],
    };
    DashboardLayout {
        version: 2,
        nodes: vec![
            metrics,
            activity_row,
            widget(6, DashboardWidgetKind::Sessions),
            widget(7, DashboardWidgetKind::Organizations),
            widget(8, DashboardWidgetKind::SecurityPosture),
            widget(9, DashboardWidgetKind::Checklist),
        ],
        widgets: Vec::new(),
    }
}

pub use crate::app::dashboard::bind::json_path_get;

/// SSRF: reject private / link-local / metadata hosts unless allow_private.
pub fn validate_http_url(url: &str, allow_private: bool) -> AuthStackResult<()> {
    let url = url.trim();
    if url.is_empty() {
        return Err(AuthStackError::validation("url is required"));
    }
    if url.len() > 2_048 {
        return Err(AuthStackError::validation("url is too long"));
    }
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err(AuthStackError::validation("url must be http(s)"));
    }
    let without_scheme = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .unwrap_or("");
    let host_port = without_scheme
        .split('/')
        .next()
        .unwrap_or("")
        .split('@')
        .next_back()
        .unwrap_or("");
    let host = host_port
        .split(':')
        .next()
        .unwrap_or("")
        .trim_matches(|c| c == '[' || c == ']');
    if host.is_empty() {
        return Err(AuthStackError::validation("url host is missing"));
    }
    if host == "localhost" || host.ends_with(".localhost") {
        if !allow_private {
            return Err(AuthStackError::validation(
                "localhost targets are blocked (set AUTH_DASHBOARD_HTTP_ALLOW_PRIVATE=true to allow)",
            ));
        }
        return Ok(());
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        let blocked = match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_private()
                    || v4.is_loopback()
                    || v4.is_link_local()
                    || v4.octets()[0] == 169 && v4.octets()[1] == 254
                    || v4.octets()[0] == 0
            }
            std::net::IpAddr::V6(v6) => v6.is_loopback() || v6.is_unique_local(),
        };
        if blocked && !allow_private {
            return Err(AuthStackError::validation(
                "private or link-local IP targets are blocked",
            ));
        }
    }
    Ok(())
}

fn default_notifications() -> Vec<crate::contracts::DashboardNotification> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    vec![
        crate::contracts::DashboardNotification {
            id: "n-welcome".to_owned(),
            title: "Welcome to your board".to_owned(),
            body: "Add, remove, and rearrange widgets. This layout is saved to your account."
                .to_owned(),
            level: "info".to_owned(),
            read: false,
            created_at_ms: ms,
        },
        crate::contracts::DashboardNotification {
            id: "n-security".to_owned(),
            title: "Harden sign-in".to_owned(),
            body: "Enroll an authenticator or passkey so step-up and phishing-resistant login are ready."
                .to_owned(),
            level: "warn".to_owned(),
            read: false,
            created_at_ms: ms.saturating_sub(60_000),
        },
    ]
}

fn validate_board_nodes(nodes: &[crate::contracts::BoardNode], depth: u8) -> AuthStackResult<()> {
    use crate::contracts::BoardNode;
    if depth > 4 {
        return Err(AuthStackError::validation(
            "dashboard containers can nest at most 4 levels",
        ));
    }
    for node in nodes {
        if node.id().trim().is_empty() || node.id().len() > 64 {
            return Err(AuthStackError::validation("node id is invalid"));
        }
        if !(1..=12).contains(&node.col_span()) {
            return Err(AuthStackError::validation("col_span must be 1–12"));
        }
        if let BoardNode::Container { children, .. } = node {
            validate_board_nodes(children, depth + 1)?;
        }
    }
    Ok(())
}

pub async fn load_dashboard_layout(
    org_id: &str,
) -> AuthStackResult<crate::contracts::DashboardLayout> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let key = dashboard_layout_key(org_id);
        let Some(bytes) = store
            .get(&key)
            .await
            .map_err(|error| AuthStackError::store(format!("dashboard layout read failed: {error}")))?
        else {
            let layout = default_dashboard_layout();
            save_dashboard_layout(org_id, &layout).await?;
            return Ok(layout);
        };
        match serde_json::from_slice::<crate::contracts::DashboardLayout>(&bytes) {
            Ok(mut layout) => {
                layout.migrate_if_needed();
                if layout.nodes.is_empty() {
                    let layout = default_dashboard_layout();
                    save_dashboard_layout(org_id, &layout).await?;
                    return Ok(layout);
                }
                // Persist migration so clients always see v2.
                if layout.version < 2 || !layout.widgets.is_empty() {
                    layout.widgets.clear();
                    layout.version = 2;
                    let _ = save_dashboard_layout(org_id, &layout).await;
                }
                Ok(layout)
            }
            _ => {
                let layout = default_dashboard_layout();
                save_dashboard_layout(org_id, &layout).await?;
                Ok(layout)
            }
        }
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = org_id;
        Ok(default_dashboard_layout())
    }
}

pub async fn save_dashboard_layout(
    org_id: &str,
    layout: &crate::contracts::DashboardLayout,
) -> AuthStackResult<()> {
    let mut layout = layout.clone();
    layout.migrate_if_needed();
    layout.widgets.clear();
    layout.version = 2;
    if layout.total_nodes() > MAX_BOARD_NODES {
        return Err(AuthStackError::validation(format!(
            "dashboard supports at most {MAX_BOARD_NODES} nodes"
        )));
    }
    if layout.nodes.is_empty() {
        return Err(AuthStackError::validation("dashboard must have at least one node"));
    }
    validate_board_nodes(&layout.nodes, 0)?;

    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let bytes = serde_json::to_vec(&layout)
            .map_err(|error| AuthStackError::serialization(error.to_string()))?;
        store
            .set(dashboard_layout_key(org_id), bytes)
            .await
            .map_err(|error| AuthStackError::store(format!("dashboard layout write failed: {error}")))
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (org_id, layout);
        Err(AuthStackError::configuration(
            "dashboard storage requires Spin key-value",
        ))
    }
}

/// One-time: copy per-user board (layout/resources/queries) into org keys when org is empty.
pub async fn migrate_legacy_user_board_to_org(
    user_id: &str,
    org_id: &str,
) -> AuthStackResult<bool> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let mut changed = false;

        // Layout
        let org_layout_key = dashboard_layout_key(org_id);
        if store
            .get(&org_layout_key)
            .await
            .map_err(|e| AuthStackError::store(format!("layout read failed: {e}")))?
            .is_none()
        {
            if let Some(bytes) = store
                .get(dashboard_layout_legacy_user_key(user_id))
                .await
                .map_err(|e| AuthStackError::store(format!("legacy layout read failed: {e}")))?
            {
                store
                    .set(&org_layout_key, bytes)
                    .await
                    .map_err(|e| AuthStackError::store(format!("layout migrate write failed: {e}")))?;
                changed = true;
            }
        }

        // Resources
        let org_res_key = dashboard_resources_key(org_id);
        if store
            .get(&org_res_key)
            .await
            .map_err(|e| AuthStackError::store(format!("resources read failed: {e}")))?
            .is_none()
        {
            if let Some(bytes) = store
                .get(dashboard_resources_legacy_user_key(user_id))
                .await
                .map_err(|e| AuthStackError::store(format!("legacy resources read failed: {e}")))?
            {
                store
                    .set(&org_res_key, bytes)
                    .await
                    .map_err(|e| {
                        AuthStackError::store(format!("resources migrate write failed: {e}"))
                    })?;
                changed = true;
            }
        }

        // Queries
        let org_q_key = dashboard_queries_key(org_id);
        if store
            .get(&org_q_key)
            .await
            .map_err(|e| AuthStackError::store(format!("queries read failed: {e}")))?
            .is_none()
        {
            if let Some(bytes) = store
                .get(dashboard_queries_legacy_user_key(user_id))
                .await
                .map_err(|e| AuthStackError::store(format!("legacy queries read failed: {e}")))?
            {
                store
                    .set(&org_q_key, bytes)
                    .await
                    .map_err(|e| AuthStackError::store(format!("queries migrate write failed: {e}")))?;
                changed = true;
            }
        }

        Ok(changed)
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (user_id, org_id);
        Ok(false)
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct StoredSecret {
    id: String,
    /// Env-like key.
    #[serde(default)]
    key: String,
    /// Human label.
    #[serde(default)]
    label: String,
    /// Legacy name field (older payloads).
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_secret_scope")]
    scope: String,
    /// Legacy plaintext (migrated away on load; never persisted after migration).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    value: String,
    #[serde(default)]
    ciphertext_b64: String,
    #[serde(default)]
    nonce_b64: String,
    /// Unused (AES-GCM tag is part of ciphertext); kept for forward-compat.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    mac_b64: String,
    #[serde(default)]
    key_version: String,
    #[serde(default)]
    created_at_ms: u64,
    #[serde(default)]
    updated_at_ms: u64,
}

fn default_secret_scope() -> String {
    "user".to_owned()
}

const VAULT_SECRET_AAD_PREFIX: &[u8] = b"fullstack-app:vault-secret:v1:org:";
const VAULT_NONCE_BYTES: usize = 12;
const MAX_VAULT_SECRETS: usize = 64;
const MAX_VAULT_VALUE_BYTES: usize = 8_192;
const VAULT_REVEAL_TTL_SECONDS: u32 = 30;

/// Env-like vault key: `^[A-Z][A-Z0-9_]{1,63}$`.
pub fn validate_vault_secret_key(key: &str) -> AuthStackResult<()> {
    let key = key.trim();
    if key.len() < 2 || key.len() > 64 {
        return Err(AuthStackError::validation(
            "secret key must be 2–64 characters (e.g. STRIPE_SECRET_KEY)",
        ));
    }
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return Err(AuthStackError::validation("secret key is required"));
    };
    if !first.is_ascii_uppercase() {
        return Err(AuthStackError::validation(
            "secret key must start with A–Z (e.g. API_TOKEN)",
        ));
    }
    if !chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') {
        return Err(AuthStackError::validation(
            "secret key may only contain A–Z, 0–9, and underscore",
        ));
    }
    Ok(())
}

fn vault_secret_aad(org_id: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(VAULT_SECRET_AAD_PREFIX.len() + org_id.len());
    aad.extend_from_slice(VAULT_SECRET_AAD_PREFIX);
    aad.extend_from_slice(org_id.as_bytes());
    aad
}

async fn dashboard_vault_key_material() -> AuthStackResult<(String, [u8; 32])> {
    let production = store_config_value(AUTH_PRODUCTION_MODE)
        .await
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false);
    let configured = store_config_value(MFA_VAULT_KEY)
        .await
        .filter(|value| !value.trim().is_empty());
    let key: [u8; 32] = match configured {
        Some(encoded) => STANDARD
            .decode(encoded.trim())
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| {
                AuthStackError::configuration("AUTH_VAULT_KEY_BASE64 must decode to 32 bytes")
            })?,
        None if production => {
            return Err(AuthStackError::configuration(
                "AUTH_VAULT_KEY_BASE64 is required in production",
            ));
        }
        None => Sha256::digest(b"fullstack-development-outbox-key").into(),
    };
    let key_version = store_config_value("AUTH_VAULT_KEY_VERSION")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "development-v1".to_owned());
    Ok((key_version, key))
}

fn encrypt_vault_value(
    org_id: &str,
    plaintext: &str,
    key: &[u8; 32],
) -> AuthStackResult<(String, String)> {
    use aes_gcm::{
        Aes256Gcm, Nonce,
        aead::{Aead, KeyInit, Payload},
    };
    let mut nonce = [0_u8; VAULT_NONCE_BYTES];
    getrandom::getrandom(&mut nonce).map_err(|error| {
        AuthStackError::store(format!("vault nonce generation failed: {error}"))
    })?;
    let cipher = Aes256Gcm::new_from_slice(key.as_slice())
        .map_err(|_| AuthStackError::configuration("vault encryption key is invalid"))?;
    let aad = vault_secret_aad(org_id);
    let ciphertext = cipher
        .encrypt(
            &Nonce::from(nonce),
            Payload {
                msg: plaintext.as_bytes(),
                aad: &aad,
            },
        )
        .map_err(|_| AuthStackError::store("vault encryption failed"))?;
    Ok((STANDARD.encode(nonce), STANDARD.encode(ciphertext)))
}

fn decrypt_vault_value(
    org_id: &str,
    secret: &StoredSecret,
    key: &[u8; 32],
) -> AuthStackResult<String> {
    use aes_gcm::{
        Aes256Gcm, Nonce,
        aead::{Aead, KeyInit, Payload},
    };
    if secret.ciphertext_b64.is_empty() || secret.nonce_b64.is_empty() {
        if !secret.value.is_empty() {
            return Ok(secret.value.clone());
        }
        return Err(AuthStackError::store("secret has no encrypted value"));
    }
    let nonce_bytes = STANDARD
        .decode(secret.nonce_b64.trim())
        .map_err(|_| AuthStackError::store("secret nonce is corrupt"))?;
    let nonce: [u8; VAULT_NONCE_BYTES] = nonce_bytes
        .as_slice()
        .try_into()
        .map_err(|_| AuthStackError::store("secret nonce length is invalid"))?;
    let ciphertext = STANDARD
        .decode(secret.ciphertext_b64.trim())
        .map_err(|_| AuthStackError::store("secret ciphertext is corrupt"))?;
    let cipher = Aes256Gcm::new_from_slice(key.as_slice())
        .map_err(|_| AuthStackError::configuration("vault encryption key is invalid"))?;
    let aad = vault_secret_aad(org_id);
    let plain = cipher
        .decrypt(
            &Nonce::from(nonce),
            Payload {
                msg: &ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| AuthStackError::store("secret decryption failed"))?;
    String::from_utf8(plain).map_err(|_| AuthStackError::store("secret plaintext is not utf-8"))
}

fn secret_display_key(secret: &StoredSecret) -> String {
    if !secret.key.is_empty() {
        secret.key.clone()
    } else if !secret.name.is_empty() {
        secret.name.clone()
    } else {
        secret.id.clone()
    }
}

fn secret_display_label(secret: &StoredSecret) -> String {
    if !secret.label.is_empty() {
        secret.label.clone()
    } else if !secret.name.is_empty() && secret.name != secret.key {
        secret.name.clone()
    } else {
        secret_display_key(secret)
    }
}

fn secret_to_summary(secret: &StoredSecret) -> crate::contracts::SecretSummary {
    let key = secret_display_key(secret);
    let label = secret_display_label(secret);
    crate::contracts::SecretSummary {
        id: secret.id.clone(),
        key: key.clone(),
        label: label.clone(),
        name: if !key.is_empty() { key } else { label },
        description: secret.description.clone(),
        scope: if secret.scope.is_empty() {
            "user".to_owned()
        } else {
            secret.scope.clone()
        },
        created_at_ms: secret.created_at_ms,
        updated_at_ms: secret.updated_at_ms,
        masked_value: "••••••••".to_owned(),
    }
}

/// Load secrets, migrate legacy plaintext → AEAD ciphertext, and return records
/// with **in-memory** plaintext in `value` for connector resolution only.
///
/// `org_id` is the workspace vault owner (organization UUID).
async fn load_secrets_resolved(org_id: &str) -> AuthStackResult<Vec<StoredSecret>> {
    let mut secrets = load_secrets_raw(org_id).await?;
    if secrets.is_empty() {
        return Ok(secrets);
    }
    let (key_version, key) = dashboard_vault_key_material().await?;
    let mut dirty = false;
    let now = dashboard_now_ms();
    for secret in &mut secrets {
        // Normalize legacy name-only rows.
        if secret.key.is_empty() && !secret.name.is_empty() {
            let candidate = secret
                .name
                .trim()
                .to_ascii_uppercase()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect::<String>();
            if validate_vault_secret_key(&candidate).is_ok() {
                secret.key = candidate;
            } else {
                secret.key = format!("LEGACY_{}", &secret.id);
            }
            dirty = true;
        }
        if secret.label.is_empty() {
            secret.label = if !secret.name.is_empty() {
                secret.name.clone()
            } else {
                secret.key.clone()
            };
            dirty = true;
        }
        if secret.created_at_ms == 0 {
            secret.created_at_ms = now;
            dirty = true;
        }
        if secret.updated_at_ms == 0 {
            secret.updated_at_ms = secret.created_at_ms;
            dirty = true;
        }

        if !secret.value.is_empty() && secret.ciphertext_b64.is_empty() {
            let (nonce_b64, ciphertext_b64) =
                encrypt_vault_value(org_id, &secret.value, &key)?;
            secret.nonce_b64 = nonce_b64;
            secret.ciphertext_b64 = ciphertext_b64;
            secret.key_version = key_version.clone();
            secret.value.clear();
            dirty = true;
        }
    }
    if dirty {
        // Persist ciphertext only (skip empty plaintext via serde skip).
        let to_save: Vec<StoredSecret> = secrets
            .iter()
            .map(|s| {
                let mut copy = s.clone();
                copy.value.clear();
                copy
            })
            .collect();
        save_secrets_raw(org_id, &to_save).await?;
    }
    // Decrypt into memory for execute path.
    for secret in &mut secrets {
        if secret.value.is_empty() && !secret.ciphertext_b64.is_empty() {
            // Try org AAD first; fall back to re-encrypt if decrypt fails (legacy user AAD).
            match decrypt_vault_value(org_id, secret, &key) {
                Ok(plain) => secret.value = plain,
                Err(_) if !secret.value.is_empty() => {}
                Err(_) => {
                    // Leave empty; create path always uses org AAD.
                    return Err(AuthStackError::store(
                        "secret decryption failed — re-create the secret in the org vault",
                    ));
                }
            }
        }
    }
    Ok(secrets)
}

pub async fn load_data_sources(user_id: &str) -> AuthStackResult<Vec<crate::contracts::DataSource>> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let Some(bytes) = store
            .get(dashboard_sources_key(user_id))
            .await
            .map_err(|e| AuthStackError::store(format!("sources read failed: {e}")))?
        else {
            return Ok(Vec::new());
        };
        Ok(serde_json::from_slice(&bytes).unwrap_or_default())
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = user_id;
        Ok(Vec::new())
    }
}

pub async fn save_data_sources(
    user_id: &str,
    sources: &[crate::contracts::DataSource],
) -> AuthStackResult<()> {
    if sources.len() > MAX_HTTP_SOURCES {
        return Err(AuthStackError::validation(format!(
            "at most {MAX_HTTP_SOURCES} HTTP sources"
        )));
    }
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let bytes = serde_json::to_vec(sources)
            .map_err(|e| AuthStackError::serialization(e.to_string()))?;
        store
            .set(dashboard_sources_key(user_id), bytes)
            .await
            .map_err(|e| AuthStackError::store(format!("sources write failed: {e}")))
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (user_id, sources);
        Err(AuthStackError::configuration(
            "dashboard storage requires Spin key-value",
        ))
    }
}

async fn load_secrets_raw(org_id: &str) -> AuthStackResult<Vec<StoredSecret>> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let Some(bytes) = store
            .get(dashboard_secrets_key(org_id))
            .await
            .map_err(|e| AuthStackError::store(format!("secrets read failed: {e}")))?
        else {
            return Ok(Vec::new());
        };
        Ok(serde_json::from_slice(&bytes).unwrap_or_default())
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = org_id;
        Ok(Vec::new())
    }
}

async fn save_secrets_raw(org_id: &str, secrets: &[StoredSecret]) -> AuthStackResult<()> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let bytes = serde_json::to_vec(secrets)
            .map_err(|e| AuthStackError::serialization(e.to_string()))?;
        store
            .set(dashboard_secrets_key(org_id), bytes)
            .await
            .map_err(|e| AuthStackError::store(format!("secrets write failed: {e}")))
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (org_id, secrets);
        Err(AuthStackError::configuration(
            "dashboard storage requires Spin key-value",
        ))
    }
}

/// Detailed secrets migrate (used by admin/user migrate API).
pub async fn migrate_legacy_user_secrets_to_org_detailed(
    user_id: &str,
    org_id: &str,
    dry_run: bool,
) -> AuthStackResult<(bool, u32, u32, Vec<String>)> {
    // returns (copied, rows_copied, skipped_reenter, keys)
    let existing = load_secrets_raw(org_id).await?;
    if !existing.is_empty() {
        return Ok((false, 0, 0, Vec::new()));
    }
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let Some(bytes) = store
            .get(dashboard_secrets_legacy_user_key(user_id))
            .await
            .map_err(|e| AuthStackError::store(format!("legacy secrets read failed: {e}")))?
        else {
            return Ok((false, 0, 0, Vec::new()));
        };
        let legacy: Vec<StoredSecret> = serde_json::from_slice(&bytes).unwrap_or_default();
        if legacy.is_empty() {
            return Ok((false, 0, 0, Vec::new()));
        }
        let (key_version, key) = dashboard_vault_key_material().await?;
        let mut migrated = Vec::new();
        let mut reenter = Vec::new();
        for mut secret in legacy {
            if !secret.value.is_empty() {
                if !dry_run {
                    let (nonce_b64, ciphertext_b64) =
                        encrypt_vault_value(org_id, &secret.value, &key)?;
                    secret.nonce_b64 = nonce_b64;
                    secret.ciphertext_b64 = ciphertext_b64;
                    secret.key_version = key_version.clone();
                    secret.value.clear();
                }
                migrated.push(secret);
            } else if !secret.ciphertext_b64.is_empty() {
                reenter.push(secret_display_key(&secret));
            }
        }
        if migrated.is_empty() {
            return Ok((false, 0, reenter.len() as u32, reenter));
        }
        if !dry_run {
            save_secrets_raw(org_id, &migrated).await?;
            let _ = store.delete(dashboard_secrets_legacy_user_key(user_id)).await;
        }
        Ok((
            true,
            migrated.len() as u32,
            reenter.len() as u32,
            reenter,
        ))
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (user_id, org_id, dry_run);
        Ok((false, 0, 0, Vec::new()))
    }
}

/// One-time best-effort: copy legacy user-scoped secrets into org vault when org vault empty.
pub async fn migrate_legacy_user_secrets_to_org(
    user_id: &str,
    org_id: &str,
) -> AuthStackResult<bool> {
    let (copied, _, _, _) =
        migrate_legacy_user_secrets_to_org_detailed(user_id, org_id, false).await?;
    Ok(copied)
}

pub async fn list_secret_summaries(
    org_id: &str,
) -> AuthStackResult<Vec<crate::contracts::SecretSummary>> {
    // Resolve migrates plaintext → ciphertext; summaries never include values.
    let secrets = load_secrets_resolved(org_id).await?;
    Ok(secrets.iter().map(secret_to_summary).collect())
}

pub async fn create_secret(
    org_id: &str,
    request: &crate::contracts::SecretCreateRequest,
) -> AuthStackResult<crate::contracts::SecretSummary> {
    let mut key = request.key.trim().to_owned();
    if key.is_empty() {
        key = request.name.trim().to_owned();
    }
    // Allow human names to be uppercased into env-like keys when possible.
    if validate_vault_secret_key(&key).is_err() {
        let candidate = key
            .trim()
            .to_ascii_uppercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>();
        key = candidate;
    }
    validate_vault_secret_key(&key)?;

    let label = {
        let l = request.label.trim();
        if l.is_empty() {
            let n = request.name.trim();
            if n.is_empty() {
                key.clone()
            } else {
                n.to_owned()
            }
        } else {
            l.to_owned()
        }
    };
    if label.len() > 120 {
        return Err(AuthStackError::validation("secret label is too long"));
    }
    let description = request.description.trim().to_owned();
    if description.len() > 500 {
        return Err(AuthStackError::validation("secret description is too long"));
    }
    let scope = match request.scope.trim().to_ascii_lowercase().as_str() {
        "" | "user" => "user".to_owned(),
        "app" => "app".to_owned(),
        _ => {
            return Err(AuthStackError::validation(
                "secret scope must be user or app",
            ));
        }
    };
    let value = request.value.as_str();
    if value.is_empty() || value.len() > MAX_VAULT_VALUE_BYTES {
        return Err(AuthStackError::validation(format!(
            "secret value must be 1–{MAX_VAULT_VALUE_BYTES} bytes"
        )));
    }

    // Migrate legacy plaintext rows first.
    let _ = load_secrets_resolved(org_id).await?;
    let mut secrets = load_secrets_raw(org_id).await?;

    if secrets.len() >= MAX_VAULT_SECRETS {
        return Err(AuthStackError::validation(format!(
            "at most {MAX_VAULT_SECRETS} vault secrets"
        )));
    }
    if secrets
        .iter()
        .any(|s| secret_display_key(s).eq_ignore_ascii_case(&key))
    {
        return Err(AuthStackError::validation(format!(
            "a secret with key {key} already exists"
        )));
    }

    let (key_version, vault_key) = dashboard_vault_key_material().await?;
    let (nonce_b64, ciphertext_b64) = encrypt_vault_value(org_id, value, &vault_key)?;
    let now = dashboard_now_ms();
    let id = new_id("sec");
    let stored = StoredSecret {
        id: id.clone(),
        key: key.clone(),
        label: label.clone(),
        name: key.clone(),
        description: description.clone(),
        scope: scope.clone(),
        value: String::new(),
        ciphertext_b64,
        nonce_b64,
        mac_b64: String::new(),
        key_version,
        created_at_ms: now,
        updated_at_ms: now,
    };
    secrets.push(stored.clone());
    save_secrets_raw(org_id, &secrets).await?;
    Ok(secret_to_summary(&stored))
}

pub async fn delete_secret(org_id: &str, secret_id: &str) -> AuthStackResult<()> {
    let mut secrets = load_secrets_raw(org_id).await?;
    let before = secrets.len();
    secrets.retain(|s| s.id != secret_id);
    if secrets.len() == before {
        return Err(AuthStackError::not_found("secret not found"));
    }
    save_secrets_raw(org_id, &secrets).await
}

pub async fn reveal_secret(
    org_id: &str,
    secret_id: &str,
) -> AuthStackResult<crate::contracts::SecretRevealResponse> {
    let secrets = load_secrets_resolved(org_id).await?;
    let secret = secrets
        .iter()
        .find(|s| s.id == secret_id)
        .ok_or_else(|| AuthStackError::not_found("secret not found"))?;
    let value = secret.value.clone();
    if value.is_empty() {
        return Err(AuthStackError::store("secret value is empty"));
    }
    tracing::info!(
        organization_id = %org_id,
        secret_id = %secret_id,
        secret_key = %secret_display_key(secret),
        "vault secret revealed"
    );
    Ok(crate::contracts::SecretRevealResponse {
        id: secret.id.clone(),
        key: secret_display_key(secret),
        value,
        reveal_ttl_seconds: VAULT_REVEAL_TTL_SECONDS,
    })
}

/// Idempotent demo pack: REST + @app Postgres resources/queries + bound widgets.
/// Writes into the **workspace** board + vault (`org_id`).
/// Returns `true` when something new was seeded.
pub async fn seed_dashboard_demos(org_id: &str) -> AuthStackResult<bool> {
    use crate::contracts::{
        BoardContainerKind, BoardNode, DashboardQuery, DashboardResource, DashboardWidgetKind,
        HttpDisplayMode, HttpMethod, QueryConfig, ResourceAuth, ResourceConfig, ResourceKind,
        TransformStep, WidgetBind,
    };

    const DEMO_REST_RES: &str = "demo-res-jsonplaceholder";
    const DEMO_PG_RES: &str = "demo-res-app-postgres";
    const DEMO_Q_LIST: &str = "demo-q-todos";
    const DEMO_Q_METRIC: &str = "demo-q-todo-count";
    const DEMO_Q_TABLE: &str = "demo-q-pg-info";
    const DEMO_ROW: &str = "demo-row-connectors";
    const DEMO_W_LIST: &str = "demo-w-list";
    const DEMO_W_METRIC: &str = "demo-w-metric";
    const DEMO_W_TABLE: &str = "demo-w-table";

    let mut resources = load_resources(org_id).await.unwrap_or_default();
    let mut queries = load_queries(org_id).await.unwrap_or_default();
    let mut layout = load_dashboard_layout(org_id).await?;
    layout.migrate_if_needed();
    let mut changed = false;

    // Placeholder vault secret for demos that show how auth pickers work.
    let secrets = load_secrets_raw(org_id).await?;
    if !secrets
        .iter()
        .any(|s| secret_display_key(s) == "DEMO_API_TOKEN")
    {
        let _ = create_secret(
            org_id,
            &crate::contracts::SecretCreateRequest {
                key: "DEMO_API_TOKEN".to_owned(),
                name: "DEMO_API_TOKEN".to_owned(),
                value: "demo-not-secret".to_owned(),
                label: "Demo API token".to_owned(),
                description: "Placeholder for resource auth pickers — not a real credential."
                    .to_owned(),
                scope: "user".to_owned(),
            },
        )
        .await?;
        changed = true;
    }

    if !resources.iter().any(|r| r.id == DEMO_REST_RES) {
        resources.push(DashboardResource {
            id: DEMO_REST_RES.to_owned(),
            name: "Demo · JSONPlaceholder".to_owned(),
            kind: ResourceKind::Rest,
            auth: ResourceAuth::None,
            default_headers: Vec::new(),
            config: ResourceConfig::Rest {
                base_url: "https://jsonplaceholder.typicode.com".to_owned(),
                timeout_ms: 15_000,
            },
        });
        changed = true;
    }

    if !resources.iter().any(|r| r.id == DEMO_PG_RES) {
        resources.push(DashboardResource {
            id: DEMO_PG_RES.to_owned(),
            name: "Demo · App Postgres".to_owned(),
            kind: ResourceKind::Postgres,
            auth: ResourceAuth::None,
            default_headers: Vec::new(),
            config: ResourceConfig::Postgres {
                host: "@app".to_owned(),
                port: 5432,
                database: String::new(),
                user: String::new(),
                password_secret_id: String::new(),
                ssl_mode: crate::contracts::PostgresSslMode::Prefer,
            },
        });
        changed = true;
    }

    if !queries.iter().any(|q| q.id == DEMO_Q_LIST) {
        queries.push(DashboardQuery {
            id: DEMO_Q_LIST.to_owned(),
            name: "Demo todos".to_owned(),
            resource_id: DEMO_REST_RES.to_owned(),
            transform: vec![
                TransformStep::AsArray,
                TransformStep::Limit { n: 5 },
            ],
            config: QueryConfig::Rest {
                method: HttpMethod::Get,
                path: "/todos".to_owned(),
                query_params: Vec::new(),
                headers: Vec::new(),
                body: None,
            },
        });
        changed = true;
    }

    if !queries.iter().any(|q| q.id == DEMO_Q_METRIC) {
        queries.push(DashboardQuery {
            id: DEMO_Q_METRIC.to_owned(),
            name: "Demo todo #1".to_owned(),
            resource_id: DEMO_REST_RES.to_owned(),
            transform: Vec::new(),
            config: QueryConfig::Rest {
                method: HttpMethod::Get,
                path: "/todos/1".to_owned(),
                query_params: Vec::new(),
                headers: Vec::new(),
                body: None,
            },
        });
        changed = true;
    }

    if !queries.iter().any(|q| q.id == DEMO_Q_TABLE) {
        queries.push(DashboardQuery {
            id: DEMO_Q_TABLE.to_owned(),
            name: "Demo pg info".to_owned(),
            resource_id: DEMO_PG_RES.to_owned(),
            transform: Vec::new(),
            config: QueryConfig::Postgres {
                sql: "SELECT current_user AS user_name, current_database() AS db, now()::text AS ts"
                    .to_owned(),
            },
        });
        changed = true;
    }

    if changed {
        save_resources(org_id, &resources).await?;
        save_queries(org_id, &queries).await?;
    }

    let has_demo_row = layout.nodes.iter().any(|n| n.id() == DEMO_ROW);
    if !has_demo_row {
        layout.nodes.insert(
            0,
            BoardNode::Container {
                id: DEMO_ROW.to_owned(),
                kind: BoardContainerKind::Row,
                col_span: 12,
                children: vec![
                    BoardNode::Widget {
                        id: DEMO_W_LIST.to_owned(),
                        kind: DashboardWidgetKind::BoundList,
                        col_span: 6,
                        note_text: None,
                        source_id: Some(DEMO_Q_LIST.to_owned()),
                        bind: WidgetBind {
                            title_path: Some("title".to_owned()),
                            subtitle_path: Some("id".to_owned()),
                            meta_path: Some("completed".to_owned()),
                            ..WidgetBind::default()
                        },
                        http_mode: HttpDisplayMode::List,
                    },
                    BoardNode::Widget {
                        id: DEMO_W_METRIC.to_owned(),
                        kind: DashboardWidgetKind::BoundMetric,
                        col_span: 3,
                        note_text: None,
                        source_id: Some(DEMO_Q_METRIC.to_owned()),
                        bind: WidgetBind {
                            value_path: Some("id".to_owned()),
                            label_path: Some("title".to_owned()),
                            ..WidgetBind::default()
                        },
                        http_mode: HttpDisplayMode::Metric,
                    },
                    BoardNode::Widget {
                        id: DEMO_W_TABLE.to_owned(),
                        kind: DashboardWidgetKind::BoundTable,
                        col_span: 3,
                        note_text: None,
                        source_id: Some(DEMO_Q_TABLE.to_owned()),
                        bind: WidgetBind::default(),
                        http_mode: HttpDisplayMode::Table,
                    },
                ],
            },
        );
        save_dashboard_layout(org_id, &layout).await?;
        changed = true;
    }

    Ok(changed)
}

// ─── Resources / Queries (Retool model) ──────────────────────────────────────

pub async fn load_resources(
    org_id: &str,
) -> AuthStackResult<Vec<crate::contracts::DashboardResource>> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let Some(bytes) = store
            .get(dashboard_resources_key(org_id))
            .await
            .map_err(|e| AuthStackError::store(format!("resources read failed: {e}")))?
        else {
            // One-time migrate from legacy HTTP sources into this org board.
            return migrate_legacy_sources_to_resources(org_id).await;
        };
        Ok(serde_json::from_slice(&bytes).unwrap_or_default())
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = org_id;
        Ok(Vec::new())
    }
}

pub async fn save_resources(
    org_id: &str,
    resources: &[crate::contracts::DashboardResource],
) -> AuthStackResult<()> {
    if resources.len() > MAX_RESOURCES {
        return Err(AuthStackError::validation(format!(
            "at most {MAX_RESOURCES} resources"
        )));
    }
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let bytes = serde_json::to_vec(resources)
            .map_err(|e| AuthStackError::serialization(e.to_string()))?;
        store
            .set(dashboard_resources_key(org_id), bytes)
            .await
            .map_err(|e| AuthStackError::store(format!("resources write failed: {e}")))
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (org_id, resources);
        Err(AuthStackError::configuration(
            "dashboard storage requires Spin key-value",
        ))
    }
}

pub async fn load_queries(org_id: &str) -> AuthStackResult<Vec<crate::contracts::DashboardQuery>> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let Some(bytes) = store
            .get(dashboard_queries_key(org_id))
            .await
            .map_err(|e| AuthStackError::store(format!("queries read failed: {e}")))?
        else {
            return Ok(Vec::new());
        };
        Ok(serde_json::from_slice(&bytes).unwrap_or_default())
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = org_id;
        Ok(Vec::new())
    }
}

pub async fn save_queries(
    org_id: &str,
    queries: &[crate::contracts::DashboardQuery],
) -> AuthStackResult<()> {
    if queries.len() > MAX_QUERIES {
        return Err(AuthStackError::validation(format!(
            "at most {MAX_QUERIES} queries"
        )));
    }
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let bytes =
            serde_json::to_vec(queries).map_err(|e| AuthStackError::serialization(e.to_string()))?;
        store
            .set(dashboard_queries_key(org_id), bytes)
            .await
            .map_err(|e| AuthStackError::store(format!("queries write failed: {e}")))
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (org_id, queries);
        Err(AuthStackError::configuration(
            "dashboard storage requires Spin key-value",
        ))
    }
}

/// Legacy HTTP `DataSource` → Resource/Query migration is no longer automatic
/// under org-scoped boards (sources stayed per-user). New orgs start empty.
async fn migrate_legacy_sources_to_resources(
    org_id: &str,
) -> AuthStackResult<Vec<crate::contracts::DashboardResource>> {
    let _ = org_id;
    Ok(Vec::new())
}

fn split_base_and_path(url: &str) -> (String, String) {
    let url = url.trim();
    // scheme://host[:port]/path]
    let after_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    let Some(rest) = after_scheme else {
        return (url.to_owned(), "/".to_owned());
    };
    let scheme = if url.starts_with("https://") {
        "https"
    } else {
        "http"
    };
    let (host_port, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };
    if host_port.is_empty() {
        return (url.to_owned(), "/".to_owned());
    }
    (format!("{scheme}://{host_port}"), path.to_owned())
}

pub fn resource_to_summary(
    resource: &crate::contracts::DashboardResource,
) -> crate::contracts::ResourceSummary {
    use crate::contracts::{ResourceAuth, ResourceConfig};
    let auth_type = match &resource.auth {
        ResourceAuth::None => "none",
        ResourceAuth::Bearer { .. } => "bearer",
        ResourceAuth::Basic { .. } => "basic",
        ResourceAuth::ApiKey { .. } => "api_key",
        ResourceAuth::OAuth2ClientCredentials { .. } => "oauth2_cc",
    }
    .to_owned();
    let detail = match &resource.config {
        ResourceConfig::Builtin => "Built-in app data".to_owned(),
        ResourceConfig::Rest { base_url, .. } => base_url.clone(),
        ResourceConfig::Postgres {
            host, port, database, ..
        } => format!("{host}:{port}/{database}"),
        ResourceConfig::Grpc {
            host,
            port,
            gateway_base_url,
            ..
        } => {
            if let Some(g) = gateway_base_url.as_ref().filter(|s| !s.is_empty()) {
                format!("gateway:{g}")
            } else {
                format!("grpc://{host}:{port}")
            }
        }
    };
    let has_secrets = !matches!(resource.auth, ResourceAuth::None)
        || resource.default_headers.iter().any(|h| {
            matches!(
                h.value,
                crate::contracts::HeaderValue::Secret { .. }
            )
        })
        || matches!(
            &resource.config,
            ResourceConfig::Postgres {
                password_secret_id, ..
            } if !password_secret_id.is_empty()
        );
    crate::contracts::ResourceSummary {
        id: resource.id.clone(),
        name: resource.name.clone(),
        kind: resource.kind.clone(),
        auth_type,
        detail,
        header_names: resource
            .default_headers
            .iter()
            .map(|h| h.name.clone())
            .collect(),
        has_secrets,
    }
}

pub fn query_to_summary(
    query: &crate::contracts::DashboardQuery,
    resources: &[crate::contracts::DashboardResource],
) -> crate::contracts::QuerySummary {
    use crate::contracts::{QueryConfig, ResourceKind};
    let kind = resources
        .iter()
        .find(|r| r.id == query.resource_id)
        .map(|r| r.kind.clone())
        .unwrap_or(ResourceKind::Rest);
    let detail = match &query.config {
        QueryConfig::Rest { method, path, .. } => format!("{} {path}", method.as_str()),
        QueryConfig::Postgres { sql } => {
            let one = sql.lines().next().unwrap_or("SQL").trim();
            if one.len() > 64 {
                format!("{}…", &one[..64])
            } else {
                one.to_owned()
            }
        }
        QueryConfig::Grpc {
            service, method, ..
        } => format!("{service}/{method}"),
        QueryConfig::Builtin { key } => key.as_str().to_owned(),
    };
    crate::contracts::QuerySummary {
        id: query.id.clone(),
        name: query.name.clone(),
        resource_id: query.resource_id.clone(),
        resource_kind: kind,
        detail,
    }
}

/// Resolve a HeaderValue against the secret vault.
pub fn resolve_header_value(
    value: &crate::contracts::HeaderValue,
    secrets: &[StoredSecret],
) -> AuthStackResult<String> {
    match value {
        crate::contracts::HeaderValue::Literal { value } => Ok(value.clone()),
        crate::contracts::HeaderValue::Secret { secret_id } => secrets
            .iter()
            .find(|s| s.id == *secret_id)
            .map(|s| s.value.clone())
            .ok_or_else(|| AuthStackError::validation(format!("missing secret {secret_id}"))),
    }
}

/// Merge resource defaults + query overrides (query wins on name, case-insensitive).
pub fn merge_headers(
    resource_headers: &[crate::contracts::HeaderBag],
    query_headers: &[crate::contracts::HeaderBag],
) -> Vec<crate::contracts::HeaderBag> {
    let mut map: std::collections::BTreeMap<String, crate::contracts::HeaderBag> =
        std::collections::BTreeMap::new();
    for h in resource_headers {
        map.insert(h.name.to_ascii_lowercase(), h.clone());
    }
    for h in query_headers {
        map.insert(h.name.to_ascii_lowercase(), h.clone());
    }
    map.into_values().collect()
}

/// Apply ResourceAuth injectors into resolved header list (name, value).
pub fn apply_resource_auth(
    auth: &crate::contracts::ResourceAuth,
    secrets: &[StoredSecret],
    headers: &mut Vec<(String, String)>,
    query_params: &mut Vec<(String, String)>,
) -> AuthStackResult<()> {
    use crate::contracts::{ApiKeyLocation, ResourceAuth};
    match auth {
        ResourceAuth::None => {}
        ResourceAuth::Bearer { secret_id } => {
            let token = secrets
                .iter()
                .find(|s| s.id == *secret_id)
                .map(|s| s.value.clone())
                .ok_or_else(|| AuthStackError::validation("bearer secret missing"))?;
            if !headers
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case("authorization"))
            {
                headers.push(("Authorization".to_owned(), format!("Bearer {token}")));
            }
        }
        ResourceAuth::Basic {
            username,
            password_secret_id,
        } => {
            let password = secrets
                .iter()
                .find(|s| s.id == *password_secret_id)
                .map(|s| s.value.clone())
                .ok_or_else(|| AuthStackError::validation("basic password secret missing"))?;
            let encoded = base64_encode(&format!("{username}:{password}"));
            if !headers
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case("authorization"))
            {
                headers.push(("Authorization".to_owned(), format!("Basic {encoded}")));
            }
        }
        ResourceAuth::ApiKey {
            location,
            name,
            secret_id,
        } => {
            let key = secrets
                .iter()
                .find(|s| s.id == *secret_id)
                .map(|s| s.value.clone())
                .ok_or_else(|| AuthStackError::validation("api key secret missing"))?;
            match location {
                ApiKeyLocation::Header => {
                    if !headers.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)) {
                        headers.push((name.clone(), key));
                    }
                }
                ApiKeyLocation::QueryParam => {
                    if !query_params.iter().any(|(n, _)| n == name) {
                        query_params.push((name.clone(), key));
                    }
                }
            }
        }
        ResourceAuth::OAuth2ClientCredentials { .. } => {
            // Token fetch happens in execute path; placeholder rejects until wired.
            return Err(AuthStackError::validation(
                "OAuth2 client credentials execution is not enabled yet",
            ));
        }
    }
    Ok(())
}

fn base64_encode(input: &str) -> String {
    #[cfg(feature = "ssr")]
    {
        use base64::Engine;
        return base64::engine::general_purpose::STANDARD.encode(input.as_bytes());
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = input;
        String::new()
    }
}

/// Declarative transform pipeline (json_path → as_array → map_fields → limit → pick_scalar).
pub fn apply_transform_pipeline(
    value: Value,
    steps: &[crate::contracts::TransformStep],
) -> Value {
    use crate::contracts::TransformStep;
    use crate::app::dashboard::bind::json_path_get;
    let mut current = value;
    for step in steps {
        current = match step {
            TransformStep::JsonPath { path } => {
                json_path_get(&current, path).unwrap_or(Value::Null)
            }
            TransformStep::AsArray => match current {
                Value::Array(_) => current,
                Value::Object(map) => Value::Array(map.into_iter().map(|(_, v)| v).collect()),
                Value::Null => Value::Array(vec![]),
                other => Value::Array(vec![other]),
            },
            TransformStep::MapFields { fields } => {
                let rows: Vec<Value> = match &current {
                    Value::Array(items) => items.clone(),
                    other => vec![other.clone()],
                };
                let mapped: Vec<Value> = rows
                    .into_iter()
                    .map(|row| {
                        let mut obj = serde_json::Map::new();
                        for (target, source_path) in fields {
                            if let Some(v) = json_path_get(&row, source_path) {
                                obj.insert(target.clone(), v);
                            }
                        }
                        Value::Object(obj)
                    })
                    .collect();
                Value::Array(mapped)
            }
            TransformStep::Limit { n } => match current {
                Value::Array(mut items) => {
                    items.truncate(*n as usize);
                    Value::Array(items)
                }
                other => other,
            },
            TransformStep::PickScalar { path } => {
                if path.trim().is_empty() {
                    current
                } else {
                    json_path_get(&current, path).unwrap_or(Value::Null)
                }
            }
        };
    }
    current
}

pub fn data_source_to_summary(source: &crate::contracts::DataSource) -> crate::contracts::DataSourceSummary {
    crate::contracts::DataSourceSummary {
        id: source.id.clone(),
        name: source.name.clone(),
        kind: source.kind.clone(),
        builtin_key: source.builtin_key.clone(),
        method: source.method.clone(),
        url: source.url.clone(),
        json_path: source.json_path.clone(),
        shape: source.shape.clone(),
        header_names: source.headers.iter().map(|h| h.name.clone()).collect(),
        has_secrets: source.headers.iter().any(|h| h.secret_id.is_some()),
    }
}

pub async fn upsert_http_source(
    user_id: &str,
    request: crate::contracts::DataSourceUpsert,
    allow_private: bool,
) -> AuthStackResult<crate::contracts::DataSourceSummary> {
    validate_http_url(&request.url, allow_private)?;
    let method = request.method.trim().to_ascii_uppercase();
    if method != "GET" && method != "POST" {
        return Err(AuthStackError::validation("method must be GET or POST"));
    }
    let name = request.name.trim();
    if name.is_empty() || name.len() > 80 {
        return Err(AuthStackError::validation("name is invalid"));
    }
    let shape = request.shape.trim().to_ascii_lowercase();
    if shape != "one" && shape != "list" {
        return Err(AuthStackError::validation("shape must be one or list"));
    }
    let mut sources = load_data_sources(user_id).await?;
    let id = request
        .id
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| new_id("ds"));
    let source = crate::contracts::DataSource {
        id: id.clone(),
        name: name.to_owned(),
        kind: crate::contracts::DataSourceKind::Http,
        builtin_key: None,
        method,
        url: request.url.trim().to_owned(),
        headers: request.headers,
        body_template: request.body_template,
        json_path: request.json_path.trim().to_owned(),
        shape,
        cache_ttl_seconds: request.cache_ttl_seconds.min(3_600).max(0),
    };
    if let Some(existing) = sources.iter_mut().find(|s| s.id == id) {
        *existing = source.clone();
    } else {
        if sources.len() >= MAX_HTTP_SOURCES {
            return Err(AuthStackError::validation("too many sources"));
        }
        sources.push(source.clone());
    }
    save_data_sources(user_id, &sources).await?;
    // Legacy HTTP sources stay per-user; org board resources are edited via Resources modal.
    Ok(data_source_to_summary(&source))
}

pub async fn upsert_resource(
    org_id: &str,
    request: crate::contracts::ResourceUpsert,
    allow_private: bool,
) -> AuthStackResult<crate::contracts::ResourceSummary> {
    use crate::contracts::{ResourceConfig, ResourceKind};
    let name = request.name.trim();
    if name.is_empty() || name.len() > 80 {
        return Err(AuthStackError::validation("resource name is invalid"));
    }
    match &request.config {
        ResourceConfig::Rest { base_url, .. } => {
            validate_http_url(base_url, allow_private)?;
        }
        ResourceConfig::Postgres {
            host,
            port,
            database,
            user,
            password_secret_id,
            ..
        } => {
            if host.trim() == "@app" {
                // App database — no extra host validation.
            } else if host.trim().is_empty() || *port == 0 || database.trim().is_empty() || user.trim().is_empty()
            {
                return Err(AuthStackError::validation(
                    "postgres host, port, database, and user are required",
                ));
            } else if password_secret_id.trim().is_empty() {
                return Err(AuthStackError::validation(
                    "postgres password secret is required (or use host @app for the app database)",
                ));
            }
        }
        ResourceConfig::Grpc {
            host,
            port,
            gateway_base_url,
            ..
        } => {
            let has_gateway = gateway_base_url
                .as_ref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            if !has_gateway && (host.trim().is_empty() || *port == 0) {
                return Err(AuthStackError::validation(
                    "grpc requires host/port or a gateway_base_url",
                ));
            }
            if has_gateway {
                if let Some(url) = gateway_base_url {
                    validate_http_url(url, allow_private)?;
                }
            }
        }
        ResourceConfig::Builtin => {}
    }
    let id = request
        .id
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| new_id("res"));
    let resource = crate::contracts::DashboardResource {
        id: id.clone(),
        name: name.to_owned(),
        kind: request.kind,
        auth: request.auth,
        default_headers: request.default_headers,
        config: request.config,
    };
    // kind must match config tag
    let kind_ok = matches!(
        (&resource.kind, &resource.config),
        (ResourceKind::Builtin, ResourceConfig::Builtin)
            | (ResourceKind::Rest, ResourceConfig::Rest { .. })
            | (ResourceKind::Postgres, ResourceConfig::Postgres { .. })
            | (ResourceKind::Grpc, ResourceConfig::Grpc { .. })
    );
    if !kind_ok {
        return Err(AuthStackError::validation(
            "resource kind does not match config",
        ));
    }
    let mut resources = load_resources(org_id).await?;
    if let Some(slot) = resources.iter_mut().find(|r| r.id == id) {
        *slot = resource.clone();
    } else {
        if resources.len() >= MAX_RESOURCES {
            return Err(AuthStackError::validation("too many resources"));
        }
        resources.push(resource.clone());
    }
    save_resources(org_id, &resources).await?;
    Ok(resource_to_summary(&resource))
}

pub async fn upsert_query(
    org_id: &str,
    request: crate::contracts::QueryUpsert,
) -> AuthStackResult<crate::contracts::QuerySummary> {
    let name = request.name.trim();
    if name.is_empty() || name.len() > 80 {
        return Err(AuthStackError::validation("query name is invalid"));
    }
    let resources = load_resources(org_id).await?;
    if !resources.iter().any(|r| r.id == request.resource_id) {
        return Err(AuthStackError::not_found("resource not found"));
    }
    let id = request
        .id
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| new_id("qry"));
    let query = crate::contracts::DashboardQuery {
        id: id.clone(),
        name: name.to_owned(),
        resource_id: request.resource_id,
        transform: request.transform,
        config: request.config,
    };
    let mut queries = load_queries(org_id).await?;
    if let Some(slot) = queries.iter_mut().find(|q| q.id == id) {
        *slot = query.clone();
    } else {
        if queries.len() >= MAX_QUERIES {
            return Err(AuthStackError::validation("too many queries"));
        }
        queries.push(query.clone());
    }
    save_queries(org_id, &queries).await?;
    Ok(query_to_summary(&query, &resources))
}

pub async fn delete_resource(org_id: &str, resource_id: &str) -> AuthStackResult<()> {
    let mut resources = load_resources(org_id).await?;
    let before = resources.len();
    resources.retain(|r| r.id != resource_id);
    if resources.len() == before {
        return Err(AuthStackError::not_found("resource not found"));
    }
    // Drop queries bound to this resource.
    let mut queries = load_queries(org_id).await?;
    queries.retain(|q| q.resource_id != resource_id);
    save_resources(org_id, &resources).await?;
    save_queries(org_id, &queries).await
}

pub async fn delete_query(org_id: &str, query_id: &str) -> AuthStackResult<()> {
    let mut queries = load_queries(org_id).await?;
    let before = queries.len();
    queries.retain(|q| q.id != query_id);
    if queries.len() == before {
        return Err(AuthStackError::not_found("query not found"));
    }
    save_queries(org_id, &queries).await
}

pub async fn delete_data_source(user_id: &str, source_id: &str) -> AuthStackResult<()> {
    let mut sources = load_data_sources(user_id).await?;
    let before = sources.len();
    sources.retain(|s| s.id != source_id);
    if sources.len() == before {
        return Err(AuthStackError::not_found("source not found"));
    }
    save_data_sources(user_id, &sources).await?;
    Ok(())
}

/// Execute a saved dashboard query (REST fully supported; other kinds return clear errors).
/// Board data and vault secrets are both scoped to `org_id`.
pub async fn execute_dashboard_query(
    org_id: &str,
    query_id: &str,
    allow_private: bool,
) -> AuthStackResult<crate::contracts::QueryResult> {
    use crate::contracts::{QueryConfig, QueryResult, ResourceConfig, ResourceKind};
    let started = dashboard_now_ms();
    let vault_org_id = Some(org_id);
    let queries = load_queries(org_id).await?;
    let Some(query) = queries.iter().find(|q| q.id == query_id).cloned() else {
        return Ok(QueryResult::err(
            query_id,
            ResourceKind::Rest,
            "query not found",
        ));
    };
    let resources = load_resources(org_id).await?;
    let Some(resource) = resources
        .iter()
        .find(|r| r.id == query.resource_id)
        .cloned()
    else {
        return Ok(QueryResult::err(
            query_id,
            ResourceKind::Rest,
            "resource not found for query",
        ));
    };

    match (&resource.config, &query.config) {
        (ResourceConfig::Rest { base_url, .. }, QueryConfig::Rest { .. }) => {
            execute_rest_query(
                org_id,
                &resource,
                &query,
                base_url,
                allow_private,
                started,
                vault_org_id,
            )
            .await
        }
        (ResourceConfig::Postgres { .. }, QueryConfig::Postgres { sql }) => {
            execute_postgres_dashboard_query(
                org_id,
                &resource,
                &query,
                sql,
                started,
                vault_org_id,
            )
            .await
        }
        (ResourceConfig::Grpc { .. }, QueryConfig::Grpc { .. }) => {
            execute_grpc_dashboard_query(org_id, &resource, &query, allow_private, started).await
        }
        (ResourceConfig::Builtin, QueryConfig::Builtin { .. }) => Ok(QueryResult::err(
            query_id,
            ResourceKind::Builtin,
            "builtin queries are served via the dashboard snapshot, not the query runtime",
        )),
        _ => Ok(QueryResult::err(
            query_id,
            resource.kind,
            "resource kind does not match query kind",
        )),
    }
}

/// Reject non-read SQL (Retool-style SQL mode = read first).
pub fn validate_readonly_sql(sql: &str) -> AuthStackResult<()> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err(AuthStackError::validation("SQL is empty"));
    }
    if trimmed.len() > 16_384 {
        return Err(AuthStackError::validation("SQL is too long"));
    }
    // Strip simple line comments for keyword scan.
    let mut cleaned = String::new();
    for line in trimmed.lines() {
        let line = line.split("--").next().unwrap_or("").trim();
        if !line.is_empty() {
            cleaned.push_str(line);
            cleaned.push(' ');
        }
    }
    let upper = cleaned.to_ascii_uppercase();
    // Allow SELECT and WITH … SELECT only.
    let starts_ok = upper.starts_with("SELECT") || upper.starts_with("WITH");
    if !starts_ok {
        return Err(AuthStackError::validation(
            "only SELECT / WITH … SELECT queries are allowed",
        ));
    }
    const BANNED: &[&str] = &[
        " INSERT ",
        " UPDATE ",
        " DELETE ",
        " DROP ",
        " ALTER ",
        " CREATE ",
        " TRUNCATE ",
        " GRANT ",
        " REVOKE ",
        " COPY ",
        " CALL ",
        " DO ",
        " EXECUTE ",
        " VACUUM ",
        " COMMENT ",
        " INTO OUTFILE",
        " INTO DUMPFILE",
    ];
    let padded = format!(" {upper} ");
    for ban in BANNED {
        if padded.contains(ban) {
            return Err(AuthStackError::validation(format!(
                "SQL contains forbidden keyword{}",
                ban.trim()
            )));
        }
    }
    // Multi-statement: ban semicolons except trailing.
    let body = trimmed.trim_end_matches(';').trim();
    if body.contains(';') {
        return Err(AuthStackError::validation(
            "multiple SQL statements are not allowed",
        ));
    }
    Ok(())
}

fn build_postgres_connection_url(
    host: &str,
    port: u16,
    database: &str,
    user: &str,
    password: &str,
    ssl_mode: &crate::contracts::PostgresSslMode,
) -> AuthStackResult<String> {
    use crate::contracts::PostgresSslMode;
    if host.trim().is_empty() || database.trim().is_empty() || user.trim().is_empty() {
        return Err(AuthStackError::validation(
            "postgres host, database, and user are required",
        ));
    }
    // Special host @app uses the app's configured DATABASE_URL / POSTGRES_URL.
    if host.trim() == "@app" {
        return Err(AuthStackError::validation(
            "resolve @app via execute path, not build_postgres_connection_url",
        ));
    }
    let ssl = match ssl_mode {
        PostgresSslMode::Disable => "disable",
        PostgresSslMode::Prefer => "prefer",
        PostgresSslMode::Require => "require",
    };
    // libpq-style URI for Spin pg Connection::open
    let user_enc = form_urlencoded_encode(user);
    let pass_enc = form_urlencoded_encode(password);
    let db_enc = form_urlencoded_encode(database);
    Ok(format!(
        "postgres://{user_enc}:{pass_enc}@{host}:{port}/{db_enc}?sslmode={ssl}"
    ))
}

async fn resolve_postgres_url(
    resource: &crate::contracts::DashboardResource,
    vault_org_id: Option<&str>,
) -> AuthStackResult<String> {
    use crate::contracts::ResourceConfig;
    let ResourceConfig::Postgres {
        host,
        port,
        database,
        user,
        password_secret_id,
        ssl_mode,
    } = &resource.config
    else {
        return Err(AuthStackError::validation("not a postgres resource"));
    };
    if host.trim() == "@app" {
        return database_url("postgres").await;
    }
    let secrets = match vault_org_id.filter(|s| !s.trim().is_empty()) {
        Some(org) => load_secrets_resolved(org).await?,
        None => {
            return Err(AuthStackError::validation(
                "select a workspace to resolve vault secrets for Postgres",
            ));
        }
    };
    let password = secrets
        .iter()
        .find(|s| s.id == *password_secret_id)
        .map(|s| s.value.clone())
        .ok_or_else(|| AuthStackError::validation("postgres password secret missing"))?;
    build_postgres_connection_url(host, *port, database, user, &password, ssl_mode)
}

async fn execute_postgres_dashboard_query(
    user_id: &str,
    resource: &crate::contracts::DashboardResource,
    query: &crate::contracts::DashboardQuery,
    sql: &str,
    started: u64,
    vault_org_id: Option<&str>,
) -> AuthStackResult<crate::contracts::QueryResult> {
    use crate::contracts::{QueryMeta, QueryResult, ResourceKind};
    let _ = user_id;
    if let Err(e) = validate_readonly_sql(sql) {
        return Ok(QueryResult::err(
            &query.id,
            ResourceKind::Postgres,
            e.public_message(),
        ));
    }
    let url = match resolve_postgres_url(resource, vault_org_id).await {
        Ok(u) => u,
        Err(e) => {
            return Ok(QueryResult::err(
                &query.id,
                ResourceKind::Postgres,
                e.public_message(),
            ));
        }
    };

    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        // Cap rows via wrapping subquery when no LIMIT present (best-effort).
        let sql_exec = if sql.to_ascii_uppercase().contains(" LIMIT ") {
            sql.to_owned()
        } else {
            format!("SELECT * FROM ({sql}) AS _dash_q LIMIT 500")
        };
        match ddd_cqrs_es::adapters::execute_spin_pg(&url, &sql_exec, Vec::new()).await {
            Ok(rows) => {
                let truncated = rows.len() >= 500;
                let row_count = rows.len() as u32;
                let raw = Value::Array(rows);
                let raw_json = serde_json::to_string(&raw).unwrap_or_else(|_| "[]".to_owned());
                let transformed = apply_transform_pipeline(raw, &query.transform);
                let data_json =
                    serde_json::to_string(&transformed).unwrap_or_else(|_| "[]".to_owned());
                Ok(QueryResult {
                    query_id: query.id.clone(),
                    ok: true,
                    error: None,
                    raw_json,
                    data_json,
                    meta: QueryMeta {
                        resource_kind: ResourceKind::Postgres,
                        status: None,
                        grpc_status: None,
                        duration_ms: dashboard_now_ms().saturating_sub(started),
                        row_count: Some(row_count),
                        truncated,
                    },
                })
            }
            Err(e) => Ok(QueryResult::err(
                &query.id,
                ResourceKind::Postgres,
                format!("postgres query failed: {e}"),
            )),
        }
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (url, started, user_id);
        Ok(QueryResult::err(
            &query.id,
            ResourceKind::Postgres,
            "postgres queries require Spin PostgreSQL",
        ))
    }
}

async fn execute_grpc_dashboard_query(
    user_id: &str,
    resource: &crate::contracts::DashboardResource,
    query: &crate::contracts::DashboardQuery,
    allow_private: bool,
    started: u64,
) -> AuthStackResult<crate::contracts::QueryResult> {
    use crate::contracts::{
        HeaderBag, HeaderValue, HttpMethod, QueryConfig, QueryMeta, QueryResult, ResourceConfig,
        ResourceKind,
    };
    let ResourceConfig::Grpc {
        host,
        port,
        tls,
        gateway_base_url,
        use_proto_json,
        ..
    } = &resource.config
    else {
        return Ok(QueryResult::err(
            &query.id,
            ResourceKind::Grpc,
            "not a grpc resource",
        ));
    };
    let QueryConfig::Grpc {
        service,
        method,
        request_json,
        headers: query_headers,
    } = &query.config
    else {
        return Ok(QueryResult::err(
            &query.id,
            ResourceKind::Grpc,
            "not a grpc query",
        ));
    };

    // Supported path: JSON HTTP gateway (grpc-gateway / envoy transcoder).
    let gateway = gateway_base_url
        .as_ref()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    if let Some(base) = gateway {
        if !use_proto_json {
            return Ok(QueryResult::err(
                &query.id,
                ResourceKind::Grpc,
                "gateway mode requires use_proto_json=true",
            ));
        }
        // Build a temporary REST-shaped execution using resource auth + metadata headers.
        let path = format!(
            "/{}/{}",
            service.trim_start_matches('/'),
            method.trim_start_matches('/')
        );
        let mut rest_resource = resource.clone();
        rest_resource.config = ResourceConfig::Rest {
            base_url: base,
            timeout_ms: 30_000,
        };
        // Merge default_headers already on resource; add Content-Type.
        let mut headers = rest_resource.default_headers.clone();
        headers.extend(query_headers.iter().cloned());
        if !headers
            .iter()
            .any(|h| h.name.eq_ignore_ascii_case("content-type"))
        {
            headers.push(HeaderBag {
                name: "Content-Type".into(),
                value: HeaderValue::literal("application/json"),
            });
        }
        rest_resource.default_headers = headers;
        let rest_query = crate::contracts::DashboardQuery {
            id: query.id.clone(),
            name: query.name.clone(),
            resource_id: query.resource_id.clone(),
            transform: query.transform.clone(),
            config: QueryConfig::Rest {
                method: HttpMethod::Post,
                path,
                query_params: Vec::new(),
                headers: Vec::new(),
                body: Some(request_json.clone()),
            },
        };
        let base_url = match &rest_resource.config {
            ResourceConfig::Rest { base_url, .. } => base_url.clone(),
            _ => unreachable!(),
        };
        return execute_rest_query(
            user_id,
            &rest_resource,
            &rest_query,
            &base_url,
            allow_private,
            started,
            None,
        )
        .await;
    }

    // Native gRPC is intentionally gated — document Spin HTTP/2 + wasi-grpc for a future cut.
    let _ = (host, port, tls, started);
    Ok(QueryResult::err(
        &query.id,
        ResourceKind::Grpc,
        "native gRPC client is not enabled on this Spin runtime. Set gateway_base_url on the gRPC resource to a grpc-gateway / JSON transcoder URL, or enable AUTH_DASHBOARD_GRPC_ENABLED after upgrading to Spin HTTP/2 outbound.",
    ))
}

async fn execute_legacy_as_query_result(
    user_id: &str,
    source_id: &str,
    allow_private: bool,
    started: u64,
    vault_org_id: Option<&str>,
) -> AuthStackResult<crate::contracts::QueryResult> {
    use crate::contracts::{QueryMeta, QueryResult, ResourceKind};
    match execute_legacy_http_source(user_id, source_id, allow_private, vault_org_id).await {
        Ok(legacy) => Ok(QueryResult {
            query_id: source_id.to_owned(),
            ok: legacy.ok,
            error: legacy.error,
            raw_json: legacy.data_json.clone(),
            data_json: legacy.data_json,
            meta: QueryMeta {
                resource_kind: ResourceKind::Rest,
                status: None,
                grpc_status: None,
                duration_ms: dashboard_now_ms().saturating_sub(started),
                row_count: None,
                truncated: false,
            },
        }),
        Err(e) => Ok(QueryResult::err(
            source_id,
            ResourceKind::Rest,
            e.public_message(),
        )),
    }
}

async fn execute_rest_query(
    user_id: &str,
    resource: &crate::contracts::DashboardResource,
    query: &crate::contracts::DashboardQuery,
    base_url: &str,
    allow_private: bool,
    started: u64,
    vault_org_id: Option<&str>,
) -> AuthStackResult<crate::contracts::QueryResult> {
    use crate::contracts::{
        HttpMethod, QueryConfig, QueryMeta, QueryResult, ResourceAuth, ResourceKind,
    };
    let _ = user_id;
    let QueryConfig::Rest {
        method,
        path,
        query_params,
        headers: query_headers,
        body,
    } = &query.config
    else {
        return Ok(QueryResult::err(
            &query.id,
            ResourceKind::Rest,
            "not a REST query",
        ));
    };

    let secrets = match vault_org_id.filter(|s| !s.trim().is_empty()) {
        Some(org) => load_secrets_resolved(org).await?,
        None => Vec::new(),
    };
    let merged = merge_headers(&resource.default_headers, query_headers);
    let mut resolved_headers: Vec<(String, String)> = Vec::new();
    for h in &merged {
        let name = h.name.trim();
        if name.is_empty() {
            continue;
        }
        resolved_headers.push((name.to_owned(), resolve_header_value(&h.value, &secrets)?));
    }
    let mut resolved_params: Vec<(String, String)> = Vec::new();
    for p in query_params {
        let name = p.name.trim();
        if name.is_empty() {
            continue;
        }
        resolved_params.push((
            name.to_owned(),
            resolve_header_value(&p.value, &secrets)?,
        ));
    }

    // OAuth2 client credentials: fetch token then inject as Bearer if no Authorization yet.
    if let ResourceAuth::OAuth2ClientCredentials {
        token_url,
        client_id,
        client_secret_id,
        scopes,
        audience,
    } = &resource.auth
    {
        let token = fetch_oauth2_client_credentials(
            token_url,
            client_id,
            client_secret_id,
            scopes,
            audience.as_deref(),
            &secrets,
            allow_private,
        )
        .await?;
        if !resolved_headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case("authorization"))
        {
            resolved_headers.push(("Authorization".to_owned(), format!("Bearer {token}")));
        }
    } else {
        apply_resource_auth(
            &resource.auth,
            &secrets,
            &mut resolved_headers,
            &mut resolved_params,
        )?;
    }

    let mut url = join_base_path(base_url, path);
    if !resolved_params.is_empty() {
        let qs = resolved_params
            .iter()
            .map(|(k, v)| format!("{}={}", form_urlencoded_encode(k), form_urlencoded_encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        if url.contains('?') {
            url.push('&');
        } else {
            url.push('?');
        }
        url.push_str(&qs);
    }
    validate_http_url(&url, allow_private)?;

    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        use http_body_util::BodyExt;
        use spin_sdk::http::{send, FullBody};

        let method_http = match method {
            HttpMethod::Get => http::Method::GET,
            HttpMethod::Post => http::Method::POST,
            HttpMethod::Put => http::Method::PUT,
            HttpMethod::Patch => http::Method::PATCH,
            HttpMethod::Delete => http::Method::DELETE,
        };
        let body_bytes = match method {
            HttpMethod::Get | HttpMethod::Delete => bytes::Bytes::new(),
            _ => bytes::Bytes::from(body.clone().unwrap_or_default().into_bytes()),
        };
        let mut builder = http::Request::builder()
            .method(method_http)
            .uri(&url)
            .header(http::header::ACCEPT, "application/json");
        if matches!(
            method,
            HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch
        ) && !resolved_headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case("content-type"))
        {
            builder = builder.header(http::header::CONTENT_TYPE, "application/json");
        }
        for (name, value) in &resolved_headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let request = builder
            .body(FullBody::new(body_bytes))
            .map_err(|e| AuthStackError::transport(format!("build request failed: {e}")))?;
        let response = send(request)
            .await
            .map_err(|e| AuthStackError::transport(format!("HTTP request failed: {e}")))?;
        let status = response.status();
        let status_code = status.as_u16();
        let bytes = response
            .into_body()
            .collect()
            .await
            .map_err(|e| AuthStackError::transport(format!("response body failed: {e:?}")))?
            .to_bytes();
        let duration_ms = dashboard_now_ms().saturating_sub(started);
        let truncated = bytes.len() > MAX_HTTP_RESPONSE_BYTES;
        let slice = if truncated {
            &bytes[..MAX_HTTP_RESPONSE_BYTES]
        } else {
            &bytes
        };
        if truncated {
            return Ok(QueryResult {
                query_id: query.id.clone(),
                ok: false,
                error: Some("response too large".to_owned()),
                raw_json: "null".to_owned(),
                data_json: "null".to_owned(),
                meta: QueryMeta {
                    resource_kind: ResourceKind::Rest,
                    status: Some(status_code),
                    grpc_status: None,
                    duration_ms,
                    row_count: None,
                    truncated: true,
                },
            });
        }
        let parsed: Value = serde_json::from_slice(slice).unwrap_or_else(|_| {
            Value::String(String::from_utf8_lossy(slice).into_owned())
        });
        let raw_json = serde_json::to_string(&parsed).unwrap_or_else(|_| "null".to_owned());
        let transformed = apply_transform_pipeline(parsed, &query.transform);
        let row_count = transformed.as_array().map(|a| a.len() as u32);
        let data_json =
            serde_json::to_string(&transformed).unwrap_or_else(|_| "null".to_owned());
        let ok = status.is_success();
        Ok(QueryResult {
            query_id: query.id.clone(),
            ok,
            error: if ok {
                None
            } else {
                Some(format!("HTTP {status_code}"))
            },
            raw_json,
            data_json,
            meta: QueryMeta {
                resource_kind: ResourceKind::Rest,
                status: Some(status_code),
                grpc_status: None,
                duration_ms,
                row_count,
                truncated: false,
            },
        })
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (resolved_headers, url, body, method, started, user_id);
        Err(AuthStackError::configuration(
            "HTTP queries require Spin outbound HTTP",
        ))
    }
}

fn join_base_path(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    let path = path.trim();
    if path.is_empty() || path == "/" {
        return base.to_owned();
    }
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_owned();
    }
    if path.starts_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

fn form_urlencoded_encode(value: &str) -> String {
    #[cfg(feature = "ssr")]
    {
        form_urlencoded::byte_serialize(value.as_bytes()).collect()
    }
    #[cfg(not(feature = "ssr"))]
    {
        let mut out = String::new();
        for b in value.as_bytes() {
            match *b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(*b as char);
                }
                b' ' => out.push('+'),
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
        out
    }
}

fn dashboard_now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

async fn fetch_oauth2_client_credentials(
    token_url: &str,
    client_id: &str,
    client_secret_id: &str,
    scopes: &[String],
    audience: Option<&str>,
    secrets: &[StoredSecret],
    allow_private: bool,
) -> AuthStackResult<String> {
    validate_http_url(token_url, allow_private)?;
    let client_secret = secrets
        .iter()
        .find(|s| s.id == *client_secret_id)
        .map(|s| s.value.clone())
        .ok_or_else(|| AuthStackError::validation("OAuth client secret missing"))?;
    let mut form = vec![
        ("grant_type".to_owned(), "client_credentials".to_owned()),
        ("client_id".to_owned(), client_id.to_owned()),
        ("client_secret".to_owned(), client_secret),
    ];
    if !scopes.is_empty() {
        form.push(("scope".to_owned(), scopes.join(" ")));
    }
    if let Some(aud) = audience {
        if !aud.is_empty() {
            form.push(("audience".to_owned(), aud.to_owned()));
        }
    }
    let body = form
        .iter()
        .map(|(k, v)| format!("{}={}", form_urlencoded_encode(k), form_urlencoded_encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        use http_body_util::BodyExt;
        use spin_sdk::http::{send, FullBody};
        let request = http::Request::builder()
            .method(http::Method::POST)
            .uri(token_url)
            .header(
                http::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .header(http::header::ACCEPT, "application/json")
            .body(FullBody::new(bytes::Bytes::from(body.into_bytes())))
            .map_err(|e| AuthStackError::transport(format!("oauth token build failed: {e}")))?;
        let response = send(request)
            .await
            .map_err(|e| AuthStackError::transport(format!("oauth token request failed: {e}")))?;
        if !response.status().is_success() {
            return Err(AuthStackError::transport(format!(
                "oauth token HTTP {}",
                response.status()
            )));
        }
        let bytes = response
            .into_body()
            .collect()
            .await
            .map_err(|e| AuthStackError::transport(format!("oauth token body failed: {e:?}")))?
            .to_bytes();
        let parsed: Value = serde_json::from_slice(&bytes)
            .map_err(|e| AuthStackError::serialization(format!("oauth token json: {e}")))?;
        parsed
            .get("access_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned())
            .ok_or_else(|| AuthStackError::transport("oauth token response missing access_token"))
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = body;
        Err(AuthStackError::configuration(
            "OAuth token fetch requires Spin outbound HTTP",
        ))
    }
}

pub async fn execute_http_source(
    user_id: &str,
    source_id: &str,
    allow_private: bool,
) -> AuthStackResult<crate::contracts::HttpQueryResult> {
    // Legacy HTTP sources only (org board queries use execute_dashboard_query directly).
    execute_legacy_http_source(user_id, source_id, allow_private, None).await
}

async fn execute_legacy_http_source(
    user_id: &str,
    source_id: &str,
    allow_private: bool,
    vault_org_id: Option<&str>,
) -> AuthStackResult<crate::contracts::HttpQueryResult> {
    let sources = load_data_sources(user_id).await?;
    let source = sources
        .iter()
        .find(|s| s.id == source_id)
        .ok_or_else(|| AuthStackError::not_found("source not found"))?
        .clone();
    if !matches!(source.kind, crate::contracts::DataSourceKind::Http) {
        return Err(AuthStackError::validation("source is not HTTP"));
    }
    validate_http_url(&source.url, allow_private)?;
    let secrets = match vault_org_id.filter(|s| !s.trim().is_empty()) {
        Some(org) => load_secrets_resolved(org).await?,
        None => Vec::new(),
    };
    let mut headers: Vec<(String, String)> = Vec::new();
    for header in &source.headers {
        let name = header.name.trim();
        if name.is_empty() {
            continue;
        }
        let value = if let Some(secret_id) = header.secret_id.as_ref() {
            secrets
                .iter()
                .find(|s| s.id == *secret_id)
                .map(|s| s.value.clone())
                .ok_or_else(|| AuthStackError::validation(format!("missing secret for header {name}")))?
        } else {
            header.value.clone()
        };
        headers.push((name.to_owned(), value));
    }

    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        use http_body_util::BodyExt;
        use spin_sdk::http::{FullBody, send};

        let body = if source.method == "POST" {
            bytes::Bytes::from(
                source
                    .body_template
                    .clone()
                    .unwrap_or_default()
                    .into_bytes(),
            )
        } else {
            bytes::Bytes::new()
        };
        let mut builder = http::Request::builder()
            .method(if source.method == "POST" {
                http::Method::POST
            } else {
                http::Method::GET
            })
            .uri(&source.url)
            .header(http::header::ACCEPT, "application/json");
        for (name, value) in &headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let request = builder
            .body(FullBody::new(body))
            .map_err(|e| AuthStackError::transport(format!("build request failed: {e}")))?;
        let response = send(request)
            .await
            .map_err(|e| AuthStackError::transport(format!("HTTP request failed: {e}")))?;
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .map_err(|e| AuthStackError::transport(format!("response body failed: {e:?}")))?
            .to_bytes();
        if bytes.len() > MAX_HTTP_RESPONSE_BYTES {
            return Ok(crate::contracts::HttpQueryResult {
                source_id: source.id,
                ok: false,
                error: Some("response too large".to_owned()),
                data_json: "null".to_owned(),
                display_mode: crate::contracts::HttpDisplayMode::List,
            });
        }
        if !status.is_success() {
            return Ok(crate::contracts::HttpQueryResult {
                source_id: source.id,
                ok: false,
                error: Some(format!("HTTP {status}")),
                data_json: "null".to_owned(),
                display_mode: crate::contracts::HttpDisplayMode::List,
            });
        }
        let parsed: Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            Value::String(String::from_utf8_lossy(&bytes).into_owned())
        });
        let extracted = json_path_get(&parsed, &source.json_path).unwrap_or(parsed);
        let data_json = serde_json::to_string(&extracted).unwrap_or_else(|_| "null".to_owned());
        Ok(crate::contracts::HttpQueryResult {
            source_id: source.id,
            ok: true,
            error: None,
            data_json,
            display_mode: if source.shape == "one" {
                crate::contracts::HttpDisplayMode::Metric
            } else {
                crate::contracts::HttpDisplayMode::List
            },
        })
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (user_id, headers);
        Err(AuthStackError::configuration(
            "HTTP sources require Spin outbound HTTP",
        ))
    }
}

pub async fn load_dashboard_notifications(
    user_id: &str,
) -> AuthStackResult<Vec<crate::contracts::DashboardNotification>> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let key = dashboard_notifs_key(user_id);
        let Some(bytes) = store
            .get(&key)
            .await
            .map_err(|error| AuthStackError::store(format!("notifications read failed: {error}")))?
        else {
            let notifs = default_notifications();
            save_dashboard_notifications(user_id, &notifs).await?;
            return Ok(notifs);
        };
        match serde_json::from_slice::<Vec<crate::contracts::DashboardNotification>>(&bytes) {
            Ok(list) => Ok(list),
            _ => {
                let notifs = default_notifications();
                save_dashboard_notifications(user_id, &notifs).await?;
                Ok(notifs)
            }
        }
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = user_id;
        Ok(default_notifications())
    }
}

pub async fn save_dashboard_notifications(
    user_id: &str,
    notifications: &[crate::contracts::DashboardNotification],
) -> AuthStackResult<()> {
    #[cfg(all(feature = "postgres", runtime_spin))]
    {
        let store = profile_kv().await?;
        let bytes = serde_json::to_vec(notifications)
            .map_err(|error| AuthStackError::serialization(error.to_string()))?;
        store
            .set(dashboard_notifs_key(user_id), bytes)
            .await
            .map_err(|error| AuthStackError::store(format!("notifications write failed: {error}")))
    }
    #[cfg(not(all(feature = "postgres", runtime_spin)))]
    {
        let _ = (user_id, notifications);
        Err(AuthStackError::configuration(
            "dashboard storage requires Spin key-value",
        ))
    }
}

pub async fn dismiss_dashboard_notification(
    user_id: &str,
    notification_id: &str,
) -> AuthStackResult<Vec<crate::contracts::DashboardNotification>> {
    let mut list = load_dashboard_notifications(user_id).await?;
    list.retain(|item| item.id != notification_id);
    save_dashboard_notifications(user_id, &list).await?;
    Ok(list)
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
        return Err(AuthStackError::validation(
            "projection batch limit is invalid",
        ));
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

#[cfg(feature = "mail-capture")]
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

/// Validates security-sensitive runtime variables without touching storage.
///
/// Schema installation and checksum verification are separate deployment and
/// health gates so the trusted-ingress hot path never performs migration I/O.
pub async fn validate_runtime_security_config() -> AuthStackResult<()> {
    if RUNTIME_SECURITY_VALIDATED.load(Ordering::Acquire) {
        return Ok(());
    }
    let lock = RUNTIME_SECURITY_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().await;
    if RUNTIME_SECURITY_VALIDATED.load(Ordering::Acquire) {
        return Ok(());
    }
    validate_runtime_security_config_uncached().await?;
    RUNTIME_SECURITY_VALIDATED.store(true, Ordering::Release);
    Ok(())
}

async fn validate_runtime_security_config_uncached() -> AuthStackResult<()> {
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
    if !config_bool("AUTH_REQUIRE_TRUSTED_INGRESS", false).await {
        return Err(AuthStackError::configuration(
            "production requires AUTH_REQUIRE_TRUSTED_INGRESS=true",
        ));
    }
    let ingress_key = store_config_value("AUTH_TRUSTED_INGRESS_KEY_BASE64")
        .await
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AuthStackError::configuration("production requires AUTH_TRUSTED_INGRESS_KEY_BASE64")
        })?;
    let ingress_key = STANDARD
        .decode(ingress_key.trim())
        .ok()
        .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
        .ok_or_else(|| {
            AuthStackError::configuration("AUTH_TRUSTED_INGRESS_KEY_BASE64 must decode to 32 bytes")
        })?;
    if store_config_value("AUTH_TRUSTED_INGRESS_AUDIENCE")
        .await
        .is_none_or(|value| value.trim().is_empty() || value.len() > 256)
    {
        return Err(AuthStackError::configuration(
            "production requires a bounded AUTH_TRUSTED_INGRESS_AUDIENCE",
        ));
    }
    let ingress_max_age = store_config_value("AUTH_TRUSTED_INGRESS_MAX_AGE_SECONDS")
        .await
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(5);
    if !(1..=30).contains(&ingress_max_age) {
        return Err(AuthStackError::configuration(
            "AUTH_TRUSTED_INGRESS_MAX_AGE_SECONDS must be between 1 and 30",
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
    let vault_key: [u8; 32] = store_config_value(MFA_VAULT_KEY)
        .await
        .and_then(|value| STANDARD.decode(value.trim()).ok())
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| {
            AuthStackError::configuration(
                "production AUTH_VAULT_KEY_BASE64 must decode to 32 bytes",
            )
        })?;
    if store_config_value("AUTH_VAULT_KEY_VERSION")
        .await
        .is_none_or(|value| {
            value.trim().is_empty()
                || value == "development-v1"
                || value.len() > 128
                || value.chars().any(char::is_control)
        })
    {
        return Err(AuthStackError::configuration(
            "production requires a bounded non-development AUTH_VAULT_KEY_VERSION",
        ));
    }
    let recovery_pepper = store_config_value(MFA_RECOVERY_PEPPER)
        .await
        .and_then(|value| STANDARD.decode(value.trim()).ok())
        .filter(|bytes| (16..=1_024).contains(&bytes.len()))
        .ok_or_else(|| {
            AuthStackError::configuration(
                "production AUTH_RECOVERY_CODE_PEPPER_BASE64 must decode to 16-1024 bytes",
            )
        })?;
    let outbox_key: [u8; 32] = store_config_value("AUTH_OUTBOX_KEY_BASE64")
        .await
        .and_then(|value| STANDARD.decode(value.trim()).ok())
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| {
            AuthStackError::configuration(
                "production AUTH_OUTBOX_KEY_BASE64 must decode to 32 bytes",
            )
        })?;
    let development_outbox_key: [u8; 32] =
        Sha256::digest(b"fullstack-development-outbox-key").into();
    if vault_key == development_outbox_key || outbox_key == development_outbox_key {
        return Err(AuthStackError::configuration(
            "production forbids development encryption keys",
        ));
    }
    if store_config_value("AUTH_OUTBOX_KEY_VERSION")
        .await
        .is_none_or(|value| value.trim().is_empty() || value == "development-v1")
    {
        return Err(AuthStackError::configuration(
            "production requires a non-development AUTH_OUTBOX_KEY_VERSION",
        ));
    }
    if ingress_key == vault_key
        || ingress_key == outbox_key
        || vault_key == outbox_key
        || recovery_pepper.as_slice() == ingress_key.as_slice()
        || recovery_pepper.as_slice() == vault_key.as_slice()
        || recovery_pepper.as_slice() == outbox_key.as_slice()
    {
        return Err(AuthStackError::configuration(
            "production requires distinct ingress, vault, outbox, and recovery secrets",
        ));
    }
    match mail_transport().await.as_str() {
        "http" => {}
        "capture" => {
            return Err(AuthStackError::configuration(
                "production forbids capture mail; run the native outbox worker with HTTP mail",
            ));
        }
        _ => {
            return Err(AuthStackError::configuration(
                "production AUTH_MAIL_TRANSPORT must be http",
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
        require_production_secret("AUTH_SPICEDB_CHECK_TOKEN").await?;
        Ok(())
    }
    #[cfg(not(all(feature = "spicedb", runtime_spin)))]
    {
        Err(AuthStackError::configuration(
            "AUTH_SPICEDB_ENABLED requires the spicedb feature on Spin",
        ))
    }
}

#[cfg(all(feature = "spicedb", runtime_spin))]
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

#[cfg(feature = "mail-capture")]
fn secure_storage_id(kind: &str) -> AuthStackResult<String> {
    Ok(format!(
        "{kind}_{}",
        URL_SAFE_NO_PAD.encode(random_bytes(32)?)
    ))
}

#[cfg(feature = "mail-capture")]
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
    use crate::contracts::{
        BoardNode, DashboardLayout, DashboardWidgetKind, HttpDisplayMode, LegacyDashboardWidget,
        WidgetBind,
    };
    use serde_json::json;

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

    #[test]
    fn split_base_and_path_keeps_origin() {
        let (base, path) = super::split_base_and_path("https://api.example.com:8443/v1/items?x=1");
        assert_eq!(base, "https://api.example.com:8443");
        assert_eq!(path, "/v1/items?x=1");
    }

    #[test]
    fn join_base_path_handles_absolute_and_relative() {
        assert_eq!(
            super::join_base_path("https://api.example.com", "/v1/x"),
            "https://api.example.com/v1/x"
        );
        assert_eq!(
            super::join_base_path("https://api.example.com/", "v1/x"),
            "https://api.example.com/v1/x"
        );
        assert_eq!(
            super::join_base_path("https://api.example.com", "https://other.test/a"),
            "https://other.test/a"
        );
    }

    #[test]
    fn validate_readonly_sql_allows_select_blocks_writes() {
        assert!(super::validate_readonly_sql("SELECT 1").is_ok());
        assert!(super::validate_readonly_sql("WITH t AS (SELECT 1) SELECT * FROM t").is_ok());
        assert!(super::validate_readonly_sql("DELETE FROM users").is_err());
        assert!(super::validate_readonly_sql("SELECT 1; DROP TABLE x").is_err());
        assert!(super::validate_readonly_sql("INSERT INTO t VALUES (1)").is_err());
    }

    #[test]
    fn merge_headers_query_wins() {
        use crate::contracts::{HeaderBag, HeaderValue};
        let resource = vec![HeaderBag {
            name: "X-Api-Key".into(),
            value: HeaderValue::literal("from-resource"),
        }];
        let query = vec![HeaderBag {
            name: "x-api-key".into(),
            value: HeaderValue::literal("from-query"),
        }];
        let merged = super::merge_headers(&resource, &query);
        assert_eq!(merged.len(), 1);
        assert!(matches!(
            &merged[0].value,
            HeaderValue::Literal { value } if value == "from-query"
        ));
    }

    #[test]
    fn transform_pipeline_path_and_limit() {
        let raw = serde_json::json!({"data":{"items":[{"n":1},{"n":2},{"n":3}]}});
        let steps = vec![
            crate::contracts::TransformStep::JsonPath {
                path: "data.items".into(),
            },
            crate::contracts::TransformStep::Limit { n: 2 },
        ];
        let out = super::apply_transform_pipeline(raw, &steps);
        assert_eq!(out.as_array().map(|a| a.len()), Some(2));
    }

    #[test]
    fn layout_v1_migrates_to_nodes_with_12_col_spans() {
        let mut layout = DashboardLayout {
            version: 1,
            nodes: Vec::new(),
            widgets: vec![
                LegacyDashboardWidget {
                    id: "a".into(),
                    kind: DashboardWidgetKind::MetricSession,
                    col_span: 1,
                    note_text: None,
                },
                LegacyDashboardWidget {
                    id: "b".into(),
                    kind: DashboardWidgetKind::Activity,
                    col_span: 2,
                    note_text: None,
                },
            ],
        };
        layout.migrate_if_needed();
        assert_eq!(layout.version, 2);
        assert!(layout.widgets.is_empty() || layout.nodes.len() == 2);
        assert_eq!(layout.nodes.len(), 2);
        match &layout.nodes[0] {
            BoardNode::Widget { col_span, .. } => assert_eq!(*col_span, 3),
            _ => panic!("expected widget"),
        }
        match &layout.nodes[1] {
            BoardNode::Widget { col_span, .. } => assert_eq!(*col_span, 6),
            _ => panic!("expected widget"),
        }
    }

    #[test]
    fn json_path_get_reads_nested_and_index() {
        let value = json!({"data": {"items": [{"name": "alpha"}, {"name": "beta"}]}});
        let extracted = json_path_get(&value, "data.items.0.name").unwrap();
        assert_eq!(extracted, json!("alpha"));
        assert_eq!(json_path_get(&value, "").unwrap(), value);
    }

    #[test]
    fn validate_http_url_blocks_private_by_default() {
        assert!(validate_http_url("https://example.com/v1", false).is_ok());
        assert!(validate_http_url("http://127.0.0.1:9/", false).is_err());
        assert!(validate_http_url("http://10.0.0.5/x", false).is_err());
        assert!(validate_http_url("http://169.254.169.254/latest", false).is_err());
        assert!(validate_http_url("http://127.0.0.1:9/", true).is_ok());
        assert!(validate_http_url("ftp://example.com", false).is_err());
    }

    #[test]
    fn vault_secret_key_validation() {
        assert!(validate_vault_secret_key("API_TOKEN").is_ok());
        assert!(validate_vault_secret_key("STRIPE_SECRET_KEY").is_ok());
        assert!(validate_vault_secret_key("A1").is_ok());
        assert!(validate_vault_secret_key("a_token").is_err());
        assert!(validate_vault_secret_key("1TOKEN").is_err());
        assert!(validate_vault_secret_key("HAS-DASH").is_err());
        assert!(validate_vault_secret_key("").is_err());
        assert!(validate_vault_secret_key("X").is_err());
    }

    #[test]
    fn vault_encrypt_decrypt_roundtrip() {
        let key = [7_u8; 32];
        let org = "org-abc";
        let (nonce_b64, ciphertext_b64) =
            encrypt_vault_value(org, "super-secret-value", &key).expect("encrypt");
        let stored = StoredSecret {
            id: "sec1".into(),
            key: "DEMO".into(),
            label: "Demo".into(),
            name: "DEMO".into(),
            description: String::new(),
            scope: "user".into(),
            value: String::new(),
            ciphertext_b64,
            nonce_b64,
            mac_b64: String::new(),
            key_version: "test-v1".into(),
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        let plain = decrypt_vault_value(org, &stored, &key).expect("decrypt");
        assert_eq!(plain, "super-secret-value");
        // Wrong org AAD must fail.
        assert!(decrypt_vault_value("other-org", &stored, &key).is_err());
    }

    #[test]
    fn org_slug_validation_and_suggest() {
        assert!(validate_org_slug("acme").is_ok());
        assert!(validate_org_slug("acme-inc").is_ok());
        assert!(validate_org_slug("a1").is_ok());
        assert!(validate_org_slug("Admin").is_err());
        assert!(validate_org_slug("admin").is_err());
        assert!(validate_org_slug("-acme").is_err());
        assert_eq!(suggest_org_slug("Acme Inc!"), "acme-inc");
    }

    #[test]
    fn default_layout_has_containers_and_twelve_col_metrics() {
        let layout = default_dashboard_layout();
        assert_eq!(layout.version, 2);
        assert!(!layout.nodes.is_empty());
        assert!(layout.total_nodes() >= 10);
        let has_row = layout.nodes.iter().any(|n| {
            matches!(
                n,
                BoardNode::Container {
                    kind: crate::contracts::BoardContainerKind::Row,
                    ..
                }
            )
        });
        assert!(has_row);
    }

    #[test]
    fn board_node_count_includes_nested() {
        let node = BoardNode::Container {
            id: "c".into(),
            kind: crate::contracts::BoardContainerKind::Row,
            col_span: 12,
            children: vec![BoardNode::Widget {
                id: "w".into(),
                kind: DashboardWidgetKind::Notes,
                col_span: 6,
                note_text: Some(String::new()),
                source_id: None,
                bind: WidgetBind::default(),
                http_mode: HttpDisplayMode::List,
            }],
        };
        assert_eq!(node.count_nodes(), 2);
    }
}
