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
use hmac::{Hmac, Mac};
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

type OAuthFlowBindingMac = Hmac<Sha256>;

/// Ties an OAuth `state` to the browser that started the flow.
///
/// `wasi-auth` owns the pending-flow record and exposes no field for a browser
/// binding, so the binding travels in a cookie whose value is a random nonce
/// plus an HMAC over `(nonce, provider, state)`. Recomputing the tag on the
/// callback proves the same browser started and finished the flow, which is
/// what a cookie hash stored on the flow record would prove.
pub async fn issue_oauth_flow_binding(provider_id: &str, state: &str) -> AuthStackResult<String> {
    let mut bytes = [0_u8; 32];
    RuntimeRandom
        .fill_bytes(&mut bytes)
        .map_err(|_| AuthStackError::store("cryptographic randomness is unavailable"))?;
    let nonce = URL_SAFE_NO_PAD.encode(bytes);
    let tag = oauth_flow_binding_mac(&nonce, provider_id, state)
        .await?
        .finalize()
        .into_bytes();
    Ok(format!("{nonce}.{}", URL_SAFE_NO_PAD.encode(tag)))
}

pub async fn verify_oauth_flow_binding(
    provider_id: &str,
    state: &str,
    presented: Option<&str>,
) -> AuthStackResult<()> {
    let presented = presented
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AuthStackError::validation(
                "this sign-in was not started in this browser; start OAuth login again",
            )
        })?;
    let (nonce, tag) = presented.split_once('.').ok_or(AuthStackError::Forbidden)?;
    let tag = URL_SAFE_NO_PAD
        .decode(tag)
        .map_err(|_| AuthStackError::Forbidden)?;
    oauth_flow_binding_mac(nonce, provider_id, state)
        .await?
        .verify_slice(&tag)
        .map_err(|_| AuthStackError::Forbidden)
}

async fn oauth_flow_binding_mac(
    nonce: &str,
    provider_id: &str,
    state: &str,
) -> AuthStackResult<OAuthFlowBindingMac> {
    let key = derived_key(OAUTH_FLOW_BINDING_INFO).await?;
    let mut mac = <OAuthFlowBindingMac as Mac>::new_from_slice(&key)
        .map_err(|_| AuthStackError::configuration("OAuth flow binding key is invalid"))?;
    mac.update(format!("{nonce}\u{0}{provider_id}\u{0}{state}").as_bytes());
    Ok(mac)
}

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
    let (access_token, refresh_token, expires_in_seconds) =
        finalize_new_session(&completion.session_id).await?;
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
    let (access_token, refresh_token, expires_in_seconds) =
        finalize_new_session(&completion.session_id).await?;
    Ok(LoginCompletionResponse {
        authenticated: true,
        redirect_url: completion.redirect_path,
        session_id: Some(completion.session_id.into_string()),
        access_token: Some(access_token),
        refresh_token: Some(refresh_token),
        expires_in_seconds,
    })
}
