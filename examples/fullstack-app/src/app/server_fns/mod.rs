//! Leptos server functions (`/api/ui/*`) — thin adapters over application services.

#![allow(unused_imports)]

use crate::app::helpers::{redirect_browser, server_error_text};
use crate::contracts::*;
use crate::error::AuthStackError;
use leptos::prelude::*;
use server_fn::ServerFnError;
use server_fn::codec::Json;

#[cfg(feature = "ssr")]
pub(crate) fn server_fn_error(error: crate::error::AuthStackError) -> ServerFnError {
    if error.is_client_error() {
        tracing::warn!(
            error = %error,
            error_code = error.public_code(),
            "auth server function rejected request"
        );
    } else {
        tracing::error!(
            error = %error,
            error_code = error.public_code(),
            "auth server function failed"
        );
    }
    error.server_fn_error()
}

#[cfg(feature = "ssr")]
pub(crate) fn current_session_id_from_cookie() -> Option<String> {
    use http::header::COOKIE;

    let parts = use_context::<http::request::Parts>()?;
    let cookie_header = parts.headers.get(COOKIE)?.to_str().ok()?;
    session_id_from_cookie_header(cookie_header)
}

#[cfg(feature = "ssr")]
pub(crate) fn server_fn_request_auth() -> crate::application::RequestAuth {
    if let Ok(context) = wasi_auth::leptos::current_verified_request_context() {
        return crate::application::RequestAuth::from_verified(context);
    }
    crate::application::RequestAuth::from_parts(current_session_id_from_cookie(), None, None)
}

#[cfg(feature = "ssr")]
pub(crate) fn session_id_from_cookie_header(cookie_header: &str) -> Option<String> {
    cookie_header.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        if matches!(name, "__Host-session" | "wasi_auth_dev_session") && !value.trim().is_empty() {
            Some(value.trim().to_string())
        } else {
            None
        }
    })
}

#[cfg(feature = "ssr")]
async fn set_session_cookie(response: &LoginCompletionResponse) {
    use http::HeaderValue;
    use http::header::SET_COOKIE;

    let Some(session_id) = response.session_id.as_deref() else {
        return;
    };
    let cookie_value = crate::application::session_cookie_header_value(
        session_id,
        Some(3600),
        crate::application::session_cookie_secure_enabled().await,
    );
    let Ok(cookie) = HeaderValue::from_str(&cookie_value) else {
        return;
    };
    if let Some(resp) = use_context::<leptos_wasi::response::ResponseOptions>() {
        resp.append_header(SET_COOKIE, cookie);
    }
}

#[cfg(any(feature = "ssr", test))]
pub(crate) fn browser_login_response(mut response: LoginCompletionResponse) -> LoginCompletionResponse {
    response.session_id = None;
    response.access_token = None;
    response.refresh_token = None;
    response
}

#[cfg(feature = "ssr")]
async fn clear_session_cookie() {
    use http::HeaderValue;
    use http::header::SET_COOKIE;

    let cookie_value = crate::application::expired_session_cookie_header_value(
        crate::application::session_cookie_secure_enabled().await,
    );
    let Ok(cookie) = HeaderValue::from_str(&cookie_value) else {
        return;
    };
    if let Some(resp) = use_context::<leptos_wasi::response::ResponseOptions>() {
        resp.append_header(SET_COOKIE, cookie);
    }
}

#[server(prefix = "/api/ui")]
pub async fn get_auth_capabilities() -> Result<AuthCapabilities, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::auth_capabilities()
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn register_email_password(
    email: String,
    password: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::register_email_password(EmailPasswordRegisterRequest {
            email,
            password,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, password, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn complete_email_verification(
    token: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response =
            crate::application::complete_email_verification(EmailVerificationCompleteRequest {
                token,
                redirect_url,
            })
            .await
            .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (token, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn resend_email_verification(
    email: String,
    redirect_url: Option<String>,
) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::resend_email_verification(EmailVerificationResendRequest {
            email,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn development_mail_capture_enabled() -> Result<bool, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        Ok(crate::auth_product::development_mail_capture_enabled().await)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn latest_development_mail(
    recipient: String,
    message_kind: String,
) -> Result<CapturedMailResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::latest_captured_mail(recipient, message_kind)
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (recipient, message_kind);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn login_email_password(
    email: String,
    password: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::login_email_password(EmailPasswordLoginRequest {
            email,
            password,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, password, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn start_password_reset(
    email: String,
    redirect_url: Option<String>,
) -> Result<PasswordResetStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_password_reset(PasswordResetStartRequest {
            email,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn complete_password_reset(
    token: String,
    password: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::complete_password_reset(PasswordResetCompleteRequest {
            token,
            password,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (token, password, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_auth_providers() -> Result<Vec<AuthProviderSummary>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_auth_providers()
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn get_current_session() -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::get_current_session_for(current_session_id_from_cookie())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn get_account_profile() -> Result<ProfileView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::get_account_profile(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn update_account_profile(
    first_name: String,
    last_name: String,
    display_name: String,
    username: String,
    is_public: bool,
    avatar_data_url: Option<String>,
) -> Result<ProfileView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::update_account_profile(
            ProfileUpdateRequest {
                first_name,
                last_name,
                display_name,
                username,
                is_public,
                avatar_data_url,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (
            first_name,
            last_name,
            display_name,
            username,
            is_public,
            avatar_data_url,
        );
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn get_public_profile(username: String) -> Result<PublicProfileView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::get_public_profile(username)
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = username;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn get_dashboard_snapshot() -> Result<DashboardSnapshot, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::get_dashboard_snapshot(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

// Nested layout (u8 col_span, enums, tree) cannot use default PostUrl/serde_qs —
// it stringifies numbers ("3") and fails with "expected u8".
#[server(prefix = "/api/ui", input = Json)]
pub async fn save_dashboard_layout(
    layout: DashboardLayout,
) -> Result<DashboardLayout, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::save_dashboard_layout(
            crate::contracts::DashboardLayoutUpdate { layout },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = layout;
        unreachable!()
    }
}

// Includes numeric fields (cache_ttl_seconds); keep JSON for the same reason.
#[server(prefix = "/api/ui", input = Json)]
pub async fn upsert_dashboard_source(
    id: Option<String>,
    name: String,
    method: String,
    url: String,
    json_path: String,
    shape: String,
    cache_ttl_seconds: u32,
    body_template: Option<String>,
) -> Result<crate::contracts::DataSourceSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::upsert_dashboard_source(
            DataSourceUpsert {
                id,
                name,
                method,
                url,
                headers: Vec::new(),
                body_template,
                json_path,
                shape,
                cache_ttl_seconds,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (id, name, method, url, json_path, shape, cache_ttl_seconds, body_template);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn delete_dashboard_source(source_id: String) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::delete_dashboard_source(source_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = source_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui", input = Json)]
pub async fn create_dashboard_secret(
    org_slug: String,
    request: SecretCreateRequest,
) -> Result<crate::contracts::SecretSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::create_dashboard_secret(
            None,
            Some(org_slug),
            request,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (org_slug, request);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn delete_dashboard_secret(
    org_slug: String,
    secret_id: String,
) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::delete_dashboard_secret(
            None,
            Some(org_slug),
            secret_id,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (org_slug, secret_id);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn reveal_dashboard_secret(
    org_slug: String,
    secret_id: String,
) -> Result<crate::contracts::SecretRevealResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::reveal_dashboard_secret(
            None,
            Some(org_slug),
            secret_id,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (org_slug, secret_id);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_dashboard_secrets(
    org_slug: String,
) -> Result<Vec<crate::contracts::SecretSummary>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_dashboard_secrets(
            None,
            Some(org_slug),
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = org_slug;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn resolve_workspace_vault_target(
) -> Result<crate::contracts::OrganizationSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::resolve_workspace_vault_target(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn seed_dashboard_demos() -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::seed_dashboard_demos(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}

#[server(prefix = "/api/ui", input = Json)]
pub async fn migrate_workspace_legacy_data(
    request: crate::contracts::WorkspaceLegacyMigrateRequest,
) -> Result<crate::contracts::WorkspaceLegacyMigrateReport, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::migrate_workspace_legacy_data(request, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = request;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn test_dashboard_http_source(
    source_id: String,
) -> Result<crate::contracts::HttpQueryResult, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::test_dashboard_http_source(source_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = source_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui", input = Json)]
pub async fn upsert_dashboard_resource(
    request: crate::contracts::ResourceUpsert,
) -> Result<crate::contracts::ResourceSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::upsert_dashboard_resource(request, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = request;
        unreachable!()
    }
}

#[server(prefix = "/api/ui", input = Json)]
pub async fn upsert_dashboard_query(
    request: crate::contracts::QueryUpsert,
) -> Result<crate::contracts::QuerySummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::upsert_dashboard_query(request, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = request;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn delete_dashboard_resource(
    resource_id: String,
) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::delete_dashboard_resource(resource_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = resource_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn delete_dashboard_query(query_id: String) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::delete_dashboard_query(query_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = query_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn run_dashboard_query(
    query_id: String,
) -> Result<crate::contracts::QueryResult, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::run_dashboard_query(query_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = query_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn dismiss_dashboard_notification(
    notification_id: String,
) -> Result<Vec<DashboardNotification>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::dismiss_dashboard_notification(
            notification_id,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = notification_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn update_dashboard_note(
    widget_id: String,
    text: String,
) -> Result<DashboardLayout, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::update_dashboard_note(
            crate::contracts::DashboardNoteUpdate { widget_id, text },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (widget_id, text);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn require_authenticated_route() -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::require_authenticated_route_for(current_session_id_from_cookie())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn require_authorized_route(permission: String) -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::require_authorized_route_for(
            &permission,
            current_session_id_from_cookie(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = permission;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn start_passkey_registration(
    email: Option<String>,
    redirect_url: Option<String>,
) -> Result<PasskeyStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_passkey_registration(
            PasskeyStartRequest {
                email,
                redirect_url,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn verify_passkey_registration(
    challenge_id: String,
    credential_json: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::verify_passkey_registration(
            PasskeyVerifyRequest {
                challenge_id,
                credential_json,
                redirect_url,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (challenge_id, credential_json, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn start_passkey_login(
    email: Option<String>,
    redirect_url: Option<String>,
) -> Result<PasskeyStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_passkey_login(PasskeyStartRequest {
            email,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn verify_passkey_login(
    challenge_id: String,
    credential_json: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::verify_passkey_login(PasskeyVerifyRequest {
            challenge_id,
            credential_json,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (challenge_id, credential_json, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn start_oauth_login(
    provider_id: String,
    redirect_url: Option<String>,
) -> Result<OAuthStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_oauth_login(provider_id, redirect_url)
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (provider_id, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn complete_oauth_callback(
    provider_id: String,
    code: Option<String>,
    state: Option<String>,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::complete_oauth_callback(OAuthCallbackRequest {
            provider_id,
            code,
            state,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (provider_id, code, state, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn logout_current_session() -> Result<LogoutResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::logout_session(current_session_id_from_cookie())
            .await
            .map_err(server_fn_error)?;
        clear_session_cookie().await;
        Ok(response)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn change_password(
    current_password: String,
    new_password: String,
) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::change_password(
            PasswordChangeRequest {
                current_password,
                new_password,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (current_password, new_password);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_account_sessions() -> Result<AccountSessionListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_sessions(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn revoke_account_session(session_id: String) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let current_session = current_session_id_from_cookie();
        let response = crate::application::revoke_account_session(
            SessionRevokeRequest {
                session_id: session_id.clone(),
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)?;
        if current_session.as_deref() == Some(session_id.as_str()) {
            clear_session_cookie().await;
        }
        Ok(response)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = session_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn get_mfa_status() -> Result<MfaStatusResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::mfa_status(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn start_totp_enrollment() -> Result<MfaEnrollStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_totp_enrollment(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn confirm_totp_enrollment(
    code: String,
) -> Result<MfaEnrollConfirmResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::confirm_totp_enrollment(
            MfaCodeRequest { code },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = code;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn verify_totp_step_up(code: String) -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::verify_totp_step_up(MfaCodeRequest { code }, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = code;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn verify_recovery_code(code: String) -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::use_recovery_code_for_step_up(
            MfaCodeRequest { code },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = code;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn save_auth_provider(
    provider_id: String,
    enabled: bool,
) -> Result<AuthProviderSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::admin_save_provider(provider_id, enabled, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (provider_id, enabled);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn save_redirect_allowlist(redirects_json: String) -> Result<bool, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::save_redirect_allowlist(redirects_json, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = redirects_json;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_signing_keys() -> Result<SigningKeyListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_signing_keys(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn rotate_signing_key(
    kid: String,
    retire_previous: bool,
) -> Result<SigningKeyRotateResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::rotate_signing_key(
            SigningKeyRotateRequest {
                kid,
                retire_previous: Some(retire_previous),
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (kid, retire_previous);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn get_authorization_capabilities()
-> Result<AuthorizationCapabilitiesResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::authorization_capabilities()
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn list_organizations() -> Result<OrganizationListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_organizations(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn create_organization(
    name: String,
    slug: String,
) -> Result<crate::contracts::OrganizationSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::create_organization(
            OrganizationCreateRequest { name, slug },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (name, slug);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn select_organization(organization_id: String) -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::select_organization(
            crate::contracts::OrganizationSelectRequest { organization_id },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = organization_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_current_organization_members() -> Result<MembershipListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::list_members(organization_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn list_current_organization_invitations() -> Result<InvitationListResponse, ServerFnError>
{
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::list_invitations(organization_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn invite_current_organization_member(
    email: String,
    role_id: String,
) -> Result<crate::contracts::InvitationSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::invite_member(
            InvitationCreateRequest {
                organization_id,
                email,
                role_id,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, role_id);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn accept_organization_invitation(
    token: String,
) -> Result<OrganizationSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::accept_invitation(
            InvitationAcceptRequest { token },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = token;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_current_organization_roles() -> Result<RoleListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::list_roles(organization_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn upsert_current_organization_role(
    role_id: String,
    name: String,
    permissions: Vec<String>,
) -> Result<crate::contracts::RoleSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::upsert_role(
            RoleUpsertRequest {
                organization_id,
                role_id,
                name,
                permissions,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (role_id, name, permissions);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_current_organization_audit() -> Result<AuditEventListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::list_audit_events(
            Some(organization_id),
            0,
            100,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn list_admin_users() -> Result<AdminUserListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_admin_users(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn get_admin_health() -> Result<HealthStatusResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::get_health(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn list_policy_versions() -> Result<PolicyVersionListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_policy_versions(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn publish_policy_version(
    policy_text: String,
    schema_text: String,
) -> Result<crate::contracts::PolicyVersionSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::publish_policy(
            PolicyPublishRequest {
                policy_text,
                schema_text,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (policy_text, schema_text);
        unreachable!()
    }
}

#[cfg(feature = "ssr")]
async fn current_organization_id() -> Result<String, ServerFnError> {
    let session =
        crate::application::require_authenticated_route_for(current_session_id_from_cookie())
            .await
            .map_err(server_fn_error)?;
    session
        .tenant_id
        .filter(|organization_id| organization_id != "tenant:default")
        .ok_or_else(|| ServerFnError::ServerError("select an organization first".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_login_response_removes_browser_visible_tokens() {
        let response = LoginCompletionResponse {
            authenticated: true,
            redirect_url: "/dashboard".to_string(),
            session_id: Some("session_123".to_string()),
            access_token: Some("access-token".to_string()),
            refresh_token: Some("refresh-token".to_string()),
            expires_in_seconds: 3600,
        };

        let redacted = browser_login_response(response);

        assert!(redacted.authenticated);
        assert_eq!(redacted.redirect_url, "/dashboard");
        assert_eq!(redacted.expires_in_seconds, 3600);
        assert_eq!(redacted.session_id, None);
        assert_eq!(redacted.access_token, None);
        assert_eq!(redacted.refresh_token, None);
    }
}
