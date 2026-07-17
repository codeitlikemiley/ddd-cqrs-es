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

pub async fn list_admin_users(session_id: &str) -> AuthStackResult<AdminUserListResponse> {
    let session_id = bounded_session_id(session_id)?;
    let users = management_service()
        .await?
        .list_admin_users(&session_id)
        .await
        .map_err(map_management_error)?;
    Ok(AdminUserListResponse {
        users: users.into_iter().map(admin_user_summary).collect(),
    })
}

pub async fn set_user_disabled(
    session_id: &str,
    user_id: &str,
    disabled: bool,
) -> AuthStackResult<AdminUserSummary> {
    let session_id = bounded_session_id(session_id)?;
    management_service()
        .await?
        .set_user_disabled(
            &session_id,
            user_id,
            disabled,
            &request_id("set-user-disabled")?,
        )
        .await
        .map(admin_user_summary)
        .map_err(map_management_error)
}

pub async fn list_audit_events(
    session_id: &str,
    organization_id: Option<&str>,
    after_cursor: u64,
    limit: usize,
) -> AuthStackResult<AuditEventListResponse> {
    let session_id = bounded_session_id(session_id)?;
    let page = management_service()
        .await?
        .list_audit_events(&session_id, organization_id, after_cursor, limit)
        .await
        .map_err(map_management_error)?;
    Ok(AuditEventListResponse {
        events: page.events.into_iter().map(audit_event_summary).collect(),
        next_cursor: page.next_cursor,
    })
}

pub async fn list_user_sessions(
    user_id: &str,
    current_session_id: &str,
) -> AuthStackResult<AccountSessionListResponse> {
    let user_id = UserId::new(user_id.to_owned()).map_err(|_| AuthStackError::AuthRequired)?;
    let sessions = SessionService::new(store().await?, RuntimeClock, RuntimeRandom)
        .list(&user_id)
        .await
        .map_err(map_session_service_error)?;
    Ok(AccountSessionListResponse {
        sessions: sessions
            .into_iter()
            .map(|session| AccountSessionSummary {
                current: session.session_id.as_str() == current_session_id,
                session_id: session.session_id.into_string(),
                organization_id: session.organization_id,
                assurance: session.assurance,
                issued_at_ms: session.created_at_ms,
                expires_at_ms: session.expires_at_ms,
            })
            .collect(),
    })
}

pub async fn revoke_user_session(
    target_session_id: &str,
    actor_session_id: &str,
) -> AuthStackResult<()> {
    let target = SessionId::new(target_session_id.to_owned())
        .map_err(|_| AuthStackError::validation("session_id is invalid"))?;
    let actor =
        SessionId::new(actor_session_id.to_owned()).map_err(|_| AuthStackError::AuthRequired)?;
    SessionService::new(store().await?, RuntimeClock, RuntimeRandom)
        .revoke(&target, &actor, &request_id("revoke-session")?)
        .await
        .map_err(map_session_service_error)
}

pub async fn logout_session(session_id: Option<&str>) -> AuthStackResult<LogoutResponse> {
    if let Some(session_id) = session_id.map(str::trim).filter(|value| !value.is_empty()) {
        revoke_user_session(session_id, session_id).await?;
    }
    Ok(LogoutResponse {
        redirect_url: "/".to_owned(),
    })
}
