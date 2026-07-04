use bytes::Bytes;
use http::{Method, StatusCode};
use http_body_util::{BodyExt, StreamBody, combinators::UnsyncBoxBody};
use serde::{Serialize, de::DeserializeOwned};

use crate::contracts::{
    AuthzBatchCheckRequest, AuthzExpandRequest, AuthzListObjectsRequest, AuthzModelWriteRequest,
    EmailPasswordLoginRequest, EmailPasswordRegisterRequest, LoginCompletionResponse,
    OAuthCallbackRequest,
    PasskeyStartRequest, PasskeyVerifyRequest, PasswordResetCompleteRequest,
    PasswordResetStartRequest, RelationshipTupleWriteRequest, SigningKeyRotateRequest,
    TokenRefreshRequest, TokenVerifyRequest,
};
use crate::error::{AuthStackError, AuthStackResult};

type RestBody = UnsyncBoxBody<Bytes, std::io::Error>;
type RestResponse = http::Response<RestBody>;
type RestRequest = http::Request<wasip3::http_compat::IncomingRequestBody>;

pub fn is_rest_request(req: &RestRequest) -> bool {
    let path = req.uri().path();
    path.starts_with("/api/auth/") || path.starts_with("/api/authz/")
}

pub async fn serve(req: RestRequest) -> AuthStackResult<RestResponse> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path = uri.path().to_string();
    let session_id = session_id_from_request(&req);

    tracing::debug!(
        method = %method,
        path,
        "handling auth REST request"
    );

    match (method, path.as_str()) {
        (Method::GET, "/api/auth/capabilities") => {
            json_result(crate::application::auth_capabilities().await)
        }
        (Method::GET, "/api/auth/providers") => {
            json_result(crate::application::list_auth_providers().await)
        }
        (Method::POST, "/api/auth/password/register") => {
            let payload = parse_json::<EmailPasswordRegisterRequest>(req).await?;
            json_result(crate::application::register_email_password(payload).await)
        }
        (Method::POST, "/api/auth/password/login") => {
            let payload = parse_json::<EmailPasswordLoginRequest>(req).await?;
            json_result(crate::application::login_email_password(payload).await)
        }
        (Method::POST, "/api/auth/password/reset/start") => {
            let payload = parse_json::<PasswordResetStartRequest>(req).await?;
            json_result(crate::application::start_password_reset(payload).await)
        }
        (Method::POST, "/api/auth/password/reset/complete") => {
            let payload = parse_json::<PasswordResetCompleteRequest>(req).await?;
            json_result(crate::application::complete_password_reset(payload).await)
        }
        (Method::POST, "/api/auth/passkeys/register/options") => {
            let payload = parse_json::<PasskeyStartRequest>(req).await?;
            json_result(crate::application::start_passkey_registration(payload).await)
        }
        (Method::POST, "/api/auth/passkeys/register/verify") => {
            let payload = parse_json::<PasskeyVerifyRequest>(req).await?;
            json_result(crate::application::verify_passkey_registration(payload).await)
        }
        (Method::POST, "/api/auth/passkeys/login/options") => {
            let payload = parse_json::<PasskeyStartRequest>(req).await?;
            json_result(crate::application::start_passkey_login(payload).await)
        }
        (Method::POST, "/api/auth/passkeys/login/verify") => {
            let payload = parse_json::<PasskeyVerifyRequest>(req).await?;
            json_result(crate::application::verify_passkey_login(payload).await)
        }
        (Method::GET, path) if path.starts_with("/api/auth/oauth/") && path.ends_with("/start") => {
            let provider_id = oauth_provider_from_path(path, "/start")?;
            json_result(
                crate::application::start_oauth_login(
                    provider_id.to_string(),
                    query_value(&uri, "next"),
                )
                .await,
            )
        }
        (Method::GET, path)
            if path.starts_with("/api/auth/oauth/") && path.ends_with("/callback") =>
        {
            let provider_id = oauth_provider_from_path(path, "/callback")?;
            let request = OAuthCallbackRequest {
                provider_id: provider_id.to_string(),
                code: query_value(&uri, "code"),
                state: query_value(&uri, "state"),
                redirect_url: query_value(&uri, "next"),
            };
            oauth_callback_result(
                provider_id,
                wants_json_response(&req, &uri),
                crate::application::complete_oauth_callback(request).await,
            )
            .await
        }
        (Method::GET, "/api/auth/session") => {
            json_result(crate::application::get_current_session_for(session_id).await)
        }
        (Method::POST, "/api/auth/token/refresh") => {
            let payload = parse_json::<TokenRefreshRequest>(req).await?;
            json_result(
                crate::application::refresh_token_for(session_id, Some(payload.refresh_token))
                    .await,
            )
        }
        (Method::POST, "/api/auth/token/verify") => {
            let payload = parse_json::<TokenVerifyRequest>(req).await?;
            json_result(crate::application::verify_access_token(payload).await)
        }
        (Method::POST, "/api/auth/logout") => {
            json_result(crate::application::logout_session(session_id).await)
        }
        (Method::GET, "/api/auth/.well-known/jwks.json") => {
            json_result(crate::application::get_jwks().await)
        }
        (Method::GET, "/api/auth/signing-keys") => {
            json_result(crate::application::list_signing_keys(admin_token_from_request(&req)).await)
        }
        (Method::GET, "/api/auth/storage/status") => {
            json_result(crate::application::storage_status(admin_token_from_request(&req)).await)
        }
        (Method::POST, "/api/auth/storage/projections/run") => {
            let batch_limit = optional_usize_query(&uri, "limit")?;
            json_result(
                crate::application::run_storage_projections(
                    admin_token_from_request(&req),
                    batch_limit,
                )
                .await,
            )
        }
        (Method::POST, "/api/auth/signing-keys/rotate") => {
            let admin_token = admin_token_from_request(&req);
            let mut payload = parse_json::<SigningKeyRotateRequest>(req).await?;
            if payload.admin_token.is_none() {
                payload.admin_token = admin_token;
            }
            json_result(crate::application::rotate_signing_key(payload).await)
        }
        (Method::POST, "/api/authz/check") => {
            let payload = parse_json(req).await?;
            json_result(crate::application::check_authorization(payload).await)
        }
        (Method::POST, "/api/authz/batch-check") => {
            let payload = parse_json::<AuthzBatchCheckRequest>(req).await?;
            json_result(crate::application::batch_check_authorization(payload).await)
        }
        (Method::POST, "/api/authz/list-objects") => {
            let payload = parse_json::<AuthzListObjectsRequest>(req).await?;
            json_result(crate::application::list_authorized_objects(payload).await)
        }
        (Method::POST, "/api/authz/expand") => {
            let payload = parse_json::<AuthzExpandRequest>(req).await?;
            json_result(crate::application::expand_authorization(payload).await)
        }
        (Method::POST, "/api/authz/models") => {
            let payload = parse_json::<AuthzModelWriteRequest>(req).await?;
            json_result(crate::application::write_authorization_model(payload).await)
        }
        (Method::POST, path)
            if path.starts_with("/api/authz/models/") && path.ends_with("/activate") =>
        {
            let model_id = model_id_from_activate_path(path)?;
            json_result(crate::application::activate_authorization_model(model_id).await)
        }
        (Method::POST, "/api/authz/tuples/write") => {
            let payload = parse_json::<RelationshipTupleWriteRequest>(req).await?;
            json_result(crate::application::write_relationship_tuples(payload).await)
        }
        (Method::POST, "/api/authz/tuples/delete") => {
            let payload = parse_json::<RelationshipTupleWriteRequest>(req).await?;
            json_result(crate::application::delete_relationship_tuples(payload).await)
        }
        (_, known_path) if known_rest_path(known_path) => validation_error_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "method is not allowed for this auth API route",
        ),
        _ => auth_error_response(&AuthStackError::not_found("unknown auth API route")),
    }
}

async fn parse_json<T: DeserializeOwned>(req: RestRequest) -> AuthStackResult<T> {
    let body = req
        .into_body()
        .collect()
        .await
        .map_err(|error| {
            AuthStackError::transport(format!("failed to read request body: {error:?}"))
        })?
        .to_bytes();

    if body.is_empty() {
        return Err(AuthStackError::validation("JSON body is required"));
    }

    serde_json::from_slice(&body)
        .map_err(|error| AuthStackError::validation(format!("invalid JSON body: {error}")))
}

fn json_result<T: Serialize>(result: AuthStackResult<T>) -> AuthStackResult<RestResponse> {
    match result {
        Ok(value) => json_response(StatusCode::OK, &value),
        Err(error) => auth_error_response(&error),
    }
}

async fn oauth_callback_result(
    provider_id: &str,
    json_mode: bool,
    result: AuthStackResult<LoginCompletionResponse>,
) -> AuthStackResult<RestResponse> {
    match result {
        Ok(value) if json_mode => json_response(StatusCode::OK, &value),
        Ok(value) => oauth_redirect_response(&value).await,
        Err(error) if json_mode => auth_error_response(&error),
        Err(error) => {
            log_rest_error(&error, error.http_status());
            redirect_response(&format!("/auth/callback/{provider_id}/error"), None)
        }
    }
}

async fn oauth_redirect_response(value: &LoginCompletionResponse) -> AuthStackResult<RestResponse> {
    let session_id = value
        .session_id
        .as_deref()
        .ok_or_else(|| AuthStackError::transport("OAuth callback did not issue a session"))?;
    let set_cookie = crate::application::session_cookie_header_value(
        session_id,
        None,
        crate::application::session_cookie_secure_enabled().await,
    );
    redirect_response(&value.redirect_url, Some(&set_cookie))
}

fn redirect_response(location: &str, set_cookie: Option<&str>) -> AuthStackResult<RestResponse> {
    let stream =
        futures::stream::once(
            async move { Ok::<_, std::io::Error>(http_body::Frame::data(Bytes::new())) },
        );
    let body = StreamBody::new(stream).boxed_unsync();
    let mut builder = http::Response::builder()
        .status(StatusCode::FOUND)
        .header(http::header::LOCATION, location);
    if let Some(set_cookie) = set_cookie {
        builder = builder.header(http::header::SET_COOKIE, set_cookie);
    }
    builder
        .body(body)
        .map_err(|error| AuthStackError::transport(error.to_string()))
}

fn wants_json_response(req: &RestRequest, uri: &http::Uri) -> bool {
    query_value(uri, "format").as_deref() == Some("json")
        || req
            .headers()
            .get(http::header::ACCEPT)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("application/json"))
}

fn json_response<T: Serialize>(status: StatusCode, value: &T) -> AuthStackResult<RestResponse> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| AuthStackError::serialization(error.to_string()))?;
    response_with_bytes(status, "application/json", Bytes::from(bytes))
}

fn auth_error_response(error: &AuthStackError) -> AuthStackResult<RestResponse> {
    log_rest_error(error, error.http_status());
    json_response(error.http_status(), &error.response_body())
}

fn validation_error_response(
    status: StatusCode,
    message: impl Into<String>,
) -> AuthStackResult<RestResponse> {
    let error = AuthStackError::validation(message);
    log_rest_error(&error, status);
    json_response(status, &error.response_body())
}

fn response_with_bytes(
    status: StatusCode,
    content_type: &'static str,
    bytes: Bytes,
) -> AuthStackResult<RestResponse> {
    let stream =
        futures::stream::once(
            async move { Ok::<_, std::io::Error>(http_body::Frame::data(bytes)) },
        );
    let body = StreamBody::new(stream).boxed_unsync();

    http::Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, content_type)
        .body(body)
        .map_err(|error| AuthStackError::transport(error.to_string()))
}

fn oauth_provider_from_path<'a>(path: &'a str, suffix: &str) -> AuthStackResult<&'a str> {
    let provider_id = path
        .strip_prefix("/api/auth/oauth/")
        .and_then(|value| value.strip_suffix(suffix))
        .ok_or_else(|| AuthStackError::not_found("unknown OAuth route"))?;

    if provider_id.is_empty() || provider_id.contains('/') {
        return Err(AuthStackError::validation("provider_id is invalid"));
    }

    Ok(provider_id)
}

fn model_id_from_activate_path(path: &str) -> AuthStackResult<String> {
    let model_id = path
        .strip_prefix("/api/authz/models/")
        .and_then(|value| value.strip_suffix("/activate"))
        .ok_or_else(|| AuthStackError::not_found("unknown authz model route"))?;

    if model_id.is_empty() || model_id.contains('/') {
        return Err(AuthStackError::validation("model_id is invalid"));
    }

    Ok(model_id.to_string())
}

fn query_value(uri: &http::Uri, key: &str) -> Option<String> {
    uri.query()?.split('&').find_map(|part| {
        let (candidate_key, value) = part.split_once('=')?;
        if candidate_key == key {
            Some(value.to_string())
        } else {
            None
        }
    })
}

fn optional_usize_query(uri: &http::Uri, key: &str) -> AuthStackResult<Option<usize>> {
    query_value(uri, key)
        .map(|value| {
            let parsed = value.parse::<usize>().map_err(|error| {
                AuthStackError::validation(format!("{key} must be a positive integer: {error}"))
            })?;
            if parsed == 0 {
                return Err(AuthStackError::validation(format!(
                    "{key} must be a positive integer"
                )));
            }
            Ok(parsed)
        })
        .transpose()
}

fn session_id_from_request(req: &RestRequest) -> Option<String> {
    req.headers()
        .get("x-auth-session")
        .and_then(|value| value.to_str().ok())
        .and_then(non_empty_string)
        .or_else(|| {
            req.headers()
                .get(http::header::COOKIE)
                .and_then(|value| value.to_str().ok())
                .and_then(session_id_from_cookie_header)
        })
        .or_else(|| query_value(req.uri(), "session_id").and_then(|value| non_empty_string(&value)))
}

fn admin_token_from_request(req: &RestRequest) -> Option<String> {
    req.headers()
        .get("x-auth-admin-token")
        .and_then(|value| value.to_str().ok())
        .and_then(non_empty_string)
        .or_else(|| query_value(req.uri(), "admin_token").and_then(|value| non_empty_string(&value)))
}

fn session_id_from_cookie_header(cookie_header: &str) -> Option<String> {
    cookie_header.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        if name == "ddd_auth_session" {
            non_empty_string(value)
        } else {
            None
        }
    })
}

fn non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn known_rest_path(path: &str) -> bool {
    matches!(
        path,
        "/api/auth/capabilities"
            | "/api/auth/providers"
            | "/api/auth/password/register"
            | "/api/auth/password/login"
            | "/api/auth/password/reset/start"
            | "/api/auth/password/reset/complete"
            | "/api/auth/passkeys/register/options"
            | "/api/auth/passkeys/register/verify"
            | "/api/auth/passkeys/login/options"
            | "/api/auth/passkeys/login/verify"
            | "/api/auth/session"
            | "/api/auth/token/refresh"
            | "/api/auth/token/verify"
            | "/api/auth/logout"
            | "/api/auth/.well-known/jwks.json"
            | "/api/auth/signing-keys"
            | "/api/auth/storage/status"
            | "/api/auth/storage/projections/run"
            | "/api/auth/signing-keys/rotate"
            | "/api/authz/check"
            | "/api/authz/batch-check"
            | "/api/authz/list-objects"
            | "/api/authz/expand"
            | "/api/authz/models"
            | "/api/authz/tuples/write"
            | "/api/authz/tuples/delete"
    ) || (path.starts_with("/api/auth/oauth/")
        && (path.ends_with("/start") || path.ends_with("/callback")))
        || (path.starts_with("/api/authz/models/") && path.ends_with("/activate"))
}

fn log_rest_error(error: &AuthStackError, status: StatusCode) {
    if error.is_client_error() {
        tracing::warn!(
            error = %error,
            error_code = error.public_code(),
            http_status = status.as_u16(),
            "auth REST request rejected"
        );
    } else {
        tracing::error!(
            error = %error,
            error_code = error.public_code(),
            http_status = status.as_u16(),
            "auth REST request failed"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oauth_provider_from_path_extracts_provider() {
        let provider = oauth_provider_from_path("/api/auth/oauth/google/start", "/start").unwrap();

        assert_eq!(provider, "google");
    }

    #[test]
    fn model_id_from_activate_path_extracts_model_id() {
        let model_id = model_id_from_activate_path("/api/authz/models/model_1/activate").unwrap();

        assert_eq!(model_id, "model_1");
    }

    #[test]
    fn query_value_reads_session_id() {
        let uri = "/api/auth/session?session_id=session_1"
            .parse::<http::Uri>()
            .unwrap();

        assert_eq!(
            query_value(&uri, "session_id").as_deref(),
            Some("session_1")
        );
    }

    #[test]
    fn session_id_from_cookie_header_reads_auth_cookie() {
        assert_eq!(
            session_id_from_cookie_header("theme=light; ddd_auth_session=session_1; other=1")
                .as_deref(),
            Some("session_1")
        );
    }
}
