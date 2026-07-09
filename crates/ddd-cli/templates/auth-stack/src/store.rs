use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use argon2::{Algorithm as Argon2Algorithm, Argon2, Params as Argon2Params, Version as Argon2Version};
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use ddd_auth::{
    access_token_key_id, decode_access_token, encode_access_token, AccessTokenClaims, Algorithm,
    DecodingKey, EncodingKey, Jwk, JwksDocument, JwksKey, SessionId, TenantId, UserId,
};
use ddd_auth::passkeys::{
    Attachment as PasskeyAttachment, AuthenticationResponse as PasskeyAuthenticationResponse,
    AuthenticationState as PasskeyAuthenticationState, CredentialId as WebauthnCredentialId,
    PasskeyCredential as WebauthnPasskeyCredential,
    RegistrationResponse as PasskeyRegistrationResponse,
    RegistrationState as PasskeyRegistrationState, Webauthn,
};
use ddd_authz::{AuthorizationModel, ObjectRef, Relation, RelationshipTuple, SubjectRef, TenantRef};
use futures::lock::Mutex;
use hmac::Hmac;
use pbkdf2::pbkdf2;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::contracts::{
    AuthProviderSummary, AuthzModelReadResponse, AuthzModelWriteRequest, AuthzModelWriteResponse,
    EmailPasswordLoginRequest, EmailPasswordRegisterRequest, LoginCompletionResponse,
    LogoutResponse, PasskeyStartResponse, PasswordResetCompleteRequest, PasswordResetStartRequest,
    PasswordResetStartResponse, RelationshipTupleWriteRequest, RelationshipTupleWriteResponse,
    SessionView, SigningKeyListResponse, SigningKeyRotateResponse, SigningKeySummary,
    StorageEventTypeCount, StorageProjectionCheckpoint, StorageProjectionRunResponse,
    StorageStatusResponse, TokenRefreshResponse, TokenVerifyRequest, TokenVerifyResponse,
};
use crate::error::{AuthStackError, AuthStackResult};

const DEFAULT_TENANT_ID: &str = "tenant:default";
const BOOTSTRAP_MODEL_ID: &str = "bootstrap-deny-by-default";
const DEFAULT_SESSION_TTL_SECONDS: u64 = 60 * 60;
const DEFAULT_REFRESH_TOKEN_TTL_SECONDS: u64 = 30 * 24 * 60 * 60;
const DEFAULT_ACCESS_TOKEN_TTL_SECONDS: u64 = 15 * 60;
const DEFAULT_JWT_ISSUER: &str = "http://127.0.0.1:3008";
const DEFAULT_JWT_AUDIENCE: &str = "auth-stack";
const DEFAULT_JWT_KID: &str = "auth-stack-dev-hs256";
const DEFAULT_JWT_SECRET: &str = "dev-auth-stack-secret-change-me";
const DEFAULT_JWT_ALGORITHM: &str = "HS256";
const AUTH_PRODUCTION_MODE: &str = "AUTH_PRODUCTION_MODE";
const SIGNING_KEY_STATUS_ACTIVE: &str = "active";
const SIGNING_KEY_STATUS_NEXT: &str = "next";
const SIGNING_KEY_STATUS_RETIRED: &str = "retired";
const SIGNING_KEY_STATUS_REVOKED: &str = "revoked";
const PASSWORD_RESET_TTL_MS: u64 = 15 * 60 * 1000;
const PASSWORD_RESET_TTL_SECONDS: u64 = PASSWORD_RESET_TTL_MS / 1000;
const OAUTH_STATE_TTL_MS: u64 = 10 * 60 * 1000;
const DEFAULT_PASSKEY_CHALLENGE_TTL_SECONDS: u64 = 5 * 60;
const DEFAULT_PASSKEY_RP_ID: &str = "localhost";
const DEFAULT_PASSKEY_RP_NAME: &str = "ddd-auth";
const DEFAULT_PASSKEY_ORIGIN: &str = "http://localhost:3008";
const PASSWORD_HASH_ALGORITHM: &str = "pbkdf2-sha256";
const DEFAULT_PASSWORD_KDF: &str = "argon2id";
const DEFAULT_PASSWORD_ARGON2_MEMORY_KIB: u32 = 19_456;
const DEFAULT_PASSWORD_ARGON2_ITERATIONS: u32 = 2;
const DEFAULT_PASSWORD_ARGON2_PARALLELISM: u32 = 1;
const DEFAULT_PASSWORD_PBKDF2_ITERATIONS: u32 = 600_000;
const MIN_PRODUCTION_PASSWORD_PBKDF2_ITERATIONS: u32 = 600_000;
const PASSWORD_SALT_BYTES: usize = 16;
const PASSWORD_HASH_BYTES: usize = 32;
const AUTH_STORAGE_PROJECTION_CHECKPOINT: &str = "auth.storage.read_models";
const AUTHZ_STORAGE_PROJECTION_CHECKPOINT: &str = "authz.storage.read_models";
const DEFAULT_STORAGE_PROJECTION_BATCH_LIMIT: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StorageBackend {
    Sqlite,
    Postgres,
    Mysql,
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
    for statement in schema_statements(backend) {
        execute_sql(statement, Vec::new()).await?;
    }

    seed_default_provider_configs().await?;
    seed_bootstrap_authorization_model().await?;
    record_schema_version().await?;

    SCHEMA_INITIALIZED.store(true, Ordering::Release);
    tracing::info!(
        auth_storage_version = ddd_auth::AUTH_STORAGE_VERSION,
        authz_storage_version = ddd_authz::AUTHZ_STORAGE_VERSION,
        auth_streams = ddd_auth::AUTH_EVENT_STREAMS.len(),
        authz_streams = ddd_authz::AUTHZ_STREAMS.len(),
        "auth storage schema initialized"
    );

    Ok(())
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

    upsert_user_email_identity(&email, &user_id, now).await?;
    execute_sql(
        "INSERT INTO auth_password_credentials \
         (tenant_id, user_id, password_hash, created_at_ms, updated_at_ms, revoked_at_ms, last_authenticated_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?4, NULL, NULL) \
         ON CONFLICT(tenant_id, user_id) DO UPDATE SET \
         password_hash = excluded.password_hash, \
         updated_at_ms = excluded.updated_at_ms, \
         revoked_at_ms = NULL",
        vec![
            json!(DEFAULT_TENANT_ID),
            json!(&user_id),
            json!(&password_hash),
            json!(now),
        ],
    )
    .await?;
    append_storage_event(
        "auth_user",
        &user_id,
        "auth_password_user_registered",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "user_id": &user_id,
            "email": &email,
            "password_hash": &password_hash,
        }),
    )
    .await?;

    issue_session_for_email(&email, redirect_url).await
}

pub async fn login_email_password(
    request: &EmailPasswordLoginRequest,
    redirect_url: &str,
) -> AuthStackResult<LoginCompletionResponse> {
    initialize_schema_async().await?;
    let email = normalize_email(&request.email);
    let Some(record) = password_credential_for_email(&email).await? else {
        return Err(AuthStackError::InvalidCredentials);
    };
    if record.disabled || record.revoked_at_ms.is_some() {
        return Err(AuthStackError::InvalidCredentials);
    }
    match verify_password(&request.password, &record.password_hash).await? {
        PasswordVerification::Invalid => {
            return Err(AuthStackError::InvalidCredentials);
        }
        PasswordVerification::ValidCurrent => {}
        PasswordVerification::ValidNeedsRehash => {
            let password_hash = hash_password(&request.password).await?;
            execute_sql(
                "UPDATE auth_password_credentials \
                 SET password_hash = ?1, updated_at_ms = ?2 \
                 WHERE tenant_id = ?3 AND user_id = ?4",
                vec![
                    json!(&password_hash),
                    json!(now_ms()),
                    json!(DEFAULT_TENANT_ID),
                    json!(&record.user_id),
                ],
            )
            .await?;
            append_storage_event(
                "auth_user",
                &record.user_id,
                "auth_password_hash_rehashed",
                json!({
                    "tenant_id": DEFAULT_TENANT_ID,
                    "user_id": &record.user_id,
                }),
            )
            .await?;
        }
    }

    execute_sql(
        "UPDATE auth_password_credentials \
         SET last_authenticated_at_ms = ?1, updated_at_ms = ?1 \
         WHERE tenant_id = ?2 AND user_id = ?3",
        vec![
            json!(now_ms()),
            json!(DEFAULT_TENANT_ID),
            json!(record.user_id),
        ],
    )
    .await?;
    append_storage_event(
        "auth_user",
        &record.user_id,
        "auth_password_login_succeeded",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "user_id": &record.user_id,
        }),
    )
    .await?;

    issue_session_for_email(&record.primary_email, redirect_url).await
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
            reset_url: None,
            expires_in_seconds: PASSWORD_RESET_TTL_SECONDS,
        });
    };

    if record.disabled || record.revoked_at_ms.is_some() {
        return Ok(PasswordResetStartResponse {
            accepted: true,
            reset_url: None,
            expires_in_seconds: PASSWORD_RESET_TTL_SECONDS,
        });
    }

    let now = now_ms();
    let grant_id = secure_storage_id("password_reset")?;
    let expires_at_ms = now.saturating_add(PASSWORD_RESET_TTL_MS);
    let payload_json = json!({
        "email": record.primary_email,
        "user_id": record.user_id,
    })
    .to_string();

    execute_sql(
        "INSERT INTO auth_token_grants \
         (grant_id, tenant_id, grant_type, subject_hint, redirect_url, payload_json, expires_at_ms, consumed_at_ms, created_at_ms) \
         VALUES (?1, ?2, 'password_reset', ?3, ?4, ?5, ?6, NULL, ?7)",
        vec![
            json!(grant_id),
            json!(DEFAULT_TENANT_ID),
            json!(email),
            json!(redirect_url),
            json!(&payload_json),
            json!(expires_at_ms),
            json!(now),
        ],
    )
    .await?;
    append_storage_event(
        "auth_session",
        &grant_id,
        "auth_password_reset_started",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "grant_id": &grant_id,
            "email": &record.primary_email,
            "user_id": &record.user_id,
            "redirect_url": redirect_url,
            "payload_json": &payload_json,
            "expires_at_ms": expires_at_ms,
        }),
    )
    .await?;

    Ok(PasswordResetStartResponse {
        accepted: true,
        reset_url: dev_password_reset_url(&grant_id).await,
        expires_in_seconds: PASSWORD_RESET_TTL_SECONDS,
    })
}

pub async fn complete_password_reset(
    request: &PasswordResetCompleteRequest,
    redirect_url: &str,
) -> AuthStackResult<LoginCompletionResponse> {
    initialize_schema_async().await?;
    let token = request.token.trim();
    let rows = execute_sql(
        "SELECT grant_id, payload_json, expires_at_ms, consumed_at_ms \
         FROM auth_token_grants \
         WHERE tenant_id = ?1 AND grant_id = ?2 AND grant_type = 'password_reset' \
         LIMIT 1",
        vec![json!(DEFAULT_TENANT_ID), json!(token)],
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
    execute_sql(
        "UPDATE auth_password_credentials \
         SET password_hash = ?1, updated_at_ms = ?2, revoked_at_ms = NULL \
         WHERE tenant_id = ?3 AND user_id = ?4",
        vec![
            json!(&password_hash),
            json!(now),
            json!(DEFAULT_TENANT_ID),
            json!(record.user_id),
        ],
    )
    .await?;
    execute_sql(
        "UPDATE auth_token_grants \
         SET consumed_at_ms = ?1 \
         WHERE tenant_id = ?2 AND grant_id = ?3 AND consumed_at_ms IS NULL",
        vec![json!(now), json!(DEFAULT_TENANT_ID), json!(token)],
    )
    .await?;
    append_storage_event(
        "auth_user",
        &record.user_id,
        "auth_password_reset_completed",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "user_id": &record.user_id,
            "grant_id": token,
            "password_hash": &password_hash,
        }),
    )
    .await?;

    issue_session_for_email(&email, redirect_url).await
}

pub async fn create_oauth_grant(provider_id: &str, redirect_url: &str) -> AuthStackResult<String> {
    initialize_schema_async().await?;
    let now = now_ms();
    let grant_id = secure_storage_id("oauth")?;
    let expires_at_ms = now.saturating_add(OAUTH_STATE_TTL_MS);
    let payload_json = json!({
        "provider_id": provider_id,
        "state": grant_id,
    })
    .to_string();

    execute_sql(
        "INSERT INTO auth_token_grants \
         (grant_id, tenant_id, grant_type, subject_hint, redirect_url, payload_json, expires_at_ms, consumed_at_ms, created_at_ms) \
         VALUES (?1, ?2, 'oauth_state', ?3, ?4, ?5, ?6, NULL, ?7)",
        vec![
            json!(grant_id),
            json!(DEFAULT_TENANT_ID),
            json!(provider_id),
            json!(redirect_url),
            json!(&payload_json),
            json!(expires_at_ms),
            json!(now),
        ],
    )
    .await?;
    append_storage_event(
        "auth_provider_config",
        provider_id,
        "auth_oauth_state_created",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "provider_id": provider_id,
            "state": &grant_id,
            "redirect_url": redirect_url,
            "payload_json": &payload_json,
            "expires_at_ms": expires_at_ms,
        }),
    )
    .await?;

    Ok(grant_id)
}

pub async fn consume_oauth_grant(
    provider_id: &str,
    state: &str,
) -> AuthStackResult<ConsumedOauthGrant> {
    initialize_schema_async().await?;
    let rows = execute_sql(
        "SELECT grant_id, subject_hint, redirect_url, expires_at_ms, consumed_at_ms \
         FROM auth_token_grants \
         WHERE tenant_id = ?1 AND grant_id = ?2 AND grant_type = 'oauth_state' \
         LIMIT 1",
        vec![json!(DEFAULT_TENANT_ID), json!(state)],
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

    execute_sql(
        "UPDATE auth_token_grants \
         SET consumed_at_ms = ?1 \
         WHERE tenant_id = ?2 AND grant_id = ?3 AND consumed_at_ms IS NULL",
        vec![json!(now_ms()), json!(DEFAULT_TENANT_ID), json!(state)],
    )
    .await?;
    append_storage_event(
        "auth_provider_config",
        provider_id,
        "auth_oauth_state_consumed",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "provider_id": provider_id,
            "state": state,
        }),
    )
    .await?;

    Ok(ConsumedOauthGrant {
        state: required_string(&row, "grant_id")?,
        provider_id: stored_provider_id,
        redirect_url: required_string(&row, "redirect_url")?,
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
    let expires_at_ms = now.saturating_add(passkey_challenge_ttl_ms().await);

    upsert_user_email_identity(&email, &user_id, now).await?;

    execute_sql(
        "INSERT INTO auth_token_grants \
         (grant_id, tenant_id, grant_type, subject_hint, redirect_url, payload_json, expires_at_ms, consumed_at_ms, created_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8)",
        vec![
            json!(challenge_id),
            json!(DEFAULT_TENANT_ID),
            json!("passkey_registration"),
            json!(email),
            json!(redirect_url),
            json!(&payload_json),
            json!(expires_at_ms),
            json!(now),
        ],
    )
    .await?;
    append_storage_event(
        "auth_passkey_credential",
        &user_id,
        "auth_passkey_registration_started",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "user_id": &user_id,
            "challenge_id": &challenge_id,
            "email": &email,
            "redirect_url": redirect_url,
            "payload_json": &payload_json,
            "expires_at_ms": expires_at_ms,
        }),
    )
    .await?;

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
    let expires_at_ms = now.saturating_add(passkey_challenge_ttl_ms().await);

    execute_sql(
        "INSERT INTO auth_token_grants \
         (grant_id, tenant_id, grant_type, subject_hint, redirect_url, payload_json, expires_at_ms, consumed_at_ms, created_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8)",
        vec![
            json!(challenge_id),
            json!(DEFAULT_TENANT_ID),
            json!("passkey_login"),
            json!(email),
            json!(redirect_url),
            json!(&payload_json),
            json!(expires_at_ms),
            json!(now),
        ],
    )
    .await?;
    append_storage_event(
        "auth_passkey_credential",
        &user_id,
        "auth_passkey_login_started",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "user_id": &user_id,
            "challenge_id": &challenge_id,
            "email": &email,
            "redirect_url": redirect_url,
            "payload_json": &payload_json,
            "expires_at_ms": expires_at_ms,
        }),
    )
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
    let rows = execute_sql(
        "SELECT grant_id, grant_type, redirect_url, payload_json, expires_at_ms, consumed_at_ms \
         FROM auth_token_grants \
         WHERE tenant_id = ?1 AND grant_id = ?2 AND grant_type LIKE 'passkey_%' \
         LIMIT 1",
        vec![json!(DEFAULT_TENANT_ID), json!(challenge_id)],
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

    execute_sql(
        "UPDATE auth_token_grants \
         SET consumed_at_ms = ?1 \
         WHERE tenant_id = ?2 AND grant_id = ?3",
        vec![
            json!(now_ms()),
            json!(DEFAULT_TENANT_ID),
            json!(challenge_id),
        ],
    )
    .await?;
    append_storage_event(
        "auth_passkey_credential",
        challenge_id,
        "auth_passkey_challenge_consumed",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "challenge_id": challenge_id,
        }),
    )
    .await?;

    Ok(ConsumedPasskeyChallenge {
        grant_type: required_string(&row, "grant_type")?,
        redirect_url: required_string(&row, "redirect_url")?,
        payload: serde_json::from_str(&required_string(&row, "payload_json")?)
            .map_err(|error| AuthStackError::store(format!("stored passkey state is invalid: {error}")))?,
    })
}

pub async fn verify_passkey_registration(
    challenge_id: &str,
    credential_json: &str,
    redirect_url: Option<String>,
) -> AuthStackResult<LoginCompletionResponse> {
    let response: PasskeyRegistrationResponse = serde_json::from_str(credential_json)
        .map_err(|error| {
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

    let webauthn = passkey_webauthn().await?;
    let credential = webauthn
        .finish_registration(&state, &response)
        .map_err(map_passkey_verification_error)?;
    persist_passkey_credential(&user_id, &credential).await?;
    issue_session_for_email(&email, &redirect_url).await
}

pub async fn verify_passkey_login(
    challenge_id: &str,
    credential_json: &str,
    redirect_url: Option<String>,
) -> AuthStackResult<LoginCompletionResponse> {
    let response: PasskeyAuthenticationResponse = serde_json::from_str(credential_json)
        .map_err(|error| {
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
    issue_session_for_email(&email, &redirect_url).await
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
        .unwrap_or_else(|| synthetic_oauth_email(&identity.provider_id, &identity.provider_subject));
    let user_id = user_id_from_email(&email);
    let now = now_ms();
    let profile_json = json!({
        "email": identity.email,
        "email_verified": identity.email_verified,
        "name": identity.name,
    })
    .to_string();

    upsert_user_email_identity(&email, &user_id, now).await?;
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
            json!(identity.provider_id),
            json!(identity.provider_subject),
            json!(&user_id),
            json!(&email),
            json!(&profile_json),
            json!(now),
        ],
    )
    .await?;
    append_storage_event(
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
        }),
    )
    .await?;

    issue_session_for_email(&email, redirect_url).await
}

async fn issue_session_for_email(
    email: &str,
    redirect_url: &str,
) -> AuthStackResult<LoginCompletionResponse> {
    initialize_schema_async().await?;
    let now = now_ms();
    let email = normalize_email(email);
    let user_id = user_id_from_email(&email);
    let session_id = secure_storage_id("session")?;
    let expires_at_ms = now.saturating_add(session_ttl_ms().await);
    let refresh_token_ttl_ms = refresh_token_ttl_ms().await;
    let access_token_ttl_seconds = access_token_ttl_seconds().await;
    let permissions = session_permissions_for_email(&email).await;
    let permissions_json = serde_json::to_string(&permissions)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;

    upsert_user_email_identity(&email, &user_id, now).await?;
    execute_sql(
        "INSERT INTO auth_sessions \
         (session_id, tenant_id, user_id, primary_email, expires_at_ms, revoked_at_ms, permissions_json, created_at_ms, updated_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?7) \
         ON CONFLICT(session_id) DO UPDATE SET \
         tenant_id = excluded.tenant_id, \
         user_id = excluded.user_id, \
         primary_email = excluded.primary_email, \
         expires_at_ms = excluded.expires_at_ms, \
         revoked_at_ms = NULL, \
         permissions_json = excluded.permissions_json, \
         updated_at_ms = excluded.updated_at_ms",
        vec![
            json!(&session_id),
            json!(DEFAULT_TENANT_ID),
            json!(&user_id),
            json!(&email),
            json!(expires_at_ms),
            json!(&permissions_json),
            json!(now),
        ],
    )
    .await?;
    let refresh_token = opaque_refresh_token()?;
    let refresh_token_hash = refresh_token_hash(&refresh_token);
    let refresh_token_expires_at_ms = now.saturating_add(refresh_token_ttl_ms);
    execute_sql(
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
    )
    .await?;
    append_storage_event(
        "auth_session",
        &session_id,
        "auth_session_issued",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "session_id": &session_id,
            "user_id": &user_id,
            "primary_email": &email,
            "permissions": &permissions,
            "permissions_json": &permissions_json,
            "expires_at_ms": expires_at_ms,
            "refresh_token_hash": &refresh_token_hash,
            "refresh_token_expires_at_ms": refresh_token_expires_at_ms,
        }),
    )
    .await?;
    let access_token = issue_access_token_for_session(
        &session_id,
        &user_id,
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
    Ok(session.into_view())
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
    if refresh_row.revoked_at_ms.is_some() || refresh_row.rotated_at_ms.is_some() {
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
    execute_sql(
        "UPDATE auth_refresh_token_hashes \
         SET rotated_at_ms = ?1 \
         WHERE tenant_id = ?2 AND token_hash = ?3 AND rotated_at_ms IS NULL",
        vec![
            json!(now),
            json!(DEFAULT_TENANT_ID),
            json!(&current_refresh_token_hash),
        ],
    )
    .await?;
    execute_sql(
        "INSERT INTO auth_refresh_token_hashes \
         (tenant_id, token_hash, session_id, expires_at_ms, rotated_at_ms, revoked_at_ms, created_at_ms) \
         VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5)",
        vec![
            json!(DEFAULT_TENANT_ID),
            json!(&next_refresh_token_hash),
            json!(session.session_id),
            json!(next_refresh_token_expires_at_ms),
            json!(now),
        ],
    )
    .await?;
    execute_sql(
        "UPDATE auth_sessions SET updated_at_ms = ?1 WHERE session_id = ?2",
        vec![json!(now), json!(session.session_id)],
    )
    .await?;
    append_storage_event(
        "auth_session",
        &session.session_id,
        "auth_refresh_token_rotated",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "session_id": &session.session_id,
            "current_refresh_token_hash": &current_refresh_token_hash,
            "next_refresh_token_hash": &next_refresh_token_hash,
            "next_refresh_token_expires_at_ms": next_refresh_token_expires_at_ms,
        }),
    )
    .await?;
    let access_token = issue_access_token_for_session(
        &session.session_id,
        &session.user_id,
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

    Ok(TokenVerifyResponse {
        active: true,
        subject: claims.sub,
        tenant_id: claims.tenant_id.map(|value| value.into_string()),
        session_id: Some(session.session_id),
        expires_at: claims.exp,
        scopes: claims.scope,
    })
}

pub async fn revoke_session(session_id: Option<&str>) -> AuthStackResult<LogoutResponse> {
    initialize_schema_async().await?;
    if let Some(session_id) = normalized_session_id(session_id) {
        let now = now_ms();
        execute_sql(
            "UPDATE auth_sessions \
             SET revoked_at_ms = ?1, updated_at_ms = ?1 \
             WHERE session_id = ?2 AND revoked_at_ms IS NULL",
            vec![json!(now), json!(session_id)],
        )
        .await?;
        execute_sql(
            "UPDATE auth_refresh_token_hashes \
             SET revoked_at_ms = ?1 \
             WHERE tenant_id = ?2 AND session_id = ?3 AND revoked_at_ms IS NULL",
            vec![json!(now), json!(DEFAULT_TENANT_ID), json!(session_id)],
        )
        .await?;
        append_storage_event(
            "auth_session",
            &session_id,
            "auth_session_revoked",
            json!({
                "tenant_id": DEFAULT_TENANT_ID,
                "session_id": session_id,
            }),
        )
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

    let previous_kid = active_signing_key_config_from(&configured_keys)
        .await
        .ok()
        .map(|key| key.kid)
        .filter(|previous| previous != kid);
    let now = now_ms();

    if retire_previous {
        if let Some(previous_kid) = previous_kid.as_deref() {
            upsert_signing_key_state(
                previous_kid,
                SigningKeyStatus::Retired,
                Some(now),
                Some(now),
                None,
            )
            .await?;
        }
    }

    upsert_signing_key_state(kid, SigningKeyStatus::Active, Some(now), None, None).await?;
    append_storage_event(
        "auth_signing_key_set",
        kid,
        "auth_signing_key_rotated",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "kid": kid,
            "previous_kid": previous_kid.as_deref(),
            "retired_previous": retire_previous,
        }),
    )
    .await?;

    Ok(SigningKeyRotateResponse {
        active_kid: kid.to_string(),
        previous_kid,
        retired_previous: retire_previous,
        keys: signing_key_summaries().await?,
    })
}

pub async fn validate_admin_token(admin_token: Option<&str>) -> AuthStackResult<()> {
    let configured = store_config_value("AUTH_ADMIN_TOKEN")
        .await
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AuthStackError::configuration(
                "AUTH_ADMIN_TOKEN is required for signing-key administration",
            )
        })?;
    let Some(candidate) = admin_token.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(AuthStackError::AuthRequired);
    };
    if !constant_time_eq(candidate.as_bytes(), configured.trim().as_bytes()) {
        return Err(AuthStackError::Forbidden);
    }
    Ok(())
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

pub async fn write_authorization_model(
    request: &AuthzModelWriteRequest,
) -> AuthStackResult<AuthzModelWriteResponse> {
    initialize_schema_async().await?;
    let model = AuthorizationModel::from_json(&request.schema_json).map_err(authz_validation_error)?;
    if model.model_id != request.model_id {
        return Err(AuthStackError::validation(
            "schema_json model_id must match request model_id",
        ));
    }
    let schema_json: Value = serde_json::to_value(&model)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;
    write_authorization_model_unchecked(request, schema_json.to_string()).await?;
    append_storage_event(
        "authz_model",
        &request.model_id,
        "authz_model_written",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "model_id": &request.model_id,
            "schema_json": &schema_json,
        }),
    )
    .await?;

    Ok(AuthzModelWriteResponse {
        model_id: request.model_id.clone(),
        active: false,
    })
}

#[allow(dead_code)]
pub async fn read_authorization_model(model_id: &str) -> AuthStackResult<AuthzModelReadResponse> {
    initialize_schema_async().await?;
    let rows = execute_sql(
        "SELECT models.model_id, models.schema_json, \
            CASE WHEN active.model_id IS NULL THEN 0 ELSE 1 END AS active \
         FROM authz_models models \
         LEFT JOIN authz_active_model active \
           ON active.tenant_id = models.tenant_id AND active.model_id = models.model_id \
         WHERE models.tenant_id = ?1 AND models.model_id = ?2 \
         LIMIT 1",
        vec![json!(DEFAULT_TENANT_ID), json!(model_id)],
    )
    .await?;
    let Some(row) = rows.into_iter().next() else {
        return Err(AuthStackError::not_found(format!(
            "authorization model '{model_id}' was not found"
        )));
    };

    Ok(AuthzModelReadResponse {
        model_id: required_string(&row, "model_id")?,
        schema_json: required_string(&row, "schema_json")?,
        active: row_bool(&row, "active").unwrap_or(false),
    })
}

pub async fn authorization_model_schema(
    tenant_id: &str,
    model_id: &str,
) -> AuthStackResult<String> {
    initialize_schema_async().await?;
    let rows = execute_sql(
        "SELECT schema_json FROM authz_models \
         WHERE tenant_id = ?1 AND model_id = ?2 \
         LIMIT 1",
        vec![json!(tenant_id), json!(model_id)],
    )
    .await?;
    rows.into_iter()
        .next()
        .and_then(|row| row_string(&row, "schema_json"))
        .ok_or_else(|| {
            AuthStackError::not_found(format!("authorization model '{model_id}' was not found"))
        })
}

async fn write_authorization_model_unchecked(
    request: &AuthzModelWriteRequest,
    schema_json: String,
) -> AuthStackResult<()> {
    let now = now_ms();
    execute_sql(
        "INSERT INTO authz_models \
         (tenant_id, model_id, schema_json, created_at_ms, updated_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?4) \
         ON CONFLICT(tenant_id, model_id) DO UPDATE SET \
         schema_json = excluded.schema_json, \
         updated_at_ms = excluded.updated_at_ms",
        vec![
            json!(DEFAULT_TENANT_ID),
            json!(request.model_id),
            json!(schema_json),
            json!(now),
        ],
    )
    .await?;
    Ok(())
}

pub async fn activate_authorization_model(
    model_id: &str,
) -> AuthStackResult<AuthzModelWriteResponse> {
    initialize_schema_async().await?;
    let rows = execute_sql(
        "SELECT model_id FROM authz_models WHERE tenant_id = ?1 AND model_id = ?2 LIMIT 1",
        vec![json!(DEFAULT_TENANT_ID), json!(model_id)],
    )
    .await?;
    if rows.is_empty() {
        return Err(AuthStackError::not_found(format!(
            "authorization model '{model_id}' was not found"
        )));
    }

    activate_authorization_model_unchecked(model_id).await?;
    append_storage_event(
        "authz_model",
        model_id,
        "authz_model_activated",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "model_id": model_id,
        }),
    )
    .await?;

    Ok(AuthzModelWriteResponse {
        model_id: model_id.to_string(),
        active: true,
    })
}

async fn activate_authorization_model_unchecked(model_id: &str) -> AuthStackResult<()> {
    execute_sql(
        "INSERT INTO authz_active_model (tenant_id, model_id, activated_at_ms) \
         VALUES (?1, ?2, ?3) \
         ON CONFLICT(tenant_id) DO UPDATE SET \
         model_id = excluded.model_id, \
         activated_at_ms = excluded.activated_at_ms",
        vec![json!(DEFAULT_TENANT_ID), json!(model_id), json!(now_ms())],
    )
    .await?;
    Ok(())
}

pub async fn active_authorization_model_id(tenant_id: &str) -> AuthStackResult<String> {
    initialize_schema_async().await?;
    let rows = execute_sql(
        "SELECT model_id FROM authz_active_model WHERE tenant_id = ?1 LIMIT 1",
        vec![json!(tenant_id)],
    )
    .await?;

    Ok(rows
        .into_iter()
        .next()
        .and_then(|row| row_string(&row, "model_id"))
        .unwrap_or_else(|| BOOTSTRAP_MODEL_ID.to_string()))
}

pub async fn write_relationship_tuples(
    request: &RelationshipTupleWriteRequest,
) -> AuthStackResult<RelationshipTupleWriteResponse> {
    initialize_schema_async().await?;
    let tuples = parse_relationship_tuple_inputs(&request.tuples_json)?;
    write_relationship_tuples_unchecked(&tuples, now_ms()).await?;
    append_storage_event(
        "authz_tuple_set",
        DEFAULT_TENANT_ID,
        "authz_relationship_tuples_written",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "tuple_count": tuples.len(),
            "tuples_json": &request.tuples_json,
        }),
    )
    .await?;

    Ok(RelationshipTupleWriteResponse {
        accepted: tuples.len() as u32,
    })
}

pub async fn delete_relationship_tuples(
    request: &RelationshipTupleWriteRequest,
) -> AuthStackResult<RelationshipTupleWriteResponse> {
    initialize_schema_async().await?;
    let tuples = parse_relationship_tuple_inputs(&request.tuples_json)?;
    delete_relationship_tuples_unchecked(&tuples).await?;
    append_storage_event(
        "authz_tuple_set",
        DEFAULT_TENANT_ID,
        "authz_relationship_tuples_deleted",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "tuple_count": tuples.len(),
            "tuples_json": &request.tuples_json,
        }),
    )
    .await?;

    Ok(RelationshipTupleWriteResponse {
        accepted: tuples.len() as u32,
    })
}

async fn write_relationship_tuples_unchecked(
    tuples: &[RelationshipTupleInput],
    recorded_at_ms: u64,
) -> AuthStackResult<()> {
    for tuple in tuples {
        execute_sql(
            "INSERT OR IGNORE INTO authz_relationship_tuples \
             (tenant_id, subject_ref, relation, object_ref, condition_name, context_json, created_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            vec![
                json!(&tuple.tenant),
                json!(&tuple.subject),
                json!(&tuple.relation),
                json!(&tuple.object),
                json!(&tuple.condition_name),
                json!(&tuple.context_json),
                json!(recorded_at_ms),
            ],
        )
        .await?;
        execute_sql(
            "INSERT OR IGNORE INTO authz_tuple_index_by_subject \
             (tenant_id, subject_ref, relation, object_ref) \
             VALUES (?1, ?2, ?3, ?4)",
            vec![
                json!(&tuple.tenant),
                json!(&tuple.subject),
                json!(&tuple.relation),
                json!(&tuple.object),
            ],
        )
        .await?;
        execute_sql(
            "INSERT OR IGNORE INTO authz_tuple_index_by_object \
             (tenant_id, object_ref, relation, subject_ref) \
             VALUES (?1, ?2, ?3, ?4)",
            vec![
                json!(&tuple.tenant),
                json!(&tuple.object),
                json!(&tuple.relation),
                json!(&tuple.subject),
            ],
        )
        .await?;
    }
    Ok(())
}

async fn delete_relationship_tuples_unchecked(
    tuples: &[RelationshipTupleInput],
) -> AuthStackResult<()> {
    for tuple in tuples {
        execute_sql(
            "DELETE FROM authz_relationship_tuples \
             WHERE tenant_id = ?1 AND subject_ref = ?2 AND relation = ?3 AND object_ref = ?4",
            vec![
                json!(&tuple.tenant),
                json!(&tuple.subject),
                json!(&tuple.relation),
                json!(&tuple.object),
            ],
        )
        .await?;
        execute_sql(
            "DELETE FROM authz_tuple_index_by_subject \
             WHERE tenant_id = ?1 AND subject_ref = ?2 AND relation = ?3 AND object_ref = ?4",
            vec![
                json!(&tuple.tenant),
                json!(&tuple.subject),
                json!(&tuple.relation),
                json!(&tuple.object),
            ],
        )
        .await?;
        execute_sql(
            "DELETE FROM authz_tuple_index_by_object \
             WHERE tenant_id = ?1 AND object_ref = ?2 AND relation = ?3 AND subject_ref = ?4",
            vec![
                json!(&tuple.tenant),
                json!(&tuple.object),
                json!(&tuple.relation),
                json!(&tuple.subject),
            ],
        )
        .await?;
    }
    Ok(())
}

pub async fn relationship_tuples_for_tenant(
    tenant: &str,
) -> AuthStackResult<Vec<RelationshipTuple>> {
    initialize_schema_async().await?;
    let rows = execute_sql(
        "SELECT tenant_id, subject_ref, relation, object_ref, condition_name \
         FROM authz_relationship_tuples \
         WHERE tenant_id = ?1",
        vec![json!(tenant)],
    )
    .await?;

    rows.into_iter().map(relationship_tuple_from_row).collect()
}

pub async fn read_relationship_tuples_json(
    tenant: &str,
    object: &str,
    relation: &str,
) -> AuthStackResult<String> {
    initialize_schema_async().await?;
    let rows = execute_sql(
        "SELECT tenant_id, subject_ref, relation, object_ref, condition_name \
         FROM authz_relationship_tuples \
         WHERE tenant_id = ?1 AND object_ref = ?2 AND relation = ?3 \
         ORDER BY subject_ref ASC",
        vec![json!(tenant), json!(object), json!(relation)],
    )
    .await?;
    let tuples = rows
        .into_iter()
        .map(|row| {
            Ok(json!({
                "tenant": required_string(&row, "tenant_id")?,
                "subject": required_string(&row, "subject_ref")?,
                "object": required_string(&row, "object_ref")?,
                "relation": required_string(&row, "relation")?,
                "condition_name": row_string(&row, "condition_name"),
            }))
        })
        .collect::<AuthStackResult<Vec<_>>>()?;
    serde_json::to_string(&tuples).map_err(|error| AuthStackError::serialization(error.to_string()))
}

fn relationship_tuple_from_row(row: Value) -> AuthStackResult<RelationshipTuple> {
    let subject =
        SubjectRef::new(required_string(&row, "subject_ref")?).map_err(authz_validation_error)?;
    let relation =
        Relation::new(required_string(&row, "relation")?).map_err(authz_validation_error)?;
    let object =
        ObjectRef::new(required_string(&row, "object_ref")?).map_err(authz_validation_error)?;
    let mut tuple = RelationshipTuple::new(subject, relation, object);
    if let Some(condition) = row_string(&row, "condition_name").filter(|value| !value.is_empty()) {
        tuple = tuple.with_condition(condition);
    }
    if let Some(tenant_id) = row_string(&row, "tenant_id").filter(|value| !value.is_empty()) {
        tuple = tuple.with_tenant(TenantRef::new(tenant_id).map_err(authz_validation_error)?);
    }
    Ok(tuple)
}

async fn seed_default_provider_configs() -> AuthStackResult<()> {
    for (provider_id, display_name) in [
        ("apple", "Apple"),
        ("facebook", "Facebook"),
        ("google", "Google"),
    ] {
        let enabled = provider_default_enabled(provider_id).await;
        insert_auth_provider_config_if_missing_unchecked(
            provider_id,
            enabled,
            display_name,
            &format!("/api/auth/oauth/{provider_id}/start"),
            now_ms(),
        )
        .await?;
        tracing::debug!(provider_id, display_name, "seeded auth provider config");
    }
    Ok(())
}

async fn insert_auth_provider_config_if_missing_unchecked(
    provider_id: &str,
    enabled: bool,
    display_name: &str,
    login_url: &str,
    now: u64,
) -> AuthStackResult<()> {
    execute_sql(
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
    .await?;
    Ok(())
}

async fn seed_bootstrap_authorization_model() -> AuthStackResult<()> {
    let request = AuthzModelWriteRequest {
        tenant: None,
        model_id: BOOTSTRAP_MODEL_ID.to_string(),
        schema_json: json!({
            "schema_version": "1.0",
            "model_id": BOOTSTRAP_MODEL_ID,
            "types": {}
        })
        .to_string(),
        idempotency_key: None,
    };
    write_authorization_model_unchecked(&request, request.schema_json.clone()).await?;
    let active_rows = execute_sql(
        "SELECT model_id FROM authz_active_model WHERE tenant_id = ?1 LIMIT 1",
        vec![json!(DEFAULT_TENANT_ID)],
    )
    .await?;
    if active_rows.is_empty() {
        activate_authorization_model_unchecked(BOOTSTRAP_MODEL_ID).await?;
    }
    Ok(())
}

async fn record_schema_version() -> AuthStackResult<()> {
    execute_sql(
        "INSERT INTO auth_schema_migrations (version, applied_at_ms) \
         VALUES (?1, ?2) \
         ON CONFLICT(version) DO UPDATE SET applied_at_ms = excluded.applied_at_ms",
        vec![
            json!(format!(
                "{}+{}",
                ddd_auth::AUTH_STORAGE_VERSION,
                ddd_authz::AUTHZ_STORAGE_VERSION
            )),
            json!(now_ms()),
        ],
    )
    .await?;
    Ok(())
}

fn schema_statements(backend: StorageBackend) -> &'static [&'static str] {
    match backend {
        StorageBackend::Sqlite | StorageBackend::Postgres => AUTH_SCHEMA_STATEMENTS,
        StorageBackend::Mysql => AUTH_MYSQL_SCHEMA_STATEMENTS,
    }
}

async fn storage_backend() -> AuthStackResult<StorageBackend> {
    let value = runtime_config_value("DATABASE_BACKEND")
        .await
        .unwrap_or_else(|| default_storage_backend().into());
    match value.trim().to_ascii_lowercase().as_str() {
        "sqlite" => Ok(StorageBackend::Sqlite),
        "postgres" | "postgresql" => Ok(StorageBackend::Postgres),
        "mysql" => Ok(StorageBackend::Mysql),
        other => Err(AuthStackError::configuration(format!(
            "unsupported DATABASE_BACKEND={other}; use sqlite, postgres, or mysql"
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
    #[cfg(all(not(feature = "sqlite"), not(feature = "postgres"), feature = "mysql"))]
    {
        return "mysql";
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
        StorageBackend::Mysql => execute_mysql(sql, params).await,
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

async fn execute_mysql(sql: &str, params: Vec<Value>) -> AuthStackResult<Vec<Value>> {
    #[cfg(all(feature = "mysql", runtime_spin))]
    {
        let url = database_url("mysql").await?;
        let (sql, params) = mysql_sql_and_params(sql, params)?;
        ddd_cqrs_es::adapters::execute_spin_mysql(&url, sql.as_ref(), params)
            .await
            .map_err(AuthStackError::store)
    }

    #[cfg(not(all(feature = "mysql", runtime_spin)))]
    {
        let _ = (sql, params);
        Err(AuthStackError::configuration(
            "mysql storage requires the mysql feature on Spin",
        ))
    }
}

#[allow(dead_code)]
fn postgres_sql(sql: &str) -> Cow<'_, str> {
    let mut converted = replace_indexed_placeholders(sql, PlaceholderStyle::Postgres);
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
fn mysql_sql(sql: &str) -> Cow<'_, str> {
    let mut converted = replace_indexed_placeholders(sql, PlaceholderStyle::Mysql);
    if converted.trim_start().starts_with("INSERT OR IGNORE INTO") {
        converted = converted.replacen("INSERT OR IGNORE INTO", "INSERT IGNORE INTO", 1);
    }
    converted = mysql_upsert_sql(&converted);
    if converted == sql {
        Cow::Borrowed(sql)
    } else {
        Cow::Owned(converted)
    }
}

#[allow(dead_code)]
fn mysql_sql_and_params(
    sql: &str,
    params: Vec<Value>,
) -> AuthStackResult<(Cow<'_, str>, Vec<Value>)> {
    let rewritten_params = mysql_indexed_params(sql, &params)?;
    Ok((mysql_sql(sql), rewritten_params))
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
enum PlaceholderStyle {
    Postgres,
    Mysql,
}

#[allow(dead_code)]
fn replace_indexed_placeholders(sql: &str, style: PlaceholderStyle) -> String {
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
            match style {
                PlaceholderStyle::Postgres => {
                    output.push('$');
                    output.push_str(&digits);
                }
                PlaceholderStyle::Mysql => output.push('?'),
            }
        }
    }

    output
}

#[allow(dead_code)]
fn mysql_indexed_params(sql: &str, params: &[Value]) -> AuthStackResult<Vec<Value>> {
    let mut rewritten = Vec::new();
    let mut chars = sql.chars().peekable();
    let mut next_unindexed = 0_usize;

    while let Some(ch) = chars.next() {
        if ch != '?' {
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
            let value = params.get(next_unindexed).cloned().ok_or_else(|| {
                AuthStackError::store(format!(
                    "MySQL SQL placeholder ? has no parameter at index {}",
                    next_unindexed + 1
                ))
            })?;
            rewritten.push(value);
            next_unindexed = next_unindexed.saturating_add(1);
            continue;
        }

        let param_index = digits.parse::<usize>().map_err(|error| {
            AuthStackError::store(format!("invalid SQL placeholder ?{digits}: {error}"))
        })?;
        let value = params
            .get(param_index.saturating_sub(1))
            .cloned()
            .ok_or_else(|| {
                AuthStackError::store(format!(
                    "MySQL SQL placeholder ?{param_index} has no parameter"
                ))
            })?;
        rewritten.push(value);
    }

    Ok(rewritten)
}

#[allow(dead_code)]
fn mysql_upsert_sql(sql: &str) -> String {
    let Some(conflict_start) = sql.find("ON CONFLICT") else {
        return sql.to_string();
    };
    let Some(update_start) = sql[conflict_start..].find("DO UPDATE SET") else {
        return sql.to_string();
    };
    let update_start = conflict_start + update_start;
    let assignments_start = update_start + "DO UPDATE SET".len();

    let mut converted = String::with_capacity(sql.len());
    converted.push_str(sql[..conflict_start].trim_end());
    converted.push_str(" ON DUPLICATE KEY UPDATE ");
    converted.push_str(&mysql_values_references(sql[assignments_start..].trim_start()));
    converted
}

#[allow(dead_code)]
fn mysql_values_references(sql: &str) -> String {
    let mut output = String::with_capacity(sql.len());
    let mut remaining = sql;
    const PREFIX: &str = "excluded.";

    while let Some(index) = remaining.find(PREFIX) {
        output.push_str(&remaining[..index]);
        let after_prefix = &remaining[index + PREFIX.len()..];
        let ident_len = after_prefix
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
            .map(char::len_utf8)
            .sum::<usize>();

        if ident_len == 0 {
            output.push_str(PREFIX);
            remaining = after_prefix;
            continue;
        }

        let ident = &after_prefix[..ident_len];
        output.push_str("VALUES(");
        output.push_str(ident);
        output.push(')');
        remaining = &after_prefix[ident_len..];
    }

    output.push_str(remaining);
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
        "source": "auth-stack",
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
        catch_up_storage_projection(StorageProjectionKind::Authz, batch_limit).await?,
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
        let applied = match kind {
            StorageProjectionKind::Auth => apply_auth_storage_event(&event).await?,
            StorageProjectionKind::Authz => apply_authz_storage_event(&event).await?,
        };
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
        StorageProjectionKind::Authz => "aggregate_type LIKE 'authz_%'",
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
            let password_hash = required_payload_string(payload, "password_hash")?;
            upsert_user_email_identity(&email, &user_id, event.recorded_at_ms).await?;
            execute_sql(
                "INSERT INTO auth_password_credentials \
                 (tenant_id, user_id, password_hash, created_at_ms, updated_at_ms, revoked_at_ms, last_authenticated_at_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?4, NULL, NULL) \
                 ON CONFLICT(tenant_id, user_id) DO UPDATE SET \
                 password_hash = excluded.password_hash, \
                 updated_at_ms = excluded.updated_at_ms, \
                 revoked_at_ms = NULL",
                vec![
                    json!(DEFAULT_TENANT_ID),
                    json!(user_id),
                    json!(password_hash),
                    json!(event.recorded_at_ms),
                ],
            )
            .await?;
            Ok(true)
        }
        "auth_password_login_succeeded" => {
            let user_id = required_payload_string(payload, "user_id")?;
            execute_sql(
                "UPDATE auth_password_credentials \
                 SET last_authenticated_at_ms = ?1, updated_at_ms = ?1 \
                 WHERE tenant_id = ?2 AND user_id = ?3",
                vec![json!(event.recorded_at_ms), json!(DEFAULT_TENANT_ID), json!(user_id)],
            )
            .await?;
            Ok(true)
        }
        "auth_password_reset_started" | "auth_oauth_state_created" => {
            let grant_id = payload_string(payload, "grant_id")
                .or_else(|| payload_string(payload, "state"))
                .ok_or_else(|| {
                    AuthStackError::store(format!(
                        "{} event is missing grant_id/state",
                        event.event_type
                    ))
                })?;
            let grant_type = if event.event_type == "auth_oauth_state_created" {
                "oauth_state"
            } else {
                "password_reset"
            };
            let subject_hint = payload_string(payload, "email")
                .or_else(|| payload_string(payload, "provider_id"))
                .unwrap_or_default();
            let redirect_url = required_payload_string(payload, "redirect_url")?;
            let payload_json = required_payload_string(payload, "payload_json")?;
            let expires_at_ms = required_payload_u64(payload, "expires_at_ms")?;
            execute_sql(
                "INSERT INTO auth_token_grants \
                 (grant_id, tenant_id, grant_type, subject_hint, redirect_url, payload_json, expires_at_ms, consumed_at_ms, created_at_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8) \
                 ON CONFLICT(grant_id) DO UPDATE SET \
                 subject_hint = excluded.subject_hint, \
                 redirect_url = excluded.redirect_url, \
                 payload_json = excluded.payload_json, \
                 expires_at_ms = excluded.expires_at_ms",
                vec![
                    json!(grant_id),
                    json!(DEFAULT_TENANT_ID),
                    json!(grant_type),
                    json!(subject_hint),
                    json!(redirect_url),
                    json!(payload_json),
                    json!(expires_at_ms),
                    json!(event.recorded_at_ms),
                ],
            )
            .await?;
            Ok(true)
        }
        "auth_password_reset_completed" => {
            let user_id = required_payload_string(payload, "user_id")?;
            let grant_id = required_payload_string(payload, "grant_id")?;
            let password_hash = required_payload_string(payload, "password_hash")?;
            execute_sql(
                "UPDATE auth_password_credentials \
                 SET password_hash = ?1, updated_at_ms = ?2, revoked_at_ms = NULL \
                 WHERE tenant_id = ?3 AND user_id = ?4",
                vec![
                    json!(password_hash),
                    json!(event.recorded_at_ms),
                    json!(DEFAULT_TENANT_ID),
                    json!(user_id),
                ],
            )
            .await?;
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
        "auth_passkey_registration_started" | "auth_passkey_login_started" => {
            let challenge_id = required_payload_string(payload, "challenge_id")?;
            let grant_type = if event.event_type == "auth_passkey_registration_started" {
                "passkey_registration"
            } else {
                "passkey_login"
            };
            let email = required_payload_string(payload, "email")?;
            let redirect_url = required_payload_string(payload, "redirect_url")?;
            let payload_json = required_payload_string(payload, "payload_json")?;
            let expires_at_ms = required_payload_u64(payload, "expires_at_ms")?;
            execute_sql(
                "INSERT INTO auth_token_grants \
                 (grant_id, tenant_id, grant_type, subject_hint, redirect_url, payload_json, expires_at_ms, consumed_at_ms, created_at_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8) \
                 ON CONFLICT(grant_id) DO UPDATE SET \
                 subject_hint = excluded.subject_hint, \
                 redirect_url = excluded.redirect_url, \
                 payload_json = excluded.payload_json, \
                 expires_at_ms = excluded.expires_at_ms",
                vec![
                    json!(challenge_id),
                    json!(DEFAULT_TENANT_ID),
                    json!(grant_type),
                    json!(email),
                    json!(redirect_url),
                    json!(payload_json),
                    json!(expires_at_ms),
                    json!(event.recorded_at_ms),
                ],
            )
            .await?;
            Ok(true)
        }
        "auth_passkey_credential_upserted" => {
            let credential_id = required_payload_string(payload, "credential_id")?;
            let user_id = required_payload_string(payload, "user_id")?;
            let public_key_json = required_payload_string(payload, "public_key_json")?;
            let transports_json = required_payload_string(payload, "transports_json")?;
            let sign_count = required_payload_u64(payload, "sign_count")?;
            execute_sql(
                "INSERT INTO auth_passkey_credentials \
                 (tenant_id, credential_id, user_id, public_key_json, transports_json, sign_count, created_at_ms, updated_at_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7) \
                 ON CONFLICT(tenant_id, credential_id) DO UPDATE SET \
                 user_id = excluded.user_id, \
                 public_key_json = excluded.public_key_json, \
                 transports_json = excluded.transports_json, \
                 sign_count = excluded.sign_count, \
                 updated_at_ms = excluded.updated_at_ms",
                vec![
                    json!(DEFAULT_TENANT_ID),
                    json!(credential_id),
                    json!(user_id),
                    json!(public_key_json),
                    json!(transports_json),
                    json!(sign_count),
                    json!(event.recorded_at_ms),
                ],
            )
            .await?;
            Ok(true)
        }
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
            let primary_email = payload_string(payload, "primary_email");
            let expires_at_ms = required_payload_u64(payload, "expires_at_ms")?;
            let permissions_json = payload_string(payload, "permissions_json")
                .unwrap_or_else(|| payload.get("permissions").cloned().unwrap_or(Value::Array(Vec::new())).to_string());
            if let Some(email) = primary_email.as_deref() {
                upsert_user_email_identity(email, &user_id, event.recorded_at_ms).await?;
            }
            execute_sql(
                "INSERT INTO auth_sessions \
                 (session_id, tenant_id, user_id, primary_email, expires_at_ms, revoked_at_ms, permissions_json, created_at_ms, updated_at_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?7) \
                 ON CONFLICT(session_id) DO UPDATE SET \
                 tenant_id = excluded.tenant_id, \
                 user_id = excluded.user_id, \
                 primary_email = excluded.primary_email, \
                 expires_at_ms = excluded.expires_at_ms, \
                 revoked_at_ms = NULL, \
                 permissions_json = excluded.permissions_json, \
                 updated_at_ms = excluded.updated_at_ms",
                vec![
                    json!(session_id),
                    json!(DEFAULT_TENANT_ID),
                    json!(user_id),
                    json!(primary_email),
                    json!(expires_at_ms),
                    json!(permissions_json),
                    json!(event.recorded_at_ms),
                ],
            )
            .await?;
            if let Some(refresh_token_hash) = payload_string(payload, "refresh_token_hash") {
                let refresh_expires =
                    required_payload_u64(payload, "refresh_token_expires_at_ms")?;
                upsert_refresh_token_hash(
                    &refresh_token_hash,
                    &session_id,
                    refresh_expires,
                    None,
                    None,
                    event.recorded_at_ms,
                )
                .await?;
            }
            Ok(true)
        }
        "auth_refresh_token_rotated" => {
            let session_id = required_payload_string(payload, "session_id")?;
            let current_hash = required_payload_string(payload, "current_refresh_token_hash")?;
            let next_hash = required_payload_string(payload, "next_refresh_token_hash")?;
            let next_expires = required_payload_u64(payload, "next_refresh_token_expires_at_ms")?;
            execute_sql(
                "UPDATE auth_refresh_token_hashes \
                 SET rotated_at_ms = ?1 \
                 WHERE tenant_id = ?2 AND token_hash = ?3 AND rotated_at_ms IS NULL",
                vec![
                    json!(event.recorded_at_ms),
                    json!(DEFAULT_TENANT_ID),
                    json!(current_hash),
                ],
            )
            .await?;
            upsert_refresh_token_hash(
                &next_hash,
                &session_id,
                next_expires,
                None,
                None,
                event.recorded_at_ms,
            )
            .await?;
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
            upsert_signing_key_state(&kid, SigningKeyStatus::Active, Some(event.recorded_at_ms), None, None)
                .await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

async fn apply_authz_storage_event(event: &StoredStorageEvent) -> AuthStackResult<bool> {
    let payload = &event.payload;
    match event.event_type.as_str() {
        "authz_model_written" => {
            let model_id = required_payload_string(payload, "model_id")?;
            let schema_json = payload_json_string(payload, "schema_json")?;
            write_authorization_model_unchecked(
                &AuthzModelWriteRequest {
                    tenant: None,
                    model_id,
                    schema_json: schema_json.clone(),
                    idempotency_key: None,
                },
                schema_json,
            )
            .await?;
            Ok(true)
        }
        "authz_model_activated" => {
            let model_id = required_payload_string(payload, "model_id")?;
            activate_authorization_model_unchecked(&model_id).await?;
            Ok(true)
        }
        "authz_relationship_tuples_written" => {
            let tuples_json = required_payload_string(payload, "tuples_json")?;
            let tuples = parse_relationship_tuple_inputs(&tuples_json)?;
            write_relationship_tuples_unchecked(&tuples, event.recorded_at_ms).await?;
            Ok(true)
        }
        "authz_relationship_tuples_deleted" => {
            let tuples_json = required_payload_string(payload, "tuples_json")?;
            let tuples = parse_relationship_tuple_inputs(&tuples_json)?;
            delete_relationship_tuples_unchecked(&tuples).await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

async fn mark_token_grant_consumed(grant_id: &str, consumed_at_ms: u64) -> AuthStackResult<()> {
    execute_sql(
        "UPDATE auth_token_grants \
         SET consumed_at_ms = ?1 \
         WHERE tenant_id = ?2 AND grant_id = ?3 AND consumed_at_ms IS NULL",
        vec![json!(consumed_at_ms), json!(DEFAULT_TENANT_ID), json!(grant_id)],
    )
    .await?;
    Ok(())
}

async fn upsert_refresh_token_hash(
    token_hash: &str,
    session_id: &str,
    expires_at_ms: u64,
    rotated_at_ms: Option<u64>,
    revoked_at_ms: Option<u64>,
    created_at_ms: u64,
) -> AuthStackResult<()> {
    execute_sql(
        "INSERT INTO auth_refresh_token_hashes \
         (tenant_id, token_hash, session_id, expires_at_ms, rotated_at_ms, revoked_at_ms, created_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
         ON CONFLICT(tenant_id, token_hash) DO UPDATE SET \
         session_id = excluded.session_id, \
         expires_at_ms = excluded.expires_at_ms, \
         rotated_at_ms = COALESCE(excluded.rotated_at_ms, auth_refresh_token_hashes.rotated_at_ms), \
         revoked_at_ms = COALESCE(excluded.revoked_at_ms, auth_refresh_token_hashes.revoked_at_ms)",
        vec![
            json!(DEFAULT_TENANT_ID),
            json!(token_hash),
            json!(session_id),
            json!(expires_at_ms),
            json!(rotated_at_ms),
            json!(revoked_at_ms),
            json!(created_at_ms),
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
            Value::Number(value) => value.as_u64().or_else(|| value.as_i64().map(|n| n.max(0) as u64)),
            Value::String(value) => value.parse::<u64>().ok(),
            _ => None,
        })
        .ok_or_else(|| AuthStackError::store(format!("event payload is missing integer '{key}'")))
}

fn payload_json_string(payload: &Value, key: &str) -> AuthStackResult<String> {
    let value = payload
        .get(key)
        .ok_or_else(|| AuthStackError::store(format!("event payload is missing '{key}'")))?;
    Ok(value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| value.to_string()))
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

fn parse_relationship_tuple_inputs(value: &str) -> AuthStackResult<Vec<RelationshipTupleInput>> {
    let tuples: Vec<RelationshipTupleInput> = serde_json::from_str(value)
        .map_err(|error| AuthStackError::validation(format!("invalid tuples_json: {error}")))?;

    for tuple in &tuples {
        if tuple.tenant.trim().is_empty()
            || tuple.subject.trim().is_empty()
            || tuple.object.trim().is_empty()
            || tuple.relation.trim().is_empty()
        {
            return Err(AuthStackError::validation(
                "tuple tenant, subject, object, and relation are required",
            ));
        }
    }

    Ok(tuples)
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
    execute_sql(
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
    )
    .await?;
    execute_sql(
        "INSERT INTO auth_users_by_email \
         (tenant_id, normalized_email, user_id) \
         VALUES (?1, ?2, ?3) \
         ON CONFLICT(tenant_id, normalized_email) DO UPDATE SET \
         user_id = excluded.user_id",
        vec![json!(DEFAULT_TENANT_ID), json!(email), json!(user_id)],
    )
    .await?;
    Ok(())
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
        vec![json!(DEFAULT_TENANT_ID), json!(user_id), json!(credential_id)],
    )
    .await?;
    let Some(row) = rows.into_iter().next() else {
        return Err(AuthStackError::InvalidCredentials);
    };

    serde_json::from_str::<WebauthnPasskeyCredential>(&required_string(
        &row,
        "public_key_json",
    )?)
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
    execute_sql(
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
    )
    .await?;
    append_storage_event(
        "auth_passkey_credential",
        &credential_id,
        "auth_passkey_credential_upserted",
        json!({
            "tenant_id": DEFAULT_TENANT_ID,
            "credential_id": &credential_id,
            "user_id": user_id,
            "public_key_json": &public_key_json,
            "transports_json": &transports_json,
            "sign_count": credential.counter,
        }),
    )
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
        "SELECT users.user_id, users.primary_email, users.disabled, credentials.password_hash, credentials.revoked_at_ms \
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
        password_hash: required_string(&row, "password_hash")?,
        revoked_at_ms: row_i64(&row, "revoked_at_ms"),
    })
}

async fn hash_password(password: &str) -> AuthStackResult<String> {
    match password_kdf_algorithm().await?.as_str() {
        "argon2id" => hash_password_argon2id(password).await,
        PASSWORD_HASH_ALGORITHM => hash_password_pbkdf2(password, password_pbkdf2_iterations().await?),
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
    pbkdf2::<Hmac<Sha256>>(
        password.as_bytes(),
        &salt,
        iterations,
        &mut output,
    )
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

async fn verify_password(password: &str, stored_hash: &str) -> AuthStackResult<PasswordVerification> {
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
        .map_err(|error| AuthStackError::store(format!("stored Argon2 parameters are invalid: {error}")))?;
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
    Argon2Params::new(memory_kib, iterations, parallelism, Some(PASSWORD_HASH_BYTES))
        .map_err(|error| AuthStackError::configuration(format!("Argon2 password KDF policy is invalid: {error}")))
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
    let Some(value) = store_config_value(name).await.filter(|value| !value.trim().is_empty())
    else {
        return Ok(default);
    };
    value
        .trim()
        .parse::<u32>()
        .map_err(|error| AuthStackError::configuration(format!("{name} must be a positive integer: {error}")))
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
        getrandom::getrandom(&mut bytes)
            .map_err(|error| AuthStackError::store(format!("secure randomness unavailable: {error}")))?;
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
        "SELECT session_id, tenant_id, user_id, primary_email, expires_at_ms, revoked_at_ms, permissions_json \
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
    claims.tenant_id = Some(TenantId::from(DEFAULT_TENANT_ID.to_string()));
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
    Ok(format!("refresh_{}", URL_SAFE_NO_PAD.encode(random_bytes(32)?)))
}

fn refresh_token_hash(refresh_token: &str) -> String {
    let digest = Sha256::digest(refresh_token.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
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
        "RS256" => Ok(Algorithm::RS256),
        other => Err(AuthStackError::configuration(format!(
            "unsupported {name} '{other}'; supported values are HS256 and RS256"
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
    {
        if let Some(key) = keys.iter().find(|key| key.kid == active_kid) {
            return Ok(key.with_status(SigningKeyStatus::Active));
        }
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
        validate_signing_key_policy(
            production_mode,
            true,
            admin_token_configured().await,
            &keys,
        )?;
        return Ok(keys);
    }

    let keys = vec![runtime_default_signing_key_config().await?];
    validate_signing_key_policy(
        production_mode,
        false,
        admin_token_configured().await,
        &keys,
    )?;
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

async fn admin_token_configured() -> bool {
    store_config_value("AUTH_ADMIN_TOKEN")
        .await
        .is_some_and(|value| !value.trim().is_empty())
}

fn validate_signing_key_policy(
    production_mode: bool,
    key_ring_configured: bool,
    admin_token_configured: bool,
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
    if !admin_token_configured {
        return Err(AuthStackError::configuration(format!(
            "{AUTH_PRODUCTION_MODE}=true requires AUTH_ADMIN_TOKEN for signing-key administration"
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
        if key.algorithm != Algorithm::RS256 {
            return Err(AuthStackError::configuration(format!(
                "{AUTH_PRODUCTION_MODE}=true only permits RS256 signing keys; '{}' uses {}",
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
                "{AUTH_PRODUCTION_MODE}=true requires private_key_der_base64 for RS256 key '{}'",
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
    let algorithm_value = row_string(value, "alg").unwrap_or_else(|| DEFAULT_JWT_ALGORITHM.to_string());
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

async fn jwt_encoding_key_for_config(
    config: &JwtSigningKeyConfig,
) -> AuthStackResult<EncodingKey> {
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
        Algorithm::RS256 => {
            let value = config
                .private_key_der_base64
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    AuthStackError::configuration(format!(
                        "RS256 signing key '{}' is missing private_key_der_base64",
                        config.kid
                    ))
                })?;
            let private_der = decode_base64_config("private_key_der_base64", value)?;
            Ok(EncodingKey::from_rsa_der(&private_der))
        }
        _ => Err(AuthStackError::configuration(
            "configured JWT algorithm is not supported for signing",
        )),
    }
}

async fn jwt_decoding_key_for_config(
    config: &JwtSigningKeyConfig,
) -> AuthStackResult<DecodingKey> {
    match config.algorithm {
        Algorithm::HS256 => {
            let secret = config
                .secret
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or(AuthStackError::InvalidToken)?;
            Ok(DecodingKey::from_secret(secret.as_bytes()))
        }
        Algorithm::RS256 => {
            let jwks = jwt_public_jwks_for_config(config).await?.ok_or_else(|| {
                AuthStackError::configuration(format!(
                    "RS256 signing key '{}' requires public_jwks_json or private_key_der_base64",
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
    if config.algorithm != Algorithm::RS256 {
        return Ok(None);
    }

    if let Some(value) = config
        .public_jwks_json
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return parse_configured_jwks(&value).map(Some);
    }

    let encoding_key = jwt_encoding_key_for_config(config).await?;
    let mut key = jwks_key_from_json_value(
        serde_json::to_value(
            Jwk::from_encoding_key(&encoding_key, Algorithm::RS256)
                .map_err(|_| AuthStackError::InvalidToken)?,
        )
        .map_err(|error| AuthStackError::serialization(error.to_string()))?,
    )?;
    key.kid = config.kid.clone();
    key.alg = "RS256".to_string();
    key.use_ = "sig".to_string();
    Ok(Some(JwksDocument { keys: vec![key] }))
}

fn decode_base64_config(name: &str, value: &str) -> AuthStackResult<Vec<u8>> {
    let compact = value.split_whitespace().collect::<String>();
    STANDARD
        .decode(compact.as_bytes())
        .or_else(|_| URL_SAFE_NO_PAD.decode(compact.as_bytes()))
        .map_err(|error| AuthStackError::configuration(format!("{name} is not valid base64: {error}")))
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
    execute_sql(
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
    .await?;
    Ok(())
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
        Algorithm::RS256 => "RS256",
        _ => "unsupported",
    }
}

fn parse_configured_jwks(value: &str) -> AuthStackResult<JwksDocument> {
    let parsed: Value = serde_json::from_str(value).map_err(|error| {
        AuthStackError::configuration(format!("AUTH_JWT_PUBLIC_JWKS_JSON is invalid JSON: {error}"))
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
    if expected_algorithm != Algorithm::RS256 {
        return Err(AuthStackError::InvalidToken);
    }
    if !key.alg.is_empty() && key.alg != "RS256" {
        return Err(AuthStackError::InvalidToken);
    }
    if key.kty != "RSA" {
        return Err(AuthStackError::InvalidToken);
    }
    let modulus = key
        .public_parameters
        .get("n")
        .filter(|value| !value.trim().is_empty())
        .ok_or(AuthStackError::InvalidToken)?;
    let exponent = key
        .public_parameters
        .get("e")
        .filter(|value| !value.trim().is_empty())
        .ok_or(AuthStackError::InvalidToken)?;
    DecodingKey::from_rsa_components(modulus, exponent).map_err(|_| AuthStackError::InvalidToken)
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
    config_u64("AUTH_ACCESS_TOKEN_TTL_SECONDS", DEFAULT_ACCESS_TOKEN_TTL_SECONDS).await
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
        .require_user_verification(config_bool("AUTH_PASSKEY_REQUIRE_USER_VERIFICATION", true).await)
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

fn map_auth_error(error: ddd_auth::AuthError) -> AuthStackError {
    match error {
        ddd_auth::AuthError::SessionExpired => AuthStackError::SessionExpired,
        ddd_auth::AuthError::SessionRevoked => AuthStackError::AuthRequired,
        ddd_auth::AuthError::Validation { message } => AuthStackError::validation(message),
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
        tenant_id: None,
        user_id: None,
        primary_email: None,
        expires_at: None,
        permissions: Vec::new(),
    }
}

fn normalized_session_id(session_id: Option<&str>) -> Option<String> {
    session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

async fn session_permissions_for_email(email: &str) -> Vec<String> {
    let mut permissions = default_session_permissions();
    if bootstrap_admin_email(email).await {
        permissions.extend(
            [
                "auth:provider:write",
                "auth:redirect:write",
                "auth:signing-key:admin",
                "auth:storage:admin",
                "authz:model:write",
                "authz:tuple:write",
            ]
            .into_iter()
            .map(ToOwned::to_owned),
        );
        permissions.sort();
        permissions.dedup();
    }
    permissions
}

fn default_session_permissions() -> Vec<String> {
    [
        "auth:session:read",
        "auth:token:refresh",
        "auth:logout",
        "authz:check",
    ]
    .into_iter()
    .map(ToOwned::to_owned)
    .collect()
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

async fn dev_password_reset_url(grant_id: &str) -> Option<String> {
    if config_bool(AUTH_PRODUCTION_MODE, false).await {
        return None;
    }
    if !config_bool("AUTH_DEV_TOOLS", false).await {
        return None;
    }
    Some(format!("/reset-password?token={grant_id}"))
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
    Ok(format!("{kind}_{}", URL_SAFE_NO_PAD.encode(random_bytes(32)?)))
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

fn authz_validation_error(error: ddd_authz::AuthzError) -> AuthStackError {
    AuthStackError::validation(error.to_string())
}

fn map_passkey_verification_error(error: impl std::fmt::Display) -> AuthStackError {
    tracing::warn!(
        error = %error,
        "passkey verification failed"
    );
    AuthStackError::InvalidCredentials
}

#[derive(Clone, Debug, Deserialize)]
struct RelationshipTupleInput {
    tenant: String,
    subject: String,
    object: String,
    relation: String,
    #[serde(default)]
    condition_name: Option<String>,
    #[serde(default = "empty_object_json")]
    context_json: String,
}

#[derive(Clone, Copy, Debug)]
enum StorageProjectionKind {
    Auth,
    Authz,
}

impl StorageProjectionKind {
    fn checkpoint_name(self) -> &'static str {
        match self {
            Self::Auth => AUTH_STORAGE_PROJECTION_CHECKPOINT,
            Self::Authz => AUTHZ_STORAGE_PROJECTION_CHECKPOINT,
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

#[derive(Clone, Debug)]
pub struct ConsumedOauthGrant {
    pub state: String,
    pub provider_id: String,
    pub redirect_url: String,
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
    password_hash: String,
    revoked_at_ms: Option<i64>,
}

impl StoredSession {
    fn into_view(self) -> SessionView {
        SessionView {
            authenticated: true,
            tenant_id: Some(self.tenant_id),
            user_id: Some(self.user_id),
            primary_email: self.primary_email,
            expires_at: Some(self.expires_at_ms.to_string()),
            permissions: self.permissions,
        }
    }
}

fn empty_object_json() -> String {
    "{}".to_string()
}

const AUTH_SCHEMA_STATEMENTS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS auth_schema_migrations (
        version TEXT PRIMARY KEY,
        applied_at_ms INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS events (
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
    )",
    "CREATE INDEX IF NOT EXISTS idx_auth_events_aggregate ON events (aggregate_type, aggregate_id)",
    "CREATE INDEX IF NOT EXISTS idx_auth_events_type_sequence ON events (aggregate_type, sequence)",
    "CREATE TABLE IF NOT EXISTS checkpoints (
        projection_name TEXT PRIMARY KEY,
        last_sequence INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS auth_users (
        user_id TEXT PRIMARY KEY,
        tenant_id TEXT NOT NULL,
        primary_email TEXT NOT NULL,
        disabled INTEGER NOT NULL DEFAULT 0,
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS auth_users_by_email (
        tenant_id TEXT NOT NULL,
        normalized_email TEXT NOT NULL,
        user_id TEXT NOT NULL,
        PRIMARY KEY (tenant_id, normalized_email)
    )",
    "CREATE TABLE IF NOT EXISTS auth_external_identities (
        tenant_id TEXT NOT NULL,
        provider_id TEXT NOT NULL,
        provider_subject TEXT NOT NULL,
        user_id TEXT NOT NULL,
        primary_email TEXT,
        profile_json TEXT NOT NULL DEFAULT '{}',
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL,
        PRIMARY KEY (tenant_id, provider_id, provider_subject)
    )",
    "CREATE TABLE IF NOT EXISTS auth_password_credentials (
        tenant_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        password_hash TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL,
        revoked_at_ms INTEGER,
        last_authenticated_at_ms INTEGER,
        PRIMARY KEY (tenant_id, user_id)
    )",
    "CREATE TABLE IF NOT EXISTS auth_sessions (
        session_id TEXT PRIMARY KEY,
        tenant_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        primary_email TEXT,
        expires_at_ms INTEGER NOT NULL,
        revoked_at_ms INTEGER,
        permissions_json TEXT NOT NULL DEFAULT '[]',
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS auth_refresh_token_hashes (
        tenant_id TEXT NOT NULL,
        token_hash TEXT NOT NULL,
        session_id TEXT NOT NULL,
        expires_at_ms INTEGER NOT NULL,
        rotated_at_ms INTEGER,
        revoked_at_ms INTEGER,
        created_at_ms INTEGER NOT NULL,
        PRIMARY KEY (tenant_id, token_hash)
    )",
    "CREATE TABLE IF NOT EXISTS auth_signing_keys (
        tenant_id TEXT NOT NULL,
        kid TEXT NOT NULL,
        alg TEXT,
        status TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL,
        activated_at_ms INTEGER,
        retired_at_ms INTEGER,
        revoked_at_ms INTEGER,
        PRIMARY KEY (tenant_id, kid)
    )",
    "CREATE TABLE IF NOT EXISTS auth_jwks (
        kid TEXT PRIMARY KEY,
        kty TEXT NOT NULL,
        alg TEXT NOT NULL,
        use_value TEXT NOT NULL,
        public_parameters_json TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL,
        retired_at_ms INTEGER
    )",
    "CREATE TABLE IF NOT EXISTS auth_provider_configs (
        tenant_id TEXT NOT NULL,
        provider_id TEXT NOT NULL,
        display_name TEXT NOT NULL,
        login_url TEXT NOT NULL,
        enabled INTEGER NOT NULL DEFAULT 0,
        issuer_url TEXT,
        client_id TEXT,
        secret_ref TEXT,
        scopes_json TEXT NOT NULL DEFAULT '[]',
        redirect_uris_json TEXT NOT NULL DEFAULT '[]',
        claim_mapping_json TEXT NOT NULL DEFAULT '{}',
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL,
        PRIMARY KEY (tenant_id, provider_id)
    )",
    "CREATE TABLE IF NOT EXISTS auth_passkey_credentials (
        tenant_id TEXT NOT NULL,
        credential_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        public_key_json TEXT NOT NULL,
        transports_json TEXT NOT NULL DEFAULT '[]',
        sign_count INTEGER NOT NULL DEFAULT 0,
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL,
        PRIMARY KEY (tenant_id, credential_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_auth_passkey_credentials_user
        ON auth_passkey_credentials (tenant_id, user_id)",
    "CREATE TABLE IF NOT EXISTS auth_token_grants (
        grant_id TEXT PRIMARY KEY,
        tenant_id TEXT NOT NULL,
        grant_type TEXT NOT NULL,
        subject_hint TEXT,
        redirect_url TEXT NOT NULL,
        payload_json TEXT NOT NULL,
        expires_at_ms INTEGER NOT NULL,
        consumed_at_ms INTEGER,
        created_at_ms INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS auth_redirect_allowlists (
        tenant_id TEXT PRIMARY KEY,
        redirects_json TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS authz_models (
        tenant_id TEXT NOT NULL,
        model_id TEXT NOT NULL,
        schema_json TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL,
        PRIMARY KEY (tenant_id, model_id)
    )",
    "CREATE TABLE IF NOT EXISTS authz_active_model (
        tenant_id TEXT PRIMARY KEY,
        model_id TEXT NOT NULL,
        activated_at_ms INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS authz_relationship_tuples (
        tenant_id TEXT NOT NULL,
        subject_ref TEXT NOT NULL,
        relation TEXT NOT NULL,
        object_ref TEXT NOT NULL,
        condition_name TEXT,
        context_json TEXT NOT NULL DEFAULT '{}',
        created_at_ms INTEGER NOT NULL,
        PRIMARY KEY (tenant_id, object_ref, relation, subject_ref)
    )",
    "CREATE INDEX IF NOT EXISTS idx_authz_tuple_by_subject ON authz_relationship_tuples (tenant_id, subject_ref, relation, object_ref)",
    "CREATE INDEX IF NOT EXISTS idx_authz_tuple_by_object ON authz_relationship_tuples (tenant_id, object_ref, relation, subject_ref)",
    "CREATE TABLE IF NOT EXISTS authz_tuple_index_by_subject (
        tenant_id TEXT NOT NULL,
        subject_ref TEXT NOT NULL,
        relation TEXT NOT NULL,
        object_ref TEXT NOT NULL,
        PRIMARY KEY (tenant_id, subject_ref, relation, object_ref)
    )",
    "CREATE TABLE IF NOT EXISTS authz_tuple_index_by_object (
        tenant_id TEXT NOT NULL,
        object_ref TEXT NOT NULL,
        relation TEXT NOT NULL,
        subject_ref TEXT NOT NULL,
        PRIMARY KEY (tenant_id, object_ref, relation, subject_ref)
    )",
    "CREATE TABLE IF NOT EXISTS authz_check_audit (
        tenant_id TEXT NOT NULL,
        check_id TEXT NOT NULL,
        subject_ref TEXT NOT NULL,
        relation TEXT NOT NULL,
        object_ref TEXT NOT NULL,
        allowed INTEGER NOT NULL,
        reason TEXT,
        checked_at_ms INTEGER NOT NULL,
        PRIMARY KEY (tenant_id, check_id)
    )",
];

const AUTH_MYSQL_SCHEMA_STATEMENTS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS auth_schema_migrations (
        version VARCHAR(255) PRIMARY KEY,
        applied_at_ms BIGINT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS events (
        sequence BIGINT AUTO_INCREMENT PRIMARY KEY,
        event_id VARCHAR(255) NOT NULL UNIQUE,
        aggregate_id VARCHAR(255) NOT NULL,
        aggregate_type VARCHAR(255) NOT NULL,
        revision BIGINT NOT NULL,
        event_type VARCHAR(255) NOT NULL,
        event_version INT NOT NULL,
        payload LONGTEXT NOT NULL,
        metadata LONGTEXT NOT NULL,
        recorded_at_ms BIGINT NOT NULL,
        UNIQUE KEY events_aggregate_revision_unique (aggregate_type, aggregate_id, revision),
        INDEX idx_auth_events_aggregate (aggregate_type, aggregate_id),
        INDEX idx_auth_events_type_sequence (aggregate_type, sequence)
    )",
    "CREATE TABLE IF NOT EXISTS checkpoints (
        projection_name VARCHAR(255) PRIMARY KEY,
        last_sequence BIGINT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS auth_users (
        user_id VARCHAR(255) PRIMARY KEY,
        tenant_id VARCHAR(255) NOT NULL,
        primary_email VARCHAR(320) NOT NULL,
        disabled TINYINT NOT NULL DEFAULT 0,
        created_at_ms BIGINT NOT NULL,
        updated_at_ms BIGINT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS auth_users_by_email (
        tenant_id VARCHAR(255) NOT NULL,
        normalized_email VARCHAR(320) NOT NULL,
        user_id VARCHAR(255) NOT NULL,
        PRIMARY KEY (tenant_id, normalized_email)
    )",
    "CREATE TABLE IF NOT EXISTS auth_external_identities (
        tenant_id VARCHAR(255) NOT NULL,
        provider_id VARCHAR(255) NOT NULL,
        provider_subject VARCHAR(255) NOT NULL,
        user_id VARCHAR(255) NOT NULL,
        primary_email VARCHAR(320),
        profile_json LONGTEXT NOT NULL,
        created_at_ms BIGINT NOT NULL,
        updated_at_ms BIGINT NOT NULL,
        PRIMARY KEY (tenant_id, provider_id, provider_subject)
    )",
    "CREATE TABLE IF NOT EXISTS auth_password_credentials (
        tenant_id VARCHAR(255) NOT NULL,
        user_id VARCHAR(255) NOT NULL,
        password_hash TEXT NOT NULL,
        created_at_ms BIGINT NOT NULL,
        updated_at_ms BIGINT NOT NULL,
        revoked_at_ms BIGINT,
        last_authenticated_at_ms BIGINT,
        PRIMARY KEY (tenant_id, user_id)
    )",
    "CREATE TABLE IF NOT EXISTS auth_sessions (
        session_id VARCHAR(255) PRIMARY KEY,
        tenant_id VARCHAR(255) NOT NULL,
        user_id VARCHAR(255) NOT NULL,
        primary_email VARCHAR(320),
        expires_at_ms BIGINT NOT NULL,
        revoked_at_ms BIGINT,
        permissions_json LONGTEXT NOT NULL,
        created_at_ms BIGINT NOT NULL,
        updated_at_ms BIGINT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS auth_refresh_token_hashes (
        tenant_id VARCHAR(255) NOT NULL,
        token_hash VARCHAR(255) NOT NULL,
        session_id VARCHAR(255) NOT NULL,
        expires_at_ms BIGINT NOT NULL,
        rotated_at_ms BIGINT,
        revoked_at_ms BIGINT,
        created_at_ms BIGINT NOT NULL,
        PRIMARY KEY (tenant_id, token_hash)
    )",
    "CREATE TABLE IF NOT EXISTS auth_signing_keys (
        tenant_id VARCHAR(255) NOT NULL,
        kid VARCHAR(255) NOT NULL,
        alg VARCHAR(64),
        status VARCHAR(64) NOT NULL,
        created_at_ms BIGINT NOT NULL,
        updated_at_ms BIGINT NOT NULL,
        activated_at_ms BIGINT,
        retired_at_ms BIGINT,
        revoked_at_ms BIGINT,
        PRIMARY KEY (tenant_id, kid)
    )",
    "CREATE TABLE IF NOT EXISTS auth_jwks (
        kid VARCHAR(255) PRIMARY KEY,
        kty VARCHAR(64) NOT NULL,
        alg VARCHAR(64) NOT NULL,
        use_value VARCHAR(64) NOT NULL,
        public_parameters_json LONGTEXT NOT NULL,
        created_at_ms BIGINT NOT NULL,
        retired_at_ms BIGINT
    )",
    "CREATE TABLE IF NOT EXISTS auth_provider_configs (
        tenant_id VARCHAR(255) NOT NULL,
        provider_id VARCHAR(255) NOT NULL,
        display_name VARCHAR(255) NOT NULL,
        login_url TEXT NOT NULL,
        enabled TINYINT NOT NULL DEFAULT 0,
        issuer_url TEXT,
        client_id TEXT,
        secret_ref TEXT,
        scopes_json LONGTEXT NOT NULL,
        redirect_uris_json LONGTEXT NOT NULL,
        claim_mapping_json LONGTEXT NOT NULL,
        created_at_ms BIGINT NOT NULL,
        updated_at_ms BIGINT NOT NULL,
        PRIMARY KEY (tenant_id, provider_id)
    )",
    "CREATE TABLE IF NOT EXISTS auth_passkey_credentials (
        tenant_id VARCHAR(255) NOT NULL,
        credential_id VARCHAR(255) NOT NULL,
        user_id VARCHAR(255) NOT NULL,
        public_key_json LONGTEXT NOT NULL,
        transports_json LONGTEXT NOT NULL,
        sign_count BIGINT NOT NULL DEFAULT 0,
        created_at_ms BIGINT NOT NULL,
        updated_at_ms BIGINT NOT NULL,
        PRIMARY KEY (tenant_id, credential_id),
        INDEX idx_auth_passkey_credentials_user (tenant_id, user_id)
    )",
    "CREATE TABLE IF NOT EXISTS auth_token_grants (
        grant_id VARCHAR(255) PRIMARY KEY,
        tenant_id VARCHAR(255) NOT NULL,
        grant_type VARCHAR(255) NOT NULL,
        subject_hint VARCHAR(320),
        redirect_url TEXT NOT NULL,
        payload_json LONGTEXT NOT NULL,
        expires_at_ms BIGINT NOT NULL,
        consumed_at_ms BIGINT,
        created_at_ms BIGINT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS auth_redirect_allowlists (
        tenant_id VARCHAR(255) PRIMARY KEY,
        redirects_json LONGTEXT NOT NULL,
        created_at_ms BIGINT NOT NULL,
        updated_at_ms BIGINT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS authz_models (
        tenant_id VARCHAR(255) NOT NULL,
        model_id VARCHAR(255) NOT NULL,
        schema_json LONGTEXT NOT NULL,
        created_at_ms BIGINT NOT NULL,
        updated_at_ms BIGINT NOT NULL,
        PRIMARY KEY (tenant_id, model_id)
    )",
    "CREATE TABLE IF NOT EXISTS authz_active_model (
        tenant_id VARCHAR(255) PRIMARY KEY,
        model_id VARCHAR(255) NOT NULL,
        activated_at_ms BIGINT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS authz_relationship_tuples (
        tenant_id VARCHAR(255) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
        subject_ref VARCHAR(512) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
        relation VARCHAR(255) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
        object_ref VARCHAR(512) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
        condition_name VARCHAR(255),
        context_json LONGTEXT NOT NULL,
        created_at_ms BIGINT NOT NULL,
        PRIMARY KEY (tenant_id, object_ref, relation, subject_ref),
        INDEX idx_authz_tuple_by_subject (tenant_id, subject_ref, relation, object_ref),
        INDEX idx_authz_tuple_by_object (tenant_id, object_ref, relation, subject_ref)
    )",
    "CREATE TABLE IF NOT EXISTS authz_tuple_index_by_subject (
        tenant_id VARCHAR(255) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
        subject_ref VARCHAR(512) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
        relation VARCHAR(255) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
        object_ref VARCHAR(512) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
        PRIMARY KEY (tenant_id, subject_ref, relation, object_ref)
    )",
    "CREATE TABLE IF NOT EXISTS authz_tuple_index_by_object (
        tenant_id VARCHAR(255) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
        object_ref VARCHAR(512) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
        relation VARCHAR(255) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
        subject_ref VARCHAR(512) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
        PRIMARY KEY (tenant_id, object_ref, relation, subject_ref)
    )",
    "CREATE TABLE IF NOT EXISTS authz_check_audit (
        tenant_id VARCHAR(255) NOT NULL,
        check_id VARCHAR(255) NOT NULL,
        subject_ref VARCHAR(512) NOT NULL,
        relation VARCHAR(255) NOT NULL,
        object_ref VARCHAR(512) NOT NULL,
        allowed TINYINT NOT NULL,
        reason TEXT,
        checked_at_ms BIGINT NOT NULL,
        PRIMARY KEY (tenant_id, check_id)
    )",
];

#[cfg(test)]
mod tests {
    use super::*;

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
    fn parse_relationship_tuple_inputs_accepts_valid_tuple_json() {
        let tuples = parse_relationship_tuple_inputs(
            r#"[{"tenant":"tenant:default","subject":"user:alice","object":"project:demo","relation":"viewer"}]"#,
        )
        .unwrap();

        assert_eq!(tuples.len(), 1);
    }

    #[test]
    fn parse_relationship_tuple_inputs_rejects_missing_relation() {
        let error = parse_relationship_tuple_inputs(
            r#"[{"tenant":"tenant:default","subject":"user:alice","object":"project:demo","relation":""}]"#,
        )
        .unwrap_err();

        assert_eq!(error.public_code(), "validation");
    }

    #[test]
    fn provider_display_name_formats_custom_provider_id() {
        assert_eq!(
            provider_display_name("github-enterprise"),
            "Github Enterprise"
        );
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
                verify_password("wrong password", &stored_hash).await.unwrap(),
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

        assert!(validate_signing_key_policy(false, false, false, &keys).is_ok());
    }

    #[test]
    fn signing_key_policy_rejects_production_runtime_key() {
        let keys = vec![test_signing_key(
            "dev-key",
            Algorithm::HS256,
            SigningKeyStatus::Active,
        )];
        let error = validate_signing_key_policy(true, false, true, &keys).unwrap_err();

        assert_eq!(error.public_code(), "configuration");
    }

    #[test]
    fn signing_key_policy_rejects_production_without_admin_token() {
        let keys = vec![test_signing_key(
            "prod-key",
            Algorithm::RS256,
            SigningKeyStatus::Active,
        )];
        let error = validate_signing_key_policy(true, true, false, &keys).unwrap_err();

        assert_eq!(error.public_code(), "configuration");
    }

    #[test]
    fn signing_key_policy_rejects_hs256_key_ring_in_production() {
        let keys = vec![test_signing_key(
            "shared-secret",
            Algorithm::HS256,
            SigningKeyStatus::Active,
        )];
        let error = validate_signing_key_policy(true, true, true, &keys).unwrap_err();

        assert_eq!(error.public_code(), "configuration");
    }

    #[test]
    fn signing_key_policy_accepts_rs256_key_ring_in_production() {
        let keys = vec![test_signing_key(
            "prod-key",
            Algorithm::RS256,
            SigningKeyStatus::Active,
        )];

        assert!(validate_signing_key_policy(true, true, true, &keys).is_ok());
    }

    #[test]
    fn schema_defines_password_credentials_table() {
        assert!(
            AUTH_SCHEMA_STATEMENTS
                .iter()
                .any(|statement| statement.contains("auth_password_credentials"))
        );
    }

    #[test]
    fn schema_defines_external_identities_table() {
        assert!(
            AUTH_SCHEMA_STATEMENTS
                .iter()
                .any(|statement| statement.contains("auth_external_identities"))
        );
    }

    #[test]
    fn schema_defines_signing_key_lifecycle_table() {
        assert!(
            AUTH_SCHEMA_STATEMENTS
                .iter()
                .any(|statement| statement.contains("auth_signing_keys"))
        );
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
        let sql = "INSERT OR IGNORE INTO authz_tuple_index_by_subject (tenant_id, subject_ref) VALUES (?1, ?2)";

        let rewritten = postgres_sql(sql);

        assert_eq!(
            rewritten,
            "INSERT INTO authz_tuple_index_by_subject (tenant_id, subject_ref) VALUES ($1, $2) ON CONFLICT DO NOTHING"
        );
    }

    #[test]
    fn mysql_sql_rewrites_indexed_placeholders_and_insert_ignore() {
        let sql = "INSERT OR IGNORE INTO authz_tuple_index_by_subject (tenant_id, subject_ref) VALUES (?1, ?2)";

        let rewritten = mysql_sql(sql);

        assert_eq!(
            rewritten,
            "INSERT IGNORE INTO authz_tuple_index_by_subject (tenant_id, subject_ref) VALUES (?, ?)"
        );
    }

    #[test]
    fn mysql_sql_rewrites_on_conflict_update() {
        let sql = "INSERT INTO checkpoints (projection_name, last_sequence) VALUES (?1, ?2) ON CONFLICT(projection_name) DO UPDATE SET last_sequence = CASE WHEN excluded.last_sequence > checkpoints.last_sequence THEN excluded.last_sequence ELSE checkpoints.last_sequence END";

        let rewritten = mysql_sql(sql);

        assert_eq!(
            rewritten,
            "INSERT INTO checkpoints (projection_name, last_sequence) VALUES (?, ?) ON DUPLICATE KEY UPDATE last_sequence = CASE WHEN VALUES(last_sequence) > checkpoints.last_sequence THEN VALUES(last_sequence) ELSE checkpoints.last_sequence END"
        );
    }

    #[test]
    fn mysql_sql_duplicates_reused_indexed_parameters() {
        let sql = "INSERT INTO auth_provider_configs (tenant_id, provider_id, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?3)";

        let (_, params) =
            mysql_sql_and_params(sql, vec![json!("tenant"), json!("google"), json!(42)])
                .expect("mysql params should expand");

        assert_eq!(
            params,
            vec![json!("tenant"), json!("google"), json!(42), json!(42)]
        );
    }

    #[test]
    fn mysql_schema_defines_tuple_indexes_inline() {
        assert!(
            AUTH_MYSQL_SCHEMA_STATEMENTS
                .iter()
                .any(|statement| statement.contains("idx_authz_tuple_by_subject"))
        );
        assert!(
            AUTH_MYSQL_SCHEMA_STATEMENTS
                .iter()
                .any(|statement| statement.contains("subject_ref VARCHAR(512) CHARACTER SET ascii COLLATE ascii_bin"))
        );
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
