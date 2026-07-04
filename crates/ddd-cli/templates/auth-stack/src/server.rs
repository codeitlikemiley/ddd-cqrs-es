use http::header::{COOKIE, LOCATION};
use leptos::config::get_configuration;
use leptos_wasi::executor::init_wasip3_spawner;
use leptos_wasi::prelude::Handler;
use wasip3::http::types::{ErrorCode, Request, Response};

use crate::app::{
    ActivateAuthorizationModel, App, CompleteOauthCallback, CompletePasswordReset,
    DeleteRelationshipTuples, GetAuthCapabilities, GetCurrentSession, ListAuthProviders,
    ListSigningKeys, LoginEmailPassword, LogoutCurrentSession, RegisterEmailPassword,
    RequireAuthenticatedRoute, RequireAuthorizedRoute, RotateSigningKey, RunAuthorizationCheck,
    SaveAuthProvider, SaveRedirectAllowlist, StartOauthLogin, StartPasskeyLogin,
    StartPasskeyRegistration, StartPasswordReset, VerifyPasskeyLogin, VerifyPasskeyRegistration,
    WriteAuthorizationModel, WriteRelationshipTuples, shell,
};

struct AuthStackServer;

impl wasip3::exports::http::handler::Guest for AuthStackServer {
    async fn handle(request: Request) -> Result<Response, ErrorCode> {
        let _ = init_wasip3_spawner();

        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let req = wasip3::http_compat::http_from_wasi_request(request)?;
        let request_path = req.uri().path().to_string();
        let request_query = req.uri().query().map(ToOwned::to_owned);
        let session_id = session_id_from_headers(req.headers());
        let transport_mode = transport_mode().await;
        tracing::debug!(
            method = %req.method(),
            path = %request_path,
            transport = %transport_mode,
            "handling auth-stack request"
        );

        if !request_path.starts_with("/pkg/")
            && let Err(error) = crate::store::initialize_schema_async().await
        {
            tracing::error!(
                error = %error,
                error_code = error.public_code(),
                "failed to initialize auth storage contract"
            );
            return Err(ErrorCode::InternalError(None));
        }

        #[cfg(all(feature = "spin-grpc", runtime_spin))]
        if grpc_enabled(&transport_mode) && crate::grpc::is_grpc_request(&req) {
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

        let wasi_res = Handler::build(req)
            .await
            .map_err(|error| {
                tracing::error!(
                    error = ?error,
                    "failed to build Leptos WASI handler"
                );
                ErrorCode::InternalError(None)
            })?
            .static_files_handler("/pkg", serve_static_files)
            .with_server_fn::<ListAuthProviders>()
            .with_server_fn::<GetAuthCapabilities>()
            .with_server_fn::<RegisterEmailPassword>()
            .with_server_fn::<LoginEmailPassword>()
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
            .with_server_fn::<WriteAuthorizationModel>()
            .with_server_fn::<ActivateAuthorizationModel>()
            .with_server_fn::<WriteRelationshipTuples>()
            .with_server_fn::<DeleteRelationshipTuples>()
            .with_server_fn::<RunAuthorizationCheck>()
            .generate_routes(App)
            .handle_with_context(move || shell(leptos_options.clone()), || {})
            .await
            .map_err(|error| {
                tracing::error!(
                    error = ?error,
                    "failed to handle Leptos WASI request"
                );
                ErrorCode::InternalError(None)
            })?;

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
        "/" | "/login" | "/register" | "/forgot-password" | "/reset-password"
    )
}

fn protected_ui_route(path: &str) -> bool {
    path == "/dashboard" || path == "/account/security" || path.starts_with("/admin/")
}

fn ui_route_permission(path: &str) -> Option<&'static str> {
    match path {
        "/admin/auth/signing-keys" => Some("auth:signing-key:admin"),
        "/admin/auth/providers" => Some("auth:provider:write"),
        "/admin/auth/redirects" => Some("auth:redirect:write"),
        "/admin/authz/models" => Some("authz:model:write"),
        "/admin/authz/tuples" => Some("authz:tuple:write"),
        "/admin/authz/check" => Some("authz:check"),
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

fn session_id_from_cookie_header(cookie_header: &str) -> Option<String> {
    cookie_header.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        let value = value.trim();
        if name == "ddd_auth_session" && !value.is_empty() {
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
    tracing::debug!(file_path, "serving auth-stack static file");

    if let Ok(bytes) = fs::read(&file_path) {
        Some(leptos_wasi::response::Body::Sync(bytes.into()))
    } else {
        tracing::warn!(file_path, "could not read auth-stack static file");
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

wasip3::http::service::export!(AuthStackServer);
