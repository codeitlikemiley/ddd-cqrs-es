use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{
    Algorithm as Argon2Algorithm, Argon2, Params as Argon2Params, Version as Argon2Version,
};
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use futures::lock::Mutex;
use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wasi_auth::authentication::RandomSource;
use wasi_auth::authentication::WorkflowError;
use wasi_auth::authentication::adapter_ids::{SessionId, TenantId, UserId};
use wasi_auth::authentication::jwt::{
    AccessTokenClaims, Algorithm, DecodingKey, EncodingKey, JwksDocument, JwksKey,
    access_token_key_id, decode_access_token, encode_access_token, jwk_from_encoding_key,
};
use wasi_auth::authentication::mfa::{
    RecoveryCode, TotpConfig, TotpSecret, hash_recovery_code, provisioning_uri, verify_totp,
};
use wasi_auth::authentication::passkeys::{
    Attachment as PasskeyAttachment, AuthenticationResponse as PasskeyAuthenticationResponse,
    AuthenticationState as PasskeyAuthenticationState, CredentialId as WebauthnCredentialId,
    PasskeyCredential as WebauthnPasskeyCredential,
    RegistrationResponse as PasskeyRegistrationResponse,
    RegistrationState as PasskeyRegistrationState, Webauthn,
};
use wasi_auth::authentication::storage_contract::{AUTH_EVENT_STREAMS, AUTH_STORAGE_VERSION};
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
use wasi_auth::storage::{EmbeddedMigration, StorageDialect, migrations};

use crate::contracts::{
    AccountSessionListResponse, AccountSessionSummary, AdminUserListResponse, AdminUserSummary,
    AuditEventListResponse, AuditEventSummary, AuthProviderSummary, CapturedMailResponse,
    EmailPasswordLoginRequest, EmailPasswordRegisterRequest, HealthStatusResponse,
    InvitationListResponse, InvitationSummary, LoginCompletionResponse, LogoutResponse,
    MembershipListResponse, MembershipSummary, MfaEnrollConfirmResponse, MfaEnrollStartResponse,
    MfaStatusResponse, OrganizationListResponse, OrganizationSummary, PasskeyStartResponse,
    PasswordResetCompleteRequest, PasswordResetStartRequest, PasswordResetStartResponse,
    PolicyVersionListResponse, PolicyVersionSummary, RoleListResponse, RoleSummary, SessionView,
    SigningKeyListResponse, SigningKeyRotateResponse, SigningKeySummary, StorageEventTypeCount,
    StorageProjectionCheckpoint, StorageProjectionRunResponse, StorageStatusResponse,
    TokenRefreshResponse, TokenVerifyRequest, TokenVerifyResponse,
};
use crate::error::{AuthStackError, AuthStackResult};

const DEFAULT_TENANT_ID: &str = "tenant:default";
const DEFAULT_SESSION_TTL_SECONDS: u64 = 60 * 60;
const DEFAULT_REFRESH_TOKEN_TTL_SECONDS: u64 = 30 * 24 * 60 * 60;
const DEFAULT_ACCESS_TOKEN_TTL_SECONDS: u64 = 15 * 60;
const DEFAULT_JWT_ISSUER: &str = "http://127.0.0.1:3008";
const DEFAULT_JWT_AUDIENCE: &str = "fullstack-app";
const DEFAULT_JWT_KID: &str = "fullstack-app-dev-hs256";
const DEFAULT_JWT_SECRET: &str = "dev-fullstack-app-secret-change-me";
const DEFAULT_JWT_ALGORITHM: &str = "HS256";
const AUTH_PRODUCTION_MODE: &str = "AUTH_PRODUCTION_MODE";
const SIGNING_KEY_STATUS_ACTIVE: &str = "active";
const SIGNING_KEY_STATUS_NEXT: &str = "next";
const SIGNING_KEY_STATUS_RETIRED: &str = "retired";
const SIGNING_KEY_STATUS_REVOKED: &str = "revoked";
const PASSWORD_RESET_TTL_MS: u64 = 15 * 60 * 1000;
const PASSWORD_RESET_TTL_SECONDS: u64 = PASSWORD_RESET_TTL_MS / 1000;
const EMAIL_VERIFICATION_TTL_MS: u64 = 24 * 60 * 60 * 1000;
const OAUTH_STATE_TTL_MS: u64 = 10 * 60 * 1000;
const DEFAULT_PASSKEY_CHALLENGE_TTL_SECONDS: u64 = 5 * 60;
const DEFAULT_PASSKEY_RP_ID: &str = "localhost";
const DEFAULT_PASSKEY_RP_NAME: &str = "wasi-auth";
const DEFAULT_PASSKEY_ORIGIN: &str = "http://localhost:3008";
const PASSWORD_HASH_ALGORITHM: &str = "pbkdf2-sha256";
const DEFAULT_PASSWORD_KDF: &str = "argon2id";
const DEFAULT_PASSWORD_ARGON2_MEMORY_KIB: u32 = 19_456;
const DEFAULT_PASSWORD_ARGON2_ITERATIONS: u32 = 2;
const DUMMY_PASSWORD_HASH: &str =
    "argon2id$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const DEFAULT_PASSWORD_ARGON2_PARALLELISM: u32 = 1;
const DEFAULT_PASSWORD_PBKDF2_ITERATIONS: u32 = 600_000;
const MIN_PRODUCTION_PASSWORD_PBKDF2_ITERATIONS: u32 = 600_000;
const PASSWORD_SALT_BYTES: usize = 16;
const PASSWORD_HASH_BYTES: usize = 32;
const AUTH_STORAGE_PROJECTION_CHECKPOINT: &str = "auth.storage.read_models";
const DEFAULT_STORAGE_PROJECTION_BATCH_LIMIT: usize = 128;
const PROJECT_SESSION_UPSERT_SQL: &str = "INSERT INTO auth_sessions \
     (session_id, tenant_id, user_id, primary_email, assurance, permissions_json, created_at_ms, updated_at_ms, expires_at_ms, revoked_at_ms) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8, NULL) \
     ON CONFLICT(session_id) DO UPDATE SET \
     tenant_id = excluded.tenant_id, \
     user_id = excluded.user_id, \
     primary_email = excluded.primary_email, \
     assurance = excluded.assurance, \
     permissions_json = excluded.permissions_json, \
     expires_at_ms = excluded.expires_at_ms, \
     revoked_at_ms = NULL, \
     updated_at_ms = excluded.updated_at_ms";
const MFA_RECOVERY_CODE_COUNT: usize = 10;
const MFA_VAULT_KEY: &str = "AUTH_VAULT_KEY_BASE64";
const MFA_RECOVERY_PEPPER: &str = "AUTH_RECOVERY_CODE_PEPPER_BASE64";
const MFA_NONCE_BYTES: usize = 12;
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
    Sqlite,
    Postgres,
}

#[derive(Debug, thiserror::Error)]
#[error("host cryptographic randomness is unavailable")]
struct HostRandomError;

struct HostRandom;

impl RandomSource for HostRandom {
    type Error = HostRandomError;

    fn fill_bytes(&self, destination: &mut [u8]) -> Result<(), Self::Error> {
        let bytes = random_bytes(destination.len()).map_err(|_| HostRandomError)?;
        if bytes.len() != destination.len() {
            return Err(HostRandomError);
        }
        destination.copy_from_slice(&bytes);
        Ok(())
    }
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

    let backend = storage_backend().await?;
    validate_runtime_security_config().await?;
    let migration = schema_migration(backend)?;
    let mut statements = Vec::new();
    if let Some(upgrade) = legacy_schema_checksum_upgrade(backend).await? {
        statements.push(upgrade);
    }
    statements.extend(
        migration
            .statements()
            .map_err(|error| AuthStackError::store(error.to_string()))?
            .iter()
            .map(|statement| AtomicSqlStatement::execute(*statement, Vec::new())),
    );
    for (provider_id, display_name) in [
        ("apple", "Apple"),
        ("facebook", "Facebook"),
        ("google", "Google"),
    ] {
        let enabled = provider_default_enabled(provider_id).await;
        statements.push(auth_provider_seed_statement(
            provider_id,
            enabled,
            display_name,
            &format!("/api/auth/oauth/{provider_id}/start"),
            now_ms(),
        ));
    }
    statements.push(AtomicSqlStatement::execute(
        "INSERT INTO auth_schema_migrations (version, checksum, applied_at_ms) \
         VALUES (?1, ?2, ?3) \
         ON CONFLICT(version) DO UPDATE SET \
             checksum = excluded.checksum, applied_at_ms = excluded.applied_at_ms",
        vec![
            json!(migration.version()),
            json!(migration.checksum_hex()),
            json!(now_ms()),
        ],
    ));
    execute_sql_atomic(statements).await?;

    SCHEMA_INITIALIZED.store(true, Ordering::Release);
    tracing::info!(
        auth_storage_version = AUTH_STORAGE_VERSION,
        auth_streams = AUTH_EVENT_STREAMS.len(),
        "auth storage schema initialized"
    );

    Ok(())
}

#[cfg(feature = "mail-capture")]
pub async fn verify_atomic_rollback_probe() -> AuthStackResult<Value> {
    initialize_schema_async().await?;
    let probe_id = secure_storage_id("atomic_probe")?;
    let email = format!("{probe_id}@rollback.invalid");
    let now = now_ms();
    let statements = vec![
        AtomicSqlStatement::execute(
            "INSERT INTO auth_users \
             (user_id, tenant_id, primary_email, disabled, email_verified, created_at_ms, updated_at_ms) \
             VALUES (?1, ?2, ?3, 0, 0, ?4, ?4)",
            vec![
                json!(&probe_id),
                json!(DEFAULT_TENANT_ID),
                json!(&email),
                json!(now),
            ],
        ),
        AtomicSqlStatement::execute(
            "INSERT INTO auth_users_by_email (tenant_id, normalized_email, user_id) \
             VALUES (?1, ?2, ?3)",
            vec![json!(DEFAULT_TENANT_ID), json!(&email), json!(&probe_id)],
        ),
        AtomicSqlStatement::execute(
            "INSERT INTO auth_password_credentials \
             (tenant_id, user_id, password_hash, created_at_ms, updated_at_ms, revoked_at_ms, last_authenticated_at_ms) \
             VALUES (?1, ?2, 'probe-hash', ?3, ?3, NULL, NULL)",
            vec![json!(DEFAULT_TENANT_ID), json!(&probe_id), json!(now)],
        ),
        AtomicSqlStatement::execute(
            "INSERT INTO events \
             (event_id, aggregate_id, aggregate_type, revision, event_type, event_version, payload, metadata, recorded_at_ms) \
             VALUES (?1, ?1, 'atomic_probe', 1, 'atomic_probe_started', 1, '{}', '{}', ?2)",
            vec![json!(&probe_id), json!(now)],
        ),
        AtomicSqlStatement::execute(
            "INSERT INTO auth_idempotency \
             (idempotency_key, operation, request_hash, response_json, status, created_at_ms, updated_at_ms) \
             VALUES (?1, 'atomic.probe', '00', NULL, 'pending', ?2, ?2)",
            vec![json!(&probe_id), json!(now)],
        ),
        AtomicSqlStatement::execute(
            "INSERT INTO auth_mail_outbox \
             (message_id, message_kind, recipient_hash, payload_encrypted, correlation_id, available_at_ms, created_at_ms, delivered_at_ms, delivery_id, attempt_count, last_error_code, lease_id, leased_until_ms) \
             VALUES (?1, 'atomic-probe', ?1, 'probe-ciphertext', ?1, ?2, ?2, NULL, NULL, 0, NULL, NULL, NULL)",
            vec![json!(&probe_id), json!(now)],
        ),
        AtomicSqlStatement::guard(
            "SELECT user_id FROM auth_users WHERE user_id = ?1 AND 1 = 0",
            vec![json!(&probe_id)],
        ),
    ];

    match execute_sql_atomic(statements).await {
        Err(error)
            if error
                .to_string()
                .contains("transaction guard returned too few rows") => {}
        Err(error) => return Err(error),
        Ok(_) => {
            execute_sql_atomic(vec![
                AtomicSqlStatement::execute(
                    "DELETE FROM auth_mail_outbox WHERE message_id = ?1",
                    vec![json!(&probe_id)],
                ),
                AtomicSqlStatement::execute(
                    "DELETE FROM auth_idempotency WHERE idempotency_key = ?1",
                    vec![json!(&probe_id)],
                ),
                AtomicSqlStatement::execute(
                    "DELETE FROM events WHERE event_id = ?1",
                    vec![json!(&probe_id)],
                ),
                AtomicSqlStatement::execute(
                    "DELETE FROM auth_password_credentials WHERE user_id = ?1",
                    vec![json!(&probe_id)],
                ),
                AtomicSqlStatement::execute(
                    "DELETE FROM auth_users_by_email WHERE user_id = ?1",
                    vec![json!(&probe_id)],
                ),
                AtomicSqlStatement::execute(
                    "DELETE FROM auth_users WHERE user_id = ?1",
                    vec![json!(&probe_id)],
                ),
            ])
            .await?;
            return Err(AuthStackError::store(
                "atomic rollback probe unexpectedly committed",
            ));
        }
    }

    let rows = execute_sql(
        "SELECT \
         (SELECT COUNT(*) FROM auth_users WHERE user_id = ?1) AS projection_rows, \
         (SELECT COUNT(*) FROM auth_password_credentials WHERE user_id = ?1) AS secret_rows, \
         (SELECT COUNT(*) FROM events WHERE event_id = ?1) AS event_rows, \
         (SELECT COUNT(*) FROM auth_idempotency WHERE idempotency_key = ?1) AS idempotency_rows, \
         (SELECT COUNT(*) FROM auth_mail_outbox WHERE message_id = ?1) AS outbox_rows",
        vec![json!(&probe_id)],
    )
    .await?;
    let row = rows
        .first()
        .ok_or_else(|| AuthStackError::store("atomic rollback verification returned no row"))?;
    let categories = [
        ("projection", row_i64(row, "projection_rows")),
        ("secret", row_i64(row, "secret_rows")),
        ("event", row_i64(row, "event_rows")),
        ("idempotency", row_i64(row, "idempotency_rows")),
        ("outbox", row_i64(row, "outbox_rows")),
    ];
    if categories.iter().any(|(_, count)| *count != Some(0)) {
        return Err(AuthStackError::store(format!(
            "atomic rollback left partial rows: {categories:?}"
        )));
    }

    Ok(json!({
        "rolled_back": true,
        "verified_categories": ["event", "projection", "secret", "idempotency", "outbox"],
    }))
}

pub async fn list_auth_providers() -> AuthStackResult<Vec<AuthProviderSummary>> {
    initialize_schema_async().await?;
    let rows = execute_sql(
        "SELECT provider_id, display_name, login_url, enabled \
         FROM auth_provider_configs \
         WHERE tenant_id = ?1 \
         ORDER BY provider_id ASC",
        vec![json!(DEFAULT_TENANT_ID)],
    )
    .await?;

    rows.into_iter().map(provider_from_row).collect()
}

pub async fn find_auth_provider(provider_id: &str) -> AuthStackResult<Option<AuthProviderSummary>> {
    initialize_schema_async().await?;
    let rows = execute_sql(
        "SELECT provider_id, display_name, login_url, enabled \
         FROM auth_provider_configs \
         WHERE tenant_id = ?1 AND provider_id = ?2 \
         LIMIT 1",
        vec![json!(DEFAULT_TENANT_ID), json!(provider_id)],
    )
    .await?;

    rows.into_iter().next().map(provider_from_row).transpose()
}

pub async fn save_auth_provider_config(
    provider_id: &str,
    enabled: bool,
) -> AuthStackResult<AuthProviderSummary> {
    initialize_schema_async().await?;
    let now = now_ms();
    let display_name = provider_display_name(provider_id);
    let login_url = format!("/api/auth/oauth/{provider_id}/start");

    upsert_auth_provider_config_unchecked(provider_id, enabled, &display_name, &login_url, now)
        .await?;
    append_storage_event(
        "auth_provider_config",
        provider_id,
        "auth_provider_config_saved",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "provider_id": provider_id,
            "display_name": &display_name,
            "login_url": &login_url,
            "enabled": enabled,
        }),
    )
    .await?;

    Ok(AuthProviderSummary {
        provider_id: provider_id.to_string(),
        display_name,
        login_url,
        enabled,
    })
}

async fn upsert_auth_provider_config_unchecked(
    provider_id: &str,
    enabled: bool,
    display_name: &str,
    login_url: &str,
    now: u64,
) -> AuthStackResult<()> {
    execute_sql(
        "INSERT INTO auth_provider_configs \
         (tenant_id, provider_id, display_name, login_url, enabled, scopes_json, redirect_uris_json, claim_mapping_json, created_at_ms, updated_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, '[]', '[]', '{}', ?6, ?6) \
         ON CONFLICT(tenant_id, provider_id) DO UPDATE SET \
         display_name = excluded.display_name, \
         login_url = excluded.login_url, \
         enabled = excluded.enabled, \
         updated_at_ms = excluded.updated_at_ms",
        vec![
            json!(DEFAULT_TENANT_ID),
            json!(provider_id),
            json!(display_name),
            json!(login_url),
            json!(if enabled { 1 } else { 0 }),
            json!(now),
        ],
    )
    .await?;
    Ok(())
}

pub async fn save_redirect_allowlist(redirects_json: &str) -> AuthStackResult<()> {
    initialize_schema_async().await?;
    let _: Vec<String> = serde_json::from_str(redirects_json)
        .map_err(|error| AuthStackError::validation(format!("invalid redirects_json: {error}")))?;
    let now = now_ms();

    execute_sql(
        "INSERT INTO auth_redirect_allowlists \
         (tenant_id, redirects_json, created_at_ms, updated_at_ms) \
         VALUES (?1, ?2, ?3, ?3) \
         ON CONFLICT(tenant_id) DO UPDATE SET \
         redirects_json = excluded.redirects_json, \
         updated_at_ms = excluded.updated_at_ms",
        vec![json!(DEFAULT_TENANT_ID), json!(redirects_json), json!(now)],
    )
    .await?;
    append_storage_event(
        "auth_provider_config",
        DEFAULT_TENANT_ID,
        "auth_redirect_allowlist_saved",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "redirects_json": redirects_json,
        }),
    )
    .await?;

    Ok(())
}

pub async fn register_email_password(
    request: &EmailPasswordRegisterRequest,
    redirect_url: &str,
) -> AuthStackResult<LoginCompletionResponse> {
    initialize_schema_async().await?;
    let email = normalize_email(&request.email);
    if password_credential_for_email(&email).await?.is_some() {
        return Err(AuthStackError::conflict(
            "An account already exists for this email address",
        ));
    }

    let now = now_ms();
    let user_id = user_id_from_email(&email);
    let password_hash = hash_password(&request.password).await?;
    let grant_id = secure_storage_id("grant")?;
    let verification_token = secure_storage_id("email_verification")?;
    let token_hash = one_time_token_hash(&verification_token);
    let expires_at_ms = now.saturating_add(EMAIL_VERIFICATION_TTL_MS);
    let payload_json = json!({"email": &email, "user_id": &user_id}).to_string();
    let mut statements = user_email_identity_statements(&email, &user_id, now);
    statements.push(AtomicSqlStatement::execute(
        "INSERT INTO auth_password_credentials \
         (tenant_id, user_id, password_hash, created_at_ms, updated_at_ms, revoked_at_ms, last_authenticated_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?4, NULL, NULL)",
        vec![
            json!(DEFAULT_TENANT_ID),
            json!(&user_id),
            json!(&password_hash),
            json!(now),
        ],
    ));
    statements.push(storage_event_statement(
        "auth_user",
        &user_id,
        "auth_password_user_registered",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "user_id": &user_id,
            "email": &email,
            "credential_id": format!("password:{user_id}"),
            "secret_version": 1,
        }),
    )?);
    statements.push(AtomicSqlStatement::execute(
        "INSERT INTO auth_token_grants \
         (grant_id, token_hash, tenant_id, grant_type, subject_hint, redirect_url, payload_json, expires_at_ms, consumed_at_ms, created_at_ms) \
         VALUES (?1, ?2, ?3, 'email_verification', ?4, ?5, ?6, ?7, NULL, ?8)",
        vec![
            json!(&grant_id),
            json!(&token_hash),
            json!(DEFAULT_TENANT_ID),
            json!(&email),
            json!(redirect_url),
            json!(&payload_json),
            json!(expires_at_ms),
            json!(now),
        ],
    ));
    statements.push(
        mail_outbox_statement(
            "email-verification",
            &email,
            "Verify your email",
            &format!("/verify-email?token={verification_token}"),
            now,
        )
        .await?,
    );
    statements.push(storage_event_statement(
        "auth_user",
        &user_id,
        "auth_email_verification_started",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "user_id": &user_id,
            "grant_id": &grant_id,
            "expires_at_ms": expires_at_ms,
        }),
    )?);
    execute_sql_atomic(statements).await?;
    Ok(LoginCompletionResponse {
        authenticated: false,
        redirect_url: "/verify-email".to_string(),
        session_id: None,
        access_token: None,
        refresh_token: None,
        expires_in_seconds: 0,
    })
}

pub async fn login_email_password(
    request: &EmailPasswordLoginRequest,
    redirect_url: &str,
) -> AuthStackResult<LoginCompletionResponse> {
    initialize_schema_async().await?;
    let email = normalize_email(&request.email);
    let Some(record) = password_credential_for_email(&email).await? else {
        consume_dummy_password_verification(&request.password).await?;
        return Err(AuthStackError::InvalidCredentials);
    };
    if record.disabled || !record.email_verified || record.revoked_at_ms.is_some() {
        consume_dummy_password_verification(&request.password).await?;
        return Err(AuthStackError::InvalidCredentials);
    }
    let now = now_ms();
    let mut statements = Vec::new();
    match verify_password(&request.password, &record.password_hash).await? {
        PasswordVerification::Invalid => {
            return Err(AuthStackError::InvalidCredentials);
        }
        PasswordVerification::ValidCurrent => {}
        PasswordVerification::ValidNeedsRehash => {
            let password_hash = hash_password(&request.password).await?;
            statements.push(AtomicSqlStatement::execute(
                "UPDATE auth_password_credentials \
                 SET password_hash = ?1, updated_at_ms = ?2 \
                 WHERE tenant_id = ?3 AND user_id = ?4",
                vec![
                    json!(&password_hash),
                    json!(now),
                    json!(DEFAULT_TENANT_ID),
                    json!(&record.user_id),
                ],
            ));
            statements.push(storage_event_statement(
                "auth_user",
                &record.user_id,
                "auth_password_hash_rehashed",
                json!({
                    "tenant_id": DEFAULT_TENANT_ID,
                    "user_id": &record.user_id,
                }),
            )?);
        }
    }

    statements.push(AtomicSqlStatement::execute(
        "UPDATE auth_password_credentials \
         SET last_authenticated_at_ms = ?1, updated_at_ms = ?1 \
         WHERE tenant_id = ?2 AND user_id = ?3",
        vec![json!(now), json!(DEFAULT_TENANT_ID), json!(&record.user_id)],
    ));
    statements.push(storage_event_statement(
        "auth_user",
        &record.user_id,
        "auth_password_login_succeeded",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "user_id": &record.user_id,
        }),
    )?);
    execute_sql_atomic(statements).await?;

    issue_session_for_email(&record.primary_email, redirect_url).await
}

pub async fn resend_email_verification(email: &str, redirect_url: &str) -> AuthStackResult<()> {
    initialize_schema_async().await?;
    let email = normalize_email(email);
    if let Some(record) = password_credential_for_email(&email).await?
        && !record.disabled
        && !record.email_verified
        && record.revoked_at_ms.is_none()
    {
        create_email_verification_grant(&email, &record.user_id, redirect_url).await?;
    }
    Ok(())
}

pub async fn complete_email_verification(
    token: &str,
    redirect_url: &str,
) -> AuthStackResult<LoginCompletionResponse> {
    initialize_schema_async().await?;
    let token_hash = one_time_token_hash(token.trim());
    let rows = execute_sql(
        "SELECT grant_id, payload_json, expires_at_ms, consumed_at_ms \
         FROM auth_token_grants \
         WHERE tenant_id = ?1 AND token_hash = ?2 AND grant_type = 'email_verification' \
         LIMIT 1",
        vec![json!(DEFAULT_TENANT_ID), json!(&token_hash)],
    )
    .await?;
    let row = rows
        .into_iter()
        .next()
        .ok_or_else(|| AuthStackError::validation("verification link is invalid or expired"))?;
    if row_i64(&row, "consumed_at_ms").is_some()
        || row_i64(&row, "expires_at_ms").unwrap_or_default() < now_ms() as i64
    {
        return Err(AuthStackError::validation(
            "verification link is invalid or expired",
        ));
    }
    let payload: Value = serde_json::from_str(&required_string(&row, "payload_json")?)
        .map_err(|_| AuthStackError::store("verification grant payload is invalid"))?;
    let user_id = required_payload_string(&payload, "user_id")?;
    let email = normalize_email(&required_payload_string(&payload, "email")?);
    let grant_id = required_string(&row, "grant_id")?;
    let now = now_ms();
    execute_sql_atomic(vec![
        AtomicSqlStatement::guard(
            "UPDATE auth_token_grants SET consumed_at_ms = ?1 \
             WHERE tenant_id = ?2 AND grant_id = ?3 AND consumed_at_ms IS NULL \
               AND expires_at_ms >= ?1 RETURNING grant_id",
            vec![json!(now), json!(DEFAULT_TENANT_ID), json!(&grant_id)],
        ),
        AtomicSqlStatement::guard(
            "UPDATE auth_users SET email_verified = 1, updated_at_ms = ?1 \
             WHERE tenant_id = ?2 AND user_id = ?3 AND disabled = 0 \
             RETURNING user_id",
            vec![json!(now), json!(DEFAULT_TENANT_ID), json!(&user_id)],
        ),
        storage_event_statement(
            "auth_user",
            &user_id,
            "auth_email_verified",
            json!({
                "tenant_id": DEFAULT_TENANT_ID,
                "user_id": &user_id,
                "grant_id": &grant_id,
            }),
        )?,
    ])
    .await?;
    issue_session_for_email(&email, redirect_url).await
}

async fn create_email_verification_grant(
    email: &str,
    user_id: &str,
    redirect_url: &str,
) -> AuthStackResult<()> {
    let now = now_ms();
    let grant_id = secure_storage_id("grant")?;
    let verification_token = secure_storage_id("email_verification")?;
    let token_hash = one_time_token_hash(&verification_token);
    let expires_at_ms = now.saturating_add(EMAIL_VERIFICATION_TTL_MS);
    let payload_json = json!({"email": email, "user_id": user_id}).to_string();
    let statements = vec![
        AtomicSqlStatement::execute(
            "INSERT INTO auth_token_grants \
         (grant_id, token_hash, tenant_id, grant_type, subject_hint, redirect_url, payload_json, expires_at_ms, consumed_at_ms, created_at_ms) \
         VALUES (?1, ?2, ?3, 'email_verification', ?4, ?5, ?6, ?7, NULL, ?8)",
            vec![
                json!(&grant_id),
                json!(&token_hash),
                json!(DEFAULT_TENANT_ID),
                json!(email),
                json!(redirect_url),
                json!(&payload_json),
                json!(expires_at_ms),
                json!(now),
            ],
        ),
        mail_outbox_statement(
            "email-verification",
            email,
            "Verify your email",
            &format!("/verify-email?token={verification_token}"),
            now,
        )
        .await?,
        storage_event_statement(
            "auth_user",
            user_id,
            "auth_email_verification_started",
            json!({
                "tenant_id": DEFAULT_TENANT_ID,
                "user_id": user_id,
                "grant_id": &grant_id,
                "expires_at_ms": expires_at_ms,
            }),
        )?,
    ];
    execute_sql_atomic(statements).await?;
    Ok(())
}

pub async fn start_password_reset(
    request: &PasswordResetStartRequest,
    redirect_url: &str,
) -> AuthStackResult<PasswordResetStartResponse> {
    initialize_schema_async().await?;
    let email = normalize_email(&request.email);
    let Some(record) = password_credential_for_email(&email).await? else {
        return Ok(PasswordResetStartResponse {
            accepted: true,
            expires_in_seconds: PASSWORD_RESET_TTL_SECONDS,
        });
    };

    if record.disabled || record.revoked_at_ms.is_some() {
        return Ok(PasswordResetStartResponse {
            accepted: true,
            expires_in_seconds: PASSWORD_RESET_TTL_SECONDS,
        });
    }

    let now = now_ms();
    let grant_id = secure_storage_id("grant")?;
    let reset_token = secure_storage_id("password_reset")?;
    let token_hash = one_time_token_hash(&reset_token);
    let expires_at_ms = now.saturating_add(PASSWORD_RESET_TTL_MS);
    let payload_json = json!({
        "email": record.primary_email,
        "user_id": record.user_id,
    })
    .to_string();

    let statements = vec![
        AtomicSqlStatement::execute(
            "INSERT INTO auth_token_grants \
         (grant_id, token_hash, tenant_id, grant_type, subject_hint, redirect_url, payload_json, expires_at_ms, consumed_at_ms, created_at_ms) \
         VALUES (?1, ?2, ?3, 'password_reset', ?4, ?5, ?6, ?7, NULL, ?8)",
            vec![
                json!(&grant_id),
                json!(&token_hash),
                json!(DEFAULT_TENANT_ID),
                json!(email),
                json!(redirect_url),
                json!(&payload_json),
                json!(expires_at_ms),
                json!(now),
            ],
        ),
        mail_outbox_statement(
            "password-reset",
            &record.primary_email,
            "Reset your password",
            &format!("/reset-password?token={reset_token}"),
            now,
        )
        .await?,
        storage_event_statement(
            "auth_session",
            &grant_id,
            "auth_password_reset_started",
            json!({
                "tenant_id": DEFAULT_TENANT_ID,
                "grant_id": &grant_id,
                "user_id": &record.user_id,
                "expires_at_ms": expires_at_ms,
            }),
        )?,
    ];
    execute_sql_atomic(statements).await?;

    Ok(PasswordResetStartResponse {
        accepted: true,
        expires_in_seconds: PASSWORD_RESET_TTL_SECONDS,
    })
}

pub async fn complete_password_reset(
    request: &PasswordResetCompleteRequest,
    redirect_url: &str,
) -> AuthStackResult<LoginCompletionResponse> {
    initialize_schema_async().await?;
    let token_hash = one_time_token_hash(request.token.trim());
    let rows = execute_sql(
        "SELECT grant_id, payload_json, expires_at_ms, consumed_at_ms \
         FROM auth_token_grants \
         WHERE tenant_id = ?1 AND token_hash = ?2 AND grant_type = 'password_reset' \
         LIMIT 1",
        vec![json!(DEFAULT_TENANT_ID), json!(&token_hash)],
    )
    .await?;
    let Some(row) = rows.into_iter().next() else {
        return Err(AuthStackError::validation(
            "password reset link is invalid or expired",
        ));
    };

    if row_i64(&row, "consumed_at_ms").is_some() {
        return Err(AuthStackError::validation(
            "password reset link has already been used",
        ));
    }
    if row_i64(&row, "expires_at_ms").unwrap_or_default() < now_ms() as i64 {
        return Err(AuthStackError::SessionExpired);
    }

    let payload_json = required_string(&row, "payload_json")?;
    let grant_id = required_string(&row, "grant_id")?;
    let payload: Value = serde_json::from_str(&payload_json).map_err(|error| {
        AuthStackError::store(format!("reset grant payload is invalid: {error}"))
    })?;
    let email = payload
        .get("email")
        .and_then(Value::as_str)
        .map(normalize_email)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AuthStackError::store("reset grant is missing an email"))?;
    let Some(record) = password_credential_for_email(&email).await? else {
        return Err(AuthStackError::validation(
            "password reset link is invalid or expired",
        ));
    };
    if record.disabled || record.revoked_at_ms.is_some() {
        return Err(AuthStackError::InvalidCredentials);
    }

    let now = now_ms();
    let password_hash = hash_password(&request.password).await?;
    execute_sql_atomic(vec![
        AtomicSqlStatement::guard(
            "UPDATE auth_token_grants \
             SET consumed_at_ms = ?1 \
             WHERE tenant_id = ?2 AND grant_id = ?3 AND consumed_at_ms IS NULL \
               AND expires_at_ms >= ?1 \
             RETURNING grant_id",
            vec![json!(now), json!(DEFAULT_TENANT_ID), json!(&grant_id)],
        ),
        AtomicSqlStatement::execute(
            "UPDATE auth_password_credentials \
             SET password_hash = ?1, updated_at_ms = ?2, revoked_at_ms = NULL \
             WHERE tenant_id = ?3 AND user_id = ?4",
            vec![
                json!(&password_hash),
                json!(now),
                json!(DEFAULT_TENANT_ID),
                json!(&record.user_id),
            ],
        ),
        AtomicSqlStatement::execute(
            "UPDATE auth_sessions SET revoked_at_ms = ?1, updated_at_ms = ?1 \
             WHERE user_id = ?2 AND revoked_at_ms IS NULL",
            vec![json!(now), json!(&record.user_id)],
        ),
        AtomicSqlStatement::execute(
            "UPDATE auth_refresh_token_hashes SET revoked_at_ms = ?1 \
             WHERE session_id IN (SELECT session_id FROM auth_sessions WHERE user_id = ?2) \
               AND revoked_at_ms IS NULL",
            vec![json!(now), json!(&record.user_id)],
        ),
        storage_event_statement(
            "auth_user",
            &record.user_id,
            "auth_password_reset_completed",
            json!({
                "tenant_id": DEFAULT_TENANT_ID,
                "user_id": &record.user_id,
                "grant_id": &grant_id,
                "credential_id": format!("password:{}", record.user_id),
                "secret_version": now,
            }),
        )?,
    ])
    .await?;

    issue_session_for_email(&email, redirect_url).await
}

pub async fn create_oauth_grant(
    provider_id: &str,
    redirect_url: &str,
) -> AuthStackResult<CreatedOauthGrant> {
    initialize_schema_async().await?;
    let now = now_ms();
    let grant_id = secure_storage_id("grant")?;
    let state = secure_storage_id("oauth_state")?;
    let nonce = URL_SAFE_NO_PAD.encode(random_bytes(32)?);
    let pkce_verifier = URL_SAFE_NO_PAD.encode(random_bytes(32)?);
    let pkce_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(pkce_verifier.as_bytes()));
    let token_hash = one_time_token_hash(&state);
    let expires_at_ms = now.saturating_add(OAUTH_STATE_TTL_MS);
    let payload = StoredOauthGrantPayload {
        provider_id: provider_id.to_owned(),
        grant_id: grant_id.clone(),
        nonce: nonce.clone(),
        pkce_verifier,
    };
    let payload_json = serde_json::to_vec(&payload)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;
    let payload_encrypted = encrypt_oauth_grant_payload(&grant_id, &payload_json).await?;

    execute_sql_atomic(vec![
        AtomicSqlStatement::execute(
        "INSERT INTO auth_token_grants \
         (grant_id, token_hash, tenant_id, grant_type, subject_hint, redirect_url, payload_json, expires_at_ms, consumed_at_ms, created_at_ms) \
         VALUES (?1, ?2, ?3, 'oauth_state', ?4, ?5, ?6, ?7, NULL, ?8)",
        vec![
            json!(&grant_id),
            json!(&token_hash),
            json!(DEFAULT_TENANT_ID),
            json!(provider_id),
            json!(redirect_url),
            json!(&payload_encrypted),
            json!(expires_at_ms),
            json!(now),
        ],
        ),
        storage_event_statement(
            "auth_provider_config",
            provider_id,
            "auth_oauth_state_created",
            json!({
                "tenant_id": DEFAULT_TENANT_ID,
                "provider_id": provider_id,
                "grant_id": &grant_id,
                "expires_at_ms": expires_at_ms,
            }),
        )?,
    ])
    .await?;

    Ok(CreatedOauthGrant {
        state,
        nonce,
        pkce_challenge,
    })
}

pub async fn consume_oauth_grant(
    provider_id: &str,
    state: &str,
) -> AuthStackResult<ConsumedOauthGrant> {
    initialize_schema_async().await?;
    let token_hash = one_time_token_hash(state);
    let rows = execute_sql(
        "SELECT grant_id, subject_hint, redirect_url, payload_json, expires_at_ms, consumed_at_ms \
         FROM auth_token_grants \
         WHERE tenant_id = ?1 AND token_hash = ?2 AND grant_type = 'oauth_state' \
         LIMIT 1",
        vec![json!(DEFAULT_TENANT_ID), json!(&token_hash)],
    )
    .await?;
    let Some(row) = rows.into_iter().next() else {
        return Err(AuthStackError::validation(
            "OAuth callback state is invalid or expired",
        ));
    };

    let stored_provider_id = required_string(&row, "subject_hint")?;
    if stored_provider_id != provider_id {
        return Err(AuthStackError::validation(
            "OAuth callback provider does not match state",
        ));
    }
    if row_i64(&row, "consumed_at_ms").is_some() {
        return Err(AuthStackError::conflict(
            "OAuth callback state has already been used",
        ));
    }
    let expires_at_ms = row_i64(&row, "expires_at_ms").unwrap_or_default();
    if expires_at_ms < now_ms() as i64 {
        return Err(AuthStackError::SessionExpired);
    }

    let grant_id = required_string(&row, "grant_id")?;
    let payload =
        decrypt_oauth_grant_payload(&grant_id, &required_string(&row, "payload_json")?).await?;
    if payload.provider_id != provider_id || payload.grant_id != grant_id {
        return Err(AuthStackError::InvalidToken);
    }
    let now = now_ms();
    execute_sql_atomic(vec![
        AtomicSqlStatement::guard(
            "UPDATE auth_token_grants \
             SET consumed_at_ms = ?1 \
             WHERE tenant_id = ?2 AND grant_id = ?3 AND consumed_at_ms IS NULL \
               AND expires_at_ms >= ?1 RETURNING grant_id",
            vec![json!(now), json!(DEFAULT_TENANT_ID), json!(&grant_id)],
        ),
        storage_event_statement(
            "auth_provider_config",
            provider_id,
            "auth_oauth_state_consumed",
            json!({
                "tenant_id": DEFAULT_TENANT_ID,
                "provider_id": provider_id,
                "grant_id": &grant_id,
            }),
        )?,
    ])
    .await?;

    Ok(ConsumedOauthGrant {
        state: grant_id,
        provider_id: stored_provider_id,
        redirect_url: required_string(&row, "redirect_url")?,
        nonce: payload.nonce,
        pkce_verifier: payload.pkce_verifier,
    })
}

pub async fn create_passkey_challenge(
    flow: &str,
    email: Option<String>,
    redirect_url: &str,
) -> AuthStackResult<PasskeyStartResponse> {
    match flow {
        "registration" => create_passkey_registration_challenge(email, redirect_url).await,
        "login" => create_passkey_login_challenge(email, redirect_url).await,
        _ => Err(AuthStackError::validation(format!(
            "unsupported passkey flow '{flow}'"
        ))),
    }
}

async fn create_passkey_registration_challenge(
    email: Option<String>,
    redirect_url: &str,
) -> AuthStackResult<PasskeyStartResponse> {
    initialize_schema_async().await?;
    let email = required_passkey_email(email, "registration")?;
    let user_id = user_id_from_email(&email);
    let now = now_ms();
    let webauthn = passkey_webauthn().await?;
    let existing_credentials = load_passkey_credentials_for_user(&user_id).await?;
    let existing_ids = existing_credentials
        .iter()
        .map(|credential| credential.id.clone())
        .collect::<Vec<_>>();
    let (public_key_options, state) =
        webauthn.start_registration(user_id.as_bytes(), &email, &email, &existing_ids);
    let public_key_options_json = serde_json::to_string(&public_key_options)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;
    let payload = StoredPasskeyChallengePayload::Registration {
        state,
        email: email.clone(),
        user_id: user_id.clone(),
    };
    let payload_json = serde_json::to_string(&payload)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;
    let challenge_id = secure_storage_id("passkey_registration")?;
    let grant_id = secure_storage_id("grant")?;
    let token_hash = one_time_token_hash(&challenge_id);
    let expires_at_ms = now.saturating_add(passkey_challenge_ttl_ms().await);

    let mut statements = user_email_identity_statements(&email, &user_id, now);
    statements.push(AtomicSqlStatement::execute(
        "INSERT INTO auth_token_grants \
         (grant_id, token_hash, tenant_id, grant_type, subject_hint, redirect_url, payload_json, expires_at_ms, consumed_at_ms, created_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9)",
        vec![
            json!(&grant_id),
            json!(&token_hash),
            json!(DEFAULT_TENANT_ID),
            json!("passkey_registration"),
            json!(email),
            json!(redirect_url),
            json!(&payload_json),
            json!(expires_at_ms),
            json!(now),
        ],
    ));
    statements.push(storage_event_statement(
        "auth_passkey_credential",
        &user_id,
        "auth_passkey_registration_started",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "user_id": &user_id,
            "grant_id": &grant_id,
            "credential_id": format!("passkey:{user_id}"),
            "expires_at_ms": expires_at_ms,
        }),
    )?);
    execute_sql_atomic(statements).await?;

    Ok(PasskeyStartResponse {
        challenge_id,
        public_key_options_json,
        redirect_url: redirect_url.to_string(),
    })
}

async fn create_passkey_login_challenge(
    email: Option<String>,
    redirect_url: &str,
) -> AuthStackResult<PasskeyStartResponse> {
    initialize_schema_async().await?;
    let email = required_passkey_email(email, "login")?;
    let user_id = user_id_from_email(&email);
    let credentials = load_passkey_credentials_for_user(&user_id).await?;
    if credentials.is_empty() {
        return Err(AuthStackError::InvalidCredentials);
    }

    let now = now_ms();
    let webauthn = passkey_webauthn().await?;
    let (public_key_options, state) =
        webauthn.start_authentication_with_creds_for_user(user_id.as_bytes(), &credentials);
    let public_key_options_json = serde_json::to_string(&public_key_options)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;
    let payload = StoredPasskeyChallengePayload::Login {
        state,
        email: email.clone(),
        user_id: user_id.clone(),
    };
    let payload_json = serde_json::to_string(&payload)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;
    let challenge_id = secure_storage_id("passkey_login")?;
    let grant_id = secure_storage_id("grant")?;
    let token_hash = one_time_token_hash(&challenge_id);
    let expires_at_ms = now.saturating_add(passkey_challenge_ttl_ms().await);

    execute_sql_atomic(vec![
        AtomicSqlStatement::execute(
            "INSERT INTO auth_token_grants \
             (grant_id, token_hash, tenant_id, grant_type, subject_hint, redirect_url, payload_json, expires_at_ms, consumed_at_ms, created_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9)",
            vec![
                json!(&grant_id),
                json!(&token_hash),
                json!(DEFAULT_TENANT_ID),
                json!("passkey_login"),
                json!(email),
                json!(redirect_url),
                json!(&payload_json),
                json!(expires_at_ms),
                json!(now),
            ],
        ),
        storage_event_statement(
            "auth_passkey_credential",
            &user_id,
            "auth_passkey_login_started",
            json!({
                "tenant_id": DEFAULT_TENANT_ID,
                "user_id": &user_id,
                "grant_id": &grant_id,
                "credential_id": format!("passkey:{user_id}"),
                "expires_at_ms": expires_at_ms,
            }),
        )?,
    ])
    .await?;

    Ok(PasskeyStartResponse {
        challenge_id,
        public_key_options_json,
        redirect_url: redirect_url.to_string(),
    })
}

pub async fn consume_passkey_challenge(
    challenge_id: &str,
) -> AuthStackResult<ConsumedPasskeyChallenge> {
    initialize_schema_async().await?;
    let token_hash = one_time_token_hash(challenge_id);
    let rows = execute_sql(
        "SELECT grant_id, grant_type, redirect_url, payload_json, expires_at_ms, consumed_at_ms \
         FROM auth_token_grants \
         WHERE tenant_id = ?1 AND token_hash = ?2 AND grant_type LIKE 'passkey_%' \
         LIMIT 1",
        vec![json!(DEFAULT_TENANT_ID), json!(&token_hash)],
    )
    .await?;
    let Some(row) = rows.into_iter().next() else {
        return Err(AuthStackError::not_found("passkey challenge was not found"));
    };

    if row_i64(&row, "consumed_at_ms").is_some() {
        return Err(AuthStackError::conflict(
            "passkey challenge was already used",
        ));
    }
    let expires_at_ms = row_i64(&row, "expires_at_ms").unwrap_or_default();
    if expires_at_ms < now_ms() as i64 {
        return Err(AuthStackError::SessionExpired);
    }

    let grant_id = required_string(&row, "grant_id")?;
    let now = now_ms();
    execute_sql_atomic(vec![
        AtomicSqlStatement::guard(
            "UPDATE auth_token_grants \
             SET consumed_at_ms = ?1 \
             WHERE tenant_id = ?2 AND grant_id = ?3 AND consumed_at_ms IS NULL \
               AND expires_at_ms >= ?1 RETURNING grant_id",
            vec![json!(now), json!(DEFAULT_TENANT_ID), json!(&grant_id)],
        ),
        storage_event_statement(
            "auth_passkey_credential",
            &grant_id,
            "auth_passkey_challenge_consumed",
            json!({
                "tenant_id": DEFAULT_TENANT_ID,
                "grant_id": &grant_id,
            }),
        )?,
    ])
    .await?;

    Ok(ConsumedPasskeyChallenge {
        grant_type: required_string(&row, "grant_type")?,
        redirect_url: required_string(&row, "redirect_url")?,
        payload: serde_json::from_str(&required_string(&row, "payload_json")?).map_err(
            |error| AuthStackError::store(format!("stored passkey state is invalid: {error}")),
        )?,
    })
}

pub async fn verify_passkey_registration(
    challenge_id: &str,
    credential_json: &str,
    redirect_url: Option<String>,
    expected_user_id: &str,
) -> AuthStackResult<LoginCompletionResponse> {
    let response: PasskeyRegistrationResponse =
        serde_json::from_str(credential_json).map_err(|error| {
            AuthStackError::validation(format!("invalid passkey registration response: {error}"))
        })?;
    let challenge = consume_passkey_challenge(challenge_id).await?;
    let redirect_url = safe_redirect_or_stored(redirect_url, &challenge.redirect_url);
    if challenge.grant_type != "passkey_registration" {
        return Err(AuthStackError::validation(
            "passkey challenge was not created for registration",
        ));
    }
    let StoredPasskeyChallengePayload::Registration {
        state,
        email,
        user_id,
    } = challenge.payload
    else {
        return Err(AuthStackError::store(
            "stored passkey registration state has the wrong shape",
        ));
    };
    if user_id != expected_user_id {
        return Err(AuthStackError::Forbidden);
    }

    let webauthn = passkey_webauthn().await?;
    let credential = webauthn
        .finish_registration(&state, &response)
        .map_err(map_passkey_verification_error)?;
    persist_passkey_credential(&user_id, &credential).await?;
    issue_session_for_email_with_assurance(&email, &redirect_url, "aal2").await
}

pub async fn verify_passkey_login(
    challenge_id: &str,
    credential_json: &str,
    redirect_url: Option<String>,
) -> AuthStackResult<LoginCompletionResponse> {
    let response: PasskeyAuthenticationResponse =
        serde_json::from_str(credential_json).map_err(|error| {
            AuthStackError::validation(format!("invalid passkey login response: {error}"))
        })?;
    let challenge = consume_passkey_challenge(challenge_id).await?;
    let redirect_url = safe_redirect_or_stored(redirect_url, &challenge.redirect_url);
    if challenge.grant_type != "passkey_login" {
        return Err(AuthStackError::validation(
            "passkey challenge was not created for login",
        ));
    }
    let StoredPasskeyChallengePayload::Login {
        state,
        email,
        user_id,
    } = challenge.payload
    else {
        return Err(AuthStackError::store(
            "stored passkey login state has the wrong shape",
        ));
    };

    let asserted_id =
        WebauthnCredentialId::from_b64url(&response.id).map_err(map_passkey_verification_error)?;
    let mut credential = load_passkey_credential_for_user(&user_id, &asserted_id).await?;
    let webauthn = passkey_webauthn().await?;
    let outcome = webauthn
        .finish_authentication(&state, &response, &credential)
        .map_err(map_passkey_verification_error)?;
    credential.counter = outcome.new_counter;
    persist_passkey_credential(&user_id, &credential).await?;
    issue_session_for_email_with_assurance(&email, &redirect_url, "aal2").await
}

pub async fn issue_oauth_development_session(
    grant: &ConsumedOauthGrant,
) -> AuthStackResult<LoginCompletionResponse> {
    let email = synthetic_oauth_email(&grant.provider_id, &grant.state);
    issue_session_for_email(&email, &grant.redirect_url).await
}

pub async fn issue_oauth_session(
    identity: &crate::oauth::VerifiedOAuthIdentity,
    redirect_url: &str,
) -> AuthStackResult<LoginCompletionResponse> {
    initialize_schema_async().await?;
    let email = identity
        .email
        .as_deref()
        .filter(|value| identity.email_verified.unwrap_or(true) && !value.trim().is_empty())
        .map(normalize_email)
        .unwrap_or_else(|| {
            synthetic_oauth_email(&identity.provider_id, &identity.provider_subject)
        });
    let user_id = user_id_from_email(&email);
    let now = now_ms();
    let profile_json = json!({
        "email": identity.email,
        "email_verified": identity.email_verified,
        "name": identity.name,
    })
    .to_string();

    let mut statements = user_email_identity_statements(&email, &user_id, now);
    statements.push(mark_user_email_verified_statement(&user_id, now));
    statements.push(AtomicSqlStatement::execute(
        "INSERT INTO auth_external_identities \
         (tenant_id, provider_id, provider_subject, user_id, primary_email, profile_json, created_at_ms, updated_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7) \
         ON CONFLICT(tenant_id, provider_id, provider_subject) DO UPDATE SET \
         user_id = excluded.user_id, \
         primary_email = excluded.primary_email, \
         profile_json = excluded.profile_json, \
         updated_at_ms = excluded.updated_at_ms",
        vec![
            json!(DEFAULT_TENANT_ID),
            json!(identity.provider_id),
            json!(identity.provider_subject),
            json!(&user_id),
            json!(&email),
            json!(&profile_json),
            json!(now),
        ],
    ));
    statements.push(storage_event_statement(
        "auth_external_identity",
        &format!("{}:{}", identity.provider_id, identity.provider_subject),
        "auth_external_identity_linked",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "provider_id": &identity.provider_id,
            "provider_subject": &identity.provider_subject,
            "user_id": &user_id,
            "primary_email": &email,
            "profile_json": &profile_json,
            "email_verified": true,
        }),
    )?);
    execute_sql_atomic(statements).await?;

    issue_session_for_email(&email, redirect_url).await
}

async fn issue_session_for_email(
    email: &str,
    redirect_url: &str,
) -> AuthStackResult<LoginCompletionResponse> {
    issue_session_for_email_with_assurance(email, redirect_url, "aal1").await
}

async fn issue_session_for_email_with_assurance(
    email: &str,
    redirect_url: &str,
    assurance: &str,
) -> AuthStackResult<LoginCompletionResponse> {
    initialize_schema_async().await?;
    let now = now_ms();
    let email = normalize_email(email);
    let user_id = user_id_from_email(&email);
    let session_id = secure_storage_id("session")?;
    let expires_at_ms = now.saturating_add(session_ttl_ms().await);
    let refresh_token_ttl_ms = refresh_token_ttl_ms().await;
    let access_token_ttl_seconds = access_token_ttl_seconds().await;
    let (selected_organization_id, permissions) =
        session_context_for_user(&email, &user_id).await?;
    let permissions_json = serde_json::to_string(&permissions)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;

    let mut statements = user_email_identity_statements(&email, &user_id, now);
    statements.push(AtomicSqlStatement::execute(
        "INSERT INTO auth_sessions \
         (session_id, tenant_id, user_id, primary_email, expires_at_ms, revoked_at_ms, permissions_json, assurance, created_at_ms, updated_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?8, ?8) \
         ON CONFLICT(session_id) DO UPDATE SET \
         tenant_id = excluded.tenant_id, \
         user_id = excluded.user_id, \
         primary_email = excluded.primary_email, \
         expires_at_ms = excluded.expires_at_ms, \
         revoked_at_ms = NULL, \
         permissions_json = excluded.permissions_json, \
         assurance = excluded.assurance, \
         updated_at_ms = excluded.updated_at_ms",
        vec![
            json!(&session_id),
            json!(&selected_organization_id),
            json!(&user_id),
            json!(&email),
            json!(expires_at_ms),
            json!(&permissions_json),
            json!(assurance),
            json!(now),
        ],
    ));
    let refresh_token = opaque_refresh_token()?;
    let refresh_token_hash = refresh_token_hash(&refresh_token);
    let refresh_token_expires_at_ms = now.saturating_add(refresh_token_ttl_ms);
    statements.push(AtomicSqlStatement::execute(
        "INSERT INTO auth_refresh_token_hashes \
         (tenant_id, token_hash, session_id, expires_at_ms, rotated_at_ms, revoked_at_ms, created_at_ms) \
         VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5)",
        vec![
            json!(DEFAULT_TENANT_ID),
            json!(&refresh_token_hash),
            json!(&session_id),
            json!(refresh_token_expires_at_ms),
            json!(now),
        ],
    ));
    statements.push(storage_event_statement(
        "auth_session",
        &session_id,
        "auth_session_issued",
        json!({
            "tenant_id": &selected_organization_id,
            "session_id": &session_id,
            "user_id": &user_id,
            "primary_email": &email,
            "permissions": &permissions,
            "permissions_json": &permissions_json,
            "assurance": assurance,
            "issued_at_ms": now,
            "expires_at_ms": expires_at_ms,
            "refresh_credential_id": format!("refresh:{session_id}"),
            "refresh_secret_version": 1,
        }),
    )?);
    execute_sql_atomic(statements).await?;
    let access_token = issue_access_token_for_session(
        &session_id,
        &user_id,
        &selected_organization_id,
        &email,
        expires_at_ms,
        &permissions,
        access_token_ttl_seconds,
    )
    .await?;

    Ok(LoginCompletionResponse {
        authenticated: true,
        redirect_url: redirect_url.to_string(),
        session_id: Some(session_id),
        access_token: Some(access_token),
        refresh_token: Some(refresh_token),
        expires_in_seconds: access_token_ttl_seconds,
    })
}

pub async fn get_session(session_id: Option<&str>) -> AuthStackResult<SessionView> {
    initialize_schema_async().await?;
    let Some(session_id) = normalized_session_id(session_id) else {
        return Ok(unauthenticated_session());
    };
    let Some(session) = load_session_row(&session_id).await? else {
        return Ok(unauthenticated_session());
    };
    if session.revoked_at_ms.is_some() || session.expires_at_ms < now_ms() as i64 {
        return Ok(unauthenticated_session());
    }
    match refresh_authoritative_session(session).await {
        Ok(session) => Ok(session.into_view()),
        Err(AuthStackError::AuthRequired | AuthStackError::SessionExpired) => {
            Ok(unauthenticated_session())
        }
        Err(error) => Err(error),
    }
}

pub async fn list_user_sessions(
    user_id: &str,
    current_session_id: &str,
) -> AuthStackResult<AccountSessionListResponse> {
    initialize_schema_async().await?;
    let rows = execute_sql(
        "SELECT session_id, tenant_id, assurance, created_at_ms, expires_at_ms \
         FROM auth_sessions WHERE user_id = ?1 AND revoked_at_ms IS NULL AND expires_at_ms > ?2 \
         ORDER BY created_at_ms DESC",
        vec![json!(user_id), json!(now_ms())],
    )
    .await?;
    Ok(AccountSessionListResponse {
        sessions: rows
            .into_iter()
            .map(|row| {
                let session_id = required_string(&row, "session_id")?;
                Ok(AccountSessionSummary {
                    current: session_id == current_session_id,
                    session_id,
                    organization_id: row_string(&row, "tenant_id")
                        .filter(|value| value != DEFAULT_TENANT_ID),
                    assurance: required_string(&row, "assurance")?,
                    issued_at_ms: row_i64(&row, "created_at_ms").unwrap_or_default() as u64,
                    expires_at_ms: row_i64(&row, "expires_at_ms").unwrap_or_default() as u64,
                })
            })
            .collect::<AuthStackResult<Vec<_>>>()?,
    })
}

pub async fn revoke_user_session(
    user_id: &str,
    session_id: &str,
    actor_session_id: &str,
) -> AuthStackResult<()> {
    initialize_schema_async().await?;
    let now = now_ms();
    execute_sql_atomic(vec![
        AtomicSqlStatement::guard(
            "UPDATE auth_sessions SET revoked_at_ms = ?1, updated_at_ms = ?1 \
             WHERE session_id = ?2 AND user_id = ?3 AND revoked_at_ms IS NULL \
             RETURNING session_id",
            vec![json!(now), json!(session_id), json!(user_id)],
        ),
        AtomicSqlStatement::execute(
            "UPDATE auth_refresh_token_hashes SET revoked_at_ms = ?1 \
             WHERE session_id = ?2 AND revoked_at_ms IS NULL",
            vec![json!(now), json!(session_id)],
        ),
        storage_event_statement(
            "auth_session",
            session_id,
            "auth_session_revoked",
            json!({
                "session_id": session_id,
                "user_id": user_id,
                "actor_session_id": actor_session_id,
            }),
        )?,
    ])
    .await?;
    Ok(())
}

pub async fn change_user_password(
    user_id: &str,
    current_password: &str,
    new_password: &str,
    current_session_id: &str,
) -> AuthStackResult<()> {
    initialize_schema_async().await?;
    let rows = execute_sql(
        "SELECT users.primary_email, credentials.password_hash \
         FROM auth_users users JOIN auth_password_credentials credentials \
          ON credentials.user_id = users.user_id AND credentials.tenant_id = users.tenant_id \
         WHERE users.user_id = ?1 AND users.disabled = 0 AND credentials.revoked_at_ms IS NULL \
         LIMIT 1",
        vec![json!(user_id)],
    )
    .await?;
    let row = rows
        .into_iter()
        .next()
        .ok_or(AuthStackError::InvalidCredentials)?;
    let verification =
        verify_password(current_password, &required_string(&row, "password_hash")?).await?;
    if matches!(verification, PasswordVerification::Invalid) {
        return Err(AuthStackError::InvalidCredentials);
    }
    let now = now_ms();
    let next_hash = hash_password(new_password).await?;
    execute_sql_atomic(vec![
        AtomicSqlStatement::guard(
            "UPDATE auth_password_credentials SET password_hash = ?1, updated_at_ms = ?2 \
             WHERE user_id = ?3 AND revoked_at_ms IS NULL RETURNING user_id",
            vec![json!(next_hash), json!(now), json!(user_id)],
        ),
        AtomicSqlStatement::execute(
            "UPDATE auth_sessions SET revoked_at_ms = ?1, updated_at_ms = ?1 \
             WHERE user_id = ?2 AND session_id <> ?3 AND revoked_at_ms IS NULL",
            vec![json!(now), json!(user_id), json!(current_session_id)],
        ),
        AtomicSqlStatement::execute(
            "UPDATE auth_refresh_token_hashes SET revoked_at_ms = ?1 \
             WHERE session_id IN (SELECT session_id FROM auth_sessions \
              WHERE user_id = ?2 AND session_id <> ?3) AND revoked_at_ms IS NULL",
            vec![json!(now), json!(user_id), json!(current_session_id)],
        ),
        storage_event_statement(
            "auth_user",
            user_id,
            "auth_password_changed",
            json!({
                "user_id": user_id,
                "credential_id": format!("password:{user_id}"),
                "secret_version": now,
                "other_sessions_revoked": true,
            }),
        )?,
        mail_outbox_statement(
            "security-notification",
            &required_string(&row, "primary_email")?,
            "Your password changed",
            "Your password was changed. Review active sessions if this was unexpected.",
            now,
        )
        .await?,
    ])
    .await?;
    Ok(())
}

pub async fn mfa_status(
    user_id: &str,
    session_assurance: &str,
) -> AuthStackResult<MfaStatusResponse> {
    initialize_schema_async().await?;
    let rows = execute_sql(
        "SELECT confirmed_at_ms FROM auth_mfa_totp \
         WHERE tenant_id = ?1 AND user_id = ?2 LIMIT 1",
        vec![json!(DEFAULT_TENANT_ID), json!(user_id)],
    )
    .await?;
    let totp_enrolled = rows
        .first()
        .and_then(|row| row_i64(row, "confirmed_at_ms"))
        .is_some();
    let rows = execute_sql(
        "SELECT COUNT(*) AS available FROM auth_recovery_codes \
         WHERE tenant_id = ?1 AND user_id = ?2 AND used_at_ms IS NULL",
        vec![json!(DEFAULT_TENANT_ID), json!(user_id)],
    )
    .await?;
    let recovery_codes_remaining = rows
        .first()
        .and_then(|row| row_i64(row, "available"))
        .unwrap_or_default()
        .clamp(0, i64::from(u32::MAX)) as u32;
    Ok(MfaStatusResponse {
        totp_enrolled,
        recovery_codes_remaining,
        assurance: session_assurance.to_owned(),
    })
}

pub async fn start_totp_enrollment(
    user_id: &str,
    primary_email: &str,
) -> AuthStackResult<MfaEnrollStartResponse> {
    initialize_schema_async().await?;
    let rows = execute_sql(
        "SELECT confirmed_at_ms FROM auth_mfa_totp \
         WHERE tenant_id = ?1 AND user_id = ?2 LIMIT 1",
        vec![json!(DEFAULT_TENANT_ID), json!(user_id)],
    )
    .await?;
    if rows
        .first()
        .and_then(|row| row_i64(row, "confirmed_at_ms"))
        .is_some()
    {
        return Err(AuthStackError::conflict(
            "TOTP is already enrolled; use an AAL2 replacement workflow",
        ));
    }

    let secret = TotpSecret::generate(&HostRandom)
        .map_err(|_| AuthStackError::store("failed to generate TOTP secret"))?;
    let credential_id = secure_storage_id("totp")?;
    let encrypted_secret = encrypt_mfa_secret(user_id, &credential_id, secret.expose()).await?;
    let issuer = store_config_value("AUTH_MFA_ISSUER")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "wasi-auth".to_owned());
    let uri = provisioning_uri(&issuer, primary_email, &secret, TotpConfig::default())
        .map_err(|_| AuthStackError::configuration("TOTP provisioning data is invalid"))?;
    let now = now_ms();
    execute_sql_atomic(vec![
        AtomicSqlStatement::execute(
            "INSERT INTO auth_mfa_totp \
             (tenant_id, user_id, credential_id, encrypted_secret, confirmed_at_ms, last_used_at_ms, created_at_ms, updated_at_ms) \
             VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5, ?5) \
             ON CONFLICT(tenant_id, user_id) DO UPDATE SET \
             credential_id = excluded.credential_id, encrypted_secret = excluded.encrypted_secret, \
             created_at_ms = excluded.created_at_ms, updated_at_ms = excluded.updated_at_ms \
             WHERE auth_mfa_totp.confirmed_at_ms IS NULL",
            vec![
                json!(DEFAULT_TENANT_ID),
                json!(user_id),
                json!(&credential_id),
                json!(&encrypted_secret),
                json!(now),
            ],
        ),
        storage_event_statement(
            "auth_user",
            user_id,
            "auth_totp_enrollment_started",
            json!({
                "user_id": user_id,
                "credential_id": &credential_id,
                "secret_version": 1,
            }),
        )?,
    ])
    .await?;
    Ok(MfaEnrollStartResponse {
        credential_id,
        secret_base32: secret.provisioning_base32(),
        provisioning_uri: uri,
    })
}

pub async fn confirm_totp_enrollment(
    user_id: &str,
    session_id: &str,
    code: &str,
) -> AuthStackResult<MfaEnrollConfirmResponse> {
    initialize_schema_async().await?;
    let rows = execute_sql(
        "SELECT credential_id, encrypted_secret, confirmed_at_ms FROM auth_mfa_totp \
         WHERE tenant_id = ?1 AND user_id = ?2 LIMIT 1",
        vec![json!(DEFAULT_TENANT_ID), json!(user_id)],
    )
    .await?;
    let row = rows
        .into_iter()
        .next()
        .ok_or_else(|| AuthStackError::not_found("TOTP enrollment was not started"))?;
    if row_i64(&row, "confirmed_at_ms").is_some() {
        return Err(AuthStackError::conflict(
            "TOTP enrollment is already confirmed",
        ));
    }
    let credential_id = required_string(&row, "credential_id")?;
    let mut secret = decrypt_mfa_secret(
        user_id,
        &credential_id,
        &required_string(&row, "encrypted_secret")?,
    )
    .await?;
    let valid = verify_totp(
        &secret,
        code.trim(),
        now_ms() / 1_000,
        TotpConfig::default(),
    )
    .unwrap_or(false);
    secret.fill(0);
    if !valid {
        return Err(AuthStackError::InvalidCredentials);
    }

    let pepper = recovery_code_pepper().await?;
    let mut recovery_codes = Vec::with_capacity(MFA_RECOVERY_CODE_COUNT);
    let mut code_hashes = Vec::with_capacity(MFA_RECOVERY_CODE_COUNT);
    for _ in 0..MFA_RECOVERY_CODE_COUNT {
        let code = RecoveryCode::generate(&HostRandom)
            .map_err(|_| AuthStackError::store("failed to generate recovery codes"))?;
        let hash = hash_recovery_code(&pepper, code.expose())
            .map_err(|_| AuthStackError::store("failed to hash recovery code"))?;
        code_hashes.push(URL_SAFE_NO_PAD.encode(hash.as_bytes()));
        recovery_codes.push(code.expose().to_owned());
    }
    let now = now_ms();
    let mut statements = vec![
        AtomicSqlStatement::guard(
            "UPDATE auth_mfa_totp SET confirmed_at_ms = ?1, last_used_at_ms = ?1, updated_at_ms = ?1 \
             WHERE tenant_id = ?2 AND user_id = ?3 AND credential_id = ?4 AND confirmed_at_ms IS NULL \
             RETURNING credential_id",
            vec![
                json!(now),
                json!(DEFAULT_TENANT_ID),
                json!(user_id),
                json!(&credential_id),
            ],
        ),
        AtomicSqlStatement::execute(
            "DELETE FROM auth_recovery_codes WHERE tenant_id = ?1 AND user_id = ?2",
            vec![json!(DEFAULT_TENANT_ID), json!(user_id)],
        ),
    ];
    statements.extend(code_hashes.into_iter().map(|code_hash| {
        AtomicSqlStatement::execute(
            "INSERT INTO auth_recovery_codes \
             (tenant_id, user_id, credential_id, code_hash, used_at_ms, created_at_ms) \
             VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
            vec![
                json!(DEFAULT_TENANT_ID),
                json!(user_id),
                json!(&credential_id),
                json!(code_hash),
                json!(now),
            ],
        )
    }));
    statements.push(session_step_up_statement(session_id, user_id, now));
    statements.push(storage_event_statement(
        "auth_user",
        user_id,
        "auth_totp_enrollment_confirmed",
        json!({
            "user_id": user_id,
            "credential_id": &credential_id,
            "recovery_code_count": MFA_RECOVERY_CODE_COUNT,
        }),
    )?);
    execute_sql_atomic(statements).await?;
    Ok(MfaEnrollConfirmResponse {
        recovery_codes,
        assurance: "aal2".to_owned(),
    })
}

pub async fn verify_totp_step_up(
    user_id: &str,
    session_id: &str,
    code: &str,
) -> AuthStackResult<SessionView> {
    initialize_schema_async().await?;
    let rows = execute_sql(
        "SELECT credential_id, encrypted_secret FROM auth_mfa_totp \
         WHERE tenant_id = ?1 AND user_id = ?2 AND confirmed_at_ms IS NOT NULL LIMIT 1",
        vec![json!(DEFAULT_TENANT_ID), json!(user_id)],
    )
    .await?;
    let row = rows
        .into_iter()
        .next()
        .ok_or(AuthStackError::InvalidCredentials)?;
    let credential_id = required_string(&row, "credential_id")?;
    let mut secret = decrypt_mfa_secret(
        user_id,
        &credential_id,
        &required_string(&row, "encrypted_secret")?,
    )
    .await?;
    let valid = verify_totp(
        &secret,
        code.trim(),
        now_ms() / 1_000,
        TotpConfig::default(),
    )
    .unwrap_or(false);
    secret.fill(0);
    if !valid {
        return Err(AuthStackError::InvalidCredentials);
    }
    let now = now_ms();
    execute_sql_atomic(vec![
        AtomicSqlStatement::guard(
            "UPDATE auth_mfa_totp SET last_used_at_ms = ?1, updated_at_ms = ?1 \
             WHERE tenant_id = ?2 AND user_id = ?3 AND credential_id = ?4 \
               AND confirmed_at_ms IS NOT NULL RETURNING credential_id",
            vec![
                json!(now),
                json!(DEFAULT_TENANT_ID),
                json!(user_id),
                json!(&credential_id),
            ],
        ),
        session_step_up_statement(session_id, user_id, now),
        storage_event_statement(
            "auth_session",
            session_id,
            "auth_mfa_totp_verified",
            json!({"user_id": user_id, "credential_id": &credential_id}),
        )?,
    ])
    .await?;
    get_session(Some(session_id)).await
}

pub async fn use_recovery_code_for_step_up(
    user_id: &str,
    session_id: &str,
    code: &str,
) -> AuthStackResult<SessionView> {
    initialize_schema_async().await?;
    let pepper = recovery_code_pepper().await?;
    let code_hash =
        hash_recovery_code(&pepper, code.trim()).map_err(|_| AuthStackError::InvalidCredentials)?;
    let encoded_hash = URL_SAFE_NO_PAD.encode(code_hash.as_bytes());
    let now = now_ms();
    let result = execute_sql_atomic(vec![
        AtomicSqlStatement::guard(
            "UPDATE auth_recovery_codes SET used_at_ms = ?1 \
             WHERE tenant_id = ?2 AND user_id = ?3 AND code_hash = ?4 AND used_at_ms IS NULL \
             RETURNING credential_id",
            vec![
                json!(now),
                json!(DEFAULT_TENANT_ID),
                json!(user_id),
                json!(encoded_hash),
            ],
        ),
        session_step_up_statement(session_id, user_id, now),
        storage_event_statement(
            "auth_session",
            session_id,
            "auth_mfa_recovery_code_used",
            json!({"user_id": user_id}),
        )?,
    ])
    .await;
    if result.is_err() {
        return Err(AuthStackError::InvalidCredentials);
    }
    get_session(Some(session_id)).await
}

fn session_step_up_statement(session_id: &str, user_id: &str, now: u64) -> AtomicSqlStatement {
    AtomicSqlStatement::guard(
        "UPDATE auth_sessions SET assurance = 'aal2', updated_at_ms = ?1 \
         WHERE session_id = ?2 AND user_id = ?3 AND revoked_at_ms IS NULL AND expires_at_ms > ?1 \
         RETURNING session_id",
        vec![json!(now), json!(session_id), json!(user_id)],
    )
}

pub async fn refresh_session(
    session_id: Option<&str>,
    refresh_token: Option<&str>,
) -> AuthStackResult<TokenRefreshResponse> {
    initialize_schema_async().await?;
    let session = load_active_session(session_id).await?;
    let refresh_token = refresh_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(AuthStackError::InvalidToken)?;
    let current_refresh_token_hash = refresh_token_hash(refresh_token);
    let refresh_row = load_refresh_token_row(&current_refresh_token_hash).await?;
    if refresh_row.session_id != session.session_id {
        return Err(AuthStackError::InvalidToken);
    }
    if refresh_row.revoked_at_ms.is_some() {
        return Err(AuthStackError::InvalidToken);
    }
    if refresh_row.rotated_at_ms.is_some() {
        revoke_refresh_family(&session.session_id).await?;
        return Err(AuthStackError::InvalidToken);
    }
    if refresh_row.expires_at_ms < now_ms() as i64 {
        return Err(AuthStackError::SessionExpired);
    }

    let now = now_ms();
    let refresh_token_ttl_ms = refresh_token_ttl_ms().await;
    let access_token_ttl_seconds = access_token_ttl_seconds().await;
    let next_refresh_token = opaque_refresh_token()?;
    let next_refresh_token_hash = refresh_token_hash(&next_refresh_token);
    let next_refresh_token_expires_at_ms = now.saturating_add(refresh_token_ttl_ms);
    let rotation = execute_sql_atomic(vec![
        AtomicSqlStatement::guard(
            "UPDATE auth_refresh_token_hashes \
             SET rotated_at_ms = ?1 \
             WHERE tenant_id = ?2 AND token_hash = ?3 AND rotated_at_ms IS NULL \
               AND revoked_at_ms IS NULL AND expires_at_ms >= ?1 \
             RETURNING token_hash",
            vec![
                json!(now),
                json!(DEFAULT_TENANT_ID),
                json!(&current_refresh_token_hash),
            ],
        ),
        AtomicSqlStatement::execute(
            "INSERT INTO auth_refresh_token_hashes \
             (tenant_id, token_hash, session_id, expires_at_ms, rotated_at_ms, revoked_at_ms, created_at_ms) \
             VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5)",
            vec![
                json!(DEFAULT_TENANT_ID),
                json!(&next_refresh_token_hash),
                json!(&session.session_id),
                json!(next_refresh_token_expires_at_ms),
                json!(now),
            ],
        ),
        AtomicSqlStatement::guard(
            "UPDATE auth_sessions SET updated_at_ms = ?1 \
             WHERE session_id = ?2 AND revoked_at_ms IS NULL AND expires_at_ms >= ?1 \
             RETURNING session_id",
            vec![json!(now), json!(&session.session_id)],
        ),
        storage_event_statement(
            "auth_session",
            &session.session_id,
            "auth_refresh_token_rotated",
            json!({
                "tenant_id": DEFAULT_TENANT_ID,
                "session_id": &session.session_id,
                "refresh_credential_id": format!("refresh:{}", session.session_id),
                "refresh_secret_version": now,
            }),
        )?,
    ])
    .await;
    if let Err(error) = rotation {
        let latest = load_refresh_token_row(&current_refresh_token_hash).await;
        if latest
            .as_ref()
            .is_ok_and(|row| row.rotated_at_ms.is_some() || row.revoked_at_ms.is_some())
        {
            revoke_refresh_family(&session.session_id).await?;
            return Err(AuthStackError::InvalidToken);
        }
        return Err(error);
    }
    let access_token = issue_access_token_for_session(
        &session.session_id,
        &session.user_id,
        &session.tenant_id,
        session.primary_email.as_deref().unwrap_or_default(),
        session.expires_at_ms as u64,
        &session.permissions,
        access_token_ttl_seconds,
    )
    .await?;

    Ok(TokenRefreshResponse {
        access_token: Some(access_token),
        refresh_token: Some(next_refresh_token),
        expires_in_seconds: access_token_ttl_seconds,
    })
}

async fn revoke_refresh_family(session_id: &str) -> AuthStackResult<()> {
    let now = now_ms();
    execute_sql_atomic(vec![
        AtomicSqlStatement::execute(
            "UPDATE auth_refresh_token_hashes SET revoked_at_ms = ?1 \
             WHERE session_id = ?2 AND revoked_at_ms IS NULL",
            vec![json!(now), json!(session_id)],
        ),
        AtomicSqlStatement::execute(
            "UPDATE auth_sessions SET revoked_at_ms = ?1, updated_at_ms = ?1 \
             WHERE session_id = ?2 AND revoked_at_ms IS NULL",
            vec![json!(now), json!(session_id)],
        ),
        storage_event_statement(
            "auth_session",
            session_id,
            "auth_refresh_token_reuse_detected",
            json!({
                "session_id": session_id,
                "refresh_family_revoked": true,
            }),
        )?,
    ])
    .await?;
    Ok(())
}

pub async fn verify_access_token(
    request: &TokenVerifyRequest,
) -> AuthStackResult<TokenVerifyResponse> {
    initialize_schema_async().await?;
    let key_id = access_token_key_id(&request.access_token).map_err(map_auth_error)?;
    let key_config = signing_key_config_for_key_id(&key_id).await?;
    let decoding_key = jwt_decoding_key_for_config(&key_config).await?;
    let claims = decode_access_token(
        &request.access_token,
        &decoding_key,
        &jwt_issuer().await,
        &jwt_audience().await,
        &[key_config.algorithm],
    )
    .map_err(map_auth_error)?;
    let session_id = claims.session_id.as_ref().map(|value| value.as_str());
    let session = load_active_session(session_id).await?;
    if claims.sub != session.user_id
        || claims.tenant_id.as_ref().map(|value| value.as_str()) != Some(session.tenant_id.as_str())
        || claims.session_id.as_ref().map(|value| value.as_str())
            != Some(session.session_id.as_str())
    {
        return Err(AuthStackError::InvalidToken);
    }

    Ok(TokenVerifyResponse {
        active: true,
        subject: session.user_id,
        tenant_id: Some(session.tenant_id),
        session_id: Some(session.session_id),
        expires_at: claims.exp,
        scopes: session.permissions.clone(),
        assurance: session.assurance.clone(),
        system_administrator: is_system_administrator(&session.permissions),
        issued_at_unix_seconds: claims.iat,
    })
}

pub async fn revoke_session(session_id: Option<&str>) -> AuthStackResult<LogoutResponse> {
    initialize_schema_async().await?;
    if let Some(session_id) = normalized_session_id(session_id) {
        let now = now_ms();
        execute_sql_atomic(vec![
            AtomicSqlStatement::execute(
                "UPDATE auth_sessions \
                 SET revoked_at_ms = ?1, updated_at_ms = ?1 \
                 WHERE session_id = ?2 AND revoked_at_ms IS NULL",
                vec![json!(now), json!(&session_id)],
            ),
            AtomicSqlStatement::execute(
                "UPDATE auth_refresh_token_hashes \
                 SET revoked_at_ms = ?1 \
                 WHERE tenant_id = ?2 AND session_id = ?3 AND revoked_at_ms IS NULL",
                vec![json!(now), json!(DEFAULT_TENANT_ID), json!(&session_id)],
            ),
            storage_event_statement(
                "auth_session",
                &session_id,
                "auth_session_revoked",
                json!({
                    "tenant_id": DEFAULT_TENANT_ID,
                    "session_id": &session_id,
                }),
            )?,
        ])
        .await?;
    }

    Ok(LogoutResponse {
        redirect_url: "/login".to_string(),
    })
}

pub async fn get_jwks() -> AuthStackResult<JwksDocument> {
    initialize_schema_async().await?;
    if let Some(jwks) = jwt_public_jwks().await? {
        return Ok(jwks);
    }

    let rows = execute_sql(
        "SELECT kid, kty, alg, use_value, public_parameters_json \
         FROM auth_jwks \
         ORDER BY kid ASC",
        Vec::new(),
    )
    .await?;

    let keys = rows
        .into_iter()
        .map(jwks_key_from_row)
        .collect::<AuthStackResult<Vec<_>>>()?;
    Ok(JwksDocument { keys })
}

pub async fn list_signing_keys() -> AuthStackResult<SigningKeyListResponse> {
    initialize_schema_async().await?;
    Ok(SigningKeyListResponse {
        keys: signing_key_summaries().await?,
    })
}

pub async fn rotate_signing_key(
    kid: &str,
    retire_previous: bool,
) -> AuthStackResult<SigningKeyRotateResponse> {
    initialize_schema_async().await?;
    let kid = kid.trim();
    if kid.is_empty() {
        return Err(AuthStackError::validation("kid is required"));
    }

    let configured_keys = configured_signing_keys().await?;
    let target = configured_keys
        .iter()
        .find(|key| key.kid == kid)
        .ok_or_else(|| {
            AuthStackError::validation(
                "signing key is not present in AUTH_JWT_KEY_RING_JSON or current JWT config",
            )
        })?;
    if target.status == SigningKeyStatus::Revoked {
        return Err(AuthStackError::validation(
            "revoked signing keys cannot be activated",
        ));
    }
    let target_alg = algorithm_name(target.algorithm).to_string();

    let previous_kid = active_signing_key_config_from(&configured_keys)
        .await
        .ok()
        .map(|key| key.kid)
        .filter(|previous| previous != kid);
    let now = now_ms();
    let mut statements = Vec::new();
    if retire_previous && let Some(previous_kid) = previous_kid.as_deref() {
        let previous_alg = configured_keys
            .iter()
            .find(|key| key.kid == previous_kid)
            .map(|key| algorithm_name(key.algorithm))
            .ok_or_else(|| AuthStackError::configuration("active signing key is not configured"))?;
        statements.push(signing_key_state_statement(
            previous_kid,
            previous_alg,
            SigningKeyStatus::Retired,
            Some(now),
            Some(now),
            None,
            now,
        ));
    }
    statements.push(signing_key_state_statement(
        kid,
        &target_alg,
        SigningKeyStatus::Active,
        Some(now),
        None,
        None,
        now,
    ));
    statements.push(storage_event_statement(
        "auth_signing_key_set",
        kid,
        "auth_signing_key_rotated",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "kid": kid,
            "previous_kid": previous_kid.as_deref(),
            "retired_previous": retire_previous,
        }),
    )?);
    execute_sql_atomic(statements).await?;

    Ok(SigningKeyRotateResponse {
        active_kid: kid.to_string(),
        previous_kid,
        retired_previous: retire_previous,
        keys: signing_key_summaries().await?,
    })
}

pub async fn csrf_token_for_session(session_id: &str) -> AuthStackResult<String> {
    let secret = if let Some(value) = store_config_value("AUTH_CSRF_SECRET")
        .await
        .filter(|value| !value.trim().is_empty())
    {
        value
    } else {
        jwt_secret().await
    };
    let digest = Sha256::digest(format!("csrf:{secret}:{session_id}").as_bytes());
    Ok(URL_SAFE_NO_PAD.encode(digest))
}

pub async fn enforce_account_rate_limit(
    operation: &str,
    account_identifier: &str,
    maximum_attempts: u64,
    window_seconds: u64,
) -> AuthStackResult<()> {
    initialize_schema_async().await?;
    let secret = store_config_value("AUTH_CSRF_SECRET")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(jwt_secret().await);
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes())
        .map_err(|_| AuthStackError::configuration("rate-limit key is invalid"))?;
    mac.update(operation.as_bytes());
    mac.update(&[0]);
    mac.update(normalize_email(account_identifier).as_bytes());
    let bucket_key = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    let now = now_ms();
    let window_expires_at_ms = now.saturating_add(window_seconds.saturating_mul(1_000));
    let rows = execute_sql(
        "INSERT INTO auth_rate_limit_buckets \
         (bucket_key, attempt_count, window_expires_at_ms, updated_at_ms) \
         VALUES (?1, 1, ?2, ?3) \
         ON CONFLICT(bucket_key) DO UPDATE SET \
           attempt_count = CASE \
             WHEN auth_rate_limit_buckets.window_expires_at_ms <= ?3 THEN 1 \
             ELSE auth_rate_limit_buckets.attempt_count + 1 END, \
           window_expires_at_ms = CASE \
             WHEN auth_rate_limit_buckets.window_expires_at_ms <= ?3 THEN ?2 \
             ELSE auth_rate_limit_buckets.window_expires_at_ms END, \
           updated_at_ms = ?3 \
         RETURNING attempt_count, window_expires_at_ms",
        vec![json!(bucket_key), json!(window_expires_at_ms), json!(now)],
    )
    .await?;
    let row = rows
        .first()
        .ok_or_else(|| AuthStackError::store("rate-limit update returned no row"))?;
    let attempts = row_i64(row, "attempt_count").unwrap_or(i64::MAX);
    if attempts > i64::try_from(maximum_attempts).unwrap_or(i64::MAX) {
        let expiry = row_i64(row, "window_expires_at_ms").unwrap_or(now as i64);
        let retry_after_seconds = u64::try_from(expiry)
            .unwrap_or(now)
            .saturating_sub(now)
            .div_ceil(1_000)
            .max(1);
        return Err(AuthStackError::RateLimited {
            retry_after_seconds,
        });
    }
    Ok(())
}

pub async fn latest_captured_mail(
    recipient: &str,
    message_kind: &str,
) -> AuthStackResult<CapturedMailResponse> {
    if config_bool(AUTH_PRODUCTION_MODE, false).await
        || !config_bool("AUTH_DEV_TOOLS", false).await
        || mail_transport().await != "capture"
    {
        return Err(AuthStackError::not_found("mail capture is not available"));
    }
    let normalized_recipient = normalize_email(recipient);
    let rows = execute_sql(
        "SELECT message_id, message_kind, payload_encrypted \
         FROM auth_mail_outbox \
         WHERE recipient_hash = ?1 AND message_kind = ?2 \
         ORDER BY created_at_ms DESC \
         LIMIT 1",
        vec![
            json!(mail_recipient_hash(&normalized_recipient)),
            json!(message_kind),
        ],
    )
    .await?;
    let row = rows
        .into_iter()
        .next()
        .ok_or_else(|| AuthStackError::not_found("captured mail was not found"))?;
    let message_id = required_string(&row, "message_id")?;
    let payload =
        decrypt_mail_payload(&message_id, &required_string(&row, "payload_encrypted")?).await?;
    Ok(CapturedMailResponse {
        message_kind: required_string(&row, "message_kind")?,
        recipient: payload.recipient,
        subject: payload.subject,
        body_text: payload.body_text,
    })
}

/// Delivers a bounded batch from the durable mail outbox.
///
/// Capture mail is read through [`latest_captured_mail`] and therefore needs
/// no network delivery. Production Spin deployments use the versioned HTTP
/// webhook adapter; SMTP is supplied by a native host worker through
/// `wasi_auth::mail::SmtpMailer` rather than by this HTTP component.
pub async fn dispatch_pending_mail() -> AuthStackResult<usize> {
    match mail_transport().await.as_str() {
        "capture" => Ok(0),
        "http" => {
            #[cfg(all(feature = "mail-http", runtime_spin))]
            {
                dispatch_pending_http_mail().await
            }
            #[cfg(not(all(feature = "mail-http", runtime_spin)))]
            {
                Err(AuthStackError::configuration(
                    "HTTP mail delivery requires the mail-http feature on Spin",
                ))
            }
        }
        "smtp" => Err(AuthStackError::configuration(
            "the Spin fullstack component requires HTTP mail; run an external SmtpMailer outbox worker for SMTP",
        )),
        _ => Err(AuthStackError::configuration(
            "AUTH_MAIL_TRANSPORT must be capture, smtp, or http",
        )),
    }
}

#[cfg(all(feature = "mail-http", runtime_spin))]
async fn dispatch_pending_http_mail() -> AuthStackResult<usize> {
    let endpoint = store_config_value("AUTH_MAIL_HTTP_URL")
        .await
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AuthStackError::configuration("AUTH_MAIL_HTTP_URL is required"))?;
    let bearer_token = store_config_value("AUTH_MAIL_HTTP_TOKEN")
        .await
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AuthStackError::configuration("AUTH_MAIL_HTTP_TOKEN is required"))?;
    let endpoint = HttpMailEndpoint::new(endpoint.trim())
        .map_err(|_| AuthStackError::configuration("AUTH_MAIL_HTTP_URL is invalid"))?;
    let bearer_token = HttpMailBearerToken::new(bearer_token)
        .map_err(|_| AuthStackError::configuration("AUTH_MAIL_HTTP_TOKEN is invalid"))?;
    let mailer = HttpMailer::new(endpoint, bearer_token, SpinOutboundHttpTransport);
    let now = now_ms();
    let rows = execute_sql(
        "SELECT message_id, message_kind, payload_encrypted, correlation_id, attempt_count \
         FROM auth_mail_outbox \
         WHERE delivered_at_ms IS NULL AND available_at_ms <= ?1 \
           AND (lease_id IS NULL OR leased_until_ms < ?1) \
         ORDER BY available_at_ms ASC, created_at_ms ASC LIMIT ?2",
        vec![json!(now), json!(MAX_MAIL_DISPATCH_BATCH)],
    )
    .await?;

    let mut delivered = 0_usize;
    for row in rows {
        let message_id = required_string(&row, "message_id")?;
        let lease_id = secure_storage_id("mail_lease")?;
        let leased_until_ms = now.saturating_add(MAIL_LEASE_MS);
        if execute_sql_atomic(vec![AtomicSqlStatement::guard(
            "UPDATE auth_mail_outbox SET lease_id = ?1, leased_until_ms = ?2 \
             WHERE message_id = ?3 AND delivered_at_ms IS NULL AND available_at_ms <= ?4 \
               AND (lease_id IS NULL OR leased_until_ms < ?4) RETURNING message_id",
            vec![
                json!(&lease_id),
                json!(leased_until_ms),
                json!(&message_id),
                json!(now),
            ],
        )])
        .await
        .is_err()
        {
            continue;
        }

        let result = async {
            let payload =
                decrypt_mail_payload(&message_id, &required_string(&row, "payload_encrypted")?)
                    .await?;
            let kind = email_kind(&required_string(&row, "message_kind")?)?;
            let recipient = Recipient::new(payload.recipient)
                .map_err(|_| AuthStackError::store("stored mail recipient is invalid"))?;
            let message = EmailMessage::new(
                kind,
                recipient,
                payload.subject,
                payload.body_text,
                required_string(&row, "correlation_id")?,
            )
            .map_err(|_| AuthStackError::store("stored mail message is invalid"))?;
            mailer
                .send(&message)
                .await
                .map_err(|_| AuthStackError::transport("mail provider delivery failed"))
        }
        .await;

        match result {
            Ok(delivery_id) => {
                execute_sql_atomic(vec![AtomicSqlStatement::guard(
                    "UPDATE auth_mail_outbox \
                     SET delivered_at_ms = ?1, delivery_id = ?2, lease_id = NULL, \
                         leased_until_ms = NULL, last_error_code = NULL \
                     WHERE message_id = ?3 AND lease_id = ?4 AND delivered_at_ms IS NULL \
                     RETURNING message_id",
                    vec![
                        json!(now_ms()),
                        json!(delivery_id.as_str()),
                        json!(&message_id),
                        json!(&lease_id),
                    ],
                )])
                .await?;
                delivered += 1;
            }
            Err(error) => {
                let attempts = row_i64(&row, "attempt_count").unwrap_or_default().max(0) as u32;
                let retry_delay_ms = mail_retry_delay_ms(attempts);
                execute_sql_atomic(vec![AtomicSqlStatement::guard(
                    "UPDATE auth_mail_outbox \
                     SET attempt_count = attempt_count + 1, available_at_ms = ?1, \
                         last_error_code = ?2, lease_id = NULL, leased_until_ms = NULL \
                     WHERE message_id = ?3 AND lease_id = ?4 AND delivered_at_ms IS NULL \
                     RETURNING message_id",
                    vec![
                        json!(now_ms().saturating_add(retry_delay_ms)),
                        json!(error.public_code()),
                        json!(&message_id),
                        json!(&lease_id),
                    ],
                )])
                .await?;
                tracing::warn!(
                    message_id,
                    error_code = error.public_code(),
                    "mail outbox delivery deferred"
                );
            }
        }
    }
    Ok(delivered)
}

#[cfg(all(feature = "mail-http", runtime_spin))]
fn email_kind(value: &str) -> AuthStackResult<EmailKind> {
    match value {
        "email-verification" => Ok(EmailKind::Verification),
        "password-reset" => Ok(EmailKind::PasswordReset),
        "invitation" => Ok(EmailKind::Invitation),
        "security-notification" => Ok(EmailKind::SecurityNotification),
        _ => Err(AuthStackError::store("stored mail kind is invalid")),
    }
}

#[cfg(all(feature = "mail-http", runtime_spin))]
fn mail_retry_delay_ms(attempts: u32) -> u64 {
    let exponent = attempts.min(6);
    30_000_u64.saturating_mul(1_u64 << exponent).min(3_600_000)
}

/// Flushes a bounded batch of optional direct-SpiceDB relationship intents.
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

#[derive(Debug, Deserialize, Serialize)]
struct StoredMailPayload {
    recipient: String,
    subject: String,
    body_text: String,
}

async fn mail_outbox_statement(
    message_kind: &str,
    recipient: &str,
    subject: &str,
    body_text: &str,
    created_at_ms: u64,
) -> AuthStackResult<AtomicSqlStatement> {
    let message_id = secure_storage_id("mail")?;
    let recipient = normalize_email(recipient);
    if recipient.len() > 320
        || !recipient.contains('@')
        || subject.is_empty()
        || subject.len() > 200
        || subject.chars().any(char::is_control)
        || body_text.is_empty()
        || body_text.len() > 128 * 1024
    {
        return Err(AuthStackError::validation("mail message is invalid"));
    }
    let payload = StoredMailPayload {
        recipient: recipient.clone(),
        subject: subject.to_owned(),
        body_text: body_text.to_owned(),
    };
    let payload_json = serde_json::to_vec(&payload)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;
    let payload_encrypted = encrypt_mail_payload(&message_id, &payload_json).await?;
    Ok(AtomicSqlStatement::execute(
        "INSERT INTO auth_mail_outbox \
         (message_id, message_kind, recipient_hash, payload_encrypted, correlation_id, \
          available_at_ms, created_at_ms, delivered_at_ms, delivery_id, attempt_count, \
          last_error_code, lease_id, leased_until_ms) \
         VALUES (?1, ?2, ?3, ?4, ?1, ?5, ?5, NULL, NULL, 0, NULL, NULL, NULL)",
        vec![
            json!(&message_id),
            json!(message_kind),
            json!(mail_recipient_hash(&recipient)),
            json!(payload_encrypted),
            json!(created_at_ms),
        ],
    ))
}

fn mail_recipient_hash(recipient: &str) -> String {
    one_time_token_hash(&format!("mail-recipient:{}", normalize_email(recipient)))
}

async fn encrypt_mail_payload(message_id: &str, plaintext: &[u8]) -> AuthStackResult<String> {
    let key = mfa_key_material(MFA_VAULT_KEY, "mail-outbox").await?;
    encrypt_mail_payload_with_key(&key, message_id, plaintext)
}

fn encrypt_mail_payload_with_key(
    key: &[u8; 32],
    message_id: &str,
    plaintext: &[u8],
) -> AuthStackResult<String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| AuthStackError::configuration("mail vault key is invalid"))?;
    let nonce_bytes: [u8; MFA_NONCE_BYTES] = random_bytes(MFA_NONCE_BYTES)?
        .try_into()
        .map_err(|_| AuthStackError::store("failed to generate mail nonce"))?;
    let nonce = Nonce::from(nonce_bytes);
    let aad = format!("mail-outbox:{message_id}:v1");
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| AuthStackError::store("failed to encrypt mail payload"))?;
    Ok(format!(
        "v1.{}.{}",
        URL_SAFE_NO_PAD.encode(nonce_bytes),
        URL_SAFE_NO_PAD.encode(ciphertext)
    ))
}

async fn decrypt_mail_payload(
    message_id: &str,
    encoded: &str,
) -> AuthStackResult<StoredMailPayload> {
    let key = mfa_key_material(MFA_VAULT_KEY, "mail-outbox").await?;
    let bytes = decrypt_mail_payload_with_key(&key, message_id, encoded)?;
    serde_json::from_slice(&bytes)
        .map_err(|_| AuthStackError::store("stored mail payload is invalid"))
}

fn decrypt_mail_payload_with_key(
    key: &[u8; 32],
    message_id: &str,
    encoded: &str,
) -> AuthStackResult<Vec<u8>> {
    let mut parts = encoded.split('.');
    if parts.next() != Some("v1") {
        return Err(AuthStackError::store(
            "stored mail payload version is invalid",
        ));
    }
    let nonce_bytes: [u8; MFA_NONCE_BYTES] = parts
        .next()
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .filter(|value| value.len() == MFA_NONCE_BYTES)
        .ok_or_else(|| AuthStackError::store("stored mail nonce is invalid"))?
        .try_into()
        .map_err(|_| AuthStackError::store("stored mail nonce is invalid"))?;
    let ciphertext = parts
        .next()
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AuthStackError::store("stored mail ciphertext is invalid"))?;
    if parts.next().is_some() {
        return Err(AuthStackError::store(
            "stored mail payload format is invalid",
        ));
    }
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| AuthStackError::configuration("mail vault key is invalid"))?;
    let nonce = Nonce::from(nonce_bytes);
    let aad = format!("mail-outbox:{message_id}:v1");
    cipher
        .decrypt(
            &nonce,
            Payload {
                msg: &ciphertext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| AuthStackError::store("stored mail payload authentication failed"))
}

fn relationship_outbox_statement(
    operation: &str,
    resource: &str,
    relation_name: &str,
    subject: &str,
    resource_revision: u64,
) -> AuthStackResult<AtomicSqlStatement> {
    let now = now_ms();
    Ok(AtomicSqlStatement::execute(
        "INSERT INTO auth_relationship_outbox \
         (intent_id, operation, resource, relation_name, subject, resource_revision, \
          consistency_token, status, attempt_count, last_error, available_at_ms, \
          lease_id, leased_until_ms, created_at_ms, updated_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, 'pending', 0, NULL, ?7, NULL, NULL, ?7, ?7)",
        vec![
            json!(secure_storage_id("relationship")?),
            json!(operation),
            json!(resource),
            json!(relation_name),
            json!(subject),
            json!(resource_revision),
            json!(now),
        ],
    ))
}

fn auth_provider_seed_statement(
    provider_id: &str,
    enabled: bool,
    display_name: &str,
    login_url: &str,
    now: u64,
) -> AtomicSqlStatement {
    AtomicSqlStatement::execute(
        "INSERT OR IGNORE INTO auth_provider_configs \
         (tenant_id, provider_id, display_name, login_url, enabled, scopes_json, redirect_uris_json, claim_mapping_json, created_at_ms, updated_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, '[]', '[]', '{}', ?6, ?6)",
        vec![
            json!(DEFAULT_TENANT_ID),
            json!(provider_id),
            json!(display_name),
            json!(login_url),
            json!(if enabled { 1 } else { 0 }),
            json!(now),
        ],
    )
}

fn schema_migration(backend: StorageBackend) -> AuthStackResult<EmbeddedMigration> {
    let dialect = match backend {
        StorageBackend::Sqlite => StorageDialect::SpinSqlite,
        StorageBackend::Postgres => StorageDialect::Postgres,
    };
    migrations(dialect)
        .map_err(|error| AuthStackError::configuration(error.to_string()))?
        .first()
        .copied()
        .ok_or_else(|| AuthStackError::store("wasi-auth storage migration set is empty"))
}

async fn legacy_schema_checksum_upgrade(
    backend: StorageBackend,
) -> AuthStackResult<Option<AtomicSqlStatement>> {
    let rows = match backend {
        StorageBackend::Sqlite => {
            execute_sqlite("PRAGMA table_info(auth_schema_migrations)", Vec::new()).await?
        }
        StorageBackend::Postgres => {
            execute_postgres(
                "SELECT column_name AS name FROM information_schema.columns \
                 WHERE table_schema = current_schema() \
                   AND table_name = 'auth_schema_migrations'",
                Vec::new(),
            )
            .await?
        }
    };
    if !schema_checksum_is_missing(&rows) {
        return Ok(None);
    }

    let sql = match backend {
        StorageBackend::Sqlite => {
            "ALTER TABLE auth_schema_migrations \
             ADD COLUMN checksum TEXT NOT NULL DEFAULT 'legacy-unchecksummed'"
        }
        StorageBackend::Postgres => {
            "ALTER TABLE auth_schema_migrations \
             ADD COLUMN IF NOT EXISTS checksum TEXT NOT NULL DEFAULT 'legacy-unchecksummed'"
        }
    };
    Ok(Some(AtomicSqlStatement::execute(sql, Vec::new())))
}

fn schema_checksum_is_missing(columns: &[Value]) -> bool {
    !columns.is_empty()
        && !columns
            .iter()
            .any(|row| row_string(row, "name").as_deref() == Some("checksum"))
}

async fn storage_backend() -> AuthStackResult<StorageBackend> {
    let value = runtime_config_value("DATABASE_BACKEND")
        .await
        .unwrap_or_else(|| default_storage_backend().into());
    match value.trim().to_ascii_lowercase().as_str() {
        "sqlite" => Ok(StorageBackend::Sqlite),
        "postgres" | "postgresql" => Ok(StorageBackend::Postgres),
        other => Err(AuthStackError::configuration(format!(
            "unsupported DATABASE_BACKEND={other}; use sqlite or postgres"
        ))),
    }
}

fn default_storage_backend() -> &'static str {
    #[cfg(feature = "sqlite")]
    {
        return "sqlite";
    }
    #[cfg(all(not(feature = "sqlite"), feature = "postgres"))]
    {
        return "postgres";
    }
    #[allow(unreachable_code)]
    "sqlite"
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
    match storage_backend().await? {
        StorageBackend::Sqlite => execute_sqlite(sql, params).await,
        StorageBackend::Postgres => execute_postgres(sql, params).await,
    }
}

async fn execute_sql_atomic(
    statements: Vec<AtomicSqlStatement>,
) -> AuthStackResult<Vec<Vec<Value>>> {
    let backend = storage_backend().await?;

    #[cfg(all(runtime_spin, any(feature = "sqlite", feature = "postgres")))]
    {
        let statements = statements
            .into_iter()
            .map(|statement| {
                let sql = match backend {
                    StorageBackend::Sqlite => statement.sql,
                    StorageBackend::Postgres => postgres_sql(&statement.sql).into_owned(),
                };
                if statement.minimum_rows > 0 {
                    ddd_cqrs_es::adapters::SpinSqlStatement::guard(sql, statement.params)
                } else if statement.returns_rows {
                    ddd_cqrs_es::adapters::SpinSqlStatement::query(sql, statement.params)
                } else {
                    ddd_cqrs_es::adapters::SpinSqlStatement::execute(sql, statement.params)
                }
            })
            .collect();
        match backend {
            StorageBackend::Sqlite => {
                #[cfg(feature = "sqlite")]
                {
                    ddd_cqrs_es::adapters::execute_spin_sqlite_atomic(statements)
                        .await
                        .map_err(AuthStackError::store)
                }
                #[cfg(not(feature = "sqlite"))]
                Err(AuthStackError::configuration(
                    "sqlite storage requires the sqlite feature on Spin",
                ))
            }
            StorageBackend::Postgres => {
                #[cfg(feature = "postgres")]
                {
                    let url = database_url("postgres").await?;
                    ddd_cqrs_es::adapters::execute_spin_pg_atomic(&url, statements)
                        .await
                        .map_err(AuthStackError::store)
                }
                #[cfg(not(feature = "postgres"))]
                Err(AuthStackError::configuration(
                    "postgres storage requires the postgres feature on Spin",
                ))
            }
        }
    }

    #[cfg(not(all(runtime_spin, any(feature = "sqlite", feature = "postgres"))))]
    {
        let _ = (backend, statements);
        Err(AuthStackError::configuration(
            "atomic SQL storage requires Spin SQLite or PostgreSQL",
        ))
    }
}

async fn execute_sqlite(sql: &str, params: Vec<Value>) -> AuthStackResult<Vec<Value>> {
    #[cfg(all(feature = "sqlite", runtime_spin))]
    {
        ddd_cqrs_es::adapters::execute_spin_sqlite(sql, params)
            .await
            .map_err(AuthStackError::store)
    }

    #[cfg(not(all(feature = "sqlite", runtime_spin)))]
    {
        let _ = (sql, params);
        Err(AuthStackError::configuration(
            "sqlite storage requires the sqlite feature on Spin",
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

async fn append_storage_event(
    aggregate_type: &str,
    aggregate_id: &str,
    event_type: &str,
    payload: Value,
) -> AuthStackResult<u64> {
    let revision = next_event_revision(aggregate_type, aggregate_id).await?;
    let now = now_ms();
    let metadata = json!({
        "tenant_id": DEFAULT_TENANT_ID,
        "source": "fullstack-app",
    });
    let event_id = storage_event_id()?;
    let payload_json = serde_json::to_string(&payload)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;
    let metadata_json = serde_json::to_string(&metadata)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;

    execute_sql(
        "INSERT INTO events \
         (event_id, aggregate_id, aggregate_type, revision, event_type, event_version, payload, metadata, recorded_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7, ?8)",
        vec![
            json!(&event_id),
            json!(aggregate_id),
            json!(aggregate_type),
            json!(revision),
            json!(event_type),
            json!(payload_json),
            json!(metadata_json),
            json!(now),
        ],
    )
    .await?;

    storage_event_sequence(&event_id).await
}

fn storage_event_statement(
    aggregate_type: &str,
    aggregate_id: &str,
    event_type: &str,
    payload: Value,
) -> AuthStackResult<AtomicSqlStatement> {
    let now = now_ms();
    let event_id = storage_event_id()?;
    let payload_json = serde_json::to_string(&payload)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;
    let metadata_json = json!({
        "tenant_id": DEFAULT_TENANT_ID,
        "source": "fullstack-app",
    })
    .to_string();
    Ok(AtomicSqlStatement::execute(
        "INSERT INTO events \
         (event_id, aggregate_id, aggregate_type, revision, event_type, event_version, payload, metadata, recorded_at_ms) \
         SELECT ?1, ?2, ?3, COALESCE(MAX(revision), 0) + 1, ?4, 1, ?5, ?6, ?7 \
         FROM events WHERE aggregate_type = ?3 AND aggregate_id = ?2",
        vec![
            json!(event_id),
            json!(aggregate_id),
            json!(aggregate_type),
            json!(event_type),
            json!(payload_json),
            json!(metadata_json),
            json!(now),
        ],
    ))
}

async fn next_event_revision(aggregate_type: &str, aggregate_id: &str) -> AuthStackResult<u64> {
    let rows = execute_sql(
        "SELECT COALESCE(MAX(revision), 0) + 1 AS next_revision \
         FROM events \
         WHERE aggregate_type = ?1 AND aggregate_id = ?2",
        vec![json!(aggregate_type), json!(aggregate_id)],
    )
    .await?;
    Ok(rows
        .first()
        .and_then(|row| row_i64(row, "next_revision"))
        .unwrap_or(1)
        .max(1) as u64)
}

async fn storage_event_sequence(event_id: &str) -> AuthStackResult<u64> {
    let rows = execute_sql(
        "SELECT sequence FROM events WHERE event_id = ?1 LIMIT 1",
        vec![json!(event_id)],
    )
    .await?;
    rows.first()
        .and_then(|row| row_i64(row, "sequence"))
        .map(|sequence| sequence.max(0) as u64)
        .ok_or_else(|| AuthStackError::store("inserted storage event was not found"))
}

async fn advance_projection_checkpoint(
    projection_name: &str,
    sequence: u64,
) -> AuthStackResult<()> {
    execute_sql(
        "INSERT INTO checkpoints (projection_name, last_sequence) \
         VALUES (?1, ?2) \
         ON CONFLICT(projection_name) DO UPDATE SET \
         last_sequence = CASE \
             WHEN excluded.last_sequence > checkpoints.last_sequence \
             THEN excluded.last_sequence \
             ELSE checkpoints.last_sequence \
         END",
        vec![json!(projection_name), json!(sequence)],
    )
    .await?;
    Ok(())
}

pub async fn storage_status() -> AuthStackResult<StorageStatusResponse> {
    initialize_schema_async().await?;
    let summary_rows = execute_sql(
        "SELECT COUNT(*) AS event_count, COALESCE(MAX(sequence), 0) AS latest_sequence FROM events",
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
        "SELECT event_type, COUNT(*) AS count \
         FROM events \
         GROUP BY event_type \
         ORDER BY event_type ASC",
        Vec::new(),
    )
    .await?
    .into_iter()
    .map(storage_event_count_from_row)
    .collect::<AuthStackResult<Vec<_>>>()?;

    let checkpoints = execute_sql(
        "SELECT projection_name, last_sequence \
         FROM checkpoints \
         ORDER BY projection_name ASC",
        Vec::new(),
    )
    .await?
    .into_iter()
    .map(storage_checkpoint_from_row)
    .collect::<AuthStackResult<Vec<_>>>()?;

    Ok(StorageStatusResponse {
        event_count,
        latest_sequence,
        event_types,
        checkpoints,
    })
}

fn storage_event_count_from_row(row: Value) -> AuthStackResult<StorageEventTypeCount> {
    Ok(StorageEventTypeCount {
        event_type: required_string(&row, "event_type")?,
        count: row_i64(&row, "count").unwrap_or_default().max(0) as u64,
    })
}

fn storage_checkpoint_from_row(row: Value) -> AuthStackResult<StorageProjectionCheckpoint> {
    Ok(StorageProjectionCheckpoint {
        projection_name: required_string(&row, "projection_name")?,
        last_sequence: row_i64(&row, "last_sequence").unwrap_or_default().max(0) as u64,
    })
}

pub async fn catch_up_storage_projections(
    batch_limit: Option<usize>,
) -> AuthStackResult<Vec<StorageProjectionRunResponse>> {
    initialize_schema_async().await?;
    let batch_limit = batch_limit
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_STORAGE_PROJECTION_BATCH_LIMIT);

    Ok(vec![
        catch_up_storage_projection(StorageProjectionKind::Auth, batch_limit).await?,
    ])
}

async fn catch_up_storage_projection(
    kind: StorageProjectionKind,
    batch_limit: usize,
) -> AuthStackResult<StorageProjectionRunResponse> {
    let projection_name = kind.checkpoint_name();
    let before = load_projection_checkpoint(projection_name).await?;
    let events = load_storage_projection_events(kind, before, batch_limit).await?;
    let events_scanned = events.len() as u64;
    let mut events_applied = 0_u64;
    let mut last_sequence = before;

    for event in events {
        let applied = apply_auth_storage_event(&event).await?;
        if applied {
            events_applied = events_applied.saturating_add(1);
        }
        last_sequence = event.sequence;
        advance_projection_checkpoint(projection_name, last_sequence).await?;
    }

    Ok(StorageProjectionRunResponse {
        projection_name: projection_name.to_string(),
        last_sequence_before: before,
        last_sequence_after: last_sequence,
        events_scanned,
        events_applied,
        events_skipped: events_scanned.saturating_sub(events_applied),
    })
}

async fn load_projection_checkpoint(projection_name: &str) -> AuthStackResult<u64> {
    let rows = execute_sql(
        "SELECT last_sequence FROM checkpoints WHERE projection_name = ?1 LIMIT 1",
        vec![json!(projection_name)],
    )
    .await?;
    Ok(rows
        .first()
        .and_then(|row| row_i64(row, "last_sequence"))
        .unwrap_or_default()
        .max(0) as u64)
}

async fn load_storage_projection_events(
    kind: StorageProjectionKind,
    after_sequence: u64,
    batch_limit: usize,
) -> AuthStackResult<Vec<StoredStorageEvent>> {
    let filter = match kind {
        StorageProjectionKind::Auth => "aggregate_type NOT LIKE 'authz_%'",
    };
    let limit = i64::try_from(batch_limit)
        .map_err(|_| AuthStackError::validation("projection batch limit is too large"))?;
    let sql = format!(
        "SELECT sequence, aggregate_type, event_type, payload, recorded_at_ms \
         FROM events \
         WHERE sequence > ?1 AND {filter} \
         ORDER BY sequence ASC \
         LIMIT ?2"
    );
    let rows = execute_sql(&sql, vec![json!(after_sequence), json!(limit)]).await?;
    rows.into_iter().map(storage_event_from_row).collect()
}

async fn apply_auth_storage_event(event: &StoredStorageEvent) -> AuthStackResult<bool> {
    let payload = &event.payload;
    match event.event_type.as_str() {
        "auth_provider_config_saved" => {
            let provider_id = required_payload_string(payload, "provider_id")?;
            let enabled = required_payload_bool(payload, "enabled")?;
            let display_name = payload_string(payload, "display_name")
                .unwrap_or_else(|| provider_display_name(&provider_id));
            let login_url = payload_string(payload, "login_url")
                .unwrap_or_else(|| format!("/api/auth/oauth/{provider_id}/start"));
            upsert_auth_provider_config_unchecked(
                &provider_id,
                enabled,
                &display_name,
                &login_url,
                event.recorded_at_ms,
            )
            .await?;
            Ok(true)
        }
        "auth_redirect_allowlist_saved" => {
            let redirects_json = required_payload_string(payload, "redirects_json")?;
            execute_sql(
                "INSERT INTO auth_redirect_allowlists \
                 (tenant_id, redirects_json, created_at_ms, updated_at_ms) \
                 VALUES (?1, ?2, ?3, ?3) \
                 ON CONFLICT(tenant_id) DO UPDATE SET \
                 redirects_json = excluded.redirects_json, \
                 updated_at_ms = excluded.updated_at_ms",
                vec![
                    json!(DEFAULT_TENANT_ID),
                    json!(redirects_json),
                    json!(event.recorded_at_ms),
                ],
            )
            .await?;
            Ok(true)
        }
        "auth_password_user_registered" => {
            let email = normalize_email(&required_payload_string(payload, "email")?);
            let user_id = required_payload_string(payload, "user_id")?;
            upsert_user_email_identity(&email, &user_id, event.recorded_at_ms).await?;
            Ok(true)
        }
        "auth_password_login_succeeded" => {
            let user_id = required_payload_string(payload, "user_id")?;
            execute_sql(
                "UPDATE auth_password_credentials \
                 SET last_authenticated_at_ms = ?1, updated_at_ms = ?1 \
                 WHERE tenant_id = ?2 AND user_id = ?3",
                vec![
                    json!(event.recorded_at_ms),
                    json!(DEFAULT_TENANT_ID),
                    json!(user_id),
                ],
            )
            .await?;
            Ok(true)
        }
        "auth_email_verified" => {
            let user_id = required_payload_string(payload, "user_id")?;
            mark_user_email_verified(&user_id, event.recorded_at_ms).await?;
            Ok(true)
        }
        "auth_email_verification_started" => Ok(false),
        "auth_password_reset_started" | "auth_oauth_state_created" => Ok(false),
        "auth_password_reset_completed" => {
            let grant_id = required_payload_string(payload, "grant_id")?;
            mark_token_grant_consumed(&grant_id, event.recorded_at_ms).await?;
            Ok(true)
        }
        "auth_oauth_state_consumed" | "auth_passkey_challenge_consumed" => {
            let grant_id = payload_string(payload, "state")
                .or_else(|| payload_string(payload, "challenge_id"))
                .ok_or_else(|| {
                    AuthStackError::store(format!(
                        "{} event is missing state/challenge_id",
                        event.event_type
                    ))
                })?;
            mark_token_grant_consumed(&grant_id, event.recorded_at_ms).await?;
            Ok(true)
        }
        "auth_passkey_registration_started"
        | "auth_passkey_login_started"
        | "auth_passkey_credential_upserted" => Ok(false),
        "auth_external_identity_linked" => {
            let provider_id = required_payload_string(payload, "provider_id")?;
            let provider_subject = required_payload_string(payload, "provider_subject")?;
            let user_id = required_payload_string(payload, "user_id")?;
            let primary_email = payload_string(payload, "primary_email");
            let profile_json =
                payload_string(payload, "profile_json").unwrap_or_else(|| "{}".to_string());
            if let Some(email) = primary_email.as_deref() {
                upsert_user_email_identity(email, &user_id, event.recorded_at_ms).await?;
            }
            if required_payload_bool(payload, "email_verified").unwrap_or(false) {
                mark_user_email_verified(&user_id, event.recorded_at_ms).await?;
            }
            execute_sql(
                "INSERT INTO auth_external_identities \
                 (tenant_id, provider_id, provider_subject, user_id, primary_email, profile_json, created_at_ms, updated_at_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7) \
                 ON CONFLICT(tenant_id, provider_id, provider_subject) DO UPDATE SET \
                 user_id = excluded.user_id, \
                 primary_email = excluded.primary_email, \
                 profile_json = excluded.profile_json, \
                 updated_at_ms = excluded.updated_at_ms",
                vec![
                    json!(DEFAULT_TENANT_ID),
                    json!(provider_id),
                    json!(provider_subject),
                    json!(user_id),
                    json!(primary_email),
                    json!(profile_json),
                    json!(event.recorded_at_ms),
                ],
            )
            .await?;
            Ok(true)
        }
        "auth_session_issued" => {
            let session_id = required_payload_string(payload, "session_id")?;
            let user_id = required_payload_string(payload, "user_id")?;
            let tenant_id = payload_string(payload, "tenant_id")
                .unwrap_or_else(|| DEFAULT_TENANT_ID.to_string());
            let primary_email = payload_string(payload, "primary_email");
            let expires_at_ms = required_payload_u64(payload, "expires_at_ms")?;
            let assurance = projected_session_assurance(payload)?;
            let permissions_json =
                payload_string(payload, "permissions_json").unwrap_or_else(|| {
                    payload
                        .get("permissions")
                        .cloned()
                        .unwrap_or(Value::Array(Vec::new()))
                        .to_string()
                });
            if let Some(email) = primary_email.as_deref() {
                upsert_user_email_identity(email, &user_id, event.recorded_at_ms).await?;
            }
            execute_sql(
                PROJECT_SESSION_UPSERT_SQL,
                vec![
                    json!(session_id),
                    json!(tenant_id),
                    json!(user_id),
                    json!(primary_email),
                    json!(assurance),
                    json!(permissions_json),
                    json!(event.recorded_at_ms),
                    json!(expires_at_ms),
                ],
            )
            .await?;
            Ok(true)
        }
        "auth_refresh_token_rotated" => {
            let session_id = required_payload_string(payload, "session_id")?;
            execute_sql(
                "UPDATE auth_sessions SET updated_at_ms = ?1 WHERE session_id = ?2",
                vec![json!(event.recorded_at_ms), json!(session_id)],
            )
            .await?;
            Ok(true)
        }
        "auth_session_revoked" => {
            let session_id = required_payload_string(payload, "session_id")?;
            execute_sql(
                "UPDATE auth_sessions \
                 SET revoked_at_ms = ?1, updated_at_ms = ?1 \
                 WHERE session_id = ?2 AND revoked_at_ms IS NULL",
                vec![json!(event.recorded_at_ms), json!(&session_id)],
            )
            .await?;
            execute_sql(
                "UPDATE auth_refresh_token_hashes \
                 SET revoked_at_ms = ?1 \
                 WHERE tenant_id = ?2 AND session_id = ?3 AND revoked_at_ms IS NULL",
                vec![
                    json!(event.recorded_at_ms),
                    json!(DEFAULT_TENANT_ID),
                    json!(session_id),
                ],
            )
            .await?;
            Ok(true)
        }
        "auth_signing_key_rotated" => {
            let kid = required_payload_string(payload, "kid")?;
            if required_payload_bool(payload, "retired_previous").unwrap_or(false)
                && let Some(previous_kid) = payload_string(payload, "previous_kid")
            {
                upsert_signing_key_state(
                    &previous_kid,
                    SigningKeyStatus::Retired,
                    Some(event.recorded_at_ms),
                    Some(event.recorded_at_ms),
                    None,
                )
                .await?;
            }
            upsert_signing_key_state(
                &kid,
                SigningKeyStatus::Active,
                Some(event.recorded_at_ms),
                None,
                None,
            )
            .await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn projected_session_assurance(payload: &Value) -> AuthStackResult<String> {
    let assurance = payload_string(payload, "assurance").unwrap_or_else(|| "aal1".into());
    if matches!(assurance.as_str(), "aal1" | "aal2") {
        Ok(assurance)
    } else {
        Err(AuthStackError::store(
            "auth_session_issued event contains invalid assurance",
        ))
    }
}

async fn mark_token_grant_consumed(grant_id: &str, consumed_at_ms: u64) -> AuthStackResult<()> {
    execute_sql(
        "UPDATE auth_token_grants \
         SET consumed_at_ms = ?1 \
         WHERE tenant_id = ?2 AND grant_id = ?3 AND consumed_at_ms IS NULL",
        vec![
            json!(consumed_at_ms),
            json!(DEFAULT_TENANT_ID),
            json!(grant_id),
        ],
    )
    .await?;
    Ok(())
}

fn storage_event_from_row(row: Value) -> AuthStackResult<StoredStorageEvent> {
    let payload_json = required_string(&row, "payload")?;
    let payload = serde_json::from_str(&payload_json).map_err(|error| {
        AuthStackError::store(format!("stored event payload is invalid: {error}"))
    })?;

    Ok(StoredStorageEvent {
        sequence: row_i64(&row, "sequence").unwrap_or_default().max(0) as u64,
        _aggregate_type: required_string(&row, "aggregate_type")?,
        event_type: required_string(&row, "event_type")?,
        payload,
        recorded_at_ms: row_i64(&row, "recorded_at_ms").unwrap_or_default().max(0) as u64,
    })
}

fn payload_string(payload: &Value, key: &str) -> Option<String> {
    row_string(payload, key).filter(|value| !value.trim().is_empty())
}

fn required_payload_string(payload: &Value, key: &str) -> AuthStackResult<String> {
    payload_string(payload, key)
        .ok_or_else(|| AuthStackError::store(format!("event payload is missing '{key}'")))
}

fn required_payload_bool(payload: &Value, key: &str) -> AuthStackResult<bool> {
    payload
        .get(key)
        .and_then(|value| match value {
            Value::Bool(value) => Some(*value),
            Value::Number(value) => value.as_i64().map(|number| number != 0),
            Value::String(value) => Some(truthy(value)),
            _ => None,
        })
        .ok_or_else(|| AuthStackError::store(format!("event payload is missing boolean '{key}'")))
}

fn required_payload_u64(payload: &Value, key: &str) -> AuthStackResult<u64> {
    payload
        .get(key)
        .and_then(|value| match value {
            Value::Number(value) => value
                .as_u64()
                .or_else(|| value.as_i64().map(|n| n.max(0) as u64)),
            Value::String(value) => value.parse::<u64>().ok(),
            _ => None,
        })
        .ok_or_else(|| AuthStackError::store(format!("event payload is missing integer '{key}'")))
}

fn provider_from_row(row: Value) -> AuthStackResult<AuthProviderSummary> {
    let provider_id = required_string(&row, "provider_id")?;
    Ok(AuthProviderSummary {
        display_name: required_string(&row, "display_name")?,
        login_url: required_string(&row, "login_url")?,
        enabled: row_bool(&row, "enabled").unwrap_or(false),
        provider_id,
    })
}

fn jwks_key_from_row(row: Value) -> AuthStackResult<JwksKey> {
    let public_parameters_json =
        row_string(&row, "public_parameters_json").unwrap_or_else(|| "{}".to_string());
    let public_parameters = serde_json::from_str(&public_parameters_json).map_err(|error| {
        AuthStackError::store(format!(
            "stored JWKS public parameters are invalid: {error}"
        ))
    })?;

    Ok(JwksKey {
        kid: required_string(&row, "kid")?,
        kty: required_string(&row, "kty")?,
        alg: required_string(&row, "alg")?,
        use_: required_string(&row, "use_value")?,
        public_parameters,
    })
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

fn row_bool(row: &Value, key: &str) -> Option<bool> {
    row.get(key).and_then(|value| match value {
        Value::Bool(value) => Some(*value),
        Value::Number(value) => value.as_i64().map(|number| number != 0),
        _ => None,
    })
}

fn row_i64(row: &Value, key: &str) -> Option<i64> {
    row.get(key).and_then(Value::as_i64)
}

async fn upsert_user_email_identity(email: &str, user_id: &str, now: u64) -> AuthStackResult<()> {
    execute_sql_atomic(user_email_identity_statements(email, user_id, now)).await?;
    Ok(())
}

fn user_email_identity_statements(email: &str, user_id: &str, now: u64) -> Vec<AtomicSqlStatement> {
    vec![
        AtomicSqlStatement::execute(
            "INSERT INTO auth_users \
         (user_id, tenant_id, primary_email, disabled, created_at_ms, updated_at_ms) \
         VALUES (?1, ?2, ?3, 0, ?4, ?4) \
         ON CONFLICT(user_id) DO UPDATE SET \
         primary_email = excluded.primary_email, \
         updated_at_ms = excluded.updated_at_ms",
            vec![
                json!(user_id),
                json!(DEFAULT_TENANT_ID),
                json!(email),
                json!(now),
            ],
        ),
        AtomicSqlStatement::execute(
            "INSERT INTO auth_users_by_email \
         (tenant_id, normalized_email, user_id) \
         VALUES (?1, ?2, ?3) \
         ON CONFLICT(tenant_id, normalized_email) DO UPDATE SET \
         user_id = excluded.user_id",
            vec![json!(DEFAULT_TENANT_ID), json!(email), json!(user_id)],
        ),
    ]
}

async fn mark_user_email_verified(user_id: &str, now: u64) -> AuthStackResult<()> {
    execute_sql_atomic(vec![mark_user_email_verified_statement(user_id, now)]).await?;
    Ok(())
}

fn mark_user_email_verified_statement(user_id: &str, now: u64) -> AtomicSqlStatement {
    AtomicSqlStatement::execute(
        "UPDATE auth_users SET email_verified = 1, updated_at_ms = ?1 \
         WHERE tenant_id = ?2 AND user_id = ?3",
        vec![json!(now), json!(DEFAULT_TENANT_ID), json!(user_id)],
    )
}

fn required_passkey_email(email: Option<String>, flow: &str) -> AuthStackResult<String> {
    let Some(email) = email else {
        return Err(AuthStackError::validation(format!(
            "email is required for passkey {flow}"
        )));
    };
    let email = normalize_email(&email);
    if email.is_empty() {
        return Err(AuthStackError::validation(format!(
            "email is required for passkey {flow}"
        )));
    }
    Ok(email)
}

fn safe_redirect_or_stored(redirect_url: Option<String>, stored_redirect_url: &str) -> String {
    redirect_url
        .filter(|value| is_safe_redirect(value))
        .unwrap_or_else(|| stored_redirect_url.to_string())
}

fn is_safe_redirect(value: &str) -> bool {
    value.starts_with('/') && !value.starts_with("//")
}

async fn load_passkey_credentials_for_user(
    user_id: &str,
) -> AuthStackResult<Vec<WebauthnPasskeyCredential>> {
    let rows = execute_sql(
        "SELECT public_key_json \
         FROM auth_passkey_credentials \
         WHERE tenant_id = ?1 AND user_id = ?2 \
         ORDER BY created_at_ms ASC",
        vec![json!(DEFAULT_TENANT_ID), json!(user_id)],
    )
    .await?;

    rows.into_iter()
        .map(|row| {
            serde_json::from_str::<WebauthnPasskeyCredential>(&required_string(
                &row,
                "public_key_json",
            )?)
            .map_err(|error| {
                AuthStackError::store(format!("stored passkey credential is invalid: {error}"))
            })
        })
        .collect()
}

async fn load_passkey_credential_for_user(
    user_id: &str,
    credential_id: &WebauthnCredentialId,
) -> AuthStackResult<WebauthnPasskeyCredential> {
    let credential_id = credential_id.to_b64url();
    let rows = execute_sql(
        "SELECT public_key_json \
         FROM auth_passkey_credentials \
         WHERE tenant_id = ?1 AND user_id = ?2 AND credential_id = ?3 \
         LIMIT 1",
        vec![
            json!(DEFAULT_TENANT_ID),
            json!(user_id),
            json!(credential_id),
        ],
    )
    .await?;
    let Some(row) = rows.into_iter().next() else {
        return Err(AuthStackError::InvalidCredentials);
    };

    serde_json::from_str::<WebauthnPasskeyCredential>(&required_string(&row, "public_key_json")?)
        .map_err(|error| {
            AuthStackError::store(format!("stored passkey credential is invalid: {error}"))
        })
}

async fn persist_passkey_credential(
    user_id: &str,
    credential: &WebauthnPasskeyCredential,
) -> AuthStackResult<()> {
    let credential_id = credential.id.to_b64url();
    if let Some(existing_user_id) = passkey_credential_owner(&credential_id).await?
        && existing_user_id != user_id
    {
        return Err(AuthStackError::conflict(
            "passkey credential is already registered to another user",
        ));
    }

    let now = now_ms();
    let public_key_json = serde_json::to_string(credential)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;
    let transports_json = serde_json::to_string(&credential.transports)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;
    execute_sql_atomic(vec![
        AtomicSqlStatement::execute(
            "INSERT INTO auth_passkey_credentials \
             (tenant_id, credential_id, user_id, public_key_json, transports_json, sign_count, created_at_ms, updated_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7) \
             ON CONFLICT(tenant_id, credential_id) DO UPDATE SET \
             public_key_json = excluded.public_key_json, \
             transports_json = excluded.transports_json, \
             sign_count = excluded.sign_count, \
             updated_at_ms = excluded.updated_at_ms",
            vec![
                json!(DEFAULT_TENANT_ID),
                json!(&credential_id),
                json!(user_id),
                json!(&public_key_json),
                json!(&transports_json),
                json!(credential.counter),
                json!(now),
            ],
        ),
        storage_event_statement(
            "auth_passkey_credential",
            &credential_id,
            "auth_passkey_credential_upserted",
            json!({
                "tenant_id": DEFAULT_TENANT_ID,
                "credential_id": &credential_id,
                "user_id": user_id,
                "credential_version": now,
            }),
        )?,
    ])
    .await?;
    Ok(())
}

async fn passkey_credential_owner(credential_id: &str) -> AuthStackResult<Option<String>> {
    let rows = execute_sql(
        "SELECT user_id \
         FROM auth_passkey_credentials \
         WHERE tenant_id = ?1 AND credential_id = ?2 \
         LIMIT 1",
        vec![json!(DEFAULT_TENANT_ID), json!(credential_id)],
    )
    .await?;
    Ok(rows
        .into_iter()
        .next()
        .and_then(|row| row_string(&row, "user_id")))
}

async fn password_credential_for_email(
    email: &str,
) -> AuthStackResult<Option<PasswordCredentialRecord>> {
    let rows = execute_sql(
        "SELECT users.user_id, users.primary_email, users.disabled, users.email_verified, credentials.password_hash, credentials.revoked_at_ms \
         FROM auth_users_by_email emails \
         JOIN auth_users users ON users.user_id = emails.user_id AND users.tenant_id = emails.tenant_id \
         JOIN auth_password_credentials credentials ON credentials.user_id = users.user_id AND credentials.tenant_id = users.tenant_id \
         WHERE emails.tenant_id = ?1 AND emails.normalized_email = ?2 \
         LIMIT 1",
        vec![json!(DEFAULT_TENANT_ID), json!(email)],
    )
    .await?;

    rows.into_iter()
        .next()
        .map(password_credential_from_row)
        .transpose()
}

fn password_credential_from_row(row: Value) -> AuthStackResult<PasswordCredentialRecord> {
    Ok(PasswordCredentialRecord {
        user_id: required_string(&row, "user_id")?,
        primary_email: required_string(&row, "primary_email")?,
        disabled: row_bool(&row, "disabled").unwrap_or(false),
        email_verified: row_bool(&row, "email_verified").unwrap_or(false),
        password_hash: required_string(&row, "password_hash")?,
        revoked_at_ms: row_i64(&row, "revoked_at_ms"),
    })
}

async fn hash_password(password: &str) -> AuthStackResult<String> {
    match password_kdf_algorithm().await?.as_str() {
        "argon2id" => hash_password_argon2id(password).await,
        PASSWORD_HASH_ALGORITHM => {
            hash_password_pbkdf2(password, password_pbkdf2_iterations().await?)
        }
        algorithm => Err(AuthStackError::configuration(format!(
            "unsupported AUTH_PASSWORD_KDF '{algorithm}'"
        ))),
    }
}

async fn hash_password_argon2id(password: &str) -> AuthStackResult<String> {
    let salt = random_bytes(PASSWORD_SALT_BYTES)?;
    let params = password_argon2_params().await?;
    let mut output = [0_u8; PASSWORD_HASH_BYTES];
    let argon2 = Argon2::new(Argon2Algorithm::Argon2id, Argon2Version::V0x13, params);
    argon2
        .hash_password_into(password.as_bytes(), &salt, &mut output)
        .map_err(|error| AuthStackError::store(format!("failed to hash password: {error}")))?;

    let params = password_argon2_params().await?;
    Ok(format!(
        "argon2id$m={},t={},p={}${}${}",
        params.m_cost(),
        params.t_cost(),
        params.p_cost(),
        URL_SAFE_NO_PAD.encode(salt),
        URL_SAFE_NO_PAD.encode(output)
    ))
}

fn hash_password_pbkdf2(password: &str, iterations: u32) -> AuthStackResult<String> {
    let salt = random_bytes(PASSWORD_SALT_BYTES)?;
    let mut output = [0_u8; PASSWORD_HASH_BYTES];
    pbkdf2::<Hmac<Sha256>>(password.as_bytes(), &salt, iterations, &mut output)
        .map_err(|error| AuthStackError::store(format!("failed to hash password: {error}")))?;

    Ok(format!(
        "{PASSWORD_HASH_ALGORITHM}${iterations}${}${}",
        URL_SAFE_NO_PAD.encode(salt),
        URL_SAFE_NO_PAD.encode(output)
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PasswordVerification {
    Invalid,
    ValidCurrent,
    ValidNeedsRehash,
}

impl PasswordVerification {
    fn from_match(matches: bool, needs_rehash: bool) -> Self {
        if !matches {
            Self::Invalid
        } else if needs_rehash {
            Self::ValidNeedsRehash
        } else {
            Self::ValidCurrent
        }
    }
}

async fn verify_password(
    password: &str,
    stored_hash: &str,
) -> AuthStackResult<PasswordVerification> {
    if stored_hash.starts_with("argon2id$") {
        return verify_password_argon2id(password, stored_hash).await;
    }

    let parts = stored_hash.split('$').collect::<Vec<_>>();
    if parts.len() != 4 || parts[0] != PASSWORD_HASH_ALGORITHM {
        return Err(AuthStackError::store(
            "stored password hash format is invalid",
        ));
    }
    let iterations = parts[1].parse::<u32>().map_err(|error| {
        AuthStackError::store(format!("stored password iterations are invalid: {error}"))
    })?;
    let salt = URL_SAFE_NO_PAD.decode(parts[2]).map_err(|error| {
        AuthStackError::store(format!("stored password salt is invalid: {error}"))
    })?;
    let expected = URL_SAFE_NO_PAD.decode(parts[3]).map_err(|error| {
        AuthStackError::store(format!("stored password hash is invalid: {error}"))
    })?;
    let mut candidate = vec![0_u8; expected.len()];
    pbkdf2::<Hmac<Sha256>>(password.as_bytes(), &salt, iterations, &mut candidate)
        .map_err(|error| AuthStackError::store(format!("failed to verify password: {error}")))?;

    let current_algorithm = password_kdf_algorithm().await?;
    let current_iterations = password_pbkdf2_iterations().await?;
    Ok(PasswordVerification::from_match(
        constant_time_eq(&candidate, &expected),
        current_algorithm != PASSWORD_HASH_ALGORITHM || iterations < current_iterations,
    ))
}

async fn consume_dummy_password_verification(password: &str) -> AuthStackResult<()> {
    let _ = verify_password(password, DUMMY_PASSWORD_HASH).await?;
    Ok(())
}

async fn verify_password_argon2id(
    password: &str,
    stored_hash: &str,
) -> AuthStackResult<PasswordVerification> {
    let parts = stored_hash.split('$').collect::<Vec<_>>();
    if parts.len() != 4 || parts[0] != "argon2id" {
        return Err(AuthStackError::store(
            "stored password hash format is invalid",
        ));
    }
    let (memory_kib, iterations, parallelism) = parse_argon2_param_part(parts[1])?;
    let salt = URL_SAFE_NO_PAD.decode(parts[2]).map_err(|error| {
        AuthStackError::store(format!("stored password salt is invalid: {error}"))
    })?;
    let expected = URL_SAFE_NO_PAD.decode(parts[3]).map_err(|error| {
        AuthStackError::store(format!("stored password hash is invalid: {error}"))
    })?;
    let params = Argon2Params::new(memory_kib, iterations, parallelism, Some(expected.len()))
        .map_err(|error| {
            AuthStackError::store(format!("stored Argon2 parameters are invalid: {error}"))
        })?;
    let argon2 = Argon2::new(Argon2Algorithm::Argon2id, Argon2Version::V0x13, params);
    let mut candidate = vec![0_u8; expected.len()];
    argon2
        .hash_password_into(password.as_bytes(), &salt, &mut candidate)
        .map_err(|error| AuthStackError::store(format!("failed to verify password: {error}")))?;

    let current_algorithm = password_kdf_algorithm().await?;
    let current_params = password_argon2_params().await?;
    let needs_rehash = current_algorithm != "argon2id"
        || memory_kib < current_params.m_cost()
        || iterations < current_params.t_cost()
        || parallelism != current_params.p_cost();
    Ok(PasswordVerification::from_match(
        constant_time_eq(&candidate, &expected),
        needs_rehash,
    ))
}

fn parse_argon2_param_part(value: &str) -> AuthStackResult<(u32, u32, u32)> {
    let mut memory_kib = None;
    let mut iterations = None;
    let mut parallelism = None;
    for part in value.split(',') {
        let (key, raw_value) = part
            .split_once('=')
            .ok_or_else(|| AuthStackError::store("stored Argon2 parameters are invalid"))?;
        let parsed = raw_value.parse::<u32>().map_err(|error| {
            AuthStackError::store(format!("stored Argon2 parameter is invalid: {error}"))
        })?;
        match key {
            "m" => memory_kib = Some(parsed),
            "t" => iterations = Some(parsed),
            "p" => parallelism = Some(parsed),
            _ => {}
        }
    }

    Ok((
        memory_kib.ok_or_else(|| AuthStackError::store("stored Argon2 memory cost is missing"))?,
        iterations.ok_or_else(|| AuthStackError::store("stored Argon2 iterations are missing"))?,
        parallelism.ok_or_else(|| AuthStackError::store("stored Argon2 parallelism is missing"))?,
    ))
}

async fn password_kdf_algorithm() -> AuthStackResult<String> {
    let algorithm = store_config_value("AUTH_PASSWORD_KDF")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_PASSWORD_KDF.to_string())
        .trim()
        .to_ascii_lowercase();
    match algorithm.as_str() {
        "argon2id" | PASSWORD_HASH_ALGORITHM => Ok(algorithm),
        _ => Err(AuthStackError::configuration(format!(
            "AUTH_PASSWORD_KDF must be 'argon2id' or '{PASSWORD_HASH_ALGORITHM}'"
        ))),
    }
}

async fn password_argon2_params() -> AuthStackResult<Argon2Params> {
    let memory_kib = config_u32(
        "AUTH_PASSWORD_ARGON2_MEMORY_KIB",
        DEFAULT_PASSWORD_ARGON2_MEMORY_KIB,
    )
    .await?;
    let iterations = config_u32(
        "AUTH_PASSWORD_ARGON2_ITERATIONS",
        DEFAULT_PASSWORD_ARGON2_ITERATIONS,
    )
    .await?;
    let parallelism = config_u32(
        "AUTH_PASSWORD_ARGON2_PARALLELISM",
        DEFAULT_PASSWORD_ARGON2_PARALLELISM,
    )
    .await?;
    Argon2Params::new(
        memory_kib,
        iterations,
        parallelism,
        Some(PASSWORD_HASH_BYTES),
    )
    .map_err(|error| {
        AuthStackError::configuration(format!("Argon2 password KDF policy is invalid: {error}"))
    })
}

async fn password_pbkdf2_iterations() -> AuthStackResult<u32> {
    let iterations = config_u32(
        "AUTH_PASSWORD_PBKDF2_ITERATIONS",
        DEFAULT_PASSWORD_PBKDF2_ITERATIONS,
    )
    .await?;
    if config_bool(AUTH_PRODUCTION_MODE, false).await
        && password_kdf_algorithm().await? == PASSWORD_HASH_ALGORITHM
        && iterations < MIN_PRODUCTION_PASSWORD_PBKDF2_ITERATIONS
    {
        return Err(AuthStackError::configuration(format!(
            "{AUTH_PRODUCTION_MODE}=true requires AUTH_PASSWORD_PBKDF2_ITERATIONS >= {MIN_PRODUCTION_PASSWORD_PBKDF2_ITERATIONS}"
        )));
    }
    Ok(iterations)
}

async fn config_u32(name: &str, default: u32) -> AuthStackResult<u32> {
    let Some(value) = store_config_value(name)
        .await
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(default);
    };
    value.trim().parse::<u32>().map_err(|error| {
        AuthStackError::configuration(format!("{name} must be a positive integer: {error}"))
    })
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |diff, (left, right)| diff | (*left ^ *right))
        == 0
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

async fn load_active_session(session_id: Option<&str>) -> AuthStackResult<StoredSession> {
    let Some(session_id) = normalized_session_id(session_id) else {
        return Err(AuthStackError::AuthRequired);
    };
    let Some(session) = load_session_row(&session_id).await? else {
        return Err(AuthStackError::AuthRequired);
    };
    if session.revoked_at_ms.is_some() {
        return Err(AuthStackError::AuthRequired);
    }
    if session.expires_at_ms < now_ms() as i64 {
        return Err(AuthStackError::SessionExpired);
    }
    refresh_authoritative_session(session).await
}

async fn refresh_authoritative_session(
    mut session: StoredSession,
) -> AuthStackResult<StoredSession> {
    let user_rows = execute_sql(
        "SELECT primary_email, disabled, email_verified FROM auth_users \
         WHERE tenant_id = ?1 AND user_id = ?2 LIMIT 1",
        vec![json!(DEFAULT_TENANT_ID), json!(&session.user_id)],
    )
    .await?;
    let user = user_rows.first().ok_or(AuthStackError::AuthRequired)?;
    if row_bool(user, "disabled").unwrap_or(true)
        || !row_bool(user, "email_verified").unwrap_or(false)
    {
        return Err(AuthStackError::AuthRequired);
    }
    let email = row_string(user, "primary_email")
        .or_else(|| session.primary_email.clone())
        .ok_or(AuthStackError::AuthRequired)?;

    let mut permissions = default_session_permissions();
    if session.tenant_id != DEFAULT_TENANT_ID {
        permissions.extend(
            organization_for_user(&session.tenant_id, &session.user_id)
                .await?
                .permissions,
        );
    }
    if bootstrap_admin_email(&email).await {
        permissions.extend(
            [
                "auth:provider:write",
                "auth:redirect:write",
                "auth:signing-key:admin",
                "auth:storage:admin",
                "system.user.manage",
                "system.provider.manage",
                "system.signing-key.manage",
                "system.policy.manage",
                "system.health.read",
            ]
            .into_iter()
            .map(ToOwned::to_owned),
        );
    }
    permissions.sort();
    permissions.dedup();
    session.primary_email = Some(email);
    session.permissions = permissions;
    Ok(session)
}

async fn load_refresh_token_row(token_hash: &str) -> AuthStackResult<StoredRefreshToken> {
    let rows = execute_sql(
        "SELECT token_hash, session_id, expires_at_ms, rotated_at_ms, revoked_at_ms \
         FROM auth_refresh_token_hashes \
         WHERE tenant_id = ?1 AND token_hash = ?2 \
         LIMIT 1",
        vec![json!(DEFAULT_TENANT_ID), json!(token_hash)],
    )
    .await?;
    rows.into_iter()
        .next()
        .map(stored_refresh_token_from_row)
        .transpose()?
        .ok_or(AuthStackError::InvalidToken)
}

async fn load_session_row(session_id: &str) -> AuthStackResult<Option<StoredSession>> {
    let rows = execute_sql(
        "SELECT session_id, tenant_id, user_id, primary_email, expires_at_ms, revoked_at_ms, permissions_json, assurance, created_at_ms \
         FROM auth_sessions \
         WHERE session_id = ?1 \
         LIMIT 1",
        vec![json!(session_id)],
    )
    .await?;

    rows.into_iter()
        .next()
        .map(stored_session_from_row)
        .transpose()
}

async fn issue_access_token_for_session(
    session_id: &str,
    user_id: &str,
    organization_id: &str,
    _email: &str,
    session_expires_at_ms: u64,
    permissions: &[String],
    access_token_ttl_seconds: u64,
) -> AuthStackResult<String> {
    let key_config = active_signing_key_config().await?;
    let encoding_key = jwt_encoding_key_for_config(&key_config).await?;
    let issued_at = now_ms() / 1000;
    let session_expires_at = session_expires_at_ms / 1000;
    let token_expires_at = issued_at
        .saturating_add(access_token_ttl_seconds)
        .min(session_expires_at);
    let mut claims = AccessTokenClaims::for_user(
        jwt_issuer().await,
        UserId::from(user_id.to_string()),
        vec![jwt_audience().await],
        token_expires_at,
        issued_at,
        secure_storage_id("jwt")?,
    );
    claims.tenant_id = Some(TenantId::from(organization_id.to_string()));
    claims.session_id = Some(SessionId::from(session_id.to_string()));
    claims.scope = permissions.to_vec();

    encode_access_token(
        &claims,
        &encoding_key,
        key_config.algorithm,
        Some(&key_config.kid),
    )
    .map_err(map_auth_error)
}

fn opaque_refresh_token() -> AuthStackResult<String> {
    Ok(format!(
        "refresh_{}",
        URL_SAFE_NO_PAD.encode(random_bytes(32)?)
    ))
}

fn refresh_token_hash(refresh_token: &str) -> String {
    one_time_token_hash(refresh_token)
}

fn one_time_token_hash(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

async fn encrypt_oauth_grant_payload(grant_id: &str, plaintext: &[u8]) -> AuthStackResult<String> {
    let key = mfa_key_material(MFA_VAULT_KEY, "oauth-grant").await?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|_| AuthStackError::configuration("OAuth vault key is invalid"))?;
    let nonce_bytes: [u8; MFA_NONCE_BYTES] = random_bytes(MFA_NONCE_BYTES)?
        .try_into()
        .map_err(|_| AuthStackError::store("failed to generate OAuth nonce"))?;
    let nonce = Nonce::from(nonce_bytes);
    let aad = format!("oauth-grant:{grant_id}:v1");
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| AuthStackError::store("failed to encrypt OAuth transaction"))?;
    Ok(format!(
        "v1.{}.{}",
        URL_SAFE_NO_PAD.encode(nonce_bytes),
        URL_SAFE_NO_PAD.encode(ciphertext)
    ))
}

async fn decrypt_oauth_grant_payload(
    grant_id: &str,
    encoded: &str,
) -> AuthStackResult<StoredOauthGrantPayload> {
    let key = mfa_key_material(MFA_VAULT_KEY, "oauth-grant").await?;
    let mut parts = encoded.split('.');
    if parts.next() != Some("v1") {
        return Err(AuthStackError::store(
            "stored OAuth transaction version is invalid",
        ));
    }
    let nonce_bytes: [u8; MFA_NONCE_BYTES] = parts
        .next()
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .filter(|value| value.len() == MFA_NONCE_BYTES)
        .ok_or_else(|| AuthStackError::store("stored OAuth nonce is invalid"))?
        .try_into()
        .map_err(|_| AuthStackError::store("stored OAuth nonce is invalid"))?;
    let ciphertext = parts
        .next()
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AuthStackError::store("stored OAuth transaction is invalid"))?;
    if parts.next().is_some() {
        return Err(AuthStackError::store(
            "stored OAuth transaction format is invalid",
        ));
    }
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|_| AuthStackError::configuration("OAuth vault key is invalid"))?;
    let nonce = Nonce::from(nonce_bytes);
    let aad = format!("oauth-grant:{grant_id}:v1");
    let plaintext = cipher
        .decrypt(
            &nonce,
            Payload {
                msg: &ciphertext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| AuthStackError::store("stored OAuth transaction authentication failed"))?;
    serde_json::from_slice(&plaintext)
        .map_err(|_| AuthStackError::store("stored OAuth transaction payload is invalid"))
}

async fn encrypt_mfa_secret(
    user_id: &str,
    credential_id: &str,
    plaintext: &[u8],
) -> AuthStackResult<String> {
    let key = mfa_key_material(MFA_VAULT_KEY, "vault").await?;
    encrypt_mfa_secret_with_key(&key, user_id, credential_id, plaintext)
}

fn encrypt_mfa_secret_with_key(
    key: &[u8; 32],
    user_id: &str,
    credential_id: &str,
    plaintext: &[u8],
) -> AuthStackResult<String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| AuthStackError::configuration("MFA vault key is invalid"))?;
    let nonce_bytes: [u8; MFA_NONCE_BYTES] = random_bytes(MFA_NONCE_BYTES)?
        .try_into()
        .map_err(|_| AuthStackError::store("failed to generate MFA nonce"))?;
    let nonce = Nonce::from(nonce_bytes);
    let aad = format!("mfa-totp:{user_id}:{credential_id}:v1");
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| AuthStackError::store("failed to encrypt MFA secret"))?;
    Ok(format!(
        "v1.{}.{}",
        URL_SAFE_NO_PAD.encode(nonce_bytes),
        URL_SAFE_NO_PAD.encode(ciphertext)
    ))
}

async fn decrypt_mfa_secret(
    user_id: &str,
    credential_id: &str,
    encoded: &str,
) -> AuthStackResult<Vec<u8>> {
    let key = mfa_key_material(MFA_VAULT_KEY, "vault").await?;
    decrypt_mfa_secret_with_key(&key, user_id, credential_id, encoded)
}

fn decrypt_mfa_secret_with_key(
    key: &[u8; 32],
    user_id: &str,
    credential_id: &str,
    encoded: &str,
) -> AuthStackResult<Vec<u8>> {
    let mut parts = encoded.split('.');
    if parts.next() != Some("v1") {
        return Err(AuthStackError::store(
            "stored MFA secret version is invalid",
        ));
    }
    let nonce_bytes: [u8; MFA_NONCE_BYTES] = parts
        .next()
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .filter(|value| value.len() == MFA_NONCE_BYTES)
        .ok_or_else(|| AuthStackError::store("stored MFA nonce is invalid"))?
        .try_into()
        .map_err(|_| AuthStackError::store("stored MFA nonce is invalid"))?;
    let nonce = Nonce::from(nonce_bytes);
    let ciphertext = parts
        .next()
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AuthStackError::store("stored MFA ciphertext is invalid"))?;
    if parts.next().is_some() {
        return Err(AuthStackError::store("stored MFA secret format is invalid"));
    }
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| AuthStackError::configuration("MFA vault key is invalid"))?;
    let aad = format!("mfa-totp:{user_id}:{credential_id}:v1");
    cipher
        .decrypt(
            &nonce,
            Payload {
                msg: &ciphertext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| AuthStackError::store("stored MFA secret authentication failed"))
}

async fn recovery_code_pepper() -> AuthStackResult<[u8; 32]> {
    mfa_key_material(MFA_RECOVERY_PEPPER, "recovery-code").await
}

async fn mfa_key_material(name: &str, development_domain: &str) -> AuthStackResult<[u8; 32]> {
    if let Some(encoded) = store_config_value(name)
        .await
        .filter(|value| !value.trim().is_empty())
    {
        let decoded = URL_SAFE_NO_PAD
            .decode(encoded.trim())
            .or_else(|_| STANDARD.decode(encoded.trim()))
            .map_err(|_| AuthStackError::configuration(format!("{name} is not valid base64")))?;
        return decoded
            .as_slice()
            .try_into()
            .map_err(|_| AuthStackError::configuration(format!("{name} must decode to 32 bytes")));
    }
    if config_bool(AUTH_PRODUCTION_MODE, false).await {
        return Err(AuthStackError::configuration(format!(
            "production requires {name}"
        )));
    }
    let secret = jwt_secret().await;
    Ok(Sha256::digest(format!("dev:{development_domain}:{secret}").as_bytes()).into())
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
    configured_signing_keys().await?;
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

async fn jwt_issuer() -> String {
    store_config_value("AUTH_JWT_ISSUER")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_JWT_ISSUER.to_string())
}

async fn jwt_audience() -> String {
    store_config_value("AUTH_JWT_AUDIENCE")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_JWT_AUDIENCE.to_string())
}

async fn jwt_key_id() -> String {
    store_config_value("AUTH_JWT_KID")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_JWT_KID.to_string())
}

async fn jwt_secret() -> String {
    store_config_value("AUTH_JWT_SECRET")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_JWT_SECRET.to_string())
}

async fn jwt_algorithm() -> AuthStackResult<Algorithm> {
    let algorithm = store_config_value("AUTH_JWT_ALGORITHM")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_JWT_ALGORITHM.to_string());

    parse_jwt_algorithm("AUTH_JWT_ALGORITHM", &algorithm)
}

fn parse_jwt_algorithm(name: &str, value: &str) -> AuthStackResult<Algorithm> {
    match value.trim().to_ascii_uppercase().as_str() {
        "HS256" => Ok(Algorithm::HS256),
        "ES256" => Ok(Algorithm::ES256),
        other => Err(AuthStackError::configuration(format!(
            "unsupported {name} '{other}'; supported values are HS256 and ES256"
        ))),
    }
}

async fn active_signing_key_config() -> AuthStackResult<JwtSigningKeyConfig> {
    let keys = configured_signing_keys().await?;
    active_signing_key_config_from(&keys).await
}

async fn active_signing_key_config_from(
    keys: &[JwtSigningKeyConfig],
) -> AuthStackResult<JwtSigningKeyConfig> {
    let states = stored_signing_key_states().await?;
    if let Some(active_kid) = states
        .values()
        .find(|state| state.status == SigningKeyStatus::Active)
        .map(|state| state.kid.as_str())
        && let Some(key) = keys.iter().find(|key| key.kid == active_kid)
    {
        return Ok(key.with_status(SigningKeyStatus::Active));
    }

    if let Some(key) = keys
        .iter()
        .find(|key| effective_signing_key_status(key, &states) == SigningKeyStatus::Active)
    {
        return Ok(key.with_status(SigningKeyStatus::Active));
    }

    keys.iter()
        .find(|key| effective_signing_key_status(key, &states) != SigningKeyStatus::Revoked)
        .cloned()
        .ok_or_else(|| AuthStackError::configuration("no usable JWT signing key is configured"))
}

async fn signing_key_config_for_key_id(key_id: &str) -> AuthStackResult<JwtSigningKeyConfig> {
    let keys = configured_signing_keys().await?;
    let states = stored_signing_key_states().await?;
    let Some(key) = keys.iter().find(|key| key.kid == key_id) else {
        return Err(AuthStackError::InvalidToken);
    };
    let status = effective_signing_key_status(key, &states);
    if status == SigningKeyStatus::Revoked {
        return Err(AuthStackError::InvalidToken);
    }
    Ok(key.with_status(status))
}

async fn configured_signing_keys() -> AuthStackResult<Vec<JwtSigningKeyConfig>> {
    let production_mode = config_bool(AUTH_PRODUCTION_MODE, false).await;
    if let Some(value) = store_config_value("AUTH_JWT_KEY_RING_JSON")
        .await
        .filter(|value| !value.trim().is_empty())
    {
        let keys = parse_jwt_key_ring(&value)?;
        if keys.is_empty() {
            return Err(AuthStackError::configuration(
                "AUTH_JWT_KEY_RING_JSON must contain at least one key",
            ));
        }
        validate_signing_key_policy(production_mode, true, &keys)?;
        return Ok(keys);
    }

    let keys = vec![runtime_default_signing_key_config().await?];
    validate_signing_key_policy(production_mode, false, &keys)?;
    Ok(keys)
}

async fn runtime_default_signing_key_config() -> AuthStackResult<JwtSigningKeyConfig> {
    Ok(JwtSigningKeyConfig {
        kid: jwt_key_id().await,
        algorithm: jwt_algorithm().await?,
        secret: Some(jwt_secret().await),
        private_key_der_base64: store_config_value("AUTH_JWT_PRIVATE_KEY_DER_BASE64").await,
        public_jwks_json: store_config_value("AUTH_JWT_PUBLIC_JWKS_JSON").await,
        status: SigningKeyStatus::Active,
        source: "runtime".to_string(),
    })
}

fn validate_signing_key_policy(
    production_mode: bool,
    key_ring_configured: bool,
    keys: &[JwtSigningKeyConfig],
) -> AuthStackResult<()> {
    if !production_mode {
        return Ok(());
    }
    if !key_ring_configured {
        return Err(AuthStackError::configuration(format!(
            "{AUTH_PRODUCTION_MODE}=true requires AUTH_JWT_KEY_RING_JSON with pre-provisioned signing keys"
        )));
    }
    if keys.is_empty() {
        return Err(AuthStackError::configuration(
            "AUTH_JWT_KEY_RING_JSON must contain at least one key",
        ));
    }
    for key in keys {
        if key.status == SigningKeyStatus::Revoked {
            continue;
        }
        if key.algorithm != Algorithm::ES256 {
            return Err(AuthStackError::configuration(format!(
                "{AUTH_PRODUCTION_MODE}=true only permits ES256 signing keys; '{}' uses {}",
                key.kid,
                algorithm_name(key.algorithm)
            )));
        }
        if key
            .private_key_der_base64
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        {
            return Err(AuthStackError::configuration(format!(
                "{AUTH_PRODUCTION_MODE}=true requires private_key_der_base64 for ES256 key '{}'",
                key.kid
            )));
        }
    }
    Ok(())
}

fn parse_jwt_key_ring(value: &str) -> AuthStackResult<Vec<JwtSigningKeyConfig>> {
    let parsed: Value = serde_json::from_str(value).map_err(|error| {
        AuthStackError::configuration(format!("AUTH_JWT_KEY_RING_JSON is invalid JSON: {error}"))
    })?;
    let entries = parsed
        .as_array()
        .or_else(|| parsed.get("keys").and_then(Value::as_array))
        .ok_or_else(|| {
            AuthStackError::configuration(
                "AUTH_JWT_KEY_RING_JSON must be an array or an object with a keys array",
            )
        })?;

    entries
        .iter()
        .enumerate()
        .map(|(index, value)| jwt_key_config_from_json(value, index))
        .collect()
}

fn jwt_key_config_from_json(value: &Value, index: usize) -> AuthStackResult<JwtSigningKeyConfig> {
    let kid = row_string(value, "kid")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AuthStackError::configuration(format!(
                "AUTH_JWT_KEY_RING_JSON key at index {index} is missing kid"
            ))
        })?;
    let algorithm_value =
        row_string(value, "alg").unwrap_or_else(|| DEFAULT_JWT_ALGORITHM.to_string());
    let algorithm = parse_jwt_algorithm("AUTH_JWT_KEY_RING_JSON.alg", &algorithm_value)?;
    let status = row_string(value, "status")
        .as_deref()
        .map(SigningKeyStatus::parse)
        .transpose()?
        .unwrap_or(SigningKeyStatus::Next);

    Ok(JwtSigningKeyConfig {
        kid,
        algorithm,
        secret: row_string(value, "secret"),
        private_key_der_base64: row_string(value, "private_key_der_base64"),
        public_jwks_json: row_string(value, "public_jwks_json")
            .or_else(|| row_string(value, "public_jwk_json")),
        status,
        source: "key_ring".to_string(),
    })
}

async fn jwt_encoding_key_for_config(config: &JwtSigningKeyConfig) -> AuthStackResult<EncodingKey> {
    match config.algorithm {
        Algorithm::HS256 => {
            let secret = config
                .secret
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    AuthStackError::configuration(format!(
                        "HS256 signing key '{}' is missing secret",
                        config.kid
                    ))
                })?;
            Ok(EncodingKey::from_secret(secret.as_bytes()))
        }
        Algorithm::ES256 => {
            let value = config
                .private_key_der_base64
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    AuthStackError::configuration(format!(
                        "ES256 signing key '{}' is missing private_key_der_base64",
                        config.kid
                    ))
                })?;
            let private_der = decode_base64_config("private_key_der_base64", value)?;
            Ok(EncodingKey::from_ec_der(&private_der))
        }
        _ => Err(AuthStackError::configuration(
            "configured JWT algorithm is not supported for signing",
        )),
    }
}

async fn jwt_decoding_key_for_config(config: &JwtSigningKeyConfig) -> AuthStackResult<DecodingKey> {
    match config.algorithm {
        Algorithm::HS256 => {
            let secret = config
                .secret
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or(AuthStackError::InvalidToken)?;
            Ok(DecodingKey::from_secret(secret.as_bytes()))
        }
        Algorithm::ES256 => {
            let jwks = jwt_public_jwks_for_config(config).await?.ok_or_else(|| {
                AuthStackError::configuration(format!(
                    "ES256 signing key '{}' requires public_jwks_json or private_key_der_base64",
                    config.kid
                ))
            })?;
            let key = jwks
                .keys
                .into_iter()
                .find(|key| key.kid == config.kid)
                .ok_or(AuthStackError::InvalidToken)?;
            decoding_key_from_jwks_key(&key, config.algorithm)
        }
        _ => Err(AuthStackError::InvalidToken),
    }
}

async fn jwt_public_jwks() -> AuthStackResult<Option<JwksDocument>> {
    let keys = configured_signing_keys().await?;
    let states = stored_signing_key_states().await?;
    let mut public_keys = Vec::new();
    for key in keys {
        if effective_signing_key_status(&key, &states) == SigningKeyStatus::Revoked {
            continue;
        }
        if let Some(jwks) = jwt_public_jwks_for_config(&key).await? {
            public_keys.extend(jwks.keys);
        }
    }

    if public_keys.is_empty() {
        Ok(None)
    } else {
        public_keys.sort_by(|left, right| left.kid.cmp(&right.kid));
        public_keys.dedup_by(|left, right| left.kid == right.kid);
        Ok(Some(JwksDocument { keys: public_keys }))
    }
}

async fn jwt_public_jwks_for_config(
    config: &JwtSigningKeyConfig,
) -> AuthStackResult<Option<JwksDocument>> {
    if config.algorithm != Algorithm::ES256 {
        return Ok(None);
    }

    if let Some(value) = config
        .public_jwks_json
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return parse_configured_jwks(value).map(Some);
    }

    let encoding_key = jwt_encoding_key_for_config(config).await?;
    let mut key = jwks_key_from_json_value(
        serde_json::to_value(
            jwk_from_encoding_key(&encoding_key, Algorithm::ES256)
                .map_err(|_| AuthStackError::InvalidToken)?,
        )
        .map_err(|error| AuthStackError::serialization(error.to_string()))?,
    )?;
    key.kid = config.kid.clone();
    key.alg = "ES256".to_string();
    key.use_ = "sig".to_string();
    Ok(Some(JwksDocument { keys: vec![key] }))
}

fn decode_base64_config(name: &str, value: &str) -> AuthStackResult<Vec<u8>> {
    let compact = value.split_whitespace().collect::<String>();
    STANDARD
        .decode(compact.as_bytes())
        .or_else(|_| URL_SAFE_NO_PAD.decode(compact.as_bytes()))
        .map_err(|error| {
            AuthStackError::configuration(format!("{name} is not valid base64: {error}"))
        })
}

async fn signing_key_summaries() -> AuthStackResult<Vec<SigningKeySummary>> {
    let keys = configured_signing_keys().await?;
    let states = stored_signing_key_states().await?;
    let active_kid = active_signing_key_config_from(&keys).await?.kid;
    let mut summaries = keys
        .iter()
        .map(|key| signing_key_summary_from_config(key, &states, &active_kid))
        .collect::<Vec<_>>();

    for state in states.values() {
        if keys.iter().any(|key| key.kid == state.kid) {
            continue;
        }
        summaries.push(SigningKeySummary {
            kid: state.kid.clone(),
            alg: state.alg.clone().unwrap_or_default(),
            status: state.status.as_str().to_string(),
            active: state.kid == active_kid,
            source: "state".to_string(),
            created_at_ms: Some(state.created_at_ms),
            activated_at_ms: state.activated_at_ms,
            retired_at_ms: state.retired_at_ms,
            revoked_at_ms: state.revoked_at_ms,
        });
    }

    summaries.sort_by(|left, right| left.kid.cmp(&right.kid));
    Ok(summaries)
}

fn signing_key_summary_from_config(
    key: &JwtSigningKeyConfig,
    states: &BTreeMap<String, StoredSigningKeyState>,
    active_kid: &str,
) -> SigningKeySummary {
    let state = states.get(&key.kid);
    let status = effective_signing_key_status(key, states);
    SigningKeySummary {
        kid: key.kid.clone(),
        alg: algorithm_name(key.algorithm).to_string(),
        status: status.as_str().to_string(),
        active: key.kid == active_kid,
        source: key.source.clone(),
        created_at_ms: state.map(|state| state.created_at_ms),
        activated_at_ms: state.and_then(|state| state.activated_at_ms),
        retired_at_ms: state.and_then(|state| state.retired_at_ms),
        revoked_at_ms: state.and_then(|state| state.revoked_at_ms),
    }
}

fn effective_signing_key_status(
    key: &JwtSigningKeyConfig,
    states: &BTreeMap<String, StoredSigningKeyState>,
) -> SigningKeyStatus {
    states
        .get(&key.kid)
        .map(|state| state.status)
        .unwrap_or(key.status)
}

async fn stored_signing_key_states() -> AuthStackResult<BTreeMap<String, StoredSigningKeyState>> {
    let rows = execute_sql(
        "SELECT tenant_id, kid, alg, status, created_at_ms, activated_at_ms, retired_at_ms, revoked_at_ms \
         FROM auth_signing_keys \
         WHERE tenant_id = ?1",
        vec![json!(DEFAULT_TENANT_ID)],
    )
    .await?;
    let mut states = BTreeMap::new();
    for row in rows {
        let state = stored_signing_key_state_from_row(row)?;
        states.insert(state.kid.clone(), state);
    }
    Ok(states)
}

async fn upsert_signing_key_state(
    kid: &str,
    status: SigningKeyStatus,
    activated_at_ms: Option<u64>,
    retired_at_ms: Option<u64>,
    revoked_at_ms: Option<u64>,
) -> AuthStackResult<()> {
    let now = now_ms();
    let alg = configured_signing_keys()
        .await?
        .into_iter()
        .find(|key| key.kid == kid)
        .map(|key| algorithm_name(key.algorithm).to_string());
    execute_sql_atomic(vec![signing_key_state_statement(
        kid,
        alg.as_deref().unwrap_or_default(),
        status,
        activated_at_ms,
        retired_at_ms,
        revoked_at_ms,
        now,
    )])
    .await?;
    Ok(())
}

fn signing_key_state_statement(
    kid: &str,
    alg: &str,
    status: SigningKeyStatus,
    activated_at_ms: Option<u64>,
    retired_at_ms: Option<u64>,
    revoked_at_ms: Option<u64>,
    now: u64,
) -> AtomicSqlStatement {
    AtomicSqlStatement::execute(
        "INSERT INTO auth_signing_keys \
         (tenant_id, kid, alg, status, created_at_ms, updated_at_ms, activated_at_ms, retired_at_ms, revoked_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7, ?8) \
         ON CONFLICT(tenant_id, kid) DO UPDATE SET \
         alg = excluded.alg, \
         status = excluded.status, \
         updated_at_ms = excluded.updated_at_ms, \
         activated_at_ms = COALESCE(excluded.activated_at_ms, auth_signing_keys.activated_at_ms), \
         retired_at_ms = excluded.retired_at_ms, \
         revoked_at_ms = excluded.revoked_at_ms",
        vec![
            json!(DEFAULT_TENANT_ID),
            json!(kid),
            json!(alg),
            json!(status.as_str()),
            json!(now),
            json!(activated_at_ms),
            json!(retired_at_ms),
            json!(revoked_at_ms),
        ],
    )
}

fn stored_signing_key_state_from_row(row: Value) -> AuthStackResult<StoredSigningKeyState> {
    Ok(StoredSigningKeyState {
        _tenant_id: required_string(&row, "tenant_id")?,
        kid: required_string(&row, "kid")?,
        alg: row_string(&row, "alg"),
        status: SigningKeyStatus::parse(&required_string(&row, "status")?)?,
        created_at_ms: row_i64(&row, "created_at_ms").unwrap_or_default() as u64,
        activated_at_ms: row_i64(&row, "activated_at_ms").map(|value| value as u64),
        retired_at_ms: row_i64(&row, "retired_at_ms").map(|value| value as u64),
        revoked_at_ms: row_i64(&row, "revoked_at_ms").map(|value| value as u64),
    })
}

fn algorithm_name(algorithm: Algorithm) -> &'static str {
    match algorithm {
        Algorithm::HS256 => "HS256",
        Algorithm::ES256 => "ES256",
        _ => "unsupported",
    }
}

fn parse_configured_jwks(value: &str) -> AuthStackResult<JwksDocument> {
    let parsed: Value = serde_json::from_str(value).map_err(|error| {
        AuthStackError::configuration(format!(
            "AUTH_JWT_PUBLIC_JWKS_JSON is invalid JSON: {error}"
        ))
    })?;
    if parsed.get("keys").is_some() {
        serde_json::from_value(parsed).map_err(|error| {
            AuthStackError::configuration(format!(
                "AUTH_JWT_PUBLIC_JWKS_JSON is not a valid JWKS document: {error}"
            ))
        })
    } else {
        Ok(JwksDocument {
            keys: vec![jwks_key_from_json_value(parsed)?],
        })
    }
}

fn jwks_key_from_json_value(value: Value) -> AuthStackResult<JwksKey> {
    serde_json::from_value(value).map_err(|error| {
        AuthStackError::configuration(format!("JWT public JWK is invalid: {error}"))
    })
}

fn decoding_key_from_jwks_key(
    key: &JwksKey,
    expected_algorithm: Algorithm,
) -> AuthStackResult<DecodingKey> {
    if expected_algorithm != Algorithm::ES256 {
        return Err(AuthStackError::InvalidToken);
    }
    if !key.alg.is_empty() && key.alg != "ES256" {
        return Err(AuthStackError::InvalidToken);
    }
    if key.kty != "EC" {
        return Err(AuthStackError::InvalidToken);
    }
    let x = key
        .public_parameters
        .get("x")
        .filter(|value| !value.trim().is_empty())
        .ok_or(AuthStackError::InvalidToken)?;
    let y = key
        .public_parameters
        .get("y")
        .filter(|value| !value.trim().is_empty())
        .ok_or(AuthStackError::InvalidToken)?;
    DecodingKey::from_ec_components(x, y).map_err(|_| AuthStackError::InvalidToken)
}

async fn session_ttl_ms() -> u64 {
    seconds_to_ms(config_u64("AUTH_SESSION_TTL_SECONDS", DEFAULT_SESSION_TTL_SECONDS).await)
}

async fn refresh_token_ttl_ms() -> u64 {
    seconds_to_ms(
        config_u64(
            "AUTH_REFRESH_TOKEN_TTL_SECONDS",
            DEFAULT_REFRESH_TOKEN_TTL_SECONDS,
        )
        .await,
    )
}

async fn access_token_ttl_seconds() -> u64 {
    config_u64(
        "AUTH_ACCESS_TOKEN_TTL_SECONDS",
        DEFAULT_ACCESS_TOKEN_TTL_SECONDS,
    )
    .await
}

async fn passkey_challenge_ttl_ms() -> u64 {
    seconds_to_ms(
        config_u64(
            "AUTH_PASSKEY_CHALLENGE_TTL_SECONDS",
            DEFAULT_PASSKEY_CHALLENGE_TTL_SECONDS,
        )
        .await,
    )
}

async fn passkey_webauthn() -> AuthStackResult<Webauthn> {
    let rp_id = config_string("AUTH_PASSKEY_RP_ID", DEFAULT_PASSKEY_RP_ID).await;
    if rp_id.contains("://") || rp_id.contains('/') || rp_id.contains(':') {
        return Err(AuthStackError::configuration(
            "AUTH_PASSKEY_RP_ID must be a bare host name such as localhost or app.example.com",
        ));
    }
    let rp_name = config_string("AUTH_PASSKEY_RP_NAME", DEFAULT_PASSKEY_RP_NAME).await;
    let origin = config_string("AUTH_PASSKEY_ORIGIN", DEFAULT_PASSKEY_ORIGIN).await;
    let attachment = passkey_attachment().await?;

    Ok(Webauthn::new(&rp_id, &rp_name, &origin)
        .authenticator_attachment(attachment)
        .require_user_verification(
            config_bool("AUTH_PASSKEY_REQUIRE_USER_VERIFICATION", true).await,
        )
        .require_user_handle(config_bool("AUTH_PASSKEY_REQUIRE_USER_HANDLE", false).await)
        .strict_base64(config_bool("AUTH_PASSKEY_STRICT_BASE64", true).await))
}

async fn passkey_attachment() -> AuthStackResult<PasskeyAttachment> {
    let value = config_string("AUTH_PASSKEY_AUTHENTICATOR_ATTACHMENT", "platform").await;
    match value.trim().to_ascii_lowercase().as_str() {
        "platform" => Ok(PasskeyAttachment::Platform),
        "cross-platform" | "cross_platform" | "roaming" => Ok(PasskeyAttachment::CrossPlatform),
        "any" | "none" | "unspecified" => Ok(PasskeyAttachment::Any),
        _ => Err(AuthStackError::configuration(
            "AUTH_PASSKEY_AUTHENTICATOR_ATTACHMENT must be platform, cross-platform, or any",
        )),
    }
}

async fn config_string(name: &str, default: &str) -> String {
    store_config_value(name)
        .await
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

async fn config_u64(name: &str, default: u64) -> u64 {
    store_config_value(name)
        .await
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

async fn config_bool(name: &str, default: bool) -> bool {
    store_config_value(name)
        .await
        .map(|value| truthy(&value))
        .unwrap_or(default)
}

fn seconds_to_ms(value: u64) -> u64 {
    value.saturating_mul(1000)
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

async fn provider_default_enabled(provider_id: &str) -> bool {
    store_config_value(&provider_enabled_env_name(provider_id))
        .await
        .map(|value| truthy(&value))
        .unwrap_or(false)
}

fn provider_enabled_env_name(provider_id: &str) -> String {
    let upper = provider_id.to_ascii_uppercase().replace(['-', '.'], "_");
    format!("AUTH_{upper}_ENABLED")
}

fn truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on" | "enabled"
    )
}

fn map_auth_error(error: WorkflowError) -> AuthStackError {
    match error {
        WorkflowError::SessionExpired => AuthStackError::SessionExpired,
        WorkflowError::SessionRevoked => AuthStackError::AuthRequired,
        WorkflowError::Validation { message } => AuthStackError::validation(message),
        _ => AuthStackError::InvalidToken,
    }
}

fn stored_session_from_row(row: Value) -> AuthStackResult<StoredSession> {
    let permissions_json = row_string(&row, "permissions_json").unwrap_or_else(|| "[]".to_string());
    let permissions = serde_json::from_str::<Vec<String>>(&permissions_json).map_err(|error| {
        AuthStackError::store(format!("stored session permissions are invalid: {error}"))
    })?;

    Ok(StoredSession {
        session_id: required_string(&row, "session_id")?,
        tenant_id: required_string(&row, "tenant_id")?,
        user_id: required_string(&row, "user_id")?,
        primary_email: row_string(&row, "primary_email"),
        expires_at_ms: row_i64(&row, "expires_at_ms").unwrap_or_default(),
        revoked_at_ms: row_i64(&row, "revoked_at_ms"),
        permissions,
        assurance: row_string(&row, "assurance").unwrap_or_else(|| "aal1".to_string()),
        created_at_ms: row_i64(&row, "created_at_ms").unwrap_or_default(),
    })
}

fn stored_refresh_token_from_row(row: Value) -> AuthStackResult<StoredRefreshToken> {
    Ok(StoredRefreshToken {
        _token_hash: required_string(&row, "token_hash")?,
        session_id: required_string(&row, "session_id")?,
        expires_at_ms: row_i64(&row, "expires_at_ms").unwrap_or_default(),
        rotated_at_ms: row_i64(&row, "rotated_at_ms"),
        revoked_at_ms: row_i64(&row, "revoked_at_ms"),
    })
}

fn unauthenticated_session() -> SessionView {
    SessionView {
        authenticated: false,
        session_id: None,
        tenant_id: None,
        user_id: None,
        primary_email: None,
        expires_at: None,
        permissions: Vec::new(),
        assurance: "none".to_string(),
        system_administrator: false,
        issued_at_unix_seconds: None,
        expires_at_unix_seconds: None,
    }
}

fn normalized_session_id(session_id: Option<&str>) -> Option<String> {
    session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

async fn session_context_for_user(
    email: &str,
    user_id: &str,
) -> AuthStackResult<(String, Vec<String>)> {
    let rows = execute_sql(
        "SELECT memberships.organization_id, roles.permissions_json \
         FROM auth_memberships memberships \
         JOIN auth_roles roles ON roles.organization_id = memberships.organization_id \
          AND roles.role_id = memberships.role_id \
         WHERE memberships.user_id = ?1 AND memberships.status = 'active' \
         ORDER BY memberships.joined_at_ms ASC LIMIT 1",
        vec![json!(user_id)],
    )
    .await?;
    let (organization_id, mut permissions) = if let Some(row) = rows.first() {
        (
            required_string(row, "organization_id")?,
            permissions_from_row(row)?,
        )
    } else {
        (DEFAULT_TENANT_ID.to_owned(), default_session_permissions())
    };
    permissions.extend(default_session_permissions());
    if bootstrap_admin_email(email).await {
        permissions.extend(
            [
                "auth:provider:write",
                "auth:redirect:write",
                "auth:signing-key:admin",
                "auth:storage:admin",
                "system.user.manage",
                "system.provider.manage",
                "system.signing-key.manage",
                "system.policy.manage",
                "system.health.read",
            ]
            .into_iter()
            .map(ToOwned::to_owned),
        );
    }
    permissions.sort();
    permissions.dedup();
    Ok((organization_id, permissions))
}

fn default_session_permissions() -> Vec<String> {
    [
        "auth:session:read",
        "auth:token:refresh",
        "auth:logout",
        "authz:check",
        "counter.view",
    ]
    .into_iter()
    .map(ToOwned::to_owned)
    .collect()
}

fn is_system_administrator(permissions: &[String]) -> bool {
    permissions
        .iter()
        .any(|permission| permission == "system.user.manage")
}

async fn bootstrap_admin_email(email: &str) -> bool {
    let email = normalize_email(email);
    store_config_value("AUTH_BOOTSTRAP_ADMIN_EMAILS")
        .await
        .unwrap_or_default()
        .split(',')
        .map(normalize_email)
        .any(|candidate| !candidate.is_empty() && candidate == email)
}

fn normalize_email(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}

fn synthetic_oauth_email(provider_id: &str, state: &str) -> String {
    format!(
        "oauth+{}+{}@auth.local",
        sanitize_identifier(provider_id),
        sanitize_identifier(state)
    )
}

async fn spicedb_subject_for_user(user_id: &str) -> String {
    let issuer = jwt_issuer().await;
    let mut identity = Vec::with_capacity(issuer.len() + user_id.len() + 1);
    identity.extend_from_slice(issuer.as_bytes());
    identity.push(0);
    identity.extend_from_slice(user_id.as_bytes());
    format!("user:v1_{}", URL_SAFE_NO_PAD.encode(identity))
}

fn user_id_from_email(email: &str) -> String {
    format!("user:{}", sanitize_identifier(email))
}

fn provider_display_name(provider_id: &str) -> String {
    match provider_id {
        "apple" => "Apple".to_string(),
        "facebook" => "Facebook".to_string(),
        "google" => "Google".to_string(),
        other => other
            .split(['-', '_'])
            .filter(|part| !part.is_empty())
            .map(capitalize_ascii)
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn capitalize_ascii(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_ascii_uppercase().to_string() + chars.as_str()
}

fn secure_storage_id(kind: &str) -> AuthStackResult<String> {
    Ok(format!(
        "{kind}_{}",
        URL_SAFE_NO_PAD.encode(random_bytes(32)?)
    ))
}

fn storage_event_id() -> AuthStackResult<String> {
    Ok(format!(
        "event_{}_{}",
        now_ms(),
        URL_SAFE_NO_PAD.encode(random_bytes(16)?)
    ))
}

fn sanitize_identifier(value: &str) -> String {
    let sanitized = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect::<String>();
    if sanitized.is_empty() {
        "anonymous".to_string()
    } else {
        sanitized
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn map_passkey_verification_error(error: impl std::fmt::Display) -> AuthStackError {
    tracing::warn!(
        error = %error,
        "passkey verification failed"
    );
    AuthStackError::InvalidCredentials
}

#[derive(Clone, Copy, Debug)]
enum StorageProjectionKind {
    Auth,
}

impl StorageProjectionKind {
    fn checkpoint_name(self) -> &'static str {
        match self {
            Self::Auth => AUTH_STORAGE_PROJECTION_CHECKPOINT,
        }
    }
}

#[derive(Clone, Debug)]
struct StoredStorageEvent {
    sequence: u64,
    _aggregate_type: String,
    event_type: String,
    payload: Value,
    recorded_at_ms: u64,
}

#[derive(Clone, Debug)]
pub struct ConsumedPasskeyChallenge {
    pub grant_type: String,
    pub redirect_url: String,
    pub payload: StoredPasskeyChallengePayload,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "flow", rename_all = "snake_case")]
pub enum StoredPasskeyChallengePayload {
    Registration {
        state: PasskeyRegistrationState,
        email: String,
        user_id: String,
    },
    Login {
        state: PasskeyAuthenticationState,
        email: String,
        user_id: String,
    },
}

#[derive(Clone)]
pub struct CreatedOauthGrant {
    pub state: String,
    pub nonce: String,
    pub pkce_challenge: String,
}

impl std::fmt::Debug for CreatedOauthGrant {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CreatedOauthGrant")
            .field("state", &"[REDACTED]")
            .field("nonce", &"[REDACTED]")
            .field("pkce_challenge", &self.pkce_challenge)
            .finish()
    }
}

#[derive(Deserialize, Serialize)]
struct StoredOauthGrantPayload {
    provider_id: String,
    grant_id: String,
    nonce: String,
    pkce_verifier: String,
}

#[derive(Clone)]
pub struct ConsumedOauthGrant {
    pub state: String,
    pub provider_id: String,
    pub redirect_url: String,
    pub nonce: String,
    pub pkce_verifier: String,
}

impl std::fmt::Debug for ConsumedOauthGrant {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConsumedOauthGrant")
            .field("state", &"[REDACTED]")
            .field("provider_id", &self.provider_id)
            .field("redirect_url", &self.redirect_url)
            .field("nonce", &"[REDACTED]")
            .field("pkce_verifier", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Debug)]
struct JwtSigningKeyConfig {
    kid: String,
    algorithm: Algorithm,
    secret: Option<String>,
    private_key_der_base64: Option<String>,
    public_jwks_json: Option<String>,
    status: SigningKeyStatus,
    source: String,
}

impl JwtSigningKeyConfig {
    fn with_status(&self, status: SigningKeyStatus) -> Self {
        let mut value = self.clone();
        value.status = status;
        value
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SigningKeyStatus {
    Active,
    Next,
    Retired,
    Revoked,
}

impl SigningKeyStatus {
    fn parse(value: &str) -> AuthStackResult<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            SIGNING_KEY_STATUS_ACTIVE => Ok(Self::Active),
            SIGNING_KEY_STATUS_NEXT => Ok(Self::Next),
            SIGNING_KEY_STATUS_RETIRED => Ok(Self::Retired),
            SIGNING_KEY_STATUS_REVOKED => Ok(Self::Revoked),
            other => Err(AuthStackError::validation(format!(
                "unsupported signing key status '{other}'"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Active => SIGNING_KEY_STATUS_ACTIVE,
            Self::Next => SIGNING_KEY_STATUS_NEXT,
            Self::Retired => SIGNING_KEY_STATUS_RETIRED,
            Self::Revoked => SIGNING_KEY_STATUS_REVOKED,
        }
    }
}

#[derive(Clone, Debug)]
struct StoredSigningKeyState {
    _tenant_id: String,
    kid: String,
    alg: Option<String>,
    status: SigningKeyStatus,
    created_at_ms: u64,
    activated_at_ms: Option<u64>,
    retired_at_ms: Option<u64>,
    revoked_at_ms: Option<u64>,
}

#[derive(Clone, Debug)]
struct StoredSession {
    session_id: String,
    tenant_id: String,
    user_id: String,
    primary_email: Option<String>,
    expires_at_ms: i64,
    revoked_at_ms: Option<i64>,
    permissions: Vec<String>,
    assurance: String,
    created_at_ms: i64,
}

#[derive(Clone, Debug)]
struct StoredRefreshToken {
    _token_hash: String,
    session_id: String,
    expires_at_ms: i64,
    rotated_at_ms: Option<i64>,
    revoked_at_ms: Option<i64>,
}

#[derive(Clone, Debug)]
struct PasswordCredentialRecord {
    user_id: String,
    primary_email: String,
    disabled: bool,
    email_verified: bool,
    password_hash: String,
    revoked_at_ms: Option<i64>,
}

impl StoredSession {
    fn into_view(self) -> SessionView {
        let system_administrator = is_system_administrator(&self.permissions);
        SessionView {
            authenticated: true,
            session_id: Some(self.session_id),
            tenant_id: Some(self.tenant_id),
            user_id: Some(self.user_id),
            primary_email: self.primary_email,
            expires_at: Some(self.expires_at_ms.to_string()),
            permissions: self.permissions,
            assurance: self.assurance,
            system_administrator,
            issued_at_unix_seconds: u64::try_from(self.created_at_ms)
                .ok()
                .map(|value| value / 1_000),
            expires_at_unix_seconds: u64::try_from(self.expires_at_ms)
                .ok()
                .map(|value| value / 1_000),
        }
    }
}

pub fn organization_permission_catalog() -> Vec<String> {
    [
        "organization.view",
        "organization.update",
        "member.view",
        "member.invite",
        "member.manage",
        "role.view",
        "role.manage",
        "audit.view",
        "counter.view",
        "counter.change",
        "counter.reset",
        "ownership.transfer",
    ]
    .into_iter()
    .map(ToOwned::to_owned)
    .collect()
}

fn built_in_role_permissions(role_id: &str) -> Option<Vec<String>> {
    let permissions = match role_id {
        "owner" => organization_permission_catalog(),
        "admin" => [
            "organization.view",
            "organization.update",
            "member.view",
            "member.invite",
            "member.manage",
            "role.view",
            "role.manage",
            "audit.view",
            "counter.view",
            "counter.change",
            "counter.reset",
        ]
        .into_iter()
        .map(ToOwned::to_owned)
        .collect(),
        "member" => [
            "organization.view",
            "member.view",
            "role.view",
            "counter.view",
            "counter.change",
        ]
        .into_iter()
        .map(ToOwned::to_owned)
        .collect(),
        "viewer" => [
            "organization.view",
            "member.view",
            "role.view",
            "counter.view",
        ]
        .into_iter()
        .map(ToOwned::to_owned)
        .collect(),
        _ => return None,
    };
    Some(permissions)
}

pub async fn list_organizations_for_user(
    user_id: &str,
) -> AuthStackResult<OrganizationListResponse> {
    initialize_schema_async().await?;
    let rows = execute_sql(
        "SELECT organizations.organization_id, organizations.name, organizations.status, \
         organizations.created_at_ms, memberships.role_id, roles.permissions_json \
         FROM auth_organizations organizations \
         JOIN auth_memberships memberships \
           ON memberships.organization_id = organizations.organization_id \
         JOIN auth_roles roles \
           ON roles.organization_id = memberships.organization_id \
          AND roles.role_id = memberships.role_id \
         WHERE memberships.user_id = ?1 AND memberships.status = 'active' \
           AND organizations.status = 'active' \
         ORDER BY organizations.name ASC",
        vec![json!(user_id)],
    )
    .await?;
    Ok(OrganizationListResponse {
        organizations: rows
            .into_iter()
            .map(organization_from_row)
            .collect::<AuthStackResult<Vec<_>>>()?,
    })
}

pub async fn organization_for_user(
    organization_id: &str,
    user_id: &str,
) -> AuthStackResult<OrganizationSummary> {
    let organizations = list_organizations_for_user(user_id).await?;
    organizations
        .organizations
        .into_iter()
        .find(|organization| organization.organization_id == organization_id)
        .ok_or(AuthStackError::Forbidden)
}

pub async fn authorization_snapshot_metadata(
    organization_id: Option<&str>,
    user_id: &str,
) -> AuthStackResult<(Vec<String>, String)> {
    let Some(organization_id) = organization_id.filter(|value| *value != DEFAULT_TENANT_ID) else {
        return Ok((Vec::new(), "system-v1".to_owned()));
    };
    let rows = execute_sql(
        "SELECT memberships.role_id, organizations.authorization_revision \
         FROM auth_memberships memberships \
         JOIN auth_organizations organizations \
           ON organizations.organization_id = memberships.organization_id \
         WHERE memberships.organization_id = ?1 AND memberships.user_id = ?2 \
           AND memberships.status = 'active' AND organizations.status = 'active' LIMIT 1",
        vec![json!(organization_id), json!(user_id)],
    )
    .await?;
    let row = rows.first().ok_or(AuthStackError::Forbidden)?;
    let role_id = required_string(row, "role_id")?;
    let revision = row_i64(row, "authorization_revision").unwrap_or(1).max(1);
    Ok((
        vec![role_id],
        format!("organization-{organization_id}-revision-{revision}"),
    ))
}

pub async fn create_organization(
    name: &str,
    owner_user_id: &str,
) -> AuthStackResult<OrganizationSummary> {
    initialize_schema_async().await?;
    let now = now_ms();
    let organization_id = secure_storage_id("organization")?;
    let owner_subject = spicedb_subject_for_user(owner_user_id).await;
    let mut statements = vec![AtomicSqlStatement::execute(
        "INSERT INTO auth_organizations \
         (organization_id, name, status, created_by, created_at_ms, updated_at_ms) \
         VALUES (?1, ?2, 'active', ?3, ?4, ?4)",
        vec![
            json!(&organization_id),
            json!(name),
            json!(owner_user_id),
            json!(now),
        ],
    )];
    for (role_id, role_name) in [
        ("owner", "Owner"),
        ("admin", "Administrator"),
        ("member", "Member"),
        ("viewer", "Viewer"),
    ] {
        let permissions = built_in_role_permissions(role_id)
            .ok_or_else(|| AuthStackError::configuration("built-in role is invalid"))?;
        let permissions_json = serde_json::to_string(&permissions)
            .map_err(|error| AuthStackError::serialization(error.to_string()))?;
        statements.push(AtomicSqlStatement::execute(
            "INSERT INTO auth_roles \
             (organization_id, role_id, name, built_in, permissions_json, created_at_ms, updated_at_ms) \
             VALUES (?1, ?2, ?3, 1, ?4, ?5, ?5)",
            vec![
                json!(&organization_id),
                json!(role_id),
                json!(role_name),
                json!(permissions_json),
                json!(now),
            ],
        ));
    }
    statements.push(AtomicSqlStatement::execute(
        "INSERT INTO auth_memberships \
         (organization_id, user_id, role_id, status, joined_at_ms, updated_at_ms) \
         VALUES (?1, ?2, 'owner', 'active', ?3, ?3)",
        vec![json!(&organization_id), json!(owner_user_id), json!(now)],
    ));
    statements.push(storage_event_statement(
        "auth_organization",
        &organization_id,
        "auth_organization_created",
        json!({
            "organization_id": &organization_id,
            "name": name,
            "owner_user_id": owner_user_id,
        }),
    )?);
    statements.push(relationship_outbox_statement(
        "grant",
        &format!("organization:{organization_id}"),
        "member",
        &owner_subject,
        now,
    )?);
    statements.push(audit_event_statement(
        Some(&organization_id),
        owner_user_id,
        "organization.create",
        "organization",
        &organization_id,
        "success",
        now,
    ));
    execute_sql_atomic(statements).await?;
    organization_for_user(&organization_id, owner_user_id).await
}

pub async fn update_organization(
    organization_id: &str,
    name: &str,
    actor_user_id: &str,
) -> AuthStackResult<OrganizationSummary> {
    require_membership_permission(organization_id, actor_user_id, "organization.update").await?;
    let now = now_ms();
    execute_sql_atomic(vec![
        AtomicSqlStatement::guard(
            "UPDATE auth_organizations SET name = ?1, updated_at_ms = ?2 \
             WHERE organization_id = ?3 AND status = 'active' RETURNING organization_id",
            vec![json!(name), json!(now), json!(organization_id)],
        ),
        storage_event_statement(
            "auth_organization",
            organization_id,
            "auth_organization_updated",
            json!({"organization_id": organization_id, "name": name}),
        )?,
        audit_event_statement(
            Some(organization_id),
            actor_user_id,
            "organization.update",
            "organization",
            organization_id,
            "success",
            now,
        ),
    ])
    .await?;
    organization_for_user(organization_id, actor_user_id).await
}

pub async fn select_organization_for_session(
    session_id: &str,
    user_id: &str,
    organization_id: &str,
) -> AuthStackResult<SessionView> {
    let organization = organization_for_user(organization_id, user_id).await?;
    let existing = get_session(Some(session_id)).await?;
    if matches!(organization.current_user_role.as_str(), "owner" | "admin")
        && existing.assurance != "aal2"
    {
        return Err(AuthStackError::AuthRequired);
    }
    let mut permissions = organization.permissions;
    permissions.extend(default_session_permissions());
    permissions.extend(
        existing.permissions.into_iter().filter(|permission| {
            permission.starts_with("system.") || permission.starts_with("auth:")
        }),
    );
    permissions.sort();
    permissions.dedup();
    let permissions_json = serde_json::to_string(&permissions)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;
    let now = now_ms();
    execute_sql_atomic(vec![
        AtomicSqlStatement::guard(
            "UPDATE auth_sessions \
             SET tenant_id = ?1, permissions_json = ?2, updated_at_ms = ?3 \
             WHERE session_id = ?4 AND user_id = ?5 AND revoked_at_ms IS NULL \
               AND expires_at_ms > ?3 RETURNING session_id",
            vec![
                json!(organization_id),
                json!(permissions_json),
                json!(now),
                json!(session_id),
                json!(user_id),
            ],
        ),
        audit_event_statement(
            Some(organization_id),
            user_id,
            "organization.select",
            "session",
            session_id,
            "success",
            now,
        ),
    ])
    .await?;
    get_session(Some(session_id)).await
}

pub async fn list_memberships(
    organization_id: &str,
    actor_user_id: &str,
) -> AuthStackResult<MembershipListResponse> {
    require_membership_permission(organization_id, actor_user_id, "member.view").await?;
    let rows = execute_sql(
        "SELECT memberships.organization_id, memberships.user_id, users.primary_email, \
         memberships.role_id, memberships.status, memberships.joined_at_ms \
         FROM auth_memberships memberships \
         JOIN auth_users users ON users.user_id = memberships.user_id \
         WHERE memberships.organization_id = ?1 \
         ORDER BY users.primary_email ASC",
        vec![json!(organization_id)],
    )
    .await?;
    Ok(MembershipListResponse {
        memberships: rows
            .into_iter()
            .map(membership_from_row)
            .collect::<AuthStackResult<Vec<_>>>()?,
    })
}

pub async fn create_invitation(
    organization_id: &str,
    email: &str,
    role_id: &str,
    actor_user_id: &str,
) -> AuthStackResult<InvitationSummary> {
    require_membership_permission(organization_id, actor_user_id, "member.invite").await?;
    ensure_assignable_role(organization_id, role_id, false).await?;
    let now = now_ms();
    let invitation_id = secure_storage_id("invitation")?;
    let token = format!("invite_{}", URL_SAFE_NO_PAD.encode(random_bytes(32)?));
    let token_hash = one_time_token_hash(&token);
    let expires_at_ms = now.saturating_add(7 * 24 * 60 * 60 * 1_000);
    execute_sql_atomic(vec![
        AtomicSqlStatement::execute(
            "INSERT INTO auth_invitations \
         (invitation_id, organization_id, normalized_email, role_id, token_hash, status, \
          expires_at_ms, accepted_at_ms, created_by, created_at_ms, updated_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, NULL, ?7, ?8, ?8)",
            vec![
                json!(&invitation_id),
                json!(organization_id),
                json!(normalize_email(email)),
                json!(role_id),
                json!(token_hash),
                json!(expires_at_ms),
                json!(actor_user_id),
                json!(now),
            ],
        ),
        mail_outbox_statement(
            "invitation",
            email,
            "Organization invitation",
            &format!("Accept your invitation: /invitations/accept?token={token}"),
            now,
        )
        .await?,
        storage_event_statement(
            "auth_invitation",
            &invitation_id,
            "auth_invitation_created",
            json!({
                "invitation_id": &invitation_id,
                "organization_id": organization_id,
                "normalized_email": normalize_email(email),
                "role_id": role_id,
                "expires_at_ms": expires_at_ms,
            }),
        )?,
        audit_event_statement(
            Some(organization_id),
            actor_user_id,
            "member.invite",
            "invitation",
            &invitation_id,
            "success",
            now,
        ),
    ])
    .await?;
    Ok(InvitationSummary {
        invitation_id,
        organization_id: organization_id.to_owned(),
        email: normalize_email(email),
        role_id: role_id.to_owned(),
        status: "pending".to_owned(),
        expires_at_ms,
    })
}

pub async fn list_invitations(
    organization_id: &str,
    actor_user_id: &str,
) -> AuthStackResult<InvitationListResponse> {
    require_membership_permission(organization_id, actor_user_id, "member.view").await?;
    let rows = execute_sql(
        "SELECT invitation_id, organization_id, normalized_email, role_id, status, expires_at_ms \
         FROM auth_invitations WHERE organization_id = ?1 ORDER BY created_at_ms DESC",
        vec![json!(organization_id)],
    )
    .await?;
    Ok(InvitationListResponse {
        invitations: rows
            .into_iter()
            .map(invitation_from_row)
            .collect::<AuthStackResult<Vec<_>>>()?,
    })
}

pub async fn accept_invitation(
    token: &str,
    user_id: &str,
    primary_email: &str,
    assurance: &str,
) -> AuthStackResult<OrganizationSummary> {
    initialize_schema_async().await?;
    let token_hash = one_time_token_hash(token);
    let rows = execute_sql(
        "SELECT invitation_id, organization_id, normalized_email, role_id, status, expires_at_ms \
         FROM auth_invitations WHERE token_hash = ?1 LIMIT 1",
        vec![json!(token_hash)],
    )
    .await?;
    let invitation = rows
        .into_iter()
        .next()
        .map(invitation_from_row)
        .transpose()?
        .ok_or(AuthStackError::InvalidToken)?;
    if invitation.status != "pending"
        || invitation.expires_at_ms <= now_ms()
        || invitation.email != normalize_email(primary_email)
    {
        return Err(AuthStackError::InvalidToken);
    }
    if matches!(invitation.role_id.as_str(), "owner" | "admin") && assurance != "aal2" {
        return Err(AuthStackError::AuthRequired);
    }
    ensure_assignable_role(&invitation.organization_id, &invitation.role_id, false).await?;
    let now = now_ms();
    let subject = spicedb_subject_for_user(user_id).await;
    execute_sql_atomic(vec![
        AtomicSqlStatement::guard(
            "UPDATE auth_invitations \
             SET status = 'accepted', accepted_at_ms = ?1, updated_at_ms = ?1 \
             WHERE invitation_id = ?2 AND status = 'pending' AND expires_at_ms >= ?1 \
             RETURNING invitation_id",
            vec![json!(now), json!(&invitation.invitation_id)],
        ),
        AtomicSqlStatement::execute(
            "INSERT INTO auth_memberships \
             (organization_id, user_id, role_id, status, joined_at_ms, updated_at_ms) \
             VALUES (?1, ?2, ?3, 'active', ?4, ?4) \
             ON CONFLICT(organization_id, user_id) DO UPDATE SET \
             role_id = excluded.role_id, status = 'active', updated_at_ms = excluded.updated_at_ms",
            vec![
                json!(&invitation.organization_id),
                json!(user_id),
                json!(&invitation.role_id),
                json!(now),
            ],
        ),
        bump_organization_authorization_revision_statement(&invitation.organization_id, now),
        relationship_outbox_statement(
            "grant",
            &format!("organization:{}", invitation.organization_id),
            "member",
            &subject,
            now,
        )?,
        storage_event_statement(
            "auth_membership",
            &format!("{}:{user_id}", invitation.organization_id),
            "auth_invitation_accepted",
            json!({
                "invitation_id": &invitation.invitation_id,
                "organization_id": &invitation.organization_id,
                "user_id": user_id,
                "role_id": &invitation.role_id,
            }),
        )?,
        audit_event_statement(
            Some(&invitation.organization_id),
            user_id,
            "invitation.accept",
            "invitation",
            &invitation.invitation_id,
            "success",
            now,
        ),
    ])
    .await?;
    organization_for_user(&invitation.organization_id, user_id).await
}

pub async fn list_roles(
    organization_id: &str,
    actor_user_id: &str,
) -> AuthStackResult<RoleListResponse> {
    require_membership_permission(organization_id, actor_user_id, "role.view").await?;
    let rows = execute_sql(
        "SELECT organization_id, role_id, name, built_in, permissions_json \
         FROM auth_roles WHERE organization_id = ?1 ORDER BY built_in DESC, name ASC",
        vec![json!(organization_id)],
    )
    .await?;
    Ok(RoleListResponse {
        roles: rows
            .into_iter()
            .map(role_from_row)
            .collect::<AuthStackResult<Vec<_>>>()?,
    })
}

pub async fn upsert_custom_role(
    organization_id: &str,
    role_id: &str,
    name: &str,
    permissions: &[String],
    actor_user_id: &str,
) -> AuthStackResult<RoleSummary> {
    require_membership_permission(organization_id, actor_user_id, "role.manage").await?;
    if built_in_role_permissions(role_id).is_some() {
        return Err(AuthStackError::validation(
            "built-in roles cannot be replaced",
        ));
    }
    let catalog = organization_permission_catalog();
    if permissions
        .iter()
        .any(|permission| permission == "ownership.transfer" || !catalog.contains(permission))
    {
        return Err(AuthStackError::validation(
            "custom role contains a restricted permission",
        ));
    }
    let now = now_ms();
    let permissions_json = serde_json::to_string(permissions)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;
    execute_sql_atomic(vec![
        AtomicSqlStatement::execute(
            "INSERT INTO auth_roles \
             (organization_id, role_id, name, built_in, permissions_json, created_at_ms, updated_at_ms) \
             VALUES (?1, ?2, ?3, 0, ?4, ?5, ?5) \
             ON CONFLICT(organization_id, role_id) DO UPDATE SET \
             name = excluded.name, permissions_json = excluded.permissions_json, updated_at_ms = excluded.updated_at_ms",
            vec![
                json!(organization_id),
                json!(role_id),
                json!(name),
                json!(permissions_json),
                json!(now),
            ],
        ),
        bump_organization_authorization_revision_statement(organization_id, now),
        storage_event_statement(
            "auth_role",
            &format!("{organization_id}:{role_id}"),
            "auth_custom_role_upserted",
            json!({
                "organization_id": organization_id,
                "role_id": role_id,
                "name": name,
                "permissions": permissions,
            }),
        )?,
        audit_event_statement(
            Some(organization_id),
            actor_user_id,
            "role.manage",
            "role",
            role_id,
            "success",
            now,
        ),
    ])
    .await?;
    let roles = list_roles(organization_id, actor_user_id).await?;
    roles
        .roles
        .into_iter()
        .find(|role| role.role_id == role_id)
        .ok_or_else(|| AuthStackError::store("new role was not readable"))
}

pub async fn assign_membership_role(
    organization_id: &str,
    user_id: &str,
    role_id: &str,
    actor_user_id: &str,
) -> AuthStackResult<MembershipSummary> {
    require_membership_permission(organization_id, actor_user_id, "member.manage").await?;
    ensure_assignable_role(organization_id, role_id, true).await?;
    preserve_final_owner(organization_id, user_id, role_id == "owner").await?;
    let now = now_ms();
    execute_sql_atomic(vec![
        lock_organization_statement(organization_id),
        AtomicSqlStatement::guard(
            "UPDATE auth_memberships SET role_id = ?1, updated_at_ms = ?2 \
             WHERE organization_id = ?3 AND user_id = ?4 AND status = 'active' \
               AND (?1 = 'owner' OR role_id <> 'owner' OR ( \
                 SELECT COUNT(*) FROM auth_memberships owners \
                 WHERE owners.organization_id = ?3 AND owners.role_id = 'owner' \
                   AND owners.status = 'active' \
               ) > 1) \
             RETURNING user_id",
            vec![
                json!(role_id),
                json!(now),
                json!(organization_id),
                json!(user_id),
            ],
        ),
        bump_organization_authorization_revision_statement(organization_id, now),
        storage_event_statement(
            "auth_membership",
            &format!("{organization_id}:{user_id}"),
            "auth_membership_role_assigned",
            json!({
                "organization_id": organization_id,
                "user_id": user_id,
                "role_id": role_id,
            }),
        )?,
        audit_event_statement(
            Some(organization_id),
            actor_user_id,
            "member.manage",
            "membership",
            user_id,
            "success",
            now,
        ),
    ])
    .await?;
    list_memberships(organization_id, actor_user_id)
        .await?
        .memberships
        .into_iter()
        .find(|membership| membership.user_id == user_id)
        .ok_or_else(|| AuthStackError::not_found("membership was not found"))
}

pub async fn remove_membership(
    organization_id: &str,
    user_id: &str,
    actor_user_id: &str,
) -> AuthStackResult<()> {
    require_membership_permission(organization_id, actor_user_id, "member.manage").await?;
    preserve_final_owner(organization_id, user_id, false).await?;
    let now = now_ms();
    let subject = spicedb_subject_for_user(user_id).await;
    execute_sql_atomic(vec![
        lock_organization_statement(organization_id),
        AtomicSqlStatement::guard(
            "UPDATE auth_memberships SET status = 'removed', updated_at_ms = ?1 \
             WHERE organization_id = ?2 AND user_id = ?3 AND status = 'active' \
               AND (role_id <> 'owner' OR ( \
                 SELECT COUNT(*) FROM auth_memberships owners \
                 WHERE owners.organization_id = ?2 AND owners.role_id = 'owner' \
                   AND owners.status = 'active' \
               ) > 1) \
             RETURNING user_id",
            vec![json!(now), json!(organization_id), json!(user_id)],
        ),
        bump_organization_authorization_revision_statement(organization_id, now),
        relationship_outbox_statement(
            "revoke",
            &format!("organization:{organization_id}"),
            "member",
            &subject,
            now,
        )?,
        audit_event_statement(
            Some(organization_id),
            actor_user_id,
            "member.remove",
            "membership",
            user_id,
            "success",
            now,
        ),
    ])
    .await?;
    Ok(())
}

pub async fn list_admin_users() -> AuthStackResult<AdminUserListResponse> {
    initialize_schema_async().await?;
    let rows = execute_sql(
        "SELECT user_id, primary_email, disabled, email_verified, created_at_ms \
         FROM auth_users ORDER BY created_at_ms DESC",
        Vec::new(),
    )
    .await?;
    Ok(AdminUserListResponse {
        users: rows
            .into_iter()
            .map(admin_user_from_row)
            .collect::<AuthStackResult<Vec<_>>>()?,
    })
}

pub async fn set_user_disabled(
    user_id: &str,
    disabled: bool,
    actor_user_id: &str,
) -> AuthStackResult<AdminUserSummary> {
    if disabled {
        ensure_user_is_not_final_owner(user_id).await?;
    }
    let now = now_ms();
    let mut organization_ids = execute_sql(
        "SELECT organization_id FROM auth_memberships \
         WHERE user_id = ?1 AND status IN ('active', 'blocked') ORDER BY organization_id",
        vec![json!(user_id)],
    )
    .await?
    .iter()
    .map(|row| required_string(row, "organization_id"))
    .collect::<AuthStackResult<Vec<_>>>()?;
    organization_ids.sort();
    organization_ids.dedup();
    let mut statements = organization_ids
        .iter()
        .map(|organization_id| lock_organization_statement(organization_id))
        .collect::<Vec<_>>();
    statements.push(AtomicSqlStatement::guard(
        "UPDATE auth_users SET disabled = ?1, security_revision = security_revision + 1, updated_at_ms = ?2 \
         WHERE user_id = ?3 AND ( \
           ?1 = 0 OR NOT EXISTS ( \
             SELECT 1 FROM auth_memberships owned \
             WHERE owned.user_id = ?3 AND owned.role_id = 'owner' AND owned.status = 'active' \
               AND (SELECT COUNT(*) FROM auth_memberships owners \
                    WHERE owners.organization_id = owned.organization_id \
                      AND owners.role_id = 'owner' AND owners.status = 'active') <= 1 \
           ) \
         ) RETURNING user_id",
        vec![json!(i32::from(disabled)), json!(now), json!(user_id)],
    ));
    if disabled {
        statements.push(AtomicSqlStatement::execute(
            "UPDATE auth_sessions SET revoked_at_ms = ?1, updated_at_ms = ?1 \
             WHERE user_id = ?2 AND revoked_at_ms IS NULL",
            vec![json!(now), json!(user_id)],
        ));
        statements.push(AtomicSqlStatement::execute(
            "UPDATE auth_memberships SET status = 'blocked', updated_at_ms = ?1 \
             WHERE user_id = ?2 AND status = 'active'",
            vec![json!(now), json!(user_id)],
        ));
    } else {
        statements.push(AtomicSqlStatement::execute(
            "UPDATE auth_memberships SET status = 'active', updated_at_ms = ?1 \
             WHERE user_id = ?2 AND status = 'blocked'",
            vec![json!(now), json!(user_id)],
        ));
    }
    for organization_id in &organization_ids {
        statements.push(bump_organization_authorization_revision_statement(
            organization_id,
            now,
        ));
    }
    statements.push(audit_event_statement(
        None,
        actor_user_id,
        if disabled {
            "system.user.disable"
        } else {
            "system.user.enable"
        },
        "user",
        user_id,
        "success",
        now,
    ));
    execute_sql_atomic(statements).await?;
    list_admin_users()
        .await?
        .users
        .into_iter()
        .find(|user| user.user_id == user_id)
        .ok_or_else(|| AuthStackError::not_found("user was not found"))
}

fn bump_organization_authorization_revision_statement(
    organization_id: &str,
    now: u64,
) -> AtomicSqlStatement {
    AtomicSqlStatement::guard(
        "UPDATE auth_organizations \
         SET authorization_revision = authorization_revision + 1, updated_at_ms = ?1 \
         WHERE organization_id = ?2 RETURNING organization_id",
        vec![json!(now), json!(organization_id)],
    )
}

fn lock_organization_statement(organization_id: &str) -> AtomicSqlStatement {
    // A no-op UPDATE provides one portable row lock for both Spin PostgreSQL
    // and SQLite. Every owner-changing path takes it before counting owners.
    AtomicSqlStatement::guard(
        "UPDATE auth_organizations SET updated_at_ms = updated_at_ms \
         WHERE organization_id = ?1 RETURNING organization_id",
        vec![json!(organization_id)],
    )
}

pub async fn list_policy_versions() -> AuthStackResult<PolicyVersionListResponse> {
    initialize_schema_async().await?;
    let rows = execute_sql(
        "SELECT version_id, status, policy_hash, published_by, created_at_ms \
         FROM auth_policy_versions ORDER BY created_at_ms DESC",
        Vec::new(),
    )
    .await?;
    Ok(PolicyVersionListResponse {
        versions: rows
            .into_iter()
            .map(policy_version_from_row)
            .collect::<AuthStackResult<Vec<_>>>()?,
    })
}

pub async fn publish_policy_version(
    policy_text: &str,
    schema_text: &str,
    actor_user_id: &str,
) -> AuthStackResult<PolicyVersionSummary> {
    let now = now_ms();
    let version_id = secure_storage_id("policy")?;
    let policy_hash = URL_SAFE_NO_PAD.encode(Sha256::digest(
        format!("{schema_text}\n{policy_text}").as_bytes(),
    ));
    execute_sql_atomic(vec![
        AtomicSqlStatement::execute(
            "UPDATE auth_policy_versions SET status = 'retired', updated_at_ms = ?1 \
             WHERE status = 'active'",
            vec![json!(now)],
        ),
        AtomicSqlStatement::execute(
            "INSERT INTO auth_policy_versions \
             (version_id, status, policy_hash, policy_text, schema_text, published_by, created_at_ms, updated_at_ms) \
             VALUES (?1, 'active', ?2, ?3, ?4, ?5, ?6, ?6)",
            vec![
                json!(&version_id),
                json!(&policy_hash),
                json!(policy_text),
                json!(schema_text),
                json!(actor_user_id),
                json!(now),
            ],
        ),
        audit_event_statement(
            None,
            actor_user_id,
            "system.policy.publish",
            "policy_version",
            &version_id,
            "success",
            now,
        ),
    ])
    .await?;
    Ok(PolicyVersionSummary {
        version_id,
        status: "active".to_owned(),
        policy_hash,
        published_by: actor_user_id.to_owned(),
        created_at_ms: now,
    })
}

pub async fn health_status() -> AuthStackResult<HealthStatusResponse> {
    initialize_schema_async().await?;
    Ok(HealthStatusResponse {
        status: "ok".to_owned(),
        storage_backend: match storage_backend().await? {
            StorageBackend::Sqlite => "sqlite",
            StorageBackend::Postgres => "postgres",
        }
        .to_owned(),
        mail_transport: mail_transport().await,
        authorization_provider: "embedded-cedar".to_owned(),
        production_mode: config_bool(AUTH_PRODUCTION_MODE, false).await,
    })
}

pub async fn list_audit_events(
    organization_id: Option<&str>,
    after_cursor: u64,
    limit: usize,
) -> AuthStackResult<AuditEventListResponse> {
    initialize_schema_async().await?;
    let limit = limit.clamp(1, 100);
    let rows = if let Some(organization_id) = organization_id {
        execute_sql(
            "SELECT sequence, organization_id, actor_user_id, action, target_type, target_id, outcome, recorded_at_ms \
             FROM auth_audit_events WHERE organization_id = ?1 AND sequence > ?2 \
             ORDER BY sequence ASC LIMIT ?3",
            vec![json!(organization_id), json!(after_cursor), json!(limit)],
        )
        .await?
    } else {
        execute_sql(
            "SELECT sequence, organization_id, actor_user_id, action, target_type, target_id, outcome, recorded_at_ms \
             FROM auth_audit_events WHERE sequence > ?1 ORDER BY sequence ASC LIMIT ?2",
            vec![json!(after_cursor), json!(limit)],
        )
        .await?
    };
    let events = rows
        .into_iter()
        .map(audit_event_from_row)
        .collect::<AuthStackResult<Vec<_>>>()?;
    let next_cursor = events.last().map_or(after_cursor, |event| event.sequence);
    Ok(AuditEventListResponse {
        events,
        next_cursor,
    })
}

fn audit_event_statement(
    organization_id: Option<&str>,
    actor_user_id: &str,
    action: &str,
    target_type: &str,
    target_id: &str,
    outcome: &str,
    recorded_at_ms: u64,
) -> AtomicSqlStatement {
    AtomicSqlStatement::execute(
        "INSERT INTO auth_audit_events \
         (organization_id, actor_user_id, action, target_type, target_id, outcome, recorded_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        vec![
            organization_id.map_or(Value::Null, |value| json!(value)),
            json!(actor_user_id),
            json!(action),
            json!(target_type),
            json!(target_id),
            json!(outcome),
            json!(recorded_at_ms),
        ],
    )
}

async fn require_membership_permission(
    organization_id: &str,
    user_id: &str,
    permission: &str,
) -> AuthStackResult<Vec<String>> {
    let rows = execute_sql(
        "SELECT roles.permissions_json FROM auth_memberships memberships \
         JOIN auth_roles roles ON roles.organization_id = memberships.organization_id \
          AND roles.role_id = memberships.role_id \
         WHERE memberships.organization_id = ?1 AND memberships.user_id = ?2 \
          AND memberships.status = 'active' LIMIT 1",
        vec![json!(organization_id), json!(user_id)],
    )
    .await?;
    let row = rows.into_iter().next().ok_or(AuthStackError::Forbidden)?;
    let permissions = permissions_from_row(&row)?;
    if permissions.iter().any(|candidate| candidate == permission) {
        Ok(permissions)
    } else {
        Err(AuthStackError::Forbidden)
    }
}

async fn ensure_assignable_role(
    organization_id: &str,
    role_id: &str,
    allow_owner: bool,
) -> AuthStackResult<()> {
    if role_id == "owner" && !allow_owner {
        return Err(AuthStackError::validation(
            "ownership requires an explicit transfer operation",
        ));
    }
    let rows = execute_sql(
        "SELECT role_id FROM auth_roles WHERE organization_id = ?1 AND role_id = ?2 LIMIT 1",
        vec![json!(organization_id), json!(role_id)],
    )
    .await?;
    if rows.is_empty() {
        Err(AuthStackError::validation("role does not exist"))
    } else {
        Ok(())
    }
}

async fn preserve_final_owner(
    organization_id: &str,
    user_id: &str,
    remains_owner: bool,
) -> AuthStackResult<()> {
    if remains_owner {
        return Ok(());
    }
    let rows = execute_sql(
        "SELECT role_id FROM auth_memberships WHERE organization_id = ?1 AND user_id = ?2 \
         AND status = 'active' LIMIT 1",
        vec![json!(organization_id), json!(user_id)],
    )
    .await?;
    if rows
        .first()
        .and_then(|row| row_string(row, "role_id"))
        .as_deref()
        != Some("owner")
    {
        return Ok(());
    }
    let rows = execute_sql(
        "SELECT COUNT(*) AS owner_count FROM auth_memberships \
         WHERE organization_id = ?1 AND role_id = 'owner' AND status = 'active'",
        vec![json!(organization_id)],
    )
    .await?;
    let owner_count = rows
        .first()
        .and_then(|row| row_i64(row, "owner_count"))
        .unwrap_or_default();
    if owner_count <= 1 {
        Err(AuthStackError::conflict(
            "organization must retain at least one owner",
        ))
    } else {
        Ok(())
    }
}

async fn ensure_user_is_not_final_owner(user_id: &str) -> AuthStackResult<()> {
    let rows = execute_sql(
        "SELECT organization_id FROM auth_memberships \
         WHERE user_id = ?1 AND role_id = 'owner' AND status = 'active'",
        vec![json!(user_id)],
    )
    .await?;
    for row in rows {
        let organization_id = required_string(&row, "organization_id")?;
        preserve_final_owner(&organization_id, user_id, false).await?;
    }
    Ok(())
}

fn permissions_from_row(row: &Value) -> AuthStackResult<Vec<String>> {
    let permissions_json = row_string(row, "permissions_json").unwrap_or_else(|| "[]".to_owned());
    serde_json::from_str(&permissions_json).map_err(|error| {
        AuthStackError::store(format!("stored role permissions are invalid: {error}"))
    })
}

fn organization_from_row(row: Value) -> AuthStackResult<OrganizationSummary> {
    Ok(OrganizationSummary {
        organization_id: required_string(&row, "organization_id")?,
        name: required_string(&row, "name")?,
        status: required_string(&row, "status")?,
        current_user_role: required_string(&row, "role_id")?,
        permissions: permissions_from_row(&row)?,
        created_at_ms: row_i64(&row, "created_at_ms").unwrap_or_default() as u64,
    })
}

fn membership_from_row(row: Value) -> AuthStackResult<MembershipSummary> {
    Ok(MembershipSummary {
        organization_id: required_string(&row, "organization_id")?,
        user_id: required_string(&row, "user_id")?,
        primary_email: required_string(&row, "primary_email")?,
        role_id: required_string(&row, "role_id")?,
        status: required_string(&row, "status")?,
        joined_at_ms: row_i64(&row, "joined_at_ms").unwrap_or_default() as u64,
    })
}

fn invitation_from_row(row: Value) -> AuthStackResult<InvitationSummary> {
    Ok(InvitationSummary {
        invitation_id: required_string(&row, "invitation_id")?,
        organization_id: required_string(&row, "organization_id")?,
        email: required_string(&row, "normalized_email")?,
        role_id: required_string(&row, "role_id")?,
        status: required_string(&row, "status")?,
        expires_at_ms: row_i64(&row, "expires_at_ms").unwrap_or_default() as u64,
    })
}

fn role_from_row(row: Value) -> AuthStackResult<RoleSummary> {
    Ok(RoleSummary {
        organization_id: required_string(&row, "organization_id")?,
        role_id: required_string(&row, "role_id")?,
        name: required_string(&row, "name")?,
        built_in: row_bool(&row, "built_in").unwrap_or(false),
        permissions: permissions_from_row(&row)?,
    })
}

fn admin_user_from_row(row: Value) -> AuthStackResult<AdminUserSummary> {
    Ok(AdminUserSummary {
        user_id: required_string(&row, "user_id")?,
        primary_email: required_string(&row, "primary_email")?,
        disabled: row_bool(&row, "disabled").unwrap_or(false),
        email_verified: row_bool(&row, "email_verified").unwrap_or(false),
        created_at_ms: row_i64(&row, "created_at_ms").unwrap_or_default() as u64,
    })
}

fn policy_version_from_row(row: Value) -> AuthStackResult<PolicyVersionSummary> {
    Ok(PolicyVersionSummary {
        version_id: required_string(&row, "version_id")?,
        status: required_string(&row, "status")?,
        policy_hash: required_string(&row, "policy_hash")?,
        published_by: required_string(&row, "published_by")?,
        created_at_ms: row_i64(&row, "created_at_ms").unwrap_or_default() as u64,
    })
}

fn audit_event_from_row(row: Value) -> AuthStackResult<AuditEventSummary> {
    Ok(AuditEventSummary {
        sequence: row_i64(&row, "sequence").unwrap_or_default() as u64,
        organization_id: row_string(&row, "organization_id"),
        actor_user_id: required_string(&row, "actor_user_id")?,
        action: required_string(&row, "action")?,
        target_type: required_string(&row, "target_type")?,
        target_id: required_string(&row, "target_id")?,
        outcome: required_string(&row, "outcome")?,
        recorded_at_ms: row_i64(&row, "recorded_at_ms").unwrap_or_default() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sqlite_schema_statements() -> Vec<&'static str> {
        schema_migration(StorageBackend::Sqlite)
            .expect("SQLite migration")
            .statements()
            .expect("valid embedded migration SQL")
    }

    #[test]
    fn row_string_serializes_structured_json_values() {
        let row = json!({
            "payload": { "email": "person@example.test" },
            "scopes_json": ["openid", "email"],
            "enabled": true
        });

        assert_eq!(
            row_string(&row, "payload").as_deref(),
            Some(r#"{"email":"person@example.test"}"#)
        );
        assert_eq!(
            row_string(&row, "scopes_json").as_deref(),
            Some(r#"["openid","email"]"#)
        );
        assert_eq!(row_string(&row, "enabled").as_deref(), Some("true"));
    }

    #[test]
    fn provider_display_name_formats_custom_provider_id() {
        assert_eq!(
            provider_display_name("github-enterprise"),
            "Github Enterprise"
        );
    }

    #[test]
    fn legacy_schema_checksum_upgrade_is_only_needed_for_old_tables() {
        assert!(!schema_checksum_is_missing(&[]));
        assert!(schema_checksum_is_missing(&[
            json!({ "name": "version" }),
            json!({ "name": "applied_at_ms" }),
        ]));
        assert!(!schema_checksum_is_missing(&[
            json!({ "name": "version" }),
            json!({ "name": "checksum" }),
        ]));
    }

    #[test]
    fn password_hash_verifies_and_rejects_wrong_password() {
        futures::executor::block_on(async {
            let stored_hash = hash_password("correct horse battery staple").await.unwrap();

            assert_eq!(
                verify_password("correct horse battery staple", &stored_hash)
                    .await
                    .unwrap(),
                PasswordVerification::ValidCurrent
            );
            assert_eq!(
                verify_password("wrong password", &stored_hash)
                    .await
                    .unwrap(),
                PasswordVerification::Invalid
            );
        });
    }

    #[test]
    fn synthetic_oauth_email_contains_provider_and_state() {
        assert_eq!(
            synthetic_oauth_email("github-enterprise", "state_123"),
            "oauth+github-enterprise+state_123@auth.local"
        );
    }

    #[test]
    fn parse_jwt_key_ring_accepts_array_shape() {
        let keys = parse_jwt_key_ring(
            r#"[{"kid":"key-a","alg":"HS256","secret":"a","status":"active"},{"kid":"key-b","alg":"HS256","secret":"b","status":"next"}]"#,
        )
        .unwrap();

        assert_eq!(keys[0].kid, "key-a");
    }

    #[test]
    fn parse_jwt_key_ring_rejects_unknown_status() {
        let error =
            parse_jwt_key_ring(r#"[{"kid":"key-a","alg":"HS256","secret":"a","status":"lost"}]"#)
                .unwrap_err();

        assert_eq!(error.public_code(), "validation");
    }

    #[test]
    fn signing_key_policy_allows_development_runtime_key() {
        let keys = vec![test_signing_key(
            "dev-key",
            Algorithm::HS256,
            SigningKeyStatus::Active,
        )];

        assert!(validate_signing_key_policy(false, false, &keys).is_ok());
    }

    #[test]
    fn signing_key_policy_rejects_production_runtime_key() {
        let keys = vec![test_signing_key(
            "dev-key",
            Algorithm::HS256,
            SigningKeyStatus::Active,
        )];
        let error = validate_signing_key_policy(true, false, &keys).unwrap_err();

        assert_eq!(error.public_code(), "configuration");
    }

    #[test]
    fn signing_key_policy_rejects_hs256_key_ring_in_production() {
        let keys = vec![test_signing_key(
            "shared-secret",
            Algorithm::HS256,
            SigningKeyStatus::Active,
        )];
        let error = validate_signing_key_policy(true, true, &keys).unwrap_err();

        assert_eq!(error.public_code(), "configuration");
    }

    #[test]
    fn signing_key_policy_accepts_es256_key_ring_in_production() {
        let keys = vec![test_signing_key(
            "prod-key",
            Algorithm::ES256,
            SigningKeyStatus::Active,
        )];

        assert!(validate_signing_key_policy(true, true, &keys).is_ok());
    }

    #[test]
    fn es256_key_ring_signs_and_publishes_public_jwks() {
        const TEST_P256_PKCS8_DER_BASE64: &str = "MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgvySSFOzwp41lOcaMdosf96P2hC1dvpZPx7Umh0QCxh2hRANCAARhnbxIrV4KQ/EHYCZtafMbLZoTs4VwivefVFoOG+kVb7QvMT4GMYGUYTr4WkEohW20V77/FbqJ/mCS5ceSMUie";
        let mut key = test_signing_key("prod-key", Algorithm::ES256, SigningKeyStatus::Active);
        key.private_key_der_base64 = Some(TEST_P256_PKCS8_DER_BASE64.to_string());
        key.public_jwks_json = None;

        let (encoding_key, jwks) = futures::executor::block_on(async {
            (
                jwt_encoding_key_for_config(&key).await.unwrap(),
                jwt_public_jwks_for_config(&key).await.unwrap().unwrap(),
            )
        });
        let public_key = jwks.keys.first().unwrap();
        assert_eq!(public_key.kid, "prod-key");
        assert_eq!(public_key.kty, "EC");
        assert_eq!(public_key.alg, "ES256");
        assert_eq!(public_key.public_parameters.get("crv").unwrap(), "P-256");
        assert!(public_key.public_parameters.contains_key("x"));
        assert!(public_key.public_parameters.contains_key("y"));
        assert!(!public_key.public_parameters.contains_key("d"));

        let now = now_ms() / 1_000;
        let claims = AccessTokenClaims::for_user(
            "https://issuer.example",
            UserId::from("user-1"),
            vec!["fullstack-app".to_string()],
            now + 300,
            now,
            "token-1",
        );
        let token = encode_access_token(&claims, &encoding_key, Algorithm::ES256, Some("prod-key"))
            .unwrap();
        let decoding_key = decoding_key_from_jwks_key(public_key, Algorithm::ES256).unwrap();
        assert!(
            decode_access_token(
                &token,
                &decoding_key,
                "https://issuer.example",
                "fullstack-app",
                &[Algorithm::ES256],
            )
            .is_ok()
        );
    }

    #[test]
    fn schema_defines_password_credentials_table() {
        assert!(
            sqlite_schema_statements()
                .iter()
                .any(|statement| statement.contains("auth_password_credentials"))
        );
    }

    #[test]
    fn schema_defines_external_identities_table() {
        assert!(
            sqlite_schema_statements()
                .iter()
                .any(|statement| statement.contains("auth_external_identities"))
        );
    }

    #[test]
    fn schema_defines_signing_key_lifecycle_table() {
        assert!(
            sqlite_schema_statements()
                .iter()
                .any(|statement| statement.contains("auth_signing_keys"))
        );
    }

    #[test]
    fn schema_defines_encrypted_totp_and_recovery_code_tables() {
        assert!(
            sqlite_schema_statements()
                .iter()
                .any(|statement| statement.contains("auth_mfa_totp")
                    && statement.contains("encrypted_secret"))
        );
        assert!(
            sqlite_schema_statements()
                .iter()
                .any(|statement| statement.contains("auth_recovery_codes")
                    && statement.contains("code_hash"))
        );
    }

    #[test]
    fn mfa_vault_ciphertext_authenticates_user_and_credential_context() {
        let key = [7_u8; 32];
        let plaintext = b"0123456789abcdef0123";
        let encoded = encrypt_mfa_secret_with_key(&key, "user-one", "totp-one", plaintext).unwrap();

        assert!(!encoded.contains("0123456789abcdef"));
        assert_eq!(
            decrypt_mfa_secret_with_key(&key, "user-one", "totp-one", &encoded).unwrap(),
            plaintext
        );
        assert!(decrypt_mfa_secret_with_key(&key, "user-two", "totp-one", &encoded).is_err());
    }

    #[test]
    fn mail_outbox_ciphertext_hides_tokens_and_authenticates_message_context() {
        let key = [11_u8; 32];
        let plaintext = br#"{"recipient":"user@example.test","subject":"Reset","body_text":"/reset?token=one-time-secret"}"#;
        let encoded = encrypt_mail_payload_with_key(&key, "mail-one", plaintext).unwrap();

        assert!(!encoded.contains("one-time-secret"));
        assert!(!encoded.contains("user@example.test"));
        assert_eq!(
            decrypt_mail_payload_with_key(&key, "mail-one", &encoded).unwrap(),
            plaintext
        );
        assert!(decrypt_mail_payload_with_key(&key, "mail-two", &encoded).is_err());
    }

    #[test]
    fn mail_outbox_schema_contains_only_hashes_and_encrypted_payloads() {
        let statement = sqlite_schema_statements()
            .into_iter()
            .find(|statement| statement.contains("CREATE TABLE IF NOT EXISTS auth_mail_outbox"))
            .expect("mail outbox schema");

        assert!(statement.contains("recipient_hash"));
        assert!(statement.contains("payload_encrypted"));
        assert!(!statement.contains("recipient TEXT"));
        assert!(!statement.contains("body_text"));
    }

    #[test]
    fn postgres_sql_rewrites_indexed_placeholders_and_autoincrement() {
        let sql = "CREATE TABLE events (sequence INTEGER PRIMARY KEY AUTOINCREMENT, recorded_at_ms INTEGER NOT NULL); SELECT * FROM events WHERE aggregate_id = ?1 AND revision = ?2";

        let rewritten = postgres_sql(sql);

        assert!(rewritten.contains("sequence BIGSERIAL PRIMARY KEY"));
        assert!(rewritten.contains("recorded_at_ms BIGINT NOT NULL"));
        assert!(rewritten.contains("aggregate_id = $1 AND revision = $2"));
    }

    #[test]
    fn postgres_sql_rewrites_insert_or_ignore() {
        let sql =
            "INSERT OR IGNORE INTO auth_provider_configs (tenant_id, provider_id) VALUES (?1, ?2)";

        let rewritten = postgres_sql(sql);

        assert_eq!(
            rewritten,
            "INSERT INTO auth_provider_configs (tenant_id, provider_id) VALUES ($1, $2) ON CONFLICT DO NOTHING"
        );
    }

    #[test]
    fn projected_sessions_preserve_typed_assurance_on_both_sql_backends() {
        assert!(PROJECT_SESSION_UPSERT_SQL.contains("primary_email, assurance, permissions_json"));
        assert!(PROJECT_SESSION_UPSERT_SQL.contains("assurance = excluded.assurance"));
        assert_eq!(
            projected_session_assurance(&json!({ "assurance": "aal2" })).unwrap(),
            "aal2"
        );
        assert_eq!(projected_session_assurance(&json!({})).unwrap(), "aal1");
        assert!(projected_session_assurance(&json!({ "assurance": "root" })).is_err());
    }

    fn test_signing_key(
        kid: &str,
        algorithm: Algorithm,
        status: SigningKeyStatus,
    ) -> JwtSigningKeyConfig {
        JwtSigningKeyConfig {
            kid: kid.to_string(),
            algorithm,
            secret: Some("shared-secret".to_string()),
            private_key_der_base64: Some("private-key-der".to_string()),
            public_jwks_json: Some(r#"{"keys":[]}"#.to_string()),
            status,
            source: "test".to_string(),
        }
    }
}
