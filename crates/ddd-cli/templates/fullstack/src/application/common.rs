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

use crate::application::request_auth::RequestAuth;
use crate::contracts::*;
use crate::error::{AuthStackError, AuthStackResult};

pub(crate) const DEFAULT_PASSWORD_MIN_LENGTH: usize = 15;
pub(crate) const MAX_PASSWORD_LENGTH: usize = 128;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ApplicationClock;

impl Clock for ApplicationClock {
    fn now_unix_seconds(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs())
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ApplicationCredentialAuthenticator;

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

pub(crate) struct AuthenticatedSessionParts {
    user_id: String,
    organization_id: Option<String>,
    session_id: Option<String>,
    assurance: String,
    system_administrator: bool,
    issued_at_unix_seconds: u64,
    expires_at_unix_seconds: u64,
    permissions: Vec<String>,
    role_ids: Vec<String>,
    policy_revision: Option<String>,
}

pub(crate) async fn authenticated_session_view(auth: RequestAuth) -> AuthStackResult<SessionView> {
    if let Some(access_token) = auth.access_token {
        let verified =
            crate::application::auth::verify_access_token(TokenVerifyRequest { access_token })
                .await?;
        if let Some(session_id) = verified.session_id.as_deref() {
            let session = crate::auth_product::get_session(Some(session_id)).await?;
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
    crate::application::session::require_authenticated_route_for(auth.session_id).await
}

pub(crate) async fn enforce_organization_scope(
    context: &VerifiedAuthContext,
    organization_id: &str,
) -> AuthStackResult<()> {
    crate::auth_product::organization_for_session(context.session_id().as_str(), organization_id)
        .await
        .map(|_| ())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AssuranceRequirement {
    Aal1,
    Aal2,
}

pub(crate) struct WorkspaceAuthorization {
    pub(crate) context: VerifiedAuthContext,
    pub(crate) user_id: String,
    pub(crate) organization_id: String,
}

pub(crate) fn assurance_satisfies(
    assurance: AuthenticationAssurance,
    requirement: AssuranceRequirement,
) -> bool {
    match requirement {
        AssuranceRequirement::Aal1 => true,
        AssuranceRequirement::Aal2 => assurance.satisfies(AuthenticationAssurance::Aal2),
    }
}

/// Whether mutating APIs enforce AAL2 step-up.
///
/// Default: only when `AUTH_PRODUCTION_MODE` is enabled. Local password login is
/// AAL1, so hard-requiring AAL2 made vault/dashboard/workspace mutations fail
/// with a misleading "authentication is required". Override with
/// `AUTH_MUTATION_REQUIRE_STEP_UP=true|false`.
pub(crate) async fn mutation_step_up_required() -> bool {
    let production = matches!(
        config_value("AUTH_PRODUCTION_MODE")
            .await
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    );
    match config_value("AUTH_MUTATION_REQUIRE_STEP_UP")
        .await
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("1" | "true" | "yes" | "on") => true,
        Some("0" | "false" | "no" | "off") => false,
        _ => production,
    }
}

/// Lower AAL2 to AAL1 when mutation step-up is disabled (local development).
pub(crate) async fn effective_assurance(
    requirement: AssuranceRequirement,
) -> AssuranceRequirement {
    match requirement {
        AssuranceRequirement::Aal1 => AssuranceRequirement::Aal1,
        AssuranceRequirement::Aal2 if mutation_step_up_required().await => {
            AssuranceRequirement::Aal2
        }
        AssuranceRequirement::Aal2 => AssuranceRequirement::Aal1,
    }
}

pub(crate) fn step_up_required_error() -> AuthStackError {
    AuthStackError::validation(
        "step-up authentication (AAL2) is required. Complete MFA on /account/mfa, then try again.",
    )
}

pub(crate) async fn require_organization_permission(
    context: &VerifiedAuthContext,
    organization_id: &str,
    permission: &str,
    assurance: AssuranceRequirement,
) -> AuthStackResult<()> {
    let assurance = effective_assurance(assurance).await;
    if !assurance_satisfies(context.assurance(), assurance) {
        return Err(step_up_required_error());
    }
    let organization = crate::auth_product::organization_for_session(
        context.session_id().as_str(),
        organization_id,
    )
    .await?;
    if organization
        .permissions
        .iter()
        .any(|candidate| candidate == permission)
    {
        Ok(())
    } else {
        Err(AuthStackError::Forbidden)
    }
}

pub(crate) async fn require_workspace_permission(
    auth: RequestAuth,
    permission: &str,
    assurance: AssuranceRequirement,
) -> AuthStackResult<WorkspaceAuthorization> {
    let assurance = effective_assurance(assurance).await;
    let (context, _) =
        verified_context_and_permissions(auth, assurance == AssuranceRequirement::Aal2).await?;
    let user_id = context.principal().user_id().as_str().to_owned();
    let session =
        crate::auth_product::ensure_default_organization(context.session_id().as_str(), &user_id)
            .await?;
    let organization_id = session
        .tenant_id
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AuthStackError::validation("create a workspace first"))?;
    require_organization_permission(&context, &organization_id, permission, assurance).await?;
    Ok(WorkspaceAuthorization {
        context,
        user_id,
        organization_id,
    })
}

pub(crate) fn validate_identifier(label: &str, value: &str) -> AuthStackResult<()> {
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

pub(crate) fn validate_mfa_code(code: &str) -> AuthStackResult<()> {
    let code = code.trim();
    if !(6..=8).contains(&code.len()) || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AuthStackError::validation("TOTP code is invalid"));
    }
    Ok(())
}

pub(crate) fn validate_display_name(
    label: &str,
    value: &str,
    max_length: usize,
) -> AuthStackResult<()> {
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

pub(crate) async fn verified_context_and_permissions(
    auth: RequestAuth,
    step_up: bool,
) -> AuthStackResult<(VerifiedAuthContext, Vec<String>)> {
    // Callers pass step_up for sensitive mutations; only enforce when policy says so
    // (production by default — see mutation_step_up_required).
    let step_up = step_up && mutation_step_up_required().await;
    if let Some(verified) = auth.verified {
        if step_up
            && !verified
                .auth()
                .assurance()
                .satisfies(AuthenticationAssurance::Aal2)
        {
            return Err(step_up_required_error());
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
    let public_base_url = crate::application::ingress::public_base_url().await;
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
            // Step-up policy failures should not look like a missing session.
            if step_up {
                step_up_required_error()
            } else {
                AuthStackError::AuthRequired
            }
        })?
        .ok_or(AuthStackError::AuthRequired)?;
    let permissions = verified
        .authorization()
        .permissions()
        .map(ToOwned::to_owned)
        .collect();
    Ok((verified.auth().clone(), permissions))
}

pub(crate) async fn authenticated_session_from_cookie(
    session_id: &str,
) -> AuthStackResult<AuthenticatedSession> {
    crate::auth_product::authenticated_session_from_cookie(session_id).await
}

pub(crate) async fn authenticated_session_from_token(
    token: &str,
) -> AuthStackResult<AuthenticatedSession> {
    let verified = crate::application::auth::verify_access_token(TokenVerifyRequest {
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
        role_ids: verified.role_ids,
        policy_revision: verified.policy_revision,
    })
    .await
}

pub(crate) async fn authenticated_session(
    parts: AuthenticatedSessionParts,
) -> AuthStackResult<AuthenticatedSession> {
    let session_id_value = parts
        .session_id
        .as_deref()
        .ok_or(AuthStackError::AuthRequired)?;
    let user_id = UserId::new(parts.user_id)
        .map_err(|_| AuthStackError::configuration("authenticated user id is invalid"))?;
    let organization_id = parts
        .organization_id
        .map(OrganizationId::new)
        .transpose()
        .map_err(|_| AuthStackError::configuration("authenticated organization is invalid"))?;
    let session_id = SessionId::new(session_id_value.to_owned())
        .map_err(|_| AuthStackError::configuration("authenticated session id is invalid"))?;
    let assurance = if parts.assurance == "aal2" {
        AuthenticationAssurance::Aal2
    } else {
        AuthenticationAssurance::Aal1
    };
    let issuer = std::env::var("AUTH_JWT_ISSUER")
        .unwrap_or_else(|_| crate::application::ingress::DEFAULT_PUBLIC_BASE_URL.to_string());
    let principal = Principal::new(user_id, issuer, parts.system_administrator)
        .map_err(|_| AuthStackError::configuration("authenticated issuer is invalid"))?;
    let role_ids = parts
        .role_ids
        .into_iter()
        .map(RoleId::new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AuthStackError::configuration("authorization role is invalid"))?;
    let policy_revision = PolicyRevision::new(
        parts
            .policy_revision
            .unwrap_or_else(|| DEFAULT_APPLICATION_POLICY_REVISION.to_owned()),
    )
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

pub(crate) fn validate_email_password_login(
    request: &EmailPasswordLoginRequest,
) -> AuthStackResult<()> {
    validate_required_email(&request.email)?;
    if request.password.is_empty() {
        return Err(AuthStackError::validation("password is required"));
    }
    validate_safe_redirect_option(request.redirect_url.as_deref())?;
    Ok(())
}

pub(crate) fn validate_email_password_register(
    request: &EmailPasswordRegisterRequest,
    min_length: usize,
) -> AuthStackResult<()> {
    validate_required_email(&request.email)?;
    validate_password_policy(&request.password, min_length)?;
    validate_safe_redirect_option(request.redirect_url.as_deref())?;
    Ok(())
}

pub(crate) fn validate_password_reset_start(
    request: &PasswordResetStartRequest,
) -> AuthStackResult<()> {
    validate_required_email(&request.email)?;
    validate_safe_redirect_option(request.redirect_url.as_deref())?;
    Ok(())
}

pub(crate) fn validate_password_reset_complete(
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

pub(crate) fn validate_required_email(email: &str) -> AuthStackResult<()> {
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

pub(crate) fn validate_password_policy(password: &str, min_length: usize) -> AuthStackResult<()> {
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

pub(crate) fn validate_passkey_verify_request(
    request: &PasskeyVerifyRequest,
) -> AuthStackResult<()> {
    if request.challenge_id.trim().is_empty() {
        return Err(AuthStackError::validation("challenge_id is required"));
    }
    if request.credential_json.trim().is_empty() {
        return Err(AuthStackError::validation("credential_json is required"));
    }
    Ok(())
}

pub(crate) fn validate_provider_id(provider_id: &str) -> AuthStackResult<()> {
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

pub(crate) fn validate_signing_key_id(kid: &str) -> AuthStackResult<()> {
    let kid = kid.trim();
    if kid.is_empty() {
        return Err(AuthStackError::validation("kid is required"));
    }
    if kid.contains('/') || kid.contains('\\') || kid.chars().any(char::is_whitespace) {
        return Err(AuthStackError::validation("kid is invalid"));
    }
    Ok(())
}

pub(crate) fn validate_optional_email(email: &str) -> AuthStackResult<()> {
    let email = email.trim();
    if email.is_empty() || !email.contains('@') {
        return Err(AuthStackError::validation(
            "email must be empty or a valid email address",
        ));
    }
    Ok(())
}

pub(crate) fn validate_safe_redirect_option(value: Option<&str>) -> AuthStackResult<()> {
    if value.is_some_and(|value| !is_safe_redirect(value)) {
        return Err(AuthStackError::validation(
            "redirect_url must be a local path",
        ));
    }
    Ok(())
}

pub(crate) fn safe_redirect_or_default(redirect_url: Option<String>) -> String {
    redirect_url
        .filter(|value| is_safe_redirect(value))
        .unwrap_or_else(|| "/dashboard".to_string())
}

pub(crate) fn is_safe_redirect(value: &str) -> bool {
    value.starts_with('/') && !value.starts_with("//")
}

pub(crate) async fn password_min_length() -> usize {
    config_value("AUTH_PASSWORD_MIN_LENGTH")
        .await
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (DEFAULT_PASSWORD_MIN_LENGTH..=MAX_PASSWORD_LENGTH).contains(value))
        .unwrap_or(DEFAULT_PASSWORD_MIN_LENGTH)
}

pub(crate) async fn feature_enabled(name: &str, default: bool) -> bool {
    config_value(name)
        .await
        .map(|value| truthy(&value))
        .unwrap_or(default)
}

pub(crate) async fn config_value(name: &str) -> Option<String> {
    #[cfg(all(feature = "postgres", runtime_spin, not(test)))]
    {
        let variable_name = name.to_ascii_lowercase();
        if let Ok(value) = spin_sdk::variables::get(&variable_name).await {
            return Some(value);
        }
    }

    std::env::var(name).ok()
}

pub(crate) fn truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on" | "enabled"
    )
}

pub(crate) fn system_administrator_may(permission: &str, session: &SessionView) -> bool {
    session.system_administrator
        && session.assurance == "aal2"
        && is_system_administration_permission(permission)
}

pub(crate) fn is_system_administration_permission(permission: &str) -> bool {
    permission.starts_with("system.")
        || permission.starts_with("auth:")
        || permission.starts_with("authz:")
}
