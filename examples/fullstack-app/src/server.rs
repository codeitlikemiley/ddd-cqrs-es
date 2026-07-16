use http::header::{COOKIE, LOCATION};
use leptos::config::get_configuration;
use leptos_wasi::wasip3::prelude::{Handler, HandlerConfig, init_wasip3_spawner};
use wasip3::http::types::{ErrorCode, Request, Response};

use crate::app::{
    AcceptOrganizationInvitation, App, AssignWorkspaceMemberRole, ChangePassword,
    CompleteEmailVerification, CompleteOauthCallback, CompletePasswordReset, ConfirmTotpEnrollment,
    CreateDashboardSecret, CreateOrganization, DeactivateWorkspace, DeleteDashboardQuery,
    DeleteDashboardResource, DeleteDashboardSecret, DeleteDashboardSource, DeleteWorkspaceRole,
    DevelopmentMailCaptureEnabled, DismissDashboardNotification, GetAccountProfile, GetAdminHealth,
    GetAuthCapabilities, GetAuthorizationCapabilities, GetCurrentSession, GetDashboardSnapshot,
    GetMfaStatus, GetPublicProfile, GetWorkspaceSettingsContext, InviteCurrentOrganizationMember,
    InviteWorkspaceMember, LatestDevelopmentMail, LeaveWorkspace, ListAccountSessions,
    ListAdminUsers, ListAuthProviders, ListCurrentOrganizationAudit,
    ListCurrentOrganizationInvitations, ListCurrentOrganizationMembers,
    ListCurrentOrganizationRoles, ListDashboardSecrets, ListOrganizations, ListPolicyVersions,
    ListSigningKeys, ListWorkspaceAudit, ListWorkspaceInvitations, ListWorkspaceMembers,
    ListWorkspacePermissions, ListWorkspaceRoles, LoginEmailPassword, LogoutCurrentSession,
    MigrateWorkspaceLegacyData, PublishPolicyVersion, RegisterEmailPassword, RemoveWorkspaceMember,
    RequireAuthenticatedRoute, RequireAuthorizedRoute, ResendEmailVerification,
    ResendWorkspaceInvitation, ResolveWorkspaceVaultTarget, RevealDashboardSecret,
    RevokeAccountSession, RevokeWorkspaceInvitation, RotateSigningKey, RunDashboardQuery,
    SaveAuthProvider, SaveDashboardLayout, SaveRedirectAllowlist, SeedDashboardDemos,
    SelectOrganization, StartOauthLogin, StartPasskeyLogin, StartPasskeyRegistration,
    StartPasswordReset, StartTotpEnrollment, TestDashboardHttpSource, TransferWorkspaceOwnership,
    UpdateAccountProfile, UpdateDashboardNote, UpdateWorkspaceName, UpsertCurrentOrganizationRole,
    UpsertDashboardQuery, UpsertDashboardResource, UpsertDashboardSource, UpsertWorkspaceRole,
    VerifyPasskeyLogin, VerifyPasskeyRegistration, VerifyRecoveryCode, VerifyTotpStepUp, shell,
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
        // WebAuthn rejects IP hosts as rpId. Canonicalize loopback IPs → localhost
        // for document navigations so passkeys and session cookies share one host.
        if is_browser_navigation
            && !request_path.starts_with("/pkg/")
            && let Some(location) =
                loopback_ip_to_localhost_redirect(req.headers(), &request_path, request_query.as_deref())
        {
            return redirect_response(&location);
        }
        if matches!(
            *req.method(),
            http::Method::POST | http::Method::PUT | http::Method::PATCH | http::Method::DELETE
        ) && !crate::rest::is_rest_request(&req)
            && !is_grpc
            && let Err(error) = crate::application::validate_browser_origin(req.headers()).await
        {
            // Server functions expect `Type|message` error bodies. Plain text
            // yields "error deserializing server function results: missing delimiter".
            if request_path.starts_with("/api/ui/") {
                return server_fn_error_response(
                    error.http_status(),
                    "Request origin rejected. Open the app with the same host as AUTH_PUBLIC_BASE_URL (localhost and 127.0.0.1 are interchangeable on loopback).",
                );
            }
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

        // Logout is an action, not a document. Keep direct navigations safe and
        // prevent the browser from ever rendering a logout page.
        if request_path == "/logout"
            && matches!(*req.method(), http::Method::GET | http::Method::HEAD)
        {
            return redirect_response("/");
        }

        // Guest-only pages bounce authenticated browsers away, but one-time
        // token links must still render. Dropping `?token=` here is what made
        // password reset appear to "already be signed in" without a form.
        if guest_only_ui_route(&request_path)
            && !tokenized_public_ui_route(&request_path, request_query.as_deref())
            && authenticated_session(session_id.clone()).await
        {
            return redirect_response(login_success_redirect(request_query.as_deref()));
        }

        if let Some(location) =
            protected_ui_redirect(&request_path, request_query.as_deref(), session_id).await
        {
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
            .with_server_fn::<GetAccountProfile>()
            .with_server_fn::<UpdateAccountProfile>()
            .with_server_fn::<GetPublicProfile>()
            .with_server_fn::<GetDashboardSnapshot>()
            .with_server_fn::<SaveDashboardLayout>()
            .with_server_fn::<DismissDashboardNotification>()
            .with_server_fn::<UpdateDashboardNote>()
            .with_server_fn::<UpsertDashboardSource>()
            .with_server_fn::<DeleteDashboardSource>()
            .with_server_fn::<CreateDashboardSecret>()
            .with_server_fn::<DeleteDashboardSecret>()
            .with_server_fn::<RevealDashboardSecret>()
            .with_server_fn::<ListDashboardSecrets>()
            .with_server_fn::<ResolveWorkspaceVaultTarget>()
            .with_server_fn::<SeedDashboardDemos>()
            .with_server_fn::<MigrateWorkspaceLegacyData>()
            .with_server_fn::<TestDashboardHttpSource>()
            .with_server_fn::<UpsertDashboardResource>()
            .with_server_fn::<UpsertDashboardQuery>()
            .with_server_fn::<DeleteDashboardResource>()
            .with_server_fn::<DeleteDashboardQuery>()
            .with_server_fn::<RunDashboardQuery>()
            .with_server_fn::<DevelopmentMailCaptureEnabled>()
            .with_server_fn::<LatestDevelopmentMail>()
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
            .with_server_fn::<GetMfaStatus>()
            .with_server_fn::<StartTotpEnrollment>()
            .with_server_fn::<ConfirmTotpEnrollment>()
            .with_server_fn::<VerifyTotpStepUp>()
            .with_server_fn::<VerifyRecoveryCode>()
            .with_server_fn::<ListOrganizations>()
            .with_server_fn::<CreateOrganization>()
            .with_server_fn::<SelectOrganization>()
            .with_server_fn::<ListCurrentOrganizationMembers>()
            .with_server_fn::<ListCurrentOrganizationInvitations>()
            .with_server_fn::<InviteCurrentOrganizationMember>()
            .with_server_fn::<AcceptOrganizationInvitation>()
            .with_server_fn::<ListCurrentOrganizationRoles>()
            .with_server_fn::<UpsertCurrentOrganizationRole>()
            .with_server_fn::<ListCurrentOrganizationAudit>()
            .with_server_fn::<GetWorkspaceSettingsContext>()
            .with_server_fn::<ListWorkspaceMembers>()
            .with_server_fn::<ListWorkspaceInvitations>()
            .with_server_fn::<ListWorkspaceRoles>()
            .with_server_fn::<ListWorkspaceAudit>()
            .with_server_fn::<UpdateWorkspaceName>()
            .with_server_fn::<AssignWorkspaceMemberRole>()
            .with_server_fn::<RemoveWorkspaceMember>()
            .with_server_fn::<InviteWorkspaceMember>()
            .with_server_fn::<RevokeWorkspaceInvitation>()
            .with_server_fn::<ResendWorkspaceInvitation>()
            .with_server_fn::<UpsertWorkspaceRole>()
            .with_server_fn::<DeleteWorkspaceRole>()
            .with_server_fn::<TransferWorkspaceOwnership>()
            .with_server_fn::<LeaveWorkspace>()
            .with_server_fn::<DeactivateWorkspace>()
            .with_server_fn::<ListWorkspacePermissions>()
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

async fn protected_ui_redirect(
    path: &str,
    query: Option<&str>,
    session_id: Option<String>,
) -> Option<String> {
    if !protected_ui_route(path) {
        return None;
    }

    let next = encode_next_target(path, query);

    let session = match crate::application::get_current_session_for(session_id.clone()).await {
        Ok(session) if session.authenticated => session,
        _ => return Some(format!("/auth/required?next={next}")),
    };

    // Organization setup is a product gate, not an optional page. Until the
    // first workspace exists, protected workspace/account/admin routes all
    // lead back to the focused onboarding screen. Invitation acceptance stays
    // reachable because accepting one can establish the first membership.
    if path != "/invitations/accept" {
        let Some(user_id) = session.user_id.as_deref() else {
            return Some(format!("/auth/required?next={next}"));
        };
        match crate::auth_product::list_organizations(user_id).await {
            Ok(organizations) => {
                if let Some(location) = workspace_setup_redirect(path, &organizations) {
                    return Some(location);
                }
            }
            Err(error) => {
                tracing::error!(
                    error = %error,
                    error_code = error.public_code(),
                    path,
                    "failed to evaluate workspace onboarding gate"
                );
                return Some(format!("/auth/session-expired?next={next}"));
            }
        }
    }

    let Some(permission) = crate::access::permission_for_ui_path(path) else {
        return None;
    };
    let permission = permission.as_str();

    match crate::application::require_authorized_route_for(permission, session_id).await {
        Ok(_) => None,
        Err(crate::error::AuthStackError::Forbidden) => {
            Some(format!("/auth/forbidden?next={next}"))
        }
        Err(crate::error::AuthStackError::AuthRequired)
        | Err(crate::error::AuthStackError::InvalidToken)
        | Err(crate::error::AuthStackError::SessionExpired) => {
            Some(format!("/auth/required?next={next}"))
        }
        Err(error) => {
            tracing::error!(
                error = %error,
                error_code = error.public_code(),
                path,
                permission,
                "failed to authorize protected UI route"
            );
            Some(format!("/auth/session-expired?next={next}"))
        }
    }
}

fn workspace_setup_redirect(
    path: &str,
    organizations: &crate::contracts::OrganizationListResponse,
) -> Option<String> {
    if organizations.organizations.is_empty() {
        return (path != "/onboarding/workspace").then(|| "/onboarding/workspace".to_owned());
    }

    // Additional workspaces are created from /organizations (modal), not
    // /onboarding/workspace?new=…. Once an org exists, leave focused onboarding.
    if path == "/onboarding/workspace" {
        return Some("/dashboard".to_owned());
    }

    None
}

fn encode_next_target(path: &str, query: Option<&str>) -> String {
    match query {
        // Preserve path-only next= values unencoded for stable smoke URLs.
        // Encode when a query is present so tokens survive auth redirects.
        Some(query) if !query.is_empty() => percent_encode_component(&format!("{path}?{query}")),
        _ => path.to_owned(),
    }
}

fn percent_encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
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

/// Email deep links that must remain reachable even when a session cookie exists.
///
/// Without this, authenticated users who open reset/verify mail never see the
/// form: the guest-only redirect sends them to `/dashboard` and discards `token`.
fn tokenized_public_ui_route(path: &str, query: Option<&str>) -> bool {
    let Some(query) = query else {
        return false;
    };
    let has_token = query.split('&').any(|part| {
        part.strip_prefix("token=")
            .is_some_and(|value| !value.is_empty())
    });
    has_token && matches!(path, "/reset-password" | "/verify-email")
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
        || path == "/onboarding/workspace"
        || path == "/invitations/accept"
        || path.starts_with("/account/")
        || path.starts_with("/org/")
        || path.starts_with("/organizations")
        || path.starts_with("/admin/")
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

fn server_fn_error_response(
    status: http::StatusCode,
    message: &'static str,
) -> Result<Response, ErrorCode> {
    use http_body_util::BodyExt;

    // Matches `server_fn::error::ServerFnErrorEncoding` (`ServerError|{message}`).
    let body_text = format!("ServerError|{message}");
    let stream = futures::stream::once(async move {
        Ok::<_, std::io::Error>(http_body::Frame::data(bytes::Bytes::from(body_text)))
    });
    let body = http_body_util::StreamBody::new(stream).boxed_unsync();
    let response = http::Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, "text/plain")
        .header("serverfnerror", "true")
        .body(body)
        .map_err(|error| {
            tracing::error!(
                error = %error,
                "failed to build server function error response"
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

/// Rewrite `http://127.0.0.1[:port]/…` (and `::1`) document URLs to `localhost`.
///
/// Browsers refuse WebAuthn when `rpId` is an IP address. Passkey enrollment and
/// login must run on `http://localhost:PORT` while Spin can still bind
/// `127.0.0.1:PORT`. REST/gRPC and static package fetches are left alone.
fn loopback_ip_to_localhost_redirect(
    headers: &http::HeaderMap,
    path: &str,
    query: Option<&str>,
) -> Option<String> {
    let host_header = headers
        .get(http::header::HOST)
        .and_then(|value| value.to_str().ok())?
        .trim();
    if host_header.is_empty() {
        return None;
    }
    let (hostname, port) = match host_header.rsplit_once(':') {
        Some((host, port))
            if !host.is_empty()
                && !host.starts_with('[')
                && port.chars().all(|c| c.is_ascii_digit()) =>
        {
            (host, Some(port))
        }
        _ => {
            // `[::1]:3008` or bare hostname
            if let Some(rest) = host_header.strip_prefix('[') {
                if let Some((host, port_part)) = rest.split_once("]:") {
                    (host, Some(port_part))
                } else if let Some(host) = rest.strip_suffix(']') {
                    (host, None)
                } else {
                    (host_header, None)
                }
            } else {
                (host_header, None)
            }
        }
    };
    let hostname = hostname.trim().trim_matches(|c| c == '[' || c == ']');
    if !matches!(hostname, "127.0.0.1" | "::1") {
        return None;
    }
    let mut location = String::from("http://localhost");
    if let Some(port) = port.filter(|p| !p.is_empty()) {
        location.push(':');
        location.push_str(port);
    }
    if path.is_empty() {
        location.push('/');
    } else {
        location.push_str(path);
    }
    if let Some(query) = query.filter(|q| !q.is_empty()) {
        location.push('?');
        location.push_str(query);
    }
    Some(location)
}

fn internal_error(error: impl std::fmt::Display) -> ErrorCode {
    tracing::error!(error = %error, "fullstack WASI request failed");
    ErrorCode::InternalError(None)
}

#[cfg(test)]
mod loopback_redirect_tests {
    use super::loopback_ip_to_localhost_redirect;
    use http::header::HOST;

    fn headers(host: &str) -> http::HeaderMap {
        let mut map = http::HeaderMap::new();
        map.insert(HOST, host.parse().expect("host header"));
        map
    }

    #[test]
    fn redirects_127_to_localhost_preserving_path_query_port() {
        let location = loopback_ip_to_localhost_redirect(
            &headers("127.0.0.1:3008"),
            "/account/passkeys",
            Some("next=%2Fdashboard"),
        );
        assert_eq!(
            location.as_deref(),
            Some("http://localhost:3008/account/passkeys?next=%2Fdashboard")
        );
    }

    #[test]
    fn redirects_ipv6_loopback() {
        let location =
            loopback_ip_to_localhost_redirect(&headers("[::1]:3008"), "/login", None);
        assert_eq!(location.as_deref(), Some("http://localhost:3008/login"));
    }

    #[test]
    fn leaves_localhost_and_public_hosts_alone() {
        assert!(
            loopback_ip_to_localhost_redirect(&headers("localhost:3008"), "/account/passkeys", None)
                .is_none()
        );
        assert!(
            loopback_ip_to_localhost_redirect(&headers("auth.example.com"), "/account/passkeys", None)
                .is_none()
        );
    }
}

wasip3::http::service::export!(FullstackServer);
