//! Thin Spin runtime adapter for product workflows owned by `wasi-auth`.
#![allow(unused_imports)]
#![allow(dead_code)]

use std::{
    collections::VecDeque,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use sha2::{Digest, Sha256};
#[cfg(feature = "mail-capture")]
use wasi_auth::mail::{EmailKind, Recipient};
#[cfg(feature = "mail-capture")]
use wasi_auth::postgres::outbox::{MailOutboxWorker, PublicBaseUrl};
use wasi_auth::{
    authentication::jwt::JwksDocument,
    authentication::mfa::TotpConfig,
    authentication::passkeys::Attachment as PasskeyAttachment,
    authentication::{Clock, RandomSource},
    context::{AuthenticationAssurance, RequestId, SessionId, UserId},
    http::{AuthenticatedSession, TrustedContextCodec},
    postgres::workflows::{
        Argon2Policy, EmailVerificationError, EmailVerificationRequest,
        EmailVerificationResendRequest as ProductEmailVerificationResendRequest,
        EmailVerificationService, OutboxSealingKey,
        PasswordChangeRequest as ProductPasswordChangeRequest, PasswordLoginError,
        PasswordLoginRequest, PasswordLoginService, PasswordRegistrationError,
        PasswordRegistrationRequest, PasswordRegistrationService,
        PasswordResetCompleteRequest as ProductPasswordResetCompleteRequest, PasswordResetError,
        PasswordResetService, PasswordResetStartRequest as ProductPasswordResetStartRequest,
    },
    postgres::{
        PostgresAuthStore, PostgresStoreError,
        flows::FlowSealingKey,
        management::{
            AdminUserRecord, AuditEventRecord, InvitationRecord, InvitationService,
            ManagementError, MembershipRecord, ORGANIZATION_PERMISSION_CATALOG,
            OrganizationManagementService, RoleRecord,
            UpsertRoleRequest as ProductUpsertRoleRequest,
        },
        mfa::{MfaKeyMaterial, MfaService, MfaServiceError},
        oauth::{
            OAuthFlowService, OAuthProviderService, OAuthProviderServiceError, OAuthServiceConfig,
            OAuthServiceError, PendingOAuthFlow, VerifiedOAuthIdentity,
        },
        organizations::{
            CreateOrganizationRequest, OrganizationError, OrganizationRecord, OrganizationService,
        },
        passkeys::{
            PasskeyConfigurationError, PasskeyService, PasskeyServiceConfig, PasskeyServiceError,
        },
        policy::{
            ActivePolicyBundle, PolicyBundleLoadError, PolicyBundleRecord, PolicyBundleService,
            PolicyBundleServiceError,
        },
        rate_limits::{RateLimitError, RateLimitService},
        sessions::{SessionService, SessionServiceError},
        signing::{SigningKeyRecord, SigningKeyService, SigningKeyServiceError},
        spin::{SpinPostgresError, SpinPostgresTransport},
        tokens::{
            AccessTokenVerifier, JwtKeyRing, RefreshSealingKey, TokenService, TokenServiceConfig,
            TokenServiceError, VerifiedAccessToken,
        },
    },
};

use crate::{
    contracts::{
        AccountSessionListResponse, AccountSessionSummary, AdminUserListResponse, AdminUserSummary,
        AuditEventListResponse, AuditEventSummary, AuthProviderSummary, CapturedMailResponse,
        EmailPasswordLoginRequest, EmailPasswordRegisterRequest, EmailVerificationCompleteRequest,
        InvitationListResponse, InvitationSummary, LoginCompletionResponse, LogoutResponse,
        MembershipListResponse, MembershipSummary, MfaEnrollConfirmResponse,
        MfaEnrollStartResponse, MfaStatusResponse, OrganizationListResponse, OrganizationSummary,
        PasskeyStartResponse, PasswordResetCompleteRequest, PasswordResetStartRequest,
        PasswordResetStartResponse, PolicyVersionListResponse, PolicyVersionSummary,
        RoleListResponse, RoleSummary, SessionView, SigningKeyListResponse,
        SigningKeyRotateResponse, SigningKeySummary, TokenRefreshResponse, TokenVerifyResponse,
    },
    error::{AuthStackError, AuthStackResult},
};

use super::*;

pub(crate) fn organization_summary(record: OrganizationRecord) -> OrganizationSummary {
    OrganizationSummary {
        organization_id: record.organization_id,
        name: record.name,
        slug: record.slug,
        status: record.status,
        current_user_role: record.role_id,
        permissions: record.permissions,
        created_at_ms: record.created_at_ms,
    }
}

pub(crate) async fn organization_summary_with_slug(
    record: OrganizationRecord,
) -> OrganizationSummary {
    let mut summary = organization_summary(record);
    // Prefer DB slug; fill KV cache and backfill empty DB slugs via ensure.
    if summary.slug.trim().is_empty() {
        if let Ok(slug) =
            crate::store::ensure_org_slug(&summary.organization_id, &summary.name).await
        {
            summary.slug = slug;
        }
    } else {
        let _ = crate::store::register_org_slug(&summary.organization_id, &summary.slug).await;
    }
    summary
}

pub(crate) fn membership_summary(record: MembershipRecord) -> MembershipSummary {
    MembershipSummary {
        organization_id: record.organization_id,
        user_id: record.user_id,
        primary_email: record.primary_email,
        role_id: record.role_id,
        status: record.status,
        joined_at_ms: record.joined_at_ms,
    }
}

pub(crate) fn invitation_summary(record: InvitationRecord) -> InvitationSummary {
    InvitationSummary {
        invitation_id: record.invitation_id,
        organization_id: record.organization_id,
        email: record.email,
        role_id: record.role_id,
        status: record.status,
        expires_at_ms: record.expires_at_ms,
    }
}

pub(crate) fn role_summary(record: RoleRecord) -> RoleSummary {
    RoleSummary {
        organization_id: record.organization_id,
        role_id: record.role_id,
        name: record.name,
        built_in: record.built_in,
        permissions: record.permissions,
    }
}

pub(crate) fn admin_user_summary(record: AdminUserRecord) -> AdminUserSummary {
    AdminUserSummary {
        user_id: record.user_id,
        primary_email: record.primary_email,
        disabled: record.status == "disabled",
        email_verified: record.status != "pending_verification",
        created_at_ms: record.created_at_ms,
    }
}

pub(crate) fn audit_event_summary(record: AuditEventRecord) -> AuditEventSummary {
    AuditEventSummary {
        sequence: record.sequence,
        organization_id: record.organization_id,
        actor_user_id: record.actor_user_id,
        action: record.action,
        target_type: record.resource_type,
        target_id: record.resource_id,
        outcome: record.outcome,
        recorded_at_ms: record.occurred_at_ms,
    }
}

pub(crate) async fn management_service() -> AuthStackResult<
    OrganizationManagementService<SpinPostgresTransport, RuntimeClock, RuntimeRandom>,
> {
    Ok(OrganizationManagementService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
    ))
}

pub(crate) fn bounded_session_id(session_id: &str) -> AuthStackResult<SessionId> {
    SessionId::new(session_id.to_owned()).map_err(|_| AuthStackError::AuthRequired)
}

pub(crate) async fn load_session(
    session_id: &str,
) -> AuthStackResult<wasi_auth::postgres::VerifiedSession> {
    let session_id =
        SessionId::new(session_id.to_owned()).map_err(|_| AuthStackError::AuthRequired)?;
    store()
        .await?
        .load_verified_session(
            &session_id,
            request_id("session")?,
            RuntimeClock.now_unix_seconds(),
        )
        .await
        .map_err(map_session_store_error)
}

pub(crate) fn unauthenticated_session() -> SessionView {
    SessionView {
        authenticated: false,
        session_id: None,
        tenant_id: None,
        user_id: None,
        primary_email: None,
        expires_at: None,
        permissions: Vec::new(),
        assurance: "none".to_owned(),
        system_administrator: false,
        issued_at_unix_seconds: None,
        expires_at_unix_seconds: None,
    }
}

pub(crate) async fn store() -> AuthStackResult<PostgresAuthStore<SpinPostgresTransport>> {
    let database_url = runtime_config_value("DATABASE_URL")
        .await
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AuthStackError::configuration("DATABASE_URL is required"))?;
    let transport = SpinPostgresTransport::new(database_url)
        .map_err(|_| AuthStackError::configuration("DATABASE_URL is invalid"))?;
    Ok(PostgresAuthStore::new(transport))
}

pub(crate) async fn outbox_key() -> AuthStackResult<OutboxSealingKey> {
    let configured = runtime_config_value("AUTH_OUTBOX_KEY_BASE64")
        .await
        .filter(|value| !value.trim().is_empty());
    let production = config_bool("AUTH_PRODUCTION_MODE", false).await;
    let key: [u8; 32] = match configured {
        Some(encoded) => STANDARD
            .decode(encoded.trim())
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| {
                AuthStackError::configuration("AUTH_OUTBOX_KEY_BASE64 must decode to 32 bytes")
            })?,
        None if production => {
            return Err(AuthStackError::configuration(
                "AUTH_OUTBOX_KEY_BASE64 is required in production",
            ));
        }
        None => Sha256::digest(DEVELOPMENT_OUTBOX_KEY).into(),
    };
    let development_key: [u8; 32] = Sha256::digest(DEVELOPMENT_OUTBOX_KEY).into();
    if production && key == development_key {
        return Err(AuthStackError::configuration(
            "production forbids the development outbox key",
        ));
    }
    let configured_version = runtime_config_value("AUTH_OUTBOX_KEY_VERSION")
        .await
        .filter(|value| !value.trim().is_empty());
    let key_version = match configured_version {
        Some(version) if !production || version != "development-v1" => version,
        Some(_) => {
            return Err(AuthStackError::configuration(
                "production forbids the development outbox key version",
            ));
        }
        None if production => {
            return Err(AuthStackError::configuration(
                "AUTH_OUTBOX_KEY_VERSION is required in production",
            ));
        }
        None => "development-v1".to_owned(),
    };
    OutboxSealingKey::new(key_version, key)
        .map_err(|_| AuthStackError::configuration("outbox key configuration is invalid"))
}

pub(crate) async fn flow_sealing_key() -> AuthStackResult<FlowSealingKey> {
    let (version, key) = vault_key_material().await?;
    FlowSealingKey::new(format!("flow:{version}"), key)
        .map_err(|_| AuthStackError::configuration("flow sealing key is invalid"))
}

pub(crate) async fn oauth_flow_service()
-> AuthStackResult<OAuthFlowService<SpinPostgresTransport, RuntimeClock, RuntimeRandom>> {
    let config = OAuthServiceConfig::new(
        config_u64("AUTH_OAUTH_STATE_TTL_SECONDS", 10 * 60).await?,
        session_ttl_seconds().await?,
    )
    .map_err(|_| AuthStackError::configuration("OAuth lifetime configuration is invalid"))?;
    Ok(OAuthFlowService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
        flow_sealing_key().await?,
        config,
    ))
}

pub(crate) async fn oauth_provider_service()
-> AuthStackResult<OAuthProviderService<SpinPostgresTransport, RuntimeClock, RuntimeRandom>> {
    Ok(OAuthProviderService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
    ))
}

pub(crate) async fn policy_bundle_service()
-> AuthStackResult<PolicyBundleService<SpinPostgresTransport, RuntimeClock, RuntimeRandom>> {
    Ok(PolicyBundleService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
    ))
}

pub(crate) async fn signing_key_service()
-> AuthStackResult<SigningKeyService<SpinPostgresTransport, RuntimeClock, RuntimeRandom>> {
    Ok(SigningKeyService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
    ))
}

/// Browsers reject IP addresses as WebAuthn `rpId` (`SecurityError`). Map
/// loopback IPs to `localhost` so local Spin defaults remain usable.
pub(crate) fn normalize_passkey_rp_id(rp_id: String) -> String {
    match rp_id.trim() {
        "127.0.0.1" | "::1" | "[::1]" => "localhost".to_owned(),
        other => other.to_owned(),
    }
}

/// Keep passkey origin host aligned with the normalized rpId for loopback.
pub(crate) fn normalize_passkey_origin(origin: String) -> String {
    let trimmed = origin.trim().trim_end_matches('/');
    for host in ["127.0.0.1", "[::1]", "::1"] {
        let needle = format!("//{host}");
        if let Some(index) = trimmed.find(&needle) {
            let mut rewritten = String::with_capacity(trimmed.len() + "localhost".len());
            rewritten.push_str(&trimmed[..index + 2]);
            rewritten.push_str("localhost");
            rewritten.push_str(&trimmed[index + 2 + host.len()..]);
            return rewritten;
        }
    }
    trimmed.to_owned()
}

pub(crate) async fn passkey_service()
-> AuthStackResult<PasskeyService<SpinPostgresTransport, RuntimeClock, RuntimeRandom>> {
    let attachment = match runtime_config_value("AUTH_PASSKEY_AUTHENTICATOR_ATTACHMENT")
        .await
        .unwrap_or_else(|| "platform".to_owned())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "platform" => PasskeyAttachment::Platform,
        "cross-platform" | "cross_platform" | "roaming" => PasskeyAttachment::CrossPlatform,
        "any" | "none" | "unspecified" => PasskeyAttachment::Any,
        _ => {
            return Err(AuthStackError::configuration(
                "AUTH_PASSKEY_AUTHENTICATOR_ATTACHMENT must be platform, cross-platform, or any",
            ));
        }
    };
    let rp_id = normalize_passkey_rp_id(
        runtime_config_value("AUTH_PASSKEY_RP_ID")
            .await
            .unwrap_or_else(|| "localhost".to_owned()),
    );
    let rp_name = runtime_config_value("AUTH_PASSKEY_RP_NAME")
        .await
        .unwrap_or_else(|| "fullstack-app".to_owned());
    let origin = normalize_passkey_origin(
        runtime_config_value("AUTH_PASSKEY_ORIGIN")
            .await
            .unwrap_or_else(|| crate::application::DEFAULT_PUBLIC_BASE_URL.to_owned()),
    );
    let config = PasskeyServiceConfig::new(
        &rp_id,
        &rp_name,
        &origin,
        config_bool("AUTH_PRODUCTION_MODE", false).await,
        attachment,
        config_bool("AUTH_PASSKEY_REQUIRE_USER_VERIFICATION", true).await,
        config_bool("AUTH_PASSKEY_REQUIRE_USER_HANDLE", false).await,
        config_u64("AUTH_PASSKEY_CHALLENGE_TTL_SECONDS", 5 * 60).await?,
        session_ttl_seconds().await?,
    )
    .map_err(|PasskeyConfigurationError::Invalid| {
        AuthStackError::configuration("passkey relying-party configuration is invalid")
    })?;
    Ok(PasskeyService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
        flow_sealing_key().await?,
        config,
    ))
}

pub(crate) async fn vault_key_material() -> AuthStackResult<(String, [u8; 32])> {
    let configured = runtime_config_value("AUTH_VAULT_KEY_BASE64")
        .await
        .filter(|value| !value.trim().is_empty());
    let production = config_bool("AUTH_PRODUCTION_MODE", false).await;
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
        None => Sha256::digest(DEVELOPMENT_OUTBOX_KEY).into(),
    };
    let key_version = runtime_config_value("AUTH_VAULT_KEY_VERSION")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "development-v1".to_owned());
    Ok((key_version, key))
}

pub(crate) async fn transactional_mail_config()
-> AuthStackResult<wasi_auth::mail::TransactionalMailConfig> {
    let product_name = runtime_config_value("AUTH_MAIL_PRODUCT_NAME")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Goldcoders".to_owned());
    let product_name = wasi_auth::mail::MailProductName::new(product_name)
        .map_err(|_| AuthStackError::configuration("AUTH_MAIL_PRODUCT_NAME is invalid"))?;
    let public_base_url = runtime_config_value("AUTH_PUBLIC_BASE_URL")
        .await
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AuthStackError::configuration("AUTH_PUBLIC_BASE_URL is required"))?;
    wasi_auth::mail::TransactionalMailConfig::new(product_name, public_base_url)
        .map_err(|_| AuthStackError::configuration("transactional mail configuration is invalid"))
}

pub(crate) async fn token_service() -> AuthStackResult<RuntimeTokenService> {
    let mut key_ring = configured_jwt_key_ring().await?;
    synchronize_signing_keys(&mut key_ring).await?;
    configured_token_service(key_ring).await
}

pub(crate) async fn token_verification_service() -> AuthStackResult<&'static RuntimeTokenVerifier> {
    if let Some(verifier) = TOKEN_VERIFIER.get() {
        return Ok(verifier);
    }
    let issuer = runtime_config_value("AUTH_JWT_ISSUER")
        .await
        .unwrap_or_else(|| crate::application::DEFAULT_PUBLIC_BASE_URL.to_owned());
    let audience = runtime_config_value("AUTH_JWT_AUDIENCE")
        .await
        .unwrap_or_else(|| "fullstack-app".to_owned());
    let verifier = AccessTokenVerifier::new(
        store().await?,
        RuntimeClock,
        configured_jwt_key_ring().await?,
        issuer,
        audience,
    )
    .map_err(|_| AuthStackError::configuration("token verification configuration is invalid"))?;
    let _ = TOKEN_VERIFIER.set(verifier);
    TOKEN_VERIFIER.get().ok_or_else(|| {
        AuthStackError::configuration("token verification service could not be initialized")
    })
}

pub async fn trusted_context_codec() -> AuthStackResult<Option<&'static TrustedContextCodec>> {
    if let Some(codec) = TRUSTED_CONTEXT_CODEC.get() {
        return Ok(Some(codec));
    }
    let Some(encoded_key) = runtime_config_value("AUTH_TRUSTED_INGRESS_KEY_BASE64")
        .await
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(None);
    };
    let key: [u8; 32] = STANDARD
        .decode(encoded_key.trim())
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| {
            AuthStackError::configuration("AUTH_TRUSTED_INGRESS_KEY_BASE64 must decode to 32 bytes")
        })?;
    let audience = runtime_config_value("AUTH_TRUSTED_INGRESS_AUDIENCE")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "fullstack-app".to_owned());
    let max_age = config_u64("AUTH_TRUSTED_INGRESS_MAX_AGE_SECONDS", 5).await?;
    let codec = TrustedContextCodec::new(audience, key)
        .and_then(|codec| codec.with_max_age_seconds(max_age))
        .map_err(|_| AuthStackError::configuration("trusted ingress configuration is invalid"))?;
    let _ = TRUSTED_CONTEXT_CODEC.set(codec);
    Ok(TRUSTED_CONTEXT_CODEC.get())
}

pub async fn trusted_ingress_required() -> bool {
    config_bool("AUTH_PRODUCTION_MODE", false).await
        || config_bool("AUTH_REQUIRE_TRUSTED_INGRESS", false).await
}

pub(crate) fn verified_token_cache() -> &'static VerifiedTokenCache {
    VERIFIED_TOKEN_CACHE.get_or_init(|| Mutex::new(VecDeque::new()))
}

pub(crate) fn token_cache_key(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

pub(crate) fn cached_verified_token(token: &str) -> Option<VerifiedAccessToken> {
    let key = token_cache_key(token);
    let mut cache = verified_token_cache().lock().ok()?;
    let index = cache.iter().position(|(candidate, _)| candidate == &key)?;
    let entry = cache.remove(index)?;
    if entry.1.expires_at_seconds <= RuntimeClock.now_unix_seconds() {
        return None;
    }
    let verified = entry.1.clone();
    cache.push_back(entry);
    Some(verified)
}

pub(crate) fn cache_verified_token(token: &str, verified: VerifiedAccessToken) {
    let key = token_cache_key(token);
    let Ok(mut cache) = verified_token_cache().lock() else {
        return;
    };
    if let Some(index) = cache.iter().position(|(candidate, _)| candidate == &key) {
        cache.remove(index);
    }
    cache.push_back((key, verified));
    while cache.len() > VERIFIED_TOKEN_CACHE_CAPACITY {
        cache.pop_front();
    }
}

pub(crate) fn remove_cached_verified_token(token: &str) {
    let key = token_cache_key(token);
    let Ok(mut cache) = verified_token_cache().lock() else {
        return;
    };
    if let Some(index) = cache.iter().position(|(candidate, _)| candidate == &key) {
        cache.remove(index);
    }
}

pub(crate) async fn configured_token_service(
    key_ring: JwtKeyRing,
) -> AuthStackResult<RuntimeTokenService> {
    let (version, key) = vault_key_material().await?;
    let sealing_key = RefreshSealingKey::new(format!("refresh:{version}"), key)
        .map_err(|_| AuthStackError::configuration("refresh sealing key is invalid"))?;
    let issuer = runtime_config_value("AUTH_JWT_ISSUER")
        .await
        .unwrap_or_else(|| crate::application::DEFAULT_PUBLIC_BASE_URL.to_owned());
    let audience = runtime_config_value("AUTH_JWT_AUDIENCE")
        .await
        .unwrap_or_else(|| "fullstack-app".to_owned());
    let config = TokenServiceConfig::new(
        issuer,
        audience,
        config_u64("AUTH_ACCESS_TOKEN_TTL_SECONDS", 15 * 60).await?,
        config_u64("AUTH_REFRESH_TOKEN_TTL_SECONDS", 30 * 24 * 60 * 60).await?,
        session_ttl_seconds().await?,
    )
    .map_err(|_| AuthStackError::configuration("token lifetime configuration is invalid"))?;
    Ok(TokenService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
        key_ring,
        sealing_key,
        config,
    ))
}

pub(crate) async fn configured_jwt_key_ring() -> AuthStackResult<JwtKeyRing> {
    let production = config_bool("AUTH_PRODUCTION_MODE", false).await;
    Ok(
        match runtime_config_value("AUTH_JWT_KEY_RING_JSON")
            .await
            .filter(|value| !value.trim().is_empty())
        {
            Some(value) => JwtKeyRing::from_json(&value, production)
                .map_err(|_| AuthStackError::configuration("AUTH_JWT_KEY_RING_JSON is invalid"))?,
            None if production => {
                return Err(AuthStackError::configuration(
                    "AUTH_JWT_KEY_RING_JSON with an active ES256 key is required in production",
                ));
            }
            None => {
                let kid = runtime_config_value("AUTH_JWT_KID")
                    .await
                    .unwrap_or_else(|| "fullstack-app-dev-hs256".to_owned());
                let secret = runtime_config_value("AUTH_JWT_SECRET")
                    .await
                    .unwrap_or_else(|| "dev-fullstack-app-secret-change-me".to_owned());
                JwtKeyRing::development_hs256(kid, secret.into_bytes())
                    .map_err(|_| AuthStackError::configuration("development JWT key is invalid"))?
            }
        },
    )
}

pub(crate) async fn synchronize_signing_keys(key_ring: &mut JwtKeyRing) -> AuthStackResult<()> {
    let service = signing_key_service().await?;
    let configured = key_ring.descriptors();
    let mut metadata = service.list().await.map_err(map_signing_key_error)?;
    if configured.iter().any(|descriptor| {
        !metadata
            .iter()
            .any(|record| record.key_id == descriptor.kid)
    }) {
        let version = runtime_config_value("AUTH_JWT_KEY_VERSION")
            .await
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "runtime-v1".to_owned());
        metadata = service
            .synchronize(key_ring, &version)
            .await
            .map_err(map_signing_key_error)?;
    }
    let statuses = metadata
        .into_iter()
        .map(|record| (record.key_id, record.status))
        .collect::<Vec<_>>();
    key_ring
        .apply_statuses(&statuses)
        .map_err(|_| AuthStackError::configuration("signing-key lifecycle is invalid"))
}

pub(crate) async fn mfa_service()
-> AuthStackResult<MfaService<SpinPostgresTransport, RuntimeClock, RuntimeRandom>> {
    let (version, encryption_key) = vault_key_material().await?;
    let production = config_bool("AUTH_PRODUCTION_MODE", false).await;
    let recovery_pepper = match runtime_config_value("AUTH_RECOVERY_CODE_PEPPER_BASE64")
        .await
        .filter(|value| !value.trim().is_empty())
    {
        Some(value) => STANDARD
            .decode(value.trim())
            .map_err(|_| AuthStackError::configuration("recovery-code pepper is invalid"))?,
        None if production => {
            return Err(AuthStackError::configuration(
                "AUTH_RECOVERY_CODE_PEPPER_BASE64 is required in production",
            ));
        }
        None => Sha256::digest(b"fullstack-development-recovery-pepper").to_vec(),
    };
    let keys = MfaKeyMaterial::new(format!("mfa:{version}"), encryption_key, recovery_pepper)
        .map_err(|_| AuthStackError::configuration("MFA key configuration is invalid"))?;
    let issuer = runtime_config_value("AUTH_MFA_ISSUER")
        .await
        .unwrap_or_else(|| "fullstack-app".to_owned());
    MfaService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
        keys,
        TotpConfig::default(),
        issuer,
    )
    .map_err(|_| AuthStackError::configuration("MFA service configuration is invalid"))
}

pub(crate) async fn issue_tokens(session_id: &SessionId) -> AuthStackResult<(String, String, u64)> {
    token_service()
        .await?
        .issue(session_id, &request_id("issue-token")?)
        .await
        .map(|pair| pair.into_parts())
        .map_err(map_token_error)
}

pub(crate) async fn finalize_new_session(
    session_id: &SessionId,
) -> AuthStackResult<(String, String, u64)> {
    let result = async {
        // wasi-auth management SQL hard-requires AAL2 for org/member/role mutations.
        // Local password login is AAL1. When mutation step-up is not enforced
        // (default outside production), promote the session so the demo UI works
        // without MFA. Production keeps AAL1 until real step-up.
        maybe_promote_development_session_assurance(session_id.as_str()).await?;
        bind_default_organization_for_session(session_id.as_str()).await?;
        issue_tokens(session_id).await
    }
    .await;
    if result.is_err() {
        let _ = revoke_user_session(session_id.as_str(), session_id.as_str()).await;
    }
    result
}

/// Promote AAL1 → AAL2 for local/dev sessions when step-up is not required.
async fn maybe_promote_development_session_assurance(session_id: &str) -> AuthStackResult<()> {
    if crate::application::mutation_step_up_required().await {
        return Ok(());
    }
    let sid = session_id.trim();
    if sid.is_empty() {
        return Ok(());
    }
    // Direct SQL: wasi-auth has no public "promote assurance without TOTP" API.
    crate::store::execute_sql(
        "UPDATE auth_sessions \
         SET assurance = 'aal2', updated_at_ms = (EXTRACT(EPOCH FROM NOW()) * 1000)::bigint \
         WHERE session_id = ?1::text::uuid \
           AND revoked_at_ms IS NULL \
           AND assurance = 'aal1'",
        vec![serde_json::Value::String(sid.to_owned())],
    )
    .await
    .map(|_| ())
}

pub(crate) async fn argon2_policy() -> AuthStackResult<Argon2Policy> {
    let memory = config_u32("AUTH_PASSWORD_ARGON2_MEMORY_KIB", 19_456).await?;
    let iterations = config_u32("AUTH_PASSWORD_ARGON2_ITERATIONS", 2).await?;
    let parallelism = config_u32("AUTH_PASSWORD_ARGON2_PARALLELISM", 1).await?;
    Argon2Policy::new(memory, iterations, parallelism)
        .map_err(|_| AuthStackError::configuration("Argon2id policy is invalid"))
}

pub(crate) async fn session_ttl_seconds() -> AuthStackResult<u64> {
    let value = runtime_config_value("AUTH_SESSION_TTL_SECONDS")
        .await
        .filter(|value| !value.trim().is_empty());
    value.map_or(Ok(3_600), |value| {
        value.trim().parse::<u64>().map_err(|_| {
            AuthStackError::configuration("AUTH_SESSION_TTL_SECONDS must be an integer")
        })
    })
}
