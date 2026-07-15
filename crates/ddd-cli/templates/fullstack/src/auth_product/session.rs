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

pub async fn change_password(
    user_id: &str,
    session_id: &str,
    current_password: &str,
    new_password: &str,
) -> AuthStackResult<()> {
    let user_id = UserId::new(user_id.to_owned()).map_err(|_| AuthStackError::AuthRequired)?;
    let session_id =
        SessionId::new(session_id.to_owned()).map_err(|_| AuthStackError::AuthRequired)?;
    PasswordLoginService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
        argon2_policy().await?,
    )
    .change_password(ProductPasswordChangeRequest::new(
        user_id,
        session_id,
        current_password,
        new_password,
        request_id("password-change")?,
    ))
    .await
    .map_err(map_login_error)
}

pub async fn mfa_status(session_id: &str) -> AuthStackResult<MfaStatusResponse> {
    let session_id = bounded_session_id(session_id)?;
    let status = mfa_service()
        .await?
        .status(&session_id)
        .await
        .map_err(map_mfa_error)?;
    Ok(MfaStatusResponse {
        totp_enrolled: status.totp_enrolled,
        recovery_codes_remaining: status.recovery_codes_remaining,
        assurance: status.assurance,
    })
}

pub async fn start_totp_enrollment(session_id: &str) -> AuthStackResult<MfaEnrollStartResponse> {
    let session_id = bounded_session_id(session_id)?;
    let session = get_session(Some(session_id.as_str())).await?;
    let user_id = session.user_id.ok_or(AuthStackError::AuthRequired)?;
    let enrollment = mfa_service()
        .await?
        .start(&session_id, &request_id("mfa-start")?)
        .await
        .map_err(map_mfa_error)?;
    let (provisioning_uri, secret_base32) = enrollment.into_parts();
    Ok(MfaEnrollStartResponse {
        credential_id: format!("totp:{user_id}"),
        secret_base32,
        provisioning_uri,
    })
}

pub async fn confirm_totp_enrollment(
    session_id: &str,
    code: &str,
) -> AuthStackResult<MfaEnrollConfirmResponse> {
    let session_id = bounded_session_id(session_id)?;
    let confirmation = mfa_service()
        .await?
        .confirm(&session_id, code, &request_id("mfa-confirm")?)
        .await
        .map_err(map_mfa_error)?;
    Ok(MfaEnrollConfirmResponse {
        recovery_codes: confirmation.into_recovery_codes(),
        assurance: "aal2".to_owned(),
    })
}

pub async fn verify_totp_step_up(session_id: &str, code: &str) -> AuthStackResult<SessionView> {
    let session_id = bounded_session_id(session_id)?;
    mfa_service()
        .await?
        .verify_step_up(&session_id, code, &request_id("mfa-step-up")?)
        .await
        .map_err(map_mfa_error)?;
    get_session(Some(session_id.as_str())).await
}

pub async fn use_recovery_code(session_id: &str, code: &str) -> AuthStackResult<SessionView> {
    let session_id = bounded_session_id(session_id)?;
    mfa_service()
        .await?
        .use_recovery_code(&session_id, code, &request_id("mfa-recovery")?)
        .await
        .map_err(map_mfa_error)?;
    get_session(Some(session_id.as_str())).await
}

pub async fn authenticated_session_from_cookie(
    session_id: &str,
) -> AuthStackResult<AuthenticatedSession> {
    let session = load_session(session_id).await?;
    let context = session.context();
    Ok(AuthenticatedSession {
        principal: context.auth().principal().clone(),
        organization_id: context.auth().organization_id().cloned(),
        session_id: context.auth().session_id().clone(),
        assurance: context.auth().assurance(),
        issued_at_unix_seconds: context.auth().issued_at_unix_seconds(),
        expires_at_unix_seconds: context.auth().expires_at_unix_seconds(),
        decision_id: context.auth().decision_id().cloned(),
        policy_revision: context.auth().policy_revision().cloned(),
        authorization: context.authorization().clone(),
    })
}

pub async fn get_session(session_id: Option<&str>) -> AuthStackResult<SessionView> {
    let Some(session_id) = session_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(unauthenticated_session());
    };
    let session = match load_session(session_id).await {
        Ok(session) => session,
        Err(AuthStackError::AuthRequired) => return Ok(unauthenticated_session()),
        Err(error) => return Err(error),
    };
    let context = session.context();
    let assurance = match context.auth().assurance() {
        AuthenticationAssurance::Aal1 => "aal1",
        AuthenticationAssurance::Aal2 => "aal2",
        _ => "aal1",
    };
    Ok(SessionView {
        authenticated: true,
        session_id: Some(context.auth().session_id().as_str().to_owned()),
        tenant_id: context
            .auth()
            .organization_id()
            .map(|organization| organization.as_str().to_owned()),
        user_id: Some(context.auth().principal().user_id().as_str().to_owned()),
        primary_email: Some(session.primary_email().to_owned()),
        expires_at: Some(
            context
                .auth()
                .expires_at_unix_seconds()
                .saturating_mul(1_000)
                .to_string(),
        ),
        permissions: context
            .authorization()
            .permissions()
            .map(ToOwned::to_owned)
            .collect(),
        assurance: assurance.to_owned(),
        system_administrator: context.auth().principal().is_system_administrator(),
        issued_at_unix_seconds: Some(context.auth().issued_at_unix_seconds()),
        expires_at_unix_seconds: Some(context.auth().expires_at_unix_seconds()),
    })
}
