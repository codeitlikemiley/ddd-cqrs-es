use http::header::{COOKIE, LOCATION};
use leptos::config::get_configuration;
use leptos_wasi::wasip3::prelude::{Handler, HandlerConfig, init_wasip3_spawner};
use wasip3::http::types::{ErrorCode, Request, Response};

use crate::app::{
    App, ChangePassword, CompleteEmailVerification, CompleteOauthCallback, CompletePasswordReset,
    CreateOrganization, GetAdminHealth, GetAuthCapabilities, GetAuthorizationCapabilities,
    GetCurrentSession, InviteCurrentOrganizationMember, ListAccountSessions, ListAdminUsers,
    ListAuthProviders, ListCurrentOrganizationAudit, ListCurrentOrganizationInvitations,
    ListCurrentOrganizationMembers, ListCurrentOrganizationRoles, ListOrganizations,
    ListPolicyVersions, ListSigningKeys, LoginEmailPassword, LogoutCurrentSession,
    PublishPolicyVersion, RegisterEmailPassword, RequireAuthenticatedRoute, RequireAuthorizedRoute,
    ResendEmailVerification, RevokeAccountSession, RotateSigningKey, SaveAuthProvider,
    SaveRedirectAllowlist, SelectOrganization, StartOauthLogin, StartPasskeyLogin,
    StartPasskeyRegistration, StartPasswordReset, UpsertCurrentOrganizationRole,
    VerifyPasskeyLogin, VerifyPasskeyRegistration, shell,
};

struct FullstackServer;

impl wasip3::exports::http::handler::Guest for FullstackServer {
    async fn handle(request: Request) -> Result<Response, ErrorCode> {
        init_wasip3_spawner().map_err(internal_error)?;

        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut req = wasip3::http_compat::http_from_wasi_request(request)?;
        ensure_request_id(&mut req)?;
        let trusted_context = match crate::application::trusted_context_from_request(&req).await {
            Ok(context) => context,
            Err(error) => {
                tracing::warn!(error = %error, "trusted ingress envelope was rejected");
                return plain_text_response(error.http_status(), "Request rejected.");
            }
        };
        wasi_auth::http::strip_untrusted_auth_metadata(req.headers_mut());
        let request_path = req.uri().path().to_string();
        let request_query = req.uri().query().map(ToOwned::to_owned);
        let transport_mode = transport_mode().await;
        tracing::debug!(
            method = %req.method(),
            path = %request_path,
            transport = %transport_mode,
            "handling fullstack request"
        );

        // Schema installation and checksum verification are deployment gates
        // (`wasi-auth-migrate apply`/`verify-database`), not request work. Spin
        // may create a fresh component instance for a request, so an in-guest
        // once flag cannot make a database preflight process-global.
        if !request_path.starts_with("/pkg/")
            && let Err(error) = crate::store::validate_runtime_security_config().await
        {
            tracing::error!(
                error = %error,
                error_code = error.public_code(),
                "invalid auth runtime security configuration"
            );
            return Err(ErrorCode::InternalError(None));
        }

        let is_grpc = {
            #[cfg(all(feature = "spin-grpc", runtime_spin))]
            {
                crate::grpc::is_grpc_request(&req)
            }
            #[cfg(not(all(feature = "spin-grpc", runtime_spin)))]
            {
                false
            }
        };
        let is_browser_navigation = !is_grpc
            && !crate::rest::is_rest_request(&req)
            && matches!(*req.method(), http::Method::GET | http::Method::HEAD);
        if matches!(
            *req.method(),
            http::Method::POST | http::Method::PUT | http::Method::PATCH | http::Method::DELETE
        ) && !crate::rest::is_rest_request(&req)
            && !is_grpc
            && let Err(error) = crate::application::validate_browser_origin(req.headers()).await
        {
            return plain_text_response(error.http_status(), "Request origin rejected.");
        }

        let request_context = if trusted_context.is_some() {
            trusted_context
        } else if crate::auth_product::trusted_ingress_required().await
            && has_authentication_credential(req.headers())
        {
            tracing::warn!("credential-bearing request bypassed required native ingress");
            return plain_text_response(http::StatusCode::UNAUTHORIZED, "Request rejected.");
        } else {
            match crate::application::authenticate_ingress(&req).await {
                Ok(context) => context,
                Err(
                    crate::error::AuthStackError::AuthRequired
                    | crate::error::AuthStackError::InvalidCredentials
                    | crate::error::AuthStackError::InvalidToken
                    | crate::error::AuthStackError::SessionExpired,
                ) if (public_authentication_route(&request_path) || is_browser_navigation)
                    && !req.headers().contains_key(http::header::AUTHORIZATION) =>
                {
                    None
                }
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        error_code = error.public_code(),
                        "trusted ingress rejected request"
                    );
                    return plain_text_response(error.http_status(), "Request rejected.");
                }
            }
        };
        let session_id = request_context
            .as_ref()
            .map(|context| context.auth().session_id().as_str().to_owned())
            .or_else(|| session_id_from_headers(req.headers()));
        if let Some(context) = request_context.as_ref() {
            req.extensions_mut().insert(context.auth().clone());
            req.extensions_mut().insert(context.clone());
        }

        #[cfg(all(feature = "spin-grpc", runtime_spin))]
        if grpc_enabled(&transport_mode) && is_grpc {
            return crate::grpc::serve(req).await;
        }

        if transport_mode == "grpc" {
            return plain_text_response(
                http::StatusCode::NOT_FOUND,
                "This component is running with AUTH_TRANSPORT=grpc.",
            );
        }

        if crate::rest::is_rest_request(&req) {
            let response = crate::rest::serve(req).await.map_err(|error| {
                tracing::error!(
                    error = %error,
                    error_code = error.public_code(),
                    "failed to build auth REST response"
                );
                ErrorCode::InternalError(None)
            })?;
            return wasip3::http_compat::http_into_wasi_response(response);
        }

        if guest_only_ui_route(&request_path) && authenticated_session(session_id.clone()).await {
            return redirect_response(login_success_redirect(request_query.as_deref()));
        }

        if let Some(location) = protected_ui_redirect(&request_path, session_id).await {
            return redirect_response(&location);
        }

        let conf = get_configuration(None).map_err(|error| {
            tracing::error!(
                error = ?error,
                "failed to load Leptos configuration"
            );
            ErrorCode::InternalError(None)
        })?;
        let leptos_options = conf.leptos_options;

        let handler = Handler::build_with_config(
            req,
            HandlerConfig::default().with_max_request_body_size(256 * 1024),
        )
        .await
        .map_err(internal_error)?;
        let handler = handler
            .static_files_handler("/pkg", serve_static_files)
            .map_err(internal_error)?
            .with_server_fn::<ListAuthProviders>()
            .with_server_fn::<GetAuthCapabilities>()
            .with_server_fn::<RegisterEmailPassword>()
            .with_server_fn::<LoginEmailPassword>()
            .with_server_fn::<CompleteEmailVerification>()
            .with_server_fn::<ResendEmailVerification>()
            .with_server_fn::<StartPasswordReset>()
            .with_server_fn::<CompletePasswordReset>()
            .with_server_fn::<GetCurrentSession>()
            .with_server_fn::<RequireAuthenticatedRoute>()
            .with_server_fn::<RequireAuthorizedRoute>()
            .with_server_fn::<StartPasskeyRegistration>()
            .with_server_fn::<VerifyPasskeyRegistration>()
            .with_server_fn::<StartPasskeyLogin>()
            .with_server_fn::<VerifyPasskeyLogin>()
            .with_server_fn::<StartOauthLogin>()
            .with_server_fn::<CompleteOauthCallback>()
            .with_server_fn::<LogoutCurrentSession>()
            .with_server_fn::<SaveAuthProvider>()
            .with_server_fn::<SaveRedirectAllowlist>()
            .with_server_fn::<ListSigningKeys>()
            .with_server_fn::<RotateSigningKey>()
            .with_server_fn::<GetAuthorizationCapabilities>()
            .with_server_fn::<ChangePassword>()
            .with_server_fn::<ListAccountSessions>()
            .with_server_fn::<RevokeAccountSession>()
            .with_server_fn::<ListOrganizations>()
            .with_server_fn::<CreateOrganization>()
            .with_server_fn::<SelectOrganization>()
            .with_server_fn::<ListCurrentOrganizationMembers>()
            .with_server_fn::<ListCurrentOrganizationInvitations>()
            .with_server_fn::<InviteCurrentOrganizationMember>()
            .with_server_fn::<ListCurrentOrganizationRoles>()
            .with_server_fn::<UpsertCurrentOrganizationRole>()
            .with_server_fn::<ListCurrentOrganizationAudit>()
            .with_server_fn::<ListAdminUsers>()
            .with_server_fn::<GetAdminHealth>()
            .with_server_fn::<ListPolicyVersions>()
            .with_server_fn::<PublishPolicyVersion>()
            .generate_routes(App)
            .map_err(internal_error)?;
        let leptos_request_context = request_context.clone();
        let wasi_res = handler
            .handle_with_context(
                move || shell(leptos_options.clone()),
                move || {
                    if let Some(context) = leptos_request_context.clone() {
                        wasi_auth::leptos::provide_verified_request_context(context);
                    }
                },
            )
            .await
            .map_err(internal_error)?;

        Ok(wasi_res)
    }
}

async fn authenticated_session(session_id: Option<String>) -> bool {
    crate::application::get_current_session_for(session_id)
        .await
        .map(|session| session.authenticated)
        .unwrap_or(false)
}

async fn protected_ui_redirect(path: &str, session_id: Option<String>) -> Option<String> {
    if !protected_ui_route(path) {
        return None;
    }

    let Some(permission) = ui_route_permission(path) else {
        return (!authenticated_session(session_id).await)
            .then(|| format!("/auth/required?next={path}"));
    };

    match crate::application::require_authorized_route_for(permission, session_id).await {
        Ok(_) => None,
        Err(crate::error::AuthStackError::Forbidden) => {
            Some(format!("/auth/forbidden?next={path}"))
        }
        Err(crate::error::AuthStackError::AuthRequired)
        | Err(crate::error::AuthStackError::InvalidToken)
        | Err(crate::error::AuthStackError::SessionExpired) => {
            Some(format!("/auth/required?next={path}"))
        }
        Err(error) => {
            tracing::error!(
                error = %error,
                error_code = error.public_code(),
                path,
                permission,
                "failed to authorize protected UI route"
            );
            Some(format!("/auth/session-expired?next={path}"))
        }
    }
}

fn guest_only_ui_route(path: &str) -> bool {
    matches!(
        path,
        "/login"
            | "/register"
            | "/forgot-password"
            | "/reset-password"
            | "/verify-email"
            | "/verify-email/resend"
    )
}

fn public_authentication_route(path: &str) -> bool {
    guest_only_ui_route(path)
        || matches!(
            path,
            "/api/auth/capabilities"
                | "/api/auth/providers"
                | "/api/auth/password/register"
                | "/api/auth/password/login"
                | "/api/auth/email/verify"
                | "/api/auth/email/verify/resend"
                | "/api/auth/password/reset/start"
                | "/api/auth/password/reset/complete"
                | "/api/auth/passkeys/login/options"
                | "/api/auth/passkeys/login/verify"
        )
        || (path.starts_with("/api/auth/oauth/")
            && (path.ends_with("/start") || path.ends_with("/callback")))
}

fn protected_ui_route(path: &str) -> bool {
    path == "/dashboard"
        || path.starts_with("/account/")
        || path.starts_with("/organizations")
        || path.starts_with("/admin/")
}

fn ui_route_permission(path: &str) -> Option<&'static str> {
    match path {
        "/admin/auth/signing-keys" => Some("auth:signing-key:admin"),
        "/admin/auth/providers" => Some("auth:provider:write"),
        "/admin/auth/redirects" => Some("auth:redirect:write"),
        "/admin/authorization/policy" => Some("authz:check"),
        "/organizations/settings" => Some("organization.update"),
        "/organizations/members" | "/organizations/invitations" => Some("member.view"),
        "/organizations/roles" | "/organizations/permissions" => Some("role.view"),
        "/organizations/audit" => Some("audit.view"),
        "/admin/users" => Some("system.user.manage"),
        "/admin/health" => Some("system.health.read"),
        "/admin/policies" => Some("system.policy.manage"),
        _ => None,
    }
}

fn login_success_redirect(query: Option<&str>) -> &str {
    query
        .and_then(|query| query.split('&').find_map(|part| part.strip_prefix("next=")))
        .filter(|value| {
            value.starts_with('/') && !value.starts_with("//") && !guest_only_ui_route(value)
        })
        .unwrap_or("/dashboard")
}

fn session_id_from_headers(headers: &http::HeaderMap) -> Option<String> {
    headers
        .get(COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(session_id_from_cookie_header)
}

fn has_authentication_credential(headers: &http::HeaderMap) -> bool {
    headers.contains_key(http::header::AUTHORIZATION)
        || headers
            .get_all(http::header::COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(';'))
            .filter_map(|cookie| cookie.trim().split_once('='))
            .any(|(name, value)| {
                !value.is_empty() && matches!(name, "__Host-session" | "wasi_auth_dev_session")
            })
}

fn ensure_request_id<B>(request: &mut http::Request<B>) -> Result<(), ErrorCode> {
    let mut request_ids = request.headers().get_all("x-request-id").iter();
    if request_ids.next().is_some() {
        if request_ids.next().is_some() {
            return Err(ErrorCode::HttpRequestHeaderSize(None));
        }
        return Ok(());
    }

    let mut random = [0_u8; 16];
    getrandom::getrandom(&mut random).map_err(internal_error)?;
    let mut value = String::with_capacity(32);
    for byte in random {
        use std::fmt::Write as _;
        write!(&mut value, "{byte:02x}").map_err(internal_error)?;
    }
    let value = http::HeaderValue::from_str(&value).map_err(internal_error)?;
    request.headers_mut().insert("x-request-id", value);
    Ok(())
}

fn session_id_from_cookie_header(cookie_header: &str) -> Option<String> {
    cookie_header.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        let value = value.trim();
        if matches!(name, "__Host-session" | "wasi_auth_dev_session") && !value.is_empty() {
            Some(value.to_string())
        } else {
            None
        }
    })
}

fn serve_static_files(path: String) -> Option<leptos_wasi::response::Body> {
    use std::fs;
    let path = path.strip_prefix('/').unwrap_or(&path);
    let file_path = format!("/{path}");
    tracing::debug!(file_path, "serving fullstack-app static file");

    if let Ok(bytes) = fs::read(&file_path) {
        Some(leptos_wasi::response::Body::Sync(bytes.into()))
    } else {
        tracing::warn!(file_path, "could not read fullstack-app static file");
        None
    }
}

async fn transport_mode() -> String {
    #[cfg(all(feature = "spin-grpc", runtime_spin))]
    {
        if let Ok(value) = spin_sdk::variables::get("auth_transport").await {
            return value;
        }
    }

    std::env::var("AUTH_TRANSPORT")
        .or_else(|_| std::env::var("TRANSPORT_MODE"))
        .unwrap_or_else(|_| "both".to_string())
}

#[cfg(all(feature = "spin-grpc", runtime_spin))]
fn grpc_enabled(transport_mode: &str) -> bool {
    matches!(transport_mode, "grpc" | "both")
}

fn plain_text_response(
    status: http::StatusCode,
    message: &'static str,
) -> Result<Response, ErrorCode> {
    use http_body_util::BodyExt;

    let stream = futures::stream::once(async move {
        Ok::<_, std::io::Error>(http_body::Frame::data(bytes::Bytes::from_static(
            message.as_bytes(),
        )))
    });
    let body = http_body_util::StreamBody::new(stream).boxed_unsync();
    let response = http::Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(body)
        .map_err(|error| {
            tracing::error!(
                error = %error,
                "failed to build auth plain text response"
            );
            ErrorCode::InternalError(None)
        })?;
    wasip3::http_compat::http_into_wasi_response(response)
}

fn redirect_response(location: &str) -> Result<Response, ErrorCode> {
    let response = http::Response::builder()
        .status(http::StatusCode::SEE_OTHER)
        .header(LOCATION, location)
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .map_err(|error| {
            tracing::error!(
                error = %error,
                "failed to build auth redirect response"
            );
            ErrorCode::InternalError(None)
        })?;
    wasip3::http_compat::http_into_wasi_response(response)
}

fn internal_error(error: impl std::fmt::Display) -> ErrorCode {
    tracing::error!(error = %error, "fullstack WASI request failed");
    ErrorCode::InternalError(None)
}

wasip3::http::service::export!(FullstackServer);
