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

pub async fn start_oauth_flow(
    provider_id: &str,
    redirect_path: &str,
) -> AuthStackResult<OAuthStartValues> {
    let (state, nonce, pkce_challenge) = oauth_flow_service()
        .await?
        .start(provider_id, redirect_path)
        .await
        .map_err(map_oauth_error)?
        .into_parts();
    Ok(OAuthStartValues {
        state,
        nonce,
        pkce_challenge,
    })
}

pub async fn load_oauth_callback(
    provider_id: &str,
    state: &str,
) -> AuthStackResult<PendingOAuthFlow> {
    oauth_flow_service()
        .await?
        .load_callback(provider_id, state)
        .await
        .map_err(map_oauth_error)
}

pub async fn complete_oauth_identity(
    pending: PendingOAuthFlow,
    identity: VerifiedOAuthIdentity,
) -> AuthStackResult<LoginCompletionResponse> {
    let completion = oauth_flow_service()
        .await?
        .complete(pending, identity, &request_id("oauth-complete")?)
        .await
        .map_err(map_oauth_error)?;
    let session_id_str = completion.session_id.as_str().to_owned();
    let (access_token, refresh_token, expires_in_seconds) =
        issue_tokens(&completion.session_id).await?;
    bind_default_organization_for_session(&session_id_str).await;
    Ok(LoginCompletionResponse {
        authenticated: true,
        redirect_url: completion.redirect_path,
        session_id: Some(completion.session_id.into_string()),
        access_token: Some(access_token),
        refresh_token: Some(refresh_token),
        expires_in_seconds,
    })
}

pub async fn start_passkey_login(
    email: &str,
    redirect_path: &str,
) -> AuthStackResult<PasskeyStartResponse> {
    let (challenge_id, public_key_options_json, redirect_url) = passkey_service()
        .await?
        .start_authentication(email, redirect_path)
        .await
        .map_err(map_passkey_error)?
        .into_parts();
    Ok(PasskeyStartResponse {
        challenge_id,
        public_key_options_json,
        redirect_url,
    })
}

pub async fn start_passkey_registration(
    session_id: &str,
    redirect_path: &str,
) -> AuthStackResult<PasskeyStartResponse> {
    let session_id = bounded_session_id(session_id)?;
    let (challenge_id, public_key_options_json, redirect_url) = passkey_service()
        .await?
        .start_registration(
            &session_id,
            &request_id("passkey-registration-start")?,
            redirect_path,
        )
        .await
        .map_err(map_passkey_error)?
        .into_parts();
    Ok(PasskeyStartResponse {
        challenge_id,
        public_key_options_json,
        redirect_url,
    })
}

pub async fn finish_passkey_login(
    challenge_id: &str,
    credential_json: &str,
) -> AuthStackResult<LoginCompletionResponse> {
    let completion = passkey_service()
        .await?
        .finish_authentication(
            challenge_id,
            credential_json,
            &request_id("passkey-login-finish")?,
        )
        .await
        .map_err(map_passkey_error)?;
    passkey_login_response(completion).await
}

pub async fn finish_passkey_registration(
    session_id: &str,
    challenge_id: &str,
    credential_json: &str,
) -> AuthStackResult<LoginCompletionResponse> {
    let session_id = bounded_session_id(session_id)?;
    let completion = passkey_service()
        .await?
        .finish_registration(
            &session_id,
            challenge_id,
            credential_json,
            &request_id("passkey-registration-finish")?,
            "Primary passkey",
        )
        .await
        .map_err(map_passkey_error)?;
    passkey_login_response(completion).await
}

pub(crate) async fn passkey_login_response(
    completion: wasi_auth::postgres::passkeys::PasskeyCompletion,
) -> AuthStackResult<LoginCompletionResponse> {
    let session_id_str = completion.session_id.as_str().to_owned();
    let (access_token, refresh_token, expires_in_seconds) =
        issue_tokens(&completion.session_id).await?;
    bind_default_organization_for_session(&session_id_str).await;
    Ok(LoginCompletionResponse {
        authenticated: true,
        redirect_url: completion.redirect_path,
        session_id: Some(completion.session_id.into_string()),
        access_token: Some(access_token),
        refresh_token: Some(refresh_token),
        expires_in_seconds,
    })
}

