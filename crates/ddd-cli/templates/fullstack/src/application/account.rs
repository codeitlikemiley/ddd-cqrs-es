#![allow(unused_imports)]
#![allow(dead_code)]

use std::sync::OnceLock;

use wasi_auth::authentication::Clock;
use wasi_auth::authentication::jwt::JwksDocument;
use wasi_auth::authorization::{
    AccessRequest, ActionName, Authorizer, MAX_BATCH_CHECKS, Resource, ResourceType,
};
use wasi_auth::cedar::{
    CedarError, CedarProvider, DEFAULT_APPLICATION_POLICY, DEFAULT_APPLICATION_POLICY_REVISION,
};
use wasi_auth::context::{
    AuthenticationAssurance, AuthorizationSnapshot, OrganizationId, PolicyRevision, Principal,
    RoleId, SessionId, UserId, VerifiedAuthContext, VerifiedRequestContext,
};
use wasi_auth::http::{
    AuthenticatedSession, Credential, CredentialAuthenticator, RoutePolicy, TrustedIngress,
    TrustedIngressConfig,
};

use super::*;
use crate::contracts::*;
use crate::error::{AuthStackError, AuthStackResult};

pub async fn change_password(
    request: PasswordChangeRequest,
    auth: RequestAuth,
) -> AuthStackResult<AcceptedResponse> {
    if request.current_password.is_empty() {
        return Err(AuthStackError::validation("current_password is required"));
    }
    if request.new_password.len() < password_min_length().await {
        return Err(AuthStackError::validation("new password is too short"));
    }
    if request.current_password == request.new_password {
        return Err(AuthStackError::validation("new password must be different"));
    }
    // Current password is the re-auth factor. Do not require AAL2 step-up so
    // password-only accounts can still rotate credentials (MFA step-up remains
    // available for higher-assurance sessions).
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    crate::auth_product::change_password(
        context.principal().user_id().as_str(),
        context.session_id().as_str(),
        &request.current_password,
        &request.new_password,
    )
    .await?;
    Ok(AcceptedResponse { accepted: true })
}

pub async fn list_sessions(auth: RequestAuth) -> AuthStackResult<AccountSessionListResponse> {
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    crate::auth_product::list_user_sessions(
        context.principal().user_id().as_str(),
        context.session_id().as_str(),
    )
    .await
}

pub async fn revoke_account_session(
    request: SessionRevokeRequest,
    auth: RequestAuth,
) -> AuthStackResult<AcceptedResponse> {
    validate_identifier("session_id", &request.session_id)?;
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    crate::auth_product::revoke_user_session(&request.session_id, context.session_id().as_str())
        .await?;
    Ok(AcceptedResponse { accepted: true })
}

pub async fn mfa_status(auth: RequestAuth) -> AuthStackResult<MfaStatusResponse> {
    let session = authenticated_session_view(auth).await?;
    crate::auth_product::mfa_status(
        session
            .session_id
            .as_deref()
            .ok_or(AuthStackError::AuthRequired)?,
    )
    .await
}

pub async fn start_totp_enrollment(auth: RequestAuth) -> AuthStackResult<MfaEnrollStartResponse> {
    let session = authenticated_session_view(auth).await?;
    crate::auth_product::start_totp_enrollment(
        session
            .session_id
            .as_deref()
            .ok_or(AuthStackError::AuthRequired)?,
    )
    .await
}

pub async fn confirm_totp_enrollment(
    request: MfaCodeRequest,
    auth: RequestAuth,
) -> AuthStackResult<MfaEnrollConfirmResponse> {
    validate_mfa_code(&request.code)?;
    let session = authenticated_session_view(auth).await?;
    crate::auth_product::confirm_totp_enrollment(
        session
            .session_id
            .as_deref()
            .ok_or(AuthStackError::AuthRequired)?,
        &request.code,
    )
    .await
}

pub async fn verify_totp_step_up(
    request: MfaCodeRequest,
    auth: RequestAuth,
) -> AuthStackResult<SessionView> {
    validate_mfa_code(&request.code)?;
    let session = authenticated_session_view(auth).await?;
    crate::auth_product::verify_totp_step_up(
        session
            .session_id
            .as_deref()
            .ok_or(AuthStackError::AuthRequired)?,
        &request.code,
    )
    .await
}

pub async fn use_recovery_code_for_step_up(
    request: MfaCodeRequest,
    auth: RequestAuth,
) -> AuthStackResult<SessionView> {
    if request.code.trim().len() < 16 || request.code.len() > 32 {
        return Err(AuthStackError::validation("recovery code is invalid"));
    }
    let session = authenticated_session_view(auth).await?;
    crate::auth_product::use_recovery_code(
        session
            .session_id
            .as_deref()
            .ok_or(AuthStackError::AuthRequired)?,
        &request.code,
    )
    .await
}
