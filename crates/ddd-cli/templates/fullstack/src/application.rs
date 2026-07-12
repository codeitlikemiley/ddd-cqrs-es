use std::sync::OnceLock;
use wasi_auth::authentication::jwt::JwksDocument;

use wasi_auth::authentication::Clock;
use wasi_auth::authorization::{
    AccessRequest, ActionName, Authorizer, MAX_BATCH_CHECKS, Resource, ResourceType,
};
use wasi_auth::cedar::{CedarError, CedarProvider};
use wasi_auth::context::{
    AuthenticationAssurance, AuthorizationSnapshot, OrganizationId, PolicyRevision, Principal,
    RoleId, SessionId, UserId, VerifiedAuthContext, VerifiedRequestContext,
};
use wasi_auth::http::{
    AuthenticatedSession, Credential, CredentialAuthenticator, RoutePolicy, TrustedIngress,
    TrustedIngressConfig,
};

use crate::contracts::{
    AcceptedResponse, AccountSessionListResponse, AdminUserListResponse, AdminUserStatusRequest,
    AdminUserSummary, AuditEventListResponse, AuthCapabilities, AuthProviderSummary,
    AuthorizationBatchCheckRequest, AuthorizationBatchCheckResponse,
    AuthorizationCapabilitiesResponse, AuthorizationCheckRequest, AuthorizationCheckResponse,
    CapturedMailResponse, CsrfTokenResponse, EmailPasswordLoginRequest,
    EmailPasswordRegisterRequest, EmailVerificationCompleteRequest, EmailVerificationResendRequest,
    HealthStatusResponse, InvitationAcceptRequest, InvitationCreateRequest, InvitationListResponse,
    InvitationSummary, LoginCompletionResponse, LogoutResponse, MembershipListResponse,
    MembershipRemoveRequest, MembershipRoleRequest, MembershipSummary, MfaCodeRequest,
    MfaEnrollConfirmResponse, MfaEnrollStartResponse, MfaStatusResponse, OAuthCallbackRequest,
    OAuthStartResponse, OrganizationCreateRequest, OrganizationListResponse,
    OrganizationSelectRequest, OrganizationSummary, OrganizationUpdateRequest, PasskeyStartRequest,
    PasskeyStartResponse, PasskeyVerifyRequest, PasswordChangeRequest,
    PasswordResetCompleteRequest, PasswordResetStartRequest, PasswordResetStartResponse,
    PermissionCatalogResponse, PolicyPublishRequest, PolicyVersionListResponse,
    PolicyVersionSummary, RoleListResponse, RoleSummary, RoleUpsertRequest, SessionRevokeRequest,
    SessionView, SigningKeyListResponse, SigningKeyRotateRequest, SigningKeyRotateResponse,
    StorageProjectionRunResponse, StorageStatusResponse, TokenRefreshResponse, TokenVerifyRequest,
    TokenVerifyResponse,
};
use crate::error::{AuthStackError, AuthStackResult};

const DEFAULT_PASSWORD_MIN_LENGTH: usize = 15;
const MAX_PASSWORD_LENGTH: usize = 128;

#[derive(Clone, Debug, Default)]
pub struct RequestAuth {
    pub session_id: Option<String>,
    pub access_token: Option<String>,
    pub request_id: Option<String>,
    pub verified: Option<VerifiedRequestContext>,
}

impl RequestAuth {
    pub fn from_parts(
        session_id: Option<String>,
        access_token: Option<String>,
        request_id: Option<String>,
    ) -> Self {
        Self {
            session_id,
            access_token,
            request_id,
            verified: None,
        }
    }

    pub fn from_verified(context: VerifiedRequestContext) -> Self {
        Self {
            session_id: Some(context.auth().session_id().as_str().to_owned()),
            access_token: None,
            request_id: Some(context.auth().request_id().as_str().to_owned()),
            verified: Some(context),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn for_revalidation(&self) -> Self {
        Self {
            session_id: self.session_id.clone(),
            access_token: self.access_token.clone(),
            request_id: self.request_id.clone(),
            verified: None,
        }
    }
}

pub async fn auth_capabilities() -> AuthStackResult<AuthCapabilities> {
    let password_enabled = feature_enabled("AUTH_ENABLE_PASSWORD_LOGIN", true).await;
    let oauth_enabled = feature_enabled("AUTH_ENABLE_OAUTH", false).await;
    let passkeys_enabled = feature_enabled("AUTH_ENABLE_PASSKEYS", false).await;
    let providers = if oauth_enabled {
        list_credentialed_auth_providers().await?
    } else {
        Vec::new()
    };

    Ok(AuthCapabilities {
        password_enabled,
        oauth_enabled: oauth_enabled && !providers.is_empty(),
        passkeys_enabled,
        providers,
    })
}

pub async fn list_auth_providers() -> AuthStackResult<Vec<AuthProviderSummary>> {
    if !feature_enabled("AUTH_ENABLE_OAUTH", false).await {
        return Ok(Vec::new());
    }
    list_credentialed_auth_providers().await
}

pub async fn register_email_password(
    request: EmailPasswordRegisterRequest,
) -> AuthStackResult<LoginCompletionResponse> {
    if !feature_enabled("AUTH_ENABLE_PASSWORD_LOGIN", true).await {
        return Err(AuthStackError::configuration(
            "email/password login is disabled",
        ));
    }
    validate_email_password_register(&request, password_min_length().await)?;
    crate::store::enforce_account_rate_limit("password-register", &request.email, 5, 3_600).await?;
    let redirect_url = safe_redirect_or_default(request.redirect_url.clone());
    let response = crate::store::register_email_password(&request, &redirect_url).await?;
    catch_up_storage_after_write("register_email_password").await;
    Ok(response)
}

pub async fn login_email_password(
    request: EmailPasswordLoginRequest,
) -> AuthStackResult<LoginCompletionResponse> {
    if !feature_enabled("AUTH_ENABLE_PASSWORD_LOGIN", true).await {
        return Err(AuthStackError::configuration(
            "email/password login is disabled",
        ));
    }
    validate_email_password_login(&request)?;
    crate::store::enforce_account_rate_limit("password-login", &request.email, 5, 15 * 60).await?;
    let redirect_url = safe_redirect_or_default(request.redirect_url.clone());
    let response = crate::store::login_email_password(&request, &redirect_url).await?;
    catch_up_storage_after_write("login_email_password").await;
    Ok(response)
}

pub async fn complete_email_verification(
    request: EmailVerificationCompleteRequest,
) -> AuthStackResult<LoginCompletionResponse> {
    if request.token.trim().is_empty() {
        return Err(AuthStackError::validation("verification token is required"));
    }
    let redirect_url = safe_redirect_or_default(request.redirect_url);
    let response = crate::store::complete_email_verification(&request.token, &redirect_url).await?;
    catch_up_storage_after_write("complete_email_verification").await;
    Ok(response)
}

pub async fn resend_email_verification(
    request: EmailVerificationResendRequest,
) -> AuthStackResult<AcceptedResponse> {
    validate_required_email(&request.email)?;
    crate::store::enforce_account_rate_limit("verification-resend", &request.email, 5, 3_600)
        .await?;
    let redirect_url = safe_redirect_or_default(request.redirect_url);
    crate::store::resend_email_verification(&request.email, &redirect_url).await?;
    catch_up_storage_after_write("resend_email_verification").await;
    Ok(AcceptedResponse { accepted: true })
}

pub async fn start_password_reset(
    request: PasswordResetStartRequest,
) -> AuthStackResult<PasswordResetStartResponse> {
    if !feature_enabled("AUTH_ENABLE_PASSWORD_LOGIN", true).await {
        return Err(AuthStackError::configuration(
            "email/password login is disabled",
        ));
    }
    validate_password_reset_start(&request)?;
    crate::store::enforce_account_rate_limit("password-reset-start", &request.email, 5, 3_600)
        .await?;
    let redirect_url = safe_redirect_or_default(request.redirect_url.clone());
    let response = crate::store::start_password_reset(&request, &redirect_url).await?;
    catch_up_storage_after_write("start_password_reset").await;
    Ok(response)
}

pub async fn complete_password_reset(
    request: PasswordResetCompleteRequest,
) -> AuthStackResult<LoginCompletionResponse> {
    if !feature_enabled("AUTH_ENABLE_PASSWORD_LOGIN", true).await {
        return Err(AuthStackError::configuration(
            "email/password login is disabled",
        ));
    }
    validate_password_reset_complete(&request, password_min_length().await)?;
    let redirect_url = safe_redirect_or_default(request.redirect_url.clone());
    let response = crate::store::complete_password_reset(&request, &redirect_url).await?;
    catch_up_storage_after_write("complete_password_reset").await;
    Ok(response)
}

pub async fn get_current_session_for(session_id: Option<String>) -> AuthStackResult<SessionView> {
    crate::store::get_session(session_id.as_deref()).await
}

pub async fn csrf_token_for_session(
    session_id: Option<String>,
) -> AuthStackResult<CsrfTokenResponse> {
    require_authenticated_route_for(session_id.clone()).await?;
    let session_id = session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(AuthStackError::AuthRequired)?;
    Ok(CsrfTokenResponse {
        token: crate::store::csrf_token_for_session(session_id).await?,
    })
}

pub async fn validate_csrf_token_for_session(
    session_id: Option<String>,
    csrf_token: Option<String>,
) -> AuthStackResult<()> {
    let Some(session_id) = session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(AuthStackError::AuthRequired);
    };
    require_authenticated_route_for(Some(session_id.to_string())).await?;
    let expected = crate::store::csrf_token_for_session(session_id).await?;
    let Some(candidate) = csrf_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(AuthStackError::validation("x-csrf-token is required"));
    };
    if expected != candidate {
        return Err(AuthStackError::Forbidden);
    }
    Ok(())
}

pub async fn session_cookie_secure_enabled() -> bool {
    if let Some(value) = config_value("AUTH_COOKIE_SECURE")
        .await
        .filter(|value| !value.trim().is_empty())
    {
        return truthy(&value);
    }
    feature_enabled("AUTH_PRODUCTION_MODE", false).await
}

pub fn session_cookie_header_value(
    session_id: &str,
    max_age_seconds: Option<u64>,
    secure: bool,
) -> String {
    let cookie_name = if secure {
        "__Host-session"
    } else {
        "wasi_auth_dev_session"
    };
    let mut value = format!("{cookie_name}={session_id}; Path=/; HttpOnly; SameSite=Lax");
    if let Some(max_age_seconds) = max_age_seconds {
        value.push_str(&format!("; Max-Age={max_age_seconds}"));
    }
    if secure {
        value.push_str("; Secure");
    }
    value
}

pub fn expired_session_cookie_header_value(secure: bool) -> String {
    session_cookie_header_value("", Some(0), secure)
}

pub async fn require_authenticated_route_for(
    session_id: Option<String>,
) -> AuthStackResult<SessionView> {
    let session = get_current_session_for(session_id).await?;
    if session.authenticated {
        Ok(session)
    } else {
        Err(AuthStackError::AuthRequired)
    }
}

pub async fn require_authorized_route_for(
    permission: &str,
    session_id: Option<String>,
) -> AuthStackResult<SessionView> {
    let session = require_authenticated_route_for(session_id).await?;
    if session.permissions.iter().any(|value| value == permission) {
        Ok(session)
    } else {
        Err(AuthStackError::Forbidden)
    }
}

pub async fn require_permission_for(
    permission: &str,
    auth: RequestAuth,
) -> AuthStackResult<SessionView> {
    if let Some(access_token) = auth
        .access_token
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        let verified = verify_access_token(TokenVerifyRequest {
            access_token: access_token.to_string(),
        })
        .await?;
        if verified.scopes.iter().any(|scope| scope == permission) {
            return Ok(SessionView {
                authenticated: true,
                session_id: verified.session_id.clone(),
                tenant_id: verified.tenant_id,
                user_id: Some(verified.subject),
                primary_email: None,
                expires_at: None,
                permissions: verified.scopes,
                assurance: verified.assurance,
                system_administrator: verified.system_administrator,
                issued_at_unix_seconds: Some(verified.issued_at_unix_seconds),
                expires_at_unix_seconds: Some(verified.expires_at),
            });
        }
        return Err(AuthStackError::Forbidden);
    }

    require_authorized_route_for(permission, auth.session_id).await
}

pub async fn require_step_up_permission_for(
    permission: &str,
    auth: RequestAuth,
) -> AuthStackResult<SessionView> {
    let session = require_permission_for(permission, auth).await?;
    if session.assurance == "aal2" {
        Ok(session)
    } else {
        Err(AuthStackError::Forbidden)
    }
}

pub async fn start_oauth_login(
    provider_id: String,
    redirect_url: Option<String>,
) -> AuthStackResult<OAuthStartResponse> {
    if !feature_enabled("AUTH_ENABLE_OAUTH", false).await {
        return Err(AuthStackError::configuration(
            "OAuth login is disabled; set AUTH_ENABLE_OAUTH=true and provider credentials to enable it",
        ));
    }
    validate_provider_id(&provider_id)?;
    let redirect_url = safe_redirect_or_default(redirect_url);
    ensure_oauth_provider_ready(&provider_id).await?;
    let grant = crate::store::create_oauth_grant(&provider_id, &redirect_url).await?;
    catch_up_storage_after_write("start_oauth_login").await;

    if development_oauth_callback_bypass_enabled().await {
        return Ok(OAuthStartResponse {
            provider_id: provider_id.clone(),
            authorization_url: development_oauth_callback_url(
                &provider_id,
                &grant.state,
                &redirect_url,
            ),
            state: grant.state,
        });
    }

    Ok(OAuthStartResponse {
        provider_id: provider_id.clone(),
        authorization_url: crate::oauth::authorization_url(
            &provider_id,
            &grant.state,
            &grant.nonce,
            &grant.pkce_challenge,
        )
        .await?,
        state: grant.state,
    })
}

pub async fn complete_oauth_callback(
    request: OAuthCallbackRequest,
) -> AuthStackResult<LoginCompletionResponse> {
    if !feature_enabled("AUTH_ENABLE_OAUTH", false).await {
        return Err(AuthStackError::configuration(
            "OAuth login is disabled; set AUTH_ENABLE_OAUTH=true and provider credentials to enable it",
        ));
    }
    validate_provider_id(&request.provider_id)?;
    if request
        .code
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err(AuthStackError::validation(
            "OAuth callback code is required",
        ));
    }
    if request
        .state
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err(AuthStackError::validation(
            "OAuth callback state is required",
        ));
    }
    ensure_oauth_provider_ready(&request.provider_id).await?;

    let code = request.code.as_deref().unwrap_or_default().trim();
    let state = request.state.as_deref().unwrap_or_default().trim();
    let grant = crate::store::consume_oauth_grant(&request.provider_id, state).await?;
    let response = if development_oauth_callback_bypass_enabled().await {
        if code != "development-oauth-code" {
            return Err(AuthStackError::validation(
                "OAuth development callback code is invalid",
            ));
        }
        crate::store::issue_oauth_development_session(&grant).await?
    } else {
        let identity =
            crate::oauth::complete_authorization_code(&request.provider_id, code, &grant).await?;
        crate::store::issue_oauth_session(&identity, &grant.redirect_url).await?
    };
    catch_up_storage_after_write("complete_oauth_callback").await;
    Ok(response)
}

pub async fn start_passkey_login(
    request: PasskeyStartRequest,
) -> AuthStackResult<PasskeyStartResponse> {
    if !feature_enabled("AUTH_ENABLE_PASSKEYS", false).await {
        return Err(AuthStackError::configuration(
            "passkey login is disabled; set AUTH_ENABLE_PASSKEYS=true to enable it",
        ));
    }
    if let Some(email) = request.email.as_deref() {
        validate_optional_email(email)?;
        crate::store::enforce_account_rate_limit("passkey-login-start", email, 5, 3_600).await?;
    }
    let redirect_url = safe_redirect_or_default(request.redirect_url);
    let response =
        crate::store::create_passkey_challenge("login", request.email, &redirect_url).await?;
    catch_up_storage_after_write("start_passkey_login").await;
    Ok(response)
}

pub async fn start_passkey_registration(
    request: PasskeyStartRequest,
    auth: RequestAuth,
) -> AuthStackResult<PasskeyStartResponse> {
    if !feature_enabled("AUTH_ENABLE_PASSKEYS", false).await {
        return Err(AuthStackError::configuration(
            "passkey registration is disabled; set AUTH_ENABLE_PASSKEYS=true to enable it",
        ));
    }
    let session = authenticated_session_view(auth).await?;
    let session_email = session.primary_email.ok_or(AuthStackError::AuthRequired)?;
    if request
        .email
        .as_deref()
        .is_some_and(|email| !email.trim().eq_ignore_ascii_case(session_email.trim()))
    {
        return Err(AuthStackError::Forbidden);
    }
    let redirect_url = safe_redirect_or_default(request.redirect_url);
    let response =
        crate::store::create_passkey_challenge("registration", Some(session_email), &redirect_url)
            .await?;
    catch_up_storage_after_write("start_passkey_registration").await;
    Ok(response)
}

pub async fn verify_passkey_login(
    request: PasskeyVerifyRequest,
) -> AuthStackResult<LoginCompletionResponse> {
    if !feature_enabled("AUTH_ENABLE_PASSKEYS", false).await {
        return Err(AuthStackError::configuration(
            "passkey login is disabled; set AUTH_ENABLE_PASSKEYS=true to enable it",
        ));
    }
    validate_passkey_verify_request(&request)?;
    let response = crate::store::verify_passkey_login(
        &request.challenge_id,
        &request.credential_json,
        request.redirect_url,
    )
    .await?;
    catch_up_storage_after_write("verify_passkey_login").await;
    Ok(response)
}

pub async fn verify_passkey_registration(
    request: PasskeyVerifyRequest,
    auth: RequestAuth,
) -> AuthStackResult<LoginCompletionResponse> {
    if !feature_enabled("AUTH_ENABLE_PASSKEYS", false).await {
        return Err(AuthStackError::configuration(
            "passkey registration is disabled; set AUTH_ENABLE_PASSKEYS=true to enable it",
        ));
    }
    validate_passkey_verify_request(&request)?;
    let session = authenticated_session_view(auth).await?;
    let response = crate::store::verify_passkey_registration(
        &request.challenge_id,
        &request.credential_json,
        request.redirect_url,
        session
            .user_id
            .as_deref()
            .ok_or(AuthStackError::AuthRequired)?,
    )
    .await?;
    catch_up_storage_after_write("verify_passkey_registration").await;
    Ok(response)
}

pub async fn refresh_token_for(
    session_id: Option<String>,
    refresh_token: Option<String>,
) -> AuthStackResult<TokenRefreshResponse> {
    let response =
        crate::store::refresh_session(session_id.as_deref(), refresh_token.as_deref()).await?;
    catch_up_storage_after_write("refresh_token_for").await;
    Ok(response)
}

pub async fn verify_access_token(
    request: TokenVerifyRequest,
) -> AuthStackResult<TokenVerifyResponse> {
    if request.access_token.trim().is_empty() {
        return Err(AuthStackError::validation("access_token is required"));
    }
    crate::store::verify_access_token(&request).await
}

pub async fn logout_session(session_id: Option<String>) -> AuthStackResult<LogoutResponse> {
    let response = crate::store::revoke_session(session_id.as_deref()).await?;
    catch_up_storage_after_write("logout_session").await;
    Ok(response)
}

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
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    crate::store::change_user_password(
        context.principal().user_id().as_str(),
        &request.current_password,
        &request.new_password,
        context.session_id().as_str(),
    )
    .await?;
    catch_up_storage_after_write("change_password").await;
    Ok(AcceptedResponse { accepted: true })
}

pub async fn list_sessions(auth: RequestAuth) -> AuthStackResult<AccountSessionListResponse> {
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    crate::store::list_user_sessions(
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
    crate::store::revoke_user_session(
        context.principal().user_id().as_str(),
        &request.session_id,
        context.session_id().as_str(),
    )
    .await?;
    catch_up_storage_after_write("revoke_account_session").await;
    Ok(AcceptedResponse { accepted: true })
}

pub async fn mfa_status(auth: RequestAuth) -> AuthStackResult<MfaStatusResponse> {
    let session = authenticated_session_view(auth).await?;
    let user_id = session.user_id.ok_or(AuthStackError::AuthRequired)?;
    crate::store::mfa_status(&user_id, &session.assurance).await
}

pub async fn start_totp_enrollment(auth: RequestAuth) -> AuthStackResult<MfaEnrollStartResponse> {
    let session = authenticated_session_view(auth).await?;
    let user_id = session.user_id.ok_or(AuthStackError::AuthRequired)?;
    let primary_email = session.primary_email.ok_or(AuthStackError::AuthRequired)?;
    let response = crate::store::start_totp_enrollment(&user_id, &primary_email).await?;
    catch_up_storage_after_write("start_totp_enrollment").await;
    Ok(response)
}

pub async fn confirm_totp_enrollment(
    request: MfaCodeRequest,
    auth: RequestAuth,
) -> AuthStackResult<MfaEnrollConfirmResponse> {
    validate_mfa_code(&request.code)?;
    let session = authenticated_session_view(auth).await?;
    let response = crate::store::confirm_totp_enrollment(
        session
            .user_id
            .as_deref()
            .ok_or(AuthStackError::AuthRequired)?,
        session
            .session_id
            .as_deref()
            .ok_or(AuthStackError::AuthRequired)?,
        &request.code,
    )
    .await?;
    catch_up_storage_after_write("confirm_totp_enrollment").await;
    Ok(response)
}

pub async fn verify_totp_step_up(
    request: MfaCodeRequest,
    auth: RequestAuth,
) -> AuthStackResult<SessionView> {
    validate_mfa_code(&request.code)?;
    let session = authenticated_session_view(auth).await?;
    let response = crate::store::verify_totp_step_up(
        session
            .user_id
            .as_deref()
            .ok_or(AuthStackError::AuthRequired)?,
        session
            .session_id
            .as_deref()
            .ok_or(AuthStackError::AuthRequired)?,
        &request.code,
    )
    .await?;
    catch_up_storage_after_write("verify_totp_step_up").await;
    Ok(response)
}

pub async fn use_recovery_code_for_step_up(
    request: MfaCodeRequest,
    auth: RequestAuth,
) -> AuthStackResult<SessionView> {
    if request.code.trim().len() < 16 || request.code.len() > 32 {
        return Err(AuthStackError::validation("recovery code is invalid"));
    }
    let session = authenticated_session_view(auth).await?;
    let response = crate::store::use_recovery_code_for_step_up(
        session
            .user_id
            .as_deref()
            .ok_or(AuthStackError::AuthRequired)?,
        session
            .session_id
            .as_deref()
            .ok_or(AuthStackError::AuthRequired)?,
        &request.code,
    )
    .await?;
    catch_up_storage_after_write("use_recovery_code_for_step_up").await;
    Ok(response)
}

pub async fn get_jwks() -> AuthStackResult<JwksDocument> {
    crate::store::get_jwks().await
}

pub async fn latest_captured_mail(
    recipient: String,
    message_kind: String,
) -> AuthStackResult<CapturedMailResponse> {
    validate_required_email(&recipient)?;
    if !matches!(
        message_kind.as_str(),
        "email-verification" | "password-reset" | "invitation"
    ) {
        return Err(AuthStackError::validation("message_kind is invalid"));
    }
    crate::store::latest_captured_mail(&recipient, &message_kind).await
}

pub async fn list_signing_keys(auth: RequestAuth) -> AuthStackResult<SigningKeyListResponse> {
    require_step_up_permission_for("auth:signing-key:admin", auth).await?;
    crate::store::list_signing_keys().await
}

pub async fn rotate_signing_key(
    request: SigningKeyRotateRequest,
    auth: RequestAuth,
) -> AuthStackResult<SigningKeyRotateResponse> {
    require_step_up_permission_for("auth:signing-key:admin", auth).await?;
    validate_signing_key_id(&request.kid)?;
    let response =
        crate::store::rotate_signing_key(&request.kid, request.retire_previous.unwrap_or(true))
            .await?;
    catch_up_storage_after_write("rotate_signing_key").await;
    Ok(response)
}

pub async fn storage_status(auth: RequestAuth) -> AuthStackResult<StorageStatusResponse> {
    require_step_up_permission_for("auth:storage:admin", auth).await?;
    crate::store::storage_status().await
}

pub async fn run_storage_projections(
    auth: RequestAuth,
    batch_limit: Option<usize>,
) -> AuthStackResult<Vec<StorageProjectionRunResponse>> {
    require_step_up_permission_for("auth:storage:admin", auth).await?;
    crate::store::catch_up_storage_projections(batch_limit).await
}

#[cfg(feature = "mail-capture")]
pub async fn verify_storage_atomic_rollback() -> AuthStackResult<serde_json::Value> {
    crate::store::verify_atomic_rollback_probe().await
}

pub async fn authorization_capabilities() -> AuthStackResult<AuthorizationCapabilitiesResponse> {
    let capabilities = Authorizer::new(cedar_provider()?).capabilities();
    let spicedb = crate::store::direct_spicedb_enabled().await;
    Ok(AuthorizationCapabilitiesResponse {
        provider: if spicedb {
            "embedded-cedar+direct-spicedb"
        } else {
            "embedded-cedar"
        }
        .to_string(),
        batch_check: capabilities.batch_check,
        list_resources: capabilities.list_resources,
        consistency_tokens: spicedb || capabilities.consistency_tokens,
        max_batch_checks: MAX_BATCH_CHECKS as u32,
    })
}

pub async fn check_authorization(
    request: AuthorizationCheckRequest,
    auth: RequestAuth,
) -> AuthStackResult<AuthorizationCheckResponse> {
    let (context, permissions) = verified_context_and_permissions(auth, false).await?;
    let access_request = authorization_access_request(request, context, &permissions)?;
    let cedar_decision = Authorizer::new(cedar_provider()?)
        .check(&access_request)
        .await
        .map_err(map_cedar_error)?;
    if !cedar_decision.is_allowed()
        || !crate::store::direct_spicedb_enabled().await
        || (access_request
            .context()
            .principal()
            .is_system_administrator()
            && access_request.context().assurance() == AuthenticationAssurance::Aal2)
    {
        return Ok(authorization_response(cedar_decision, None));
    }
    if let Some(organization_id) = access_request.context().organization_id() {
        let (decision, resource_revision) = crate::store::check_direct_spicedb_membership(
            access_request.context().clone(),
            organization_id.as_str(),
        )
        .await?;
        return Ok(authorization_response(decision, resource_revision));
    }
    Ok(authorization_response(cedar_decision, None))
}

pub async fn batch_check_authorization(
    request: AuthorizationBatchCheckRequest,
    auth: RequestAuth,
) -> AuthStackResult<AuthorizationBatchCheckResponse> {
    if request.checks.len() > MAX_BATCH_CHECKS {
        return Err(AuthStackError::validation(format!(
            "authorization batch exceeds the maximum of {MAX_BATCH_CHECKS}"
        )));
    }
    let (context, permissions) = verified_context_and_permissions(auth, false).await?;
    let requests = request
        .checks
        .into_iter()
        .map(|request| authorization_access_request(request, context.clone(), &permissions))
        .collect::<AuthStackResult<Vec<_>>>()?;
    let decisions = Authorizer::new(cedar_provider()?)
        .batch_check(&requests)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "embedded Cedar batch failed closed");
            AuthStackError::Forbidden
        })?;
    let spicedb = crate::store::direct_spicedb_enabled().await;
    let mut results = Vec::with_capacity(decisions.len());
    for (access_request, cedar_decision) in requests.iter().zip(decisions) {
        if cedar_decision.is_allowed()
            && spicedb
            && !(access_request
                .context()
                .principal()
                .is_system_administrator()
                && access_request.context().assurance() == AuthenticationAssurance::Aal2)
            && let Some(organization_id) = access_request.context().organization_id()
        {
            let (decision, resource_revision) = crate::store::check_direct_spicedb_membership(
                access_request.context().clone(),
                organization_id.as_str(),
            )
            .await?;
            results.push(authorization_response(decision, resource_revision));
        } else {
            results.push(authorization_response(cedar_decision, None));
        }
    }
    Ok(AuthorizationBatchCheckResponse { results })
}

pub async fn list_organizations(auth: RequestAuth) -> AuthStackResult<OrganizationListResponse> {
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    crate::store::list_organizations_for_user(context.principal().user_id().as_str()).await
}

pub async fn create_organization(
    request: OrganizationCreateRequest,
    auth: RequestAuth,
) -> AuthStackResult<OrganizationSummary> {
    validate_display_name("organization name", &request.name, 120)?;
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    let organization = crate::store::create_organization(
        request.name.trim(),
        context.principal().user_id().as_str(),
    )
    .await?;
    catch_up_storage_after_write("create_organization").await;
    Ok(organization)
}

pub async fn update_organization(
    request: OrganizationUpdateRequest,
    auth: RequestAuth,
) -> AuthStackResult<OrganizationSummary> {
    validate_identifier("organization_id", &request.organization_id)?;
    validate_display_name("organization name", &request.name, 120)?;
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    enforce_organization_scope(&context, &request.organization_id).await?;
    let organization = crate::store::update_organization(
        &request.organization_id,
        request.name.trim(),
        context.principal().user_id().as_str(),
    )
    .await?;
    catch_up_storage_after_write("update_organization").await;
    Ok(organization)
}

pub async fn select_organization(
    request: OrganizationSelectRequest,
    auth: RequestAuth,
) -> AuthStackResult<SessionView> {
    validate_identifier("organization_id", &request.organization_id)?;
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    let session = crate::store::select_organization_for_session(
        context.session_id().as_str(),
        context.principal().user_id().as_str(),
        &request.organization_id,
    )
    .await?;
    catch_up_storage_after_write("select_organization").await;
    Ok(session)
}

pub async fn list_members(
    organization_id: String,
    auth: RequestAuth,
) -> AuthStackResult<MembershipListResponse> {
    validate_identifier("organization_id", &organization_id)?;
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    enforce_organization_scope(&context, &organization_id).await?;
    crate::store::list_memberships(&organization_id, context.principal().user_id().as_str()).await
}

pub async fn invite_member(
    request: InvitationCreateRequest,
    auth: RequestAuth,
) -> AuthStackResult<InvitationSummary> {
    validate_identifier("organization_id", &request.organization_id)?;
    validate_required_email(&request.email)?;
    validate_identifier("role_id", &request.role_id)?;
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    enforce_organization_scope(&context, &request.organization_id).await?;
    let invitation = crate::store::create_invitation(
        &request.organization_id,
        &request.email,
        &request.role_id,
        context.principal().user_id().as_str(),
    )
    .await?;
    catch_up_storage_after_write("invite_member").await;
    Ok(invitation)
}

pub async fn list_invitations(
    organization_id: String,
    auth: RequestAuth,
) -> AuthStackResult<InvitationListResponse> {
    validate_identifier("organization_id", &organization_id)?;
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    enforce_organization_scope(&context, &organization_id).await?;
    crate::store::list_invitations(&organization_id, context.principal().user_id().as_str()).await
}

pub async fn accept_invitation(
    request: InvitationAcceptRequest,
    auth: RequestAuth,
) -> AuthStackResult<OrganizationSummary> {
    if request.token.trim().is_empty() {
        return Err(AuthStackError::validation("invitation token is required"));
    }
    let session = authenticated_session_view(auth.clone()).await?;
    let user_id = session.user_id.ok_or(AuthStackError::AuthRequired)?;
    let primary_email = session.primary_email.ok_or(AuthStackError::AuthRequired)?;
    let organization = crate::store::accept_invitation(
        request.token.trim(),
        &user_id,
        &primary_email,
        &session.assurance,
    )
    .await?;
    catch_up_storage_after_write("accept_invitation").await;
    Ok(organization)
}

pub async fn assign_role(
    request: MembershipRoleRequest,
    auth: RequestAuth,
) -> AuthStackResult<MembershipSummary> {
    validate_identifier("organization_id", &request.organization_id)?;
    validate_identifier("user_id", &request.user_id)?;
    validate_identifier("role_id", &request.role_id)?;
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    enforce_organization_scope(&context, &request.organization_id).await?;
    let membership = crate::store::assign_membership_role(
        &request.organization_id,
        &request.user_id,
        &request.role_id,
        context.principal().user_id().as_str(),
    )
    .await?;
    catch_up_storage_after_write("assign_role").await;
    Ok(membership)
}

pub async fn remove_member(
    request: MembershipRemoveRequest,
    auth: RequestAuth,
) -> AuthStackResult<AcceptedResponse> {
    validate_identifier("organization_id", &request.organization_id)?;
    validate_identifier("user_id", &request.user_id)?;
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    enforce_organization_scope(&context, &request.organization_id).await?;
    crate::store::remove_membership(
        &request.organization_id,
        &request.user_id,
        context.principal().user_id().as_str(),
    )
    .await?;
    catch_up_storage_after_write("remove_member").await;
    Ok(AcceptedResponse { accepted: true })
}

pub async fn list_roles(
    organization_id: String,
    auth: RequestAuth,
) -> AuthStackResult<RoleListResponse> {
    validate_identifier("organization_id", &organization_id)?;
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    enforce_organization_scope(&context, &organization_id).await?;
    crate::store::list_roles(&organization_id, context.principal().user_id().as_str()).await
}

pub async fn upsert_role(
    request: RoleUpsertRequest,
    auth: RequestAuth,
) -> AuthStackResult<RoleSummary> {
    validate_identifier("organization_id", &request.organization_id)?;
    validate_identifier("role_id", &request.role_id)?;
    validate_display_name("role name", &request.name, 80)?;
    if request.permissions.len() > 100 {
        return Err(AuthStackError::validation(
            "role permission list is too large",
        ));
    }
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    enforce_organization_scope(&context, &request.organization_id).await?;
    let role = crate::store::upsert_custom_role(
        &request.organization_id,
        &request.role_id,
        request.name.trim(),
        &request.permissions,
        context.principal().user_id().as_str(),
    )
    .await?;
    catch_up_storage_after_write("upsert_role").await;
    Ok(role)
}

pub async fn list_permissions(
    organization_id: String,
    auth: RequestAuth,
) -> AuthStackResult<PermissionCatalogResponse> {
    validate_identifier("organization_id", &organization_id)?;
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    let organization = crate::store::organization_for_user(
        &organization_id,
        context.principal().user_id().as_str(),
    )
    .await?;
    if !organization
        .permissions
        .iter()
        .any(|permission| permission == "role.view")
    {
        return Err(AuthStackError::Forbidden);
    }
    Ok(PermissionCatalogResponse {
        permissions: crate::store::organization_permission_catalog(),
    })
}

pub async fn list_admin_users(auth: RequestAuth) -> AuthStackResult<AdminUserListResponse> {
    require_step_up_permission_for("system.user.manage", auth).await?;
    crate::store::list_admin_users().await
}

pub async fn set_admin_user_status(
    request: AdminUserStatusRequest,
    auth: RequestAuth,
) -> AuthStackResult<AdminUserSummary> {
    validate_identifier("user_id", &request.user_id)?;
    let actor = require_step_up_permission_for("system.user.manage", auth).await?;
    let actor_user_id = actor.user_id.ok_or(AuthStackError::AuthRequired)?;
    let user =
        crate::store::set_user_disabled(&request.user_id, request.disabled, &actor_user_id).await?;
    catch_up_storage_after_write("set_admin_user_status").await;
    Ok(user)
}

pub async fn admin_list_providers(auth: RequestAuth) -> AuthStackResult<Vec<AuthProviderSummary>> {
    require_step_up_permission_for("system.provider.manage", auth).await?;
    crate::store::list_auth_providers().await
}

pub async fn admin_save_provider(
    provider_id: String,
    enabled: bool,
    auth: RequestAuth,
) -> AuthStackResult<AuthProviderSummary> {
    require_step_up_permission_for("system.provider.manage", auth).await?;
    save_auth_provider_config(provider_id, enabled).await
}

pub async fn list_policy_versions(auth: RequestAuth) -> AuthStackResult<PolicyVersionListResponse> {
    require_step_up_permission_for("system.policy.manage", auth).await?;
    crate::store::list_policy_versions().await
}

pub async fn publish_policy(
    request: PolicyPublishRequest,
    auth: RequestAuth,
) -> AuthStackResult<PolicyVersionSummary> {
    if request.policy_text.len() > 1024 * 1024 || request.schema_text.len() > 1024 * 1024 {
        return Err(AuthStackError::validation("policy bundle is too large"));
    }
    CedarProvider::new_validated(
        &request.policy_text,
        &request.schema_text,
        "[]",
        "candidate",
    )
    .map_err(map_cedar_error)?;
    let actor = require_step_up_permission_for("system.policy.manage", auth).await?;
    let actor_user_id = actor.user_id.ok_or(AuthStackError::AuthRequired)?;
    let version = crate::store::publish_policy_version(
        &request.policy_text,
        &request.schema_text,
        &actor_user_id,
    )
    .await?;
    catch_up_storage_after_write("publish_policy").await;
    Ok(version)
}

pub async fn get_health(auth: RequestAuth) -> AuthStackResult<HealthStatusResponse> {
    require_step_up_permission_for("system.health.read", auth).await?;
    crate::store::health_status().await
}

pub async fn list_audit_events(
    organization_id: Option<String>,
    after_cursor: u64,
    limit: usize,
    auth: RequestAuth,
) -> AuthStackResult<AuditEventListResponse> {
    let (context, permissions) = verified_context_and_permissions(auth, false).await?;
    let is_system_admin = context.principal().is_system_administrator()
        && context.assurance() == AuthenticationAssurance::Aal2;
    let organization_id = organization_id
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            context
                .organization_id()
                .map(|value| value.as_str().to_owned())
        })
        .filter(|value| value != "tenant:default");
    if !is_system_admin {
        let organization_id = organization_id
            .as_deref()
            .ok_or(AuthStackError::Forbidden)?;
        let organization = crate::store::organization_for_user(
            organization_id,
            context.principal().user_id().as_str(),
        )
        .await?;
        if !permissions
            .iter()
            .any(|permission| permission == "audit.view")
            && !organization
                .permissions
                .iter()
                .any(|permission| permission == "audit.view")
        {
            return Err(AuthStackError::Forbidden);
        }
    }
    crate::store::list_audit_events(organization_id.as_deref(), after_cursor, limit).await
}

async fn authenticated_session_view(auth: RequestAuth) -> AuthStackResult<SessionView> {
    if let Some(access_token) = auth.access_token {
        let verified = verify_access_token(TokenVerifyRequest { access_token }).await?;
        if let Some(session_id) = verified.session_id.as_deref() {
            let session = crate::store::get_session(Some(session_id)).await?;
            if session.authenticated {
                return Ok(session);
            }
        }
        return Ok(SessionView {
            authenticated: true,
            session_id: verified.session_id,
            tenant_id: verified.tenant_id,
            user_id: Some(verified.subject),
            primary_email: None,
            expires_at: None,
            permissions: verified.scopes,
            assurance: verified.assurance,
            system_administrator: verified.system_administrator,
            issued_at_unix_seconds: Some(verified.issued_at_unix_seconds),
            expires_at_unix_seconds: Some(verified.expires_at),
        });
    }
    require_authenticated_route_for(auth.session_id).await
}

async fn enforce_organization_scope(
    context: &VerifiedAuthContext,
    organization_id: &str,
) -> AuthStackResult<()> {
    if context.principal().is_system_administrator()
        && context.assurance() == AuthenticationAssurance::Aal2
    {
        return Ok(());
    }
    crate::store::organization_for_user(organization_id, context.principal().user_id().as_str())
        .await
        .map(|_| ())
}

fn validate_identifier(label: &str, value: &str) -> AuthStackResult<()> {
    if value.is_empty()
        || value.len() > 256
        || value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        Err(AuthStackError::validation(format!("{label} is invalid")))
    } else {
        Ok(())
    }
}

fn validate_mfa_code(code: &str) -> AuthStackResult<()> {
    let code = code.trim();
    if !(6..=8).contains(&code.len()) || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AuthStackError::validation("TOTP code is invalid"));
    }
    Ok(())
}

fn validate_display_name(label: &str, value: &str, max_length: usize) -> AuthStackResult<()> {
    if value.trim().is_empty()
        || value.trim() != value
        || value.len() > max_length
        || value.chars().any(char::is_control)
    {
        Err(AuthStackError::validation(format!("{label} is invalid")))
    } else {
        Ok(())
    }
}

fn authorization_access_request(
    request: AuthorizationCheckRequest,
    context: VerifiedAuthContext,
    permissions: &[String],
) -> AuthStackResult<AccessRequest> {
    let requested_action = ActionName::new(request.action.clone())
        .map_err(|_| AuthStackError::validation("action is invalid"))?;
    let requested_resource_type = ResourceType::new(request.resource_type.clone())
        .map_err(|_| AuthStackError::validation("resource_type is invalid"))?;
    let organization_id = request
        .organization_id
        .map(OrganizationId::new)
        .transpose()
        .map_err(|_| AuthStackError::validation("organization_id is invalid"))?
        .or_else(|| context.organization_id().cloned());
    let resource_id = format!(
        "{}:{}",
        requested_resource_type.as_str(),
        request.resource_id
    );
    let resource = Resource::new(
        ResourceType::new("ApplicationResource")
            .map_err(|_| AuthStackError::configuration("embedded Cedar resource is invalid"))?,
        resource_id,
        organization_id,
    )
    .map_err(|_| AuthStackError::validation("resource_id is invalid"))?;
    let mut effective_permissions = permissions.to_vec();
    if context.principal().is_system_administrator()
        && context.assurance() == AuthenticationAssurance::Aal2
    {
        effective_permissions.push(requested_action.as_str().to_owned());
    }
    let authorization = AuthorizationSnapshot::new(effective_permissions, [], None, None)
        .map_err(|_| AuthStackError::configuration("authorization snapshot is invalid"))?;
    let access_request = AccessRequest::new(
        context,
        ActionName::new("authorization.check")
            .map_err(|_| AuthStackError::configuration("embedded Cedar action is invalid"))?,
        resource,
    )
    .map_err(|_| AuthStackError::Forbidden)?
    .with_authorization_snapshot(authorization)
    .with_attribute("requested_action", requested_action.as_str())
    .and_then(|request| {
        request.with_attribute("requested_resource_type", requested_resource_type.as_str())
    })
    .map_err(|_| AuthStackError::validation("authorization context is invalid"))?;
    Ok(access_request)
}

fn authorization_response(
    decision: wasi_auth::authorization::Decision,
    resource_revision: Option<u64>,
) -> AuthorizationCheckResponse {
    AuthorizationCheckResponse {
        allowed: decision.is_allowed(),
        reason: decision.reason().to_string(),
        policy_revision: decision.policy_revision().as_str().to_string(),
        consistency_token: decision.consistency_token().map(ToOwned::to_owned),
        resource_revision,
    }
}

fn cedar_provider() -> AuthStackResult<&'static CedarProvider> {
    static PROVIDER: OnceLock<Result<CedarProvider, CedarError>> = OnceLock::new();
    match PROVIDER.get_or_init(|| {
        CedarProvider::new_validated(
            EMBEDDED_CEDAR_POLICY,
            EMBEDDED_CEDAR_SCHEMA,
            "[]",
            "embedded-v1",
        )
    }) {
        Ok(provider) => Ok(provider),
        Err(error) => Err(map_cedar_error(*error)),
    }
}

fn map_cedar_error(error: CedarError) -> AuthStackError {
    tracing::error!(error = %error, "embedded Cedar failed closed");
    AuthStackError::Forbidden
}

#[derive(Clone, Copy, Debug)]
struct ApplicationClock;

impl Clock for ApplicationClock {
    fn now_unix_seconds(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs())
    }
}

#[derive(Clone, Copy, Debug)]
struct ApplicationCredentialAuthenticator;

impl CredentialAuthenticator for ApplicationCredentialAuthenticator {
    type Error = AuthStackError;

    async fn authenticate(
        &self,
        credential: &Credential,
    ) -> Result<AuthenticatedSession, Self::Error> {
        match credential {
            Credential::Bearer(token) => authenticated_session_from_token(token).await,
            Credential::SessionCookie(session_id) => {
                authenticated_session_from_cookie(session_id).await
            }
            _ => Err(AuthStackError::InvalidToken),
        }
    }
}

pub async fn authenticate_ingress<B>(
    request: &http::Request<B>,
) -> AuthStackResult<Option<VerifiedRequestContext>> {
    let public_base_url = config_value("AUTH_PUBLIC_BASE_URL")
        .await
        .unwrap_or_else(|| "http://localhost:3008".to_string());
    TrustedIngress::new(
        TrustedIngressConfig::new(public_base_url)
            .map_err(|_| AuthStackError::configuration("trusted ingress origin is invalid"))?
            .with_development_session_cookie(),
        ApplicationCredentialAuthenticator,
        ApplicationClock,
    )
    .authenticate_request(request, RoutePolicy::Optional)
    .await
    .map_err(|error| {
        tracing::warn!(error = %error, "trusted ingress rejected request credentials");
        match error {
            wasi_auth::http::HttpBoundaryError::MissingCredentials => AuthStackError::AuthRequired,
            wasi_auth::http::HttpBoundaryError::InsufficientAssurance => AuthStackError::Forbidden,
            wasi_auth::http::HttpBoundaryError::BodyTooLarge
            | wasi_auth::http::HttpBoundaryError::InvalidContentLength
            | wasi_auth::http::HttpBoundaryError::InvalidRequestId
            | wasi_auth::http::HttpBoundaryError::InvalidCredentials
            | wasi_auth::http::HttpBoundaryError::Csrf => {
                AuthStackError::validation("request failed trusted ingress validation")
            }
            wasi_auth::http::HttpBoundaryError::Authenticator(_)
            | wasi_auth::http::HttpBoundaryError::InvalidContext(_) => AuthStackError::AuthRequired,
            _ => AuthStackError::AuthRequired,
        }
    })
}

pub async fn validate_browser_origin(headers: &http::HeaderMap) -> AuthStackResult<()> {
    let allowed = public_base_url().await;
    let origin = headers
        .get(http::header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AuthStackError::validation("browser mutation origin is required"))?;
    if origin != allowed {
        return Err(AuthStackError::validation(
            "browser mutation origin is not allowed",
        ));
    }
    Ok(())
}

pub async fn public_base_url() -> String {
    config_value("AUTH_PUBLIC_BASE_URL")
        .await
        .unwrap_or_else(|| "http://localhost:3008".to_owned())
}

async fn verified_context_and_permissions(
    auth: RequestAuth,
    step_up: bool,
) -> AuthStackResult<(VerifiedAuthContext, Vec<String>)> {
    if let Some(verified) = auth.verified {
        if step_up && verified.auth().assurance() != AuthenticationAssurance::Aal2 {
            return Err(AuthStackError::AuthRequired);
        }
        let permissions = verified
            .authorization()
            .permissions()
            .map(ToOwned::to_owned)
            .collect();
        return Ok((verified.auth().clone(), permissions));
    }

    let credential_header = if let Some(token) = auth
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        format!("Bearer {token}")
    } else if let Some(session_id) = auth
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        format!("__Host-session={session_id}")
    } else {
        return Err(AuthStackError::AuthRequired);
    };

    let request_id = auth
        .request_id
        .unwrap_or_else(|| format!("request-{}", ApplicationClock.now_unix_seconds()));
    let mut builder = http::Request::builder()
        .method(http::Method::GET)
        .uri("/internal/auth-context")
        .header("x-request-id", request_id);
    if credential_header.starts_with("Bearer ") {
        builder = builder.header(http::header::AUTHORIZATION, credential_header);
    } else {
        builder = builder.header(http::header::COOKIE, credential_header);
    }
    let request = builder
        .body(())
        .map_err(|_| AuthStackError::configuration("trusted ingress request is invalid"))?;
    let public_base_url = config_value("AUTH_PUBLIC_BASE_URL")
        .await
        .unwrap_or_else(|| "http://localhost:3008".to_string());
    let ingress = TrustedIngress::new(
        TrustedIngressConfig::new(public_base_url)
            .map_err(|_| AuthStackError::configuration("trusted ingress origin is invalid"))?
            .with_development_session_cookie(),
        ApplicationCredentialAuthenticator,
        ApplicationClock,
    );
    let verified = ingress
        .authenticate_request(
            &request,
            if step_up {
                RoutePolicy::StepUp
            } else {
                RoutePolicy::Authenticated
            },
        )
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, "trusted ingress rejected credentials");
            AuthStackError::AuthRequired
        })?
        .ok_or(AuthStackError::AuthRequired)?;
    let permissions = verified
        .authorization()
        .permissions()
        .map(ToOwned::to_owned)
        .collect();
    Ok((verified.auth().clone(), permissions))
}

async fn authenticated_session_from_cookie(
    session_id: &str,
) -> AuthStackResult<AuthenticatedSession> {
    let session = require_authenticated_route_for(Some(session_id.to_string())).await?;
    authenticated_session_from_view(session).await
}

async fn authenticated_session_from_token(token: &str) -> AuthStackResult<AuthenticatedSession> {
    let verified = verify_access_token(TokenVerifyRequest {
        access_token: token.to_string(),
    })
    .await?;
    authenticated_session(AuthenticatedSessionParts {
        user_id: verified.subject,
        organization_id: verified.tenant_id,
        session_id: verified.session_id,
        assurance: verified.assurance,
        system_administrator: verified.system_administrator,
        issued_at_unix_seconds: verified.issued_at_unix_seconds,
        expires_at_unix_seconds: verified.expires_at,
        permissions: verified.scopes,
    })
    .await
}

async fn authenticated_session_from_view(
    session: SessionView,
) -> AuthStackResult<AuthenticatedSession> {
    let permissions = session.permissions.clone();
    authenticated_session(AuthenticatedSessionParts {
        user_id: session.user_id.ok_or(AuthStackError::AuthRequired)?,
        organization_id: session.tenant_id,
        session_id: session.session_id,
        assurance: session.assurance,
        system_administrator: session.system_administrator,
        issued_at_unix_seconds: session
            .issued_at_unix_seconds
            .ok_or(AuthStackError::AuthRequired)?,
        expires_at_unix_seconds: session
            .expires_at_unix_seconds
            .ok_or(AuthStackError::AuthRequired)?,
        permissions,
    })
    .await
}

struct AuthenticatedSessionParts {
    user_id: String,
    organization_id: Option<String>,
    session_id: Option<String>,
    assurance: String,
    system_administrator: bool,
    issued_at_unix_seconds: u64,
    expires_at_unix_seconds: u64,
    permissions: Vec<String>,
}

async fn authenticated_session(
    parts: AuthenticatedSessionParts,
) -> AuthStackResult<AuthenticatedSession> {
    let (role_ids, policy_revision) = crate::store::authorization_snapshot_metadata(
        parts.organization_id.as_deref(),
        &parts.user_id,
    )
    .await?;
    let user_id = UserId::new(parts.user_id)
        .map_err(|_| AuthStackError::configuration("authenticated user id is invalid"))?;
    let organization_id = parts
        .organization_id
        .map(OrganizationId::new)
        .transpose()
        .map_err(|_| AuthStackError::configuration("authenticated organization is invalid"))?;
    let session_id = SessionId::new(
        parts
            .session_id
            .unwrap_or_else(|| format!("session-{}", parts.issued_at_unix_seconds)),
    )
    .map_err(|_| AuthStackError::configuration("authenticated session id is invalid"))?;
    let assurance = if parts.assurance == "aal2" {
        AuthenticationAssurance::Aal2
    } else {
        AuthenticationAssurance::Aal1
    };
    let issuer =
        std::env::var("AUTH_JWT_ISSUER").unwrap_or_else(|_| "http://localhost:3008".to_string());
    let principal = Principal::new(user_id, issuer, parts.system_administrator)
        .map_err(|_| AuthStackError::configuration("authenticated issuer is invalid"))?;
    let role_ids = role_ids
        .into_iter()
        .map(RoleId::new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AuthStackError::configuration("authorization role is invalid"))?;
    let policy_revision = PolicyRevision::new(policy_revision)
        .map_err(|_| AuthStackError::configuration("policy revision is invalid"))?;
    let authorization = AuthorizationSnapshot::new(
        parts.permissions,
        role_ids,
        Some(policy_revision.clone()),
        None,
    )
    .map_err(|_| AuthStackError::configuration("authorization snapshot is invalid"))?;
    Ok(AuthenticatedSession {
        principal,
        organization_id,
        session_id,
        assurance,
        issued_at_unix_seconds: parts.issued_at_unix_seconds,
        expires_at_unix_seconds: parts.expires_at_unix_seconds,
        decision_id: None,
        policy_revision: Some(policy_revision),
        authorization,
    })
}

const EMBEDDED_CEDAR_POLICY: &str = r#"
permit (
    principal is User,
    action == Action::"authorization.check",
    resource is ApplicationResource
) when {
    principal.wasi_permissions.contains(context.requested_action) &&
    (
        (principal.wasi_system_administrator && principal.wasi_assurance == "aal2") ||
        (
            !principal.wasi_system_administrator &&
            principal has wasi_organization_id &&
            resource has wasi_organization_id &&
            principal.wasi_organization_id == resource.wasi_organization_id
        )
    )
};
"#;

const EMBEDDED_CEDAR_SCHEMA: &str = r#"{
  "": {
    "entityTypes": {
      "User": {"shape": {"type": "Record", "attributes": {
        "wasi_issuer": {"type": "String", "required": true},
        "wasi_assurance": {"type": "String", "required": true},
        "wasi_system_administrator": {"type": "Boolean", "required": true},
        "wasi_permissions": {"type": "Set", "element": {"type": "String"}, "required": true},
        "wasi_organization_id": {"type": "String", "required": false}
      }}},
      "ApplicationResource": {"shape": {"type": "Record", "attributes": {
        "wasi_organization_id": {"type": "String", "required": false}
      }}}
    },
    "actions": {
      "authorization.check": {"appliesTo": {
        "principalTypes": ["User"],
        "resourceTypes": ["ApplicationResource"],
        "context": {"type": "Record", "attributes": {
          "requested_action": {"type": "String", "required": true},
          "requested_resource_type": {"type": "String", "required": true},
          "resource_owner_id": {"type": "String", "required": false},
          "resource_state": {"type": "String", "required": false},
          "wasi_organization_id": {"type": "String", "required": false}
        }}
      }}
    }
  }
}"#;

pub async fn save_auth_provider_config(
    provider_id: String,
    enabled: bool,
) -> AuthStackResult<AuthProviderSummary> {
    validate_provider_id(&provider_id)?;
    let response = crate::store::save_auth_provider_config(&provider_id, enabled).await?;
    catch_up_storage_after_write("save_auth_provider_config").await;
    Ok(response)
}

pub async fn save_redirect_allowlist(redirects_json: String) -> AuthStackResult<bool> {
    let redirects: Vec<String> = serde_json::from_str(&redirects_json)
        .map_err(|error| AuthStackError::validation(format!("invalid redirects_json: {error}")))?;
    for redirect in redirects {
        if !redirect.starts_with('/') || redirect.starts_with("//") {
            return Err(AuthStackError::validation(
                "redirect allowlist entries must be local paths",
            ));
        }
    }
    crate::store::save_redirect_allowlist(&redirects_json).await?;
    catch_up_storage_after_write("save_redirect_allowlist").await;
    Ok(true)
}

async fn list_credentialed_auth_providers() -> AuthStackResult<Vec<AuthProviderSummary>> {
    let providers = crate::store::list_auth_providers().await?;
    let mut credentialed = Vec::new();
    for mut provider in providers {
        if provider_enabled(&provider.provider_id, provider.enabled).await
            && provider_has_credentials(&provider.provider_id).await
        {
            provider.enabled = true;
            credentialed.push(provider);
        }
    }
    Ok(credentialed)
}

async fn ensure_oauth_provider_ready(provider_id: &str) -> AuthStackResult<AuthProviderSummary> {
    let Some(mut provider) = crate::store::find_auth_provider(provider_id).await? else {
        return Err(AuthStackError::not_found(format!(
            "OAuth provider '{provider_id}' is not configured"
        )));
    };
    if !provider_enabled(provider_id, provider.enabled).await {
        return Err(AuthStackError::configuration(format!(
            "OAuth provider '{provider_id}' is disabled"
        )));
    }
    if !provider_has_credentials(provider_id).await {
        return Err(AuthStackError::configuration(format!(
            "OAuth provider '{provider_id}' is missing credentials"
        )));
    }
    provider.enabled = true;
    Ok(provider)
}

fn validate_email_password_login(request: &EmailPasswordLoginRequest) -> AuthStackResult<()> {
    validate_required_email(&request.email)?;
    if request.password.is_empty() {
        return Err(AuthStackError::validation("password is required"));
    }
    validate_safe_redirect_option(request.redirect_url.as_deref())?;
    Ok(())
}

fn validate_email_password_register(
    request: &EmailPasswordRegisterRequest,
    min_length: usize,
) -> AuthStackResult<()> {
    validate_required_email(&request.email)?;
    validate_password_policy(&request.password, min_length)?;
    validate_safe_redirect_option(request.redirect_url.as_deref())?;
    Ok(())
}

fn validate_password_reset_start(request: &PasswordResetStartRequest) -> AuthStackResult<()> {
    validate_required_email(&request.email)?;
    validate_safe_redirect_option(request.redirect_url.as_deref())?;
    Ok(())
}

fn validate_password_reset_complete(
    request: &PasswordResetCompleteRequest,
    min_length: usize,
) -> AuthStackResult<()> {
    if request.token.trim().is_empty() {
        return Err(AuthStackError::validation("reset token is required"));
    }
    validate_password_policy(&request.password, min_length)?;
    validate_safe_redirect_option(request.redirect_url.as_deref())?;
    Ok(())
}

fn validate_required_email(email: &str) -> AuthStackResult<()> {
    let email = email.trim();
    if email.is_empty() {
        return Err(AuthStackError::validation("email is required"));
    }
    if !email.contains('@') || !email.contains('.') {
        return Err(AuthStackError::validation(
            "email must be a valid email address",
        ));
    }
    Ok(())
}

fn validate_password_policy(password: &str, min_length: usize) -> AuthStackResult<()> {
    let character_count = password.chars().count();
    if character_count < min_length {
        return Err(AuthStackError::validation(format!(
            "password must be at least {min_length} characters"
        )));
    }
    if character_count > MAX_PASSWORD_LENGTH {
        return Err(AuthStackError::validation(format!(
            "password must be at most {MAX_PASSWORD_LENGTH} characters"
        )));
    }
    let normalized = password.trim().to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        "password"
            | "password123"
            | "123456789"
            | "qwertyuiop"
            | "letmein"
            | "welcome123"
            | "admin123"
            | "iloveyou"
    ) {
        return Err(AuthStackError::validation(
            "password appears in the blocked common-password list",
        ));
    }
    Ok(())
}

fn validate_passkey_verify_request(request: &PasskeyVerifyRequest) -> AuthStackResult<()> {
    if request.challenge_id.trim().is_empty() {
        return Err(AuthStackError::validation("challenge_id is required"));
    }
    if request.credential_json.trim().is_empty() {
        return Err(AuthStackError::validation("credential_json is required"));
    }
    Ok(())
}

fn validate_provider_id(provider_id: &str) -> AuthStackResult<()> {
    if provider_id.trim().is_empty() {
        return Err(AuthStackError::validation("provider_id is required"));
    }
    if !provider_id
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
    {
        return Err(AuthStackError::validation(
            "provider_id must contain only lowercase letters, digits, hyphen, or underscore",
        ));
    }
    Ok(())
}

fn validate_signing_key_id(kid: &str) -> AuthStackResult<()> {
    let kid = kid.trim();
    if kid.is_empty() {
        return Err(AuthStackError::validation("kid is required"));
    }
    if kid.contains('/') || kid.contains('\\') || kid.chars().any(char::is_whitespace) {
        return Err(AuthStackError::validation("kid is invalid"));
    }
    Ok(())
}

fn validate_optional_email(email: &str) -> AuthStackResult<()> {
    let email = email.trim();
    if email.is_empty() || !email.contains('@') {
        return Err(AuthStackError::validation(
            "email must be empty or a valid email address",
        ));
    }
    Ok(())
}

fn validate_safe_redirect_option(value: Option<&str>) -> AuthStackResult<()> {
    if value.is_some_and(|value| !is_safe_redirect(value)) {
        return Err(AuthStackError::validation(
            "redirect_url must be a local path",
        ));
    }
    Ok(())
}

fn safe_redirect_or_default(redirect_url: Option<String>) -> String {
    redirect_url
        .filter(|value| is_safe_redirect(value))
        .unwrap_or_else(|| "/dashboard".to_string())
}

fn is_safe_redirect(value: &str) -> bool {
    value.starts_with('/') && !value.starts_with("//")
}

async fn password_min_length() -> usize {
    config_value("AUTH_PASSWORD_MIN_LENGTH")
        .await
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (DEFAULT_PASSWORD_MIN_LENGTH..=MAX_PASSWORD_LENGTH).contains(value))
        .unwrap_or(DEFAULT_PASSWORD_MIN_LENGTH)
}

async fn feature_enabled(name: &str, default: bool) -> bool {
    config_value(name)
        .await
        .map(|value| truthy(&value))
        .unwrap_or(default)
}

async fn provider_has_credentials(provider_id: &str) -> bool {
    match provider_id {
        "google" => {
            all_config_values_present(&["AUTH_GOOGLE_CLIENT_ID", "AUTH_GOOGLE_CLIENT_SECRET"]).await
        }
        "facebook" => {
            all_config_values_present(&["AUTH_FACEBOOK_CLIENT_ID", "AUTH_FACEBOOK_CLIENT_SECRET"])
                .await
        }
        "apple" => {
            all_config_values_present(&["AUTH_APPLE_CLIENT_ID"]).await
                && (all_config_values_present(&["AUTH_APPLE_GENERATED_CLIENT_SECRET"]).await
                    || all_config_values_present(&[
                        "AUTH_APPLE_TEAM_ID",
                        "AUTH_APPLE_KEY_ID",
                        "AUTH_APPLE_PRIVATE_KEY",
                    ])
                    .await)
        }
        other => {
            let upper = other.to_ascii_uppercase().replace(['-', '.'], "_");
            let client_id = format!("AUTH_{upper}_CLIENT_ID");
            let client_secret = format!("AUTH_{upper}_CLIENT_SECRET");
            all_config_values_present(&[client_id.as_str(), client_secret.as_str()]).await
        }
    }
}

async fn provider_enabled(provider_id: &str, stored_enabled: bool) -> bool {
    stored_enabled || feature_enabled(&provider_enabled_env_name(provider_id), false).await
}

fn provider_enabled_env_name(provider_id: &str) -> String {
    let upper = provider_id.to_ascii_uppercase().replace(['-', '.'], "_");
    format!("AUTH_{upper}_ENABLED")
}

async fn development_oauth_callback_bypass_enabled() -> bool {
    feature_enabled("AUTH_OAUTH_DEVELOPMENT_CALLBACK_BYPASS", false).await
}

async fn storage_auto_catch_up_enabled() -> bool {
    feature_enabled("AUTH_STORAGE_AUTO_CATCH_UP", true).await
}

async fn catch_up_storage_after_write(operation: &str) {
    if !storage_auto_catch_up_enabled().await {
        return;
    }
    match crate::store::catch_up_storage_projections(None).await {
        Ok(outcomes) => {
            tracing::debug!(
                operation,
                projection_count = outcomes.len(),
                "auth storage projections caught up after write"
            );
        }
        Err(error) => {
            tracing::error!(
                operation,
                error = %error,
                error_code = error.public_code(),
                "auth storage projection catch-up failed after write"
            );
        }
    }
    match crate::store::dispatch_pending_mail().await {
        Ok(delivered) if delivered > 0 => {
            tracing::debug!(operation, delivered, "mail outbox batch delivered");
        }
        Ok(_) => {}
        Err(error) => {
            tracing::error!(
                operation,
                error = %error,
                error_code = error.public_code(),
                "mail outbox dispatch failed; durable messages remain pending"
            );
        }
    }
    match crate::store::dispatch_pending_relationships().await {
        Ok(completed) if completed > 0 => {
            tracing::debug!(
                operation,
                completed,
                "SpiceDB relationship outbox batch completed"
            );
        }
        Ok(_) => {}
        Err(error) => {
            tracing::error!(
                operation,
                error = %error,
                error_code = error.public_code(),
                "relationship outbox dispatch failed; intents remain fail-closed"
            );
        }
    }
}

async fn all_config_values_present(names: &[&str]) -> bool {
    for name in names {
        if config_value(name)
            .await
            .is_none_or(|value| value.trim().is_empty())
        {
            return false;
        }
    }
    true
}

async fn config_value(name: &str) -> Option<String> {
    #[cfg(all(feature = "sqlite", runtime_spin, not(test)))]
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

fn development_oauth_callback_url(provider_id: &str, state: &str, redirect_url: &str) -> String {
    format!(
        "/api/auth/oauth/{provider_id}/callback?code=development-oauth-code&state={}&next={}",
        url_query_component(state),
        url_query_component(redirect_url)
    )
}

fn url_query_component(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_redirect_falls_back_to_dashboard() {
        assert_eq!(
            safe_redirect_or_default(Some("https://example.com".to_string())),
            "/dashboard"
        );
    }

    #[test]
    fn invalid_provider_id_is_rejected() {
        let error = validate_provider_id("../google").unwrap_err();

        assert_eq!(error.public_code(), "validation");
    }

    #[test]
    fn development_oauth_callback_url_encodes_redirect_component() {
        let url = development_oauth_callback_url("google", "state_1", "/dashboard?tab=home");

        assert_eq!(
            url,
            "/api/auth/oauth/google/callback?code=development-oauth-code&state=state_1&next=%2Fdashboard%3Ftab%3Dhome"
        );
    }

    #[test]
    fn session_cookie_header_value_adds_secure_when_enabled() {
        assert_eq!(
            session_cookie_header_value("session_1", Some(3600), true),
            "__Host-session=session_1; Path=/; HttpOnly; SameSite=Lax; Max-Age=3600; Secure"
        );
    }

    #[test]
    fn session_cookie_header_value_omits_secure_when_disabled() {
        assert_eq!(
            session_cookie_header_value("session_1", None, false),
            "wasi_auth_dev_session=session_1; Path=/; HttpOnly; SameSite=Lax"
        );
    }

    #[test]
    fn embedded_cedar_policy_passes_strict_validation() {
        assert!(cedar_provider().is_ok());
    }
}
