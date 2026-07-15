//! Shared server-fn helpers (cookies, error mapping).

#![allow(unused_imports)]

use crate::contracts::LoginCompletionResponse;
use leptos::prelude::*;
use server_fn::ServerFnError;

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
pub(crate) async fn set_session_cookie(response: &LoginCompletionResponse) {
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

#[cfg(feature = "ssr")]
pub(crate) async fn clear_session_cookie() {
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

#[cfg(any(feature = "ssr", test))]
pub(crate) fn browser_login_response(
    mut response: LoginCompletionResponse,
) -> LoginCompletionResponse {
    response.session_id = None;
    response.access_token = None;
    response.refresh_token = None;
    response
}

#[cfg(feature = "ssr")]
pub(crate) async fn current_organization_id() -> Result<String, ServerFnError> {
    let session =
        crate::application::require_authenticated_route_for(current_session_id_from_cookie())
            .await
            .map_err(server_fn_error)?;
    session
        .tenant_id
        .filter(|organization_id| organization_id != "tenant:default")
        .ok_or_else(|| ServerFnError::ServerError("select an organization first".to_owned()))
}
