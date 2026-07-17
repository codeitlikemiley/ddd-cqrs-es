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
    crate::auth_product::enforce_account_rate_limit("password-register", &request.email, 5, 3_600)
        .await?;
    let redirect_url = safe_redirect_or_default(request.redirect_url.clone());
    crate::auth_product::register_email_password(&request, &redirect_url).await
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
    crate::auth_product::enforce_account_rate_limit("password-login", &request.email, 5, 15 * 60)
        .await?;
    let redirect_url = safe_redirect_or_default(request.redirect_url.clone());
    crate::auth_product::login_email_password(&request, &redirect_url).await
}

pub async fn complete_email_verification(
    request: EmailVerificationCompleteRequest,
) -> AuthStackResult<LoginCompletionResponse> {
    if request.token.trim().is_empty() {
        return Err(AuthStackError::validation("verification token is required"));
    }
    let redirect_url = safe_redirect_or_default(request.redirect_url.clone());
    crate::auth_product::complete_email_verification(&request, &redirect_url).await
}

pub async fn resend_email_verification(
    request: EmailVerificationResendRequest,
) -> AuthStackResult<AcceptedResponse> {
    validate_required_email(&request.email)?;
    let redirect_url = safe_redirect_or_default(request.redirect_url);
    crate::auth_product::resend_email_verification(&request.email, &redirect_url).await?;
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
    crate::auth_product::enforce_account_rate_limit(
        "password-reset-start",
        &request.email,
        5,
        3_600,
    )
    .await?;
    let redirect_url = safe_redirect_or_default(request.redirect_url.clone());
    crate::auth_product::start_password_reset(&request, &redirect_url).await
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
    crate::auth_product::complete_password_reset(&request, &redirect_url).await
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
    let grant = crate::auth_product::start_oauth_flow(&provider_id, &redirect_url).await?;

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
    let grant = crate::auth_product::load_oauth_callback(&request.provider_id, state).await?;
    let identity = if development_oauth_callback_bypass_enabled().await {
        if code != "development-oauth-code" {
            return Err(AuthStackError::validation(
                "OAuth development callback code is invalid",
            ));
        }
        let subject = grant.development_subject();
        wasi_auth::postgres::oauth::VerifiedOAuthIdentity {
            provider_id: request.provider_id.clone(),
            provider_subject: subject.clone(),
            email: Some(format!("{}-{subject}@oauth.local", request.provider_id)),
            email_verified: true,
            profile: serde_json::json!({"development_bypass": true}),
        }
    } else {
        crate::oauth::complete_authorization_code(&request.provider_id, code, &grant).await?
    };
    let response = crate::auth_product::complete_oauth_identity(grant, identity).await?;
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
        crate::auth_product::enforce_account_rate_limit("passkey-login-start", email, 5, 3_600)
            .await?;
    }
    let redirect_url = safe_redirect_or_default(request.redirect_url);
    let email = request
        .email
        .as_deref()
        .ok_or_else(|| AuthStackError::validation("email is required for passkey login"))?;
    let response = crate::auth_product::start_passkey_login(email, &redirect_url).await?;
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
    let response = crate::auth_product::start_passkey_registration(
        session
            .session_id
            .as_deref()
            .ok_or(AuthStackError::AuthRequired)?,
        &redirect_url,
    )
    .await?;
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
    let response =
        crate::auth_product::finish_passkey_login(&request.challenge_id, &request.credential_json)
            .await?;
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
    let response = crate::auth_product::finish_passkey_registration(
        session
            .session_id
            .as_deref()
            .ok_or(AuthStackError::AuthRequired)?,
        &request.challenge_id,
        &request.credential_json,
    )
    .await?;
    Ok(response)
}

pub async fn refresh_token_for(
    session_id: Option<String>,
    refresh_token: Option<String>,
) -> AuthStackResult<TokenRefreshResponse> {
    let refresh_token = refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(AuthStackError::InvalidToken)?;
    crate::auth_product::refresh_tokens(session_id.as_deref(), refresh_token).await
}

pub async fn verify_access_token(
    request: TokenVerifyRequest,
) -> AuthStackResult<TokenVerifyResponse> {
    if request.access_token.trim().is_empty() {
        return Err(AuthStackError::validation("access_token is required"));
    }
    crate::auth_product::verify_access_token(&request.access_token).await
}

pub async fn logout_session(session_id: Option<String>) -> AuthStackResult<LogoutResponse> {
    crate::auth_product::logout_session(session_id.as_deref()).await
}

pub async fn get_jwks() -> AuthStackResult<JwksDocument> {
    crate::auth_product::get_jwks().await
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
    crate::auth_product::latest_captured_mail(&recipient, &message_kind).await
}

pub(crate) async fn list_credentialed_auth_providers() -> AuthStackResult<Vec<AuthProviderSummary>>
{
    let providers = crate::auth_product::list_oauth_providers().await?;
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

pub(crate) async fn ensure_oauth_provider_ready(
    provider_id: &str,
) -> AuthStackResult<AuthProviderSummary> {
    let Some(mut provider) = crate::auth_product::find_oauth_provider(provider_id).await? else {
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

pub(crate) async fn provider_has_credentials(provider_id: &str) -> bool {
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

pub(crate) async fn provider_enabled(provider_id: &str, stored_enabled: bool) -> bool {
    stored_enabled || feature_enabled(&provider_enabled_env_name(provider_id), false).await
}

pub(crate) fn provider_enabled_env_name(provider_id: &str) -> String {
    let upper = provider_id.to_ascii_uppercase().replace(['-', '.'], "_");
    format!("AUTH_{upper}_ENABLED")
}

pub(crate) async fn development_oauth_callback_bypass_enabled() -> bool {
    feature_enabled("AUTH_OAUTH_DEVELOPMENT_CALLBACK_BYPASS", false).await
}

pub(crate) async fn all_config_values_present(names: &[&str]) -> bool {
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

pub(crate) fn development_oauth_callback_url(
    provider_id: &str,
    state: &str,
    redirect_url: &str,
) -> String {
    format!(
        "/api/auth/oauth/{provider_id}/callback?code=development-oauth-code&state={}&next={}",
        url_query_component(state),
        url_query_component(redirect_url)
    )
}

pub(crate) fn url_query_component(value: &str) -> String {
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
