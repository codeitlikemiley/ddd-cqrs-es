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

pub async fn list_oauth_providers() -> AuthStackResult<Vec<AuthProviderSummary>> {
    oauth_provider_service()
        .await?
        .list()
        .await
        .map_err(map_oauth_provider_error)
        .map(|providers| {
            providers
                .into_iter()
                .map(|provider| AuthProviderSummary {
                    login_url: format!("/api/auth/oauth/{}/start", provider.provider_id),
                    provider_id: provider.provider_id,
                    display_name: provider.display_name,
                    enabled: provider.enabled,
                })
                .collect()
        })
}

pub async fn find_oauth_provider(
    provider_id: &str,
) -> AuthStackResult<Option<AuthProviderSummary>> {
    oauth_provider_service()
        .await?
        .get(provider_id)
        .await
        .map_err(map_oauth_provider_error)
        .map(|provider| {
            provider.map(|provider| AuthProviderSummary {
                login_url: format!("/api/auth/oauth/{}/start", provider.provider_id),
                provider_id: provider.provider_id,
                display_name: provider.display_name,
                enabled: provider.enabled,
            })
        })
}

pub async fn save_oauth_provider(
    session_id: &str,
    provider_id: &str,
    enabled: bool,
) -> AuthStackResult<AuthProviderSummary> {
    let session_id = bounded_session_id(session_id)?;
    let display_name = provider_id
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + characters.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ");
    oauth_provider_service()
        .await?
        .save(
            &session_id,
            provider_id,
            &display_name,
            enabled,
            &request_id("oauth-provider-save")?,
        )
        .await
        .map_err(map_oauth_provider_error)
        .map(|provider| AuthProviderSummary {
            login_url: format!("/api/auth/oauth/{}/start", provider.provider_id),
            provider_id: provider.provider_id,
            display_name: provider.display_name,
            enabled: provider.enabled,
        })
}

pub async fn replace_oauth_redirects(
    session_id: &str,
    redirects: &[String],
) -> AuthStackResult<Vec<String>> {
    let session_id = bounded_session_id(session_id)?;
    oauth_provider_service()
        .await?
        .replace_redirects(
            &session_id,
            redirects,
            &request_id("oauth-redirects-replace")?,
        )
        .await
        .map_err(map_oauth_provider_error)
}

pub async fn list_policy_versions() -> AuthStackResult<PolicyVersionListResponse> {
    let versions = policy_bundle_service()
        .await?
        .list(100)
        .await
        .map_err(map_policy_error)?
        .into_iter()
        .map(policy_version_summary)
        .collect();
    Ok(PolicyVersionListResponse { versions })
}

pub async fn list_signing_keys() -> AuthStackResult<SigningKeyListResponse> {
    let mut key_ring = configured_jwt_key_ring().await?;
    synchronize_signing_keys(&mut key_ring).await?;
    let keys = signing_key_service()
        .await?
        .list()
        .await
        .map_err(map_signing_key_error)?
        .into_iter()
        .map(signing_key_summary)
        .collect();
    Ok(SigningKeyListResponse { keys })
}

pub async fn rotate_signing_key(
    session_id: &str,
    key_id: &str,
    retire_previous: bool,
) -> AuthStackResult<SigningKeyRotateResponse> {
    let session_id = bounded_session_id(session_id)?;
    let mut key_ring = configured_jwt_key_ring().await?;
    synchronize_signing_keys(&mut key_ring).await?;
    let rotation = signing_key_service()
        .await?
        .rotate(
            &session_id,
            key_id,
            retire_previous,
            &request_id("signing-key-rotate")?,
        )
        .await
        .map_err(map_signing_key_error)?;
    let keys = signing_key_service()
        .await?
        .list()
        .await
        .map_err(map_signing_key_error)?
        .into_iter()
        .map(signing_key_summary)
        .collect();
    Ok(SigningKeyRotateResponse {
        active_kid: rotation.key.key_id,
        previous_kid: rotation.previous_key_id,
        retired_previous: retire_previous,
        keys,
    })
}

pub(crate) fn signing_key_summary(record: SigningKeyRecord) -> SigningKeySummary {
    SigningKeySummary {
        kid: record.key_id,
        alg: record.algorithm,
        active: record.status == "active",
        status: record.status,
        source: record.secret_reference,
        created_at_ms: Some(record.created_at_ms),
        activated_at_ms: record.activated_at_ms,
        retired_at_ms: record.retired_at_ms,
        revoked_at_ms: record.revoked_at_ms,
    }
}

pub async fn publish_policy_version(
    session_id: &str,
    policy_text: &str,
    schema_text: &str,
) -> AuthStackResult<PolicyVersionSummary> {
    let session_id = bounded_session_id(session_id)?;
    policy_bundle_service()
        .await?
        .publish(
            &session_id,
            schema_text,
            policy_text,
            serde_json::json!([]),
            &request_id("policy-publish")?,
        )
        .await
        .map(policy_version_summary)
        .map_err(map_policy_error)
}

pub async fn active_policy_bundle() -> AuthStackResult<Option<ActivePolicyBundle>> {
    store()
        .await?
        .load_active_policy_bundle()
        .await
        .map_err(map_policy_load_error)
}

pub(crate) fn policy_version_summary(record: PolicyBundleRecord) -> PolicyVersionSummary {
    PolicyVersionSummary {
        version_id: record.policy_revision,
        status: record.status,
        policy_hash: record.checksum_hex,
        published_by: record.created_by,
        created_at_ms: record.created_at_ms,
    }
}

