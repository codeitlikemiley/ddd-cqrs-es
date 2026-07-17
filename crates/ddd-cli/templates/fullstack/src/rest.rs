use bytes::Bytes;
use http::{Method, StatusCode};
use http_body_util::{BodyExt, StreamBody, combinators::UnsyncBoxBody};
use serde::{Serialize, de::DeserializeOwned};

use crate::contracts::{
    AdminProviderRequest, AdminUserStatusRequest, AuthorizationBatchCheckRequest,
    EmailPasswordLoginRequest, EmailPasswordRegisterRequest, EmailVerificationCompleteRequest,
    EmailVerificationResendRequest, InvitationAcceptRequest, InvitationCreateRequest,
    LoginCompletionResponse, MembershipRemoveRequest, MembershipRoleRequest, MfaCodeRequest,
    OAuthCallbackRequest, OrganizationCreateRequest, OrganizationSelectRequest,
    OrganizationUpdateRequest, PasskeyStartRequest, PasskeyVerifyRequest, PasswordChangeRequest,
    PasswordResetCompleteRequest, PasswordResetStartRequest, PolicyPublishRequest,
    RoleUpsertRequest, SessionRevokeRequest, SigningKeyRotateRequest, TokenRefreshRequest,
    TokenVerifyRequest,
};
use crate::error::{AuthStackError, AuthStackResult};

type RestBody = UnsyncBoxBody<Bytes, std::io::Error>;
type RestResponse = http::Response<RestBody>;
type RestRequest = http::Request<wasip3::http_compat::IncomingRequestBody>;
const MAX_REST_BODY_BYTES: usize = 256 * 1024;

pub fn is_rest_request(req: &RestRequest) -> bool {
    let path = req.uri().path();
    path.starts_with("/api/auth/")
        || path.starts_with("/api/authorization/")
        || path.starts_with("/api/organizations")
        || path.starts_with("/api/admin/")
        || path.starts_with("/api/audit/")
        // ddd:domain-rest-prefix
        // ddd:domain-rest-prefix:end
}

pub async fn serve(req: RestRequest) -> AuthStackResult<RestResponse> {
    let request_headers = req.headers().clone();
    let cors = wasi_auth::http::CorsConfig::new(
        [crate::application::public_base_url().await],
        ["GET", "POST", "PATCH", "DELETE", "OPTIONS"],
        [
            "authorization",
            "content-type",
            "x-csrf-token",
            "x-request-id",
        ],
        true,
    )
    .map_err(|_| AuthStackError::configuration("REST CORS policy is invalid"))?;
    let mut response = if req.method() == Method::OPTIONS {
        let preflight = cors
            .preflight(&request_headers)
            .map_err(|_| AuthStackError::validation("CORS preflight is not allowed"))?;
        let mut response = response_with_bytes(preflight.status(), "text/plain", Bytes::new())?;
        response.headers_mut().extend(preflight.headers().clone());
        response
    } else {
        match dispatch(req).await {
            Ok(response) => Ok(response),
            Err(error) => auth_error_response(&error),
        }?
    };
    cors.apply(&request_headers, &mut response);
    wasi_auth::http::apply_response_security(
        &mut response,
        wasi_auth::http::ResponseSecurityPolicy::Sensitive,
        crate::application::session_cookie_secure_enabled().await,
    );
    Ok(response)
}

async fn dispatch(req: RestRequest) -> AuthStackResult<RestResponse> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path = uri.path().to_string();
    // ddd:domain-rest-dispatch
    // ddd:domain-rest-dispatch:end
    let cookie_session_id = cookie_session_id_from_request(&req);
    let request_auth = request_auth_from_request(&req);
    let session_id = request_auth
        .session_id
        .clone()
        .or_else(|| session_id_from_request(&req));

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
        (Method::POST, "/api/auth/email/verify") => {
            let payload = parse_json::<EmailVerificationCompleteRequest>(req).await?;
            json_result(crate::application::complete_email_verification(payload).await)
        }
        (Method::POST, "/api/auth/email/verify/resend") => {
            let payload = parse_json::<EmailVerificationResendRequest>(req).await?;
            json_result(crate::application::resend_email_verification(payload).await)
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
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            let payload = parse_json::<PasskeyStartRequest>(req).await?;
            json_result(crate::application::start_passkey_registration(payload, request_auth).await)
        }
        (Method::POST, "/api/auth/passkeys/register/verify") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            let payload = parse_json::<PasskeyVerifyRequest>(req).await?;
            json_result(
                crate::application::verify_passkey_registration(payload, request_auth).await,
            )
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
        (Method::GET, "/api/auth/csrf") => {
            json_result(crate::application::csrf_token_for_session(cookie_session_id).await)
        }
        (Method::POST, "/api/auth/token/refresh") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
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
        (Method::POST, "/api/auth/password/change") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            let payload = parse_json::<PasswordChangeRequest>(req).await?;
            json_result(crate::application::change_password(payload, request_auth).await)
        }
        (Method::GET, "/api/auth/sessions") => {
            json_result(crate::application::list_sessions(request_auth).await)
        }
        (Method::POST, "/api/auth/sessions/revoke") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            let payload = parse_json::<SessionRevokeRequest>(req).await?;
            json_result(crate::application::revoke_account_session(payload, request_auth).await)
        }
        (Method::GET, "/api/auth/mfa") => {
            json_result(crate::application::mfa_status(request_auth).await)
        }
        (Method::POST, "/api/auth/mfa/totp/enroll/start") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            json_result(crate::application::start_totp_enrollment(request_auth).await)
        }
        (Method::POST, "/api/auth/mfa/totp/enroll/confirm") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            let payload = parse_json::<MfaCodeRequest>(req).await?;
            json_result(crate::application::confirm_totp_enrollment(payload, request_auth).await)
        }
        (Method::POST, "/api/auth/mfa/totp/verify") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            let payload = parse_json::<MfaCodeRequest>(req).await?;
            json_result(crate::application::verify_totp_step_up(payload, request_auth).await)
        }
        (Method::POST, "/api/auth/mfa/recovery/verify") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            let payload = parse_json::<MfaCodeRequest>(req).await?;
            json_result(
                crate::application::use_recovery_code_for_step_up(payload, request_auth).await,
            )
        }
        (Method::POST, "/api/auth/logout") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            json_result(crate::application::logout_session(session_id).await)
        }
        (Method::GET, "/api/auth/.well-known/jwks.json") => {
            json_result(crate::application::get_jwks().await)
        }
        (Method::GET, "/api/auth/dev/mail/latest") => {
            let recipient = query_value(&uri, "recipient").unwrap_or_default();
            let message_kind = query_value(&uri, "kind").unwrap_or_default();
            json_result(crate::application::latest_captured_mail(recipient, message_kind).await)
        }
        #[cfg(feature = "mail-capture")]
        (Method::POST, "/api/auth/dev/storage/rollback-probe") => {
            json_result(crate::application::verify_storage_atomic_rollback().await)
        }
        (Method::GET, "/api/auth/signing-keys") => {
            json_result(crate::application::list_signing_keys(request_auth).await)
        }
        (Method::GET, "/api/auth/storage/status") => {
            json_result(crate::application::storage_status(request_auth).await)
        }
        (Method::POST, "/api/auth/storage/projections/run") => {
            let batch_limit = optional_usize_query(&uri, "limit")?;
            json_result(
                crate::application::run_storage_projections(request_auth, batch_limit).await,
            )
        }
        (Method::POST, "/api/auth/signing-keys/rotate") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            let payload = parse_json::<SigningKeyRotateRequest>(req).await?;
            json_result(crate::application::rotate_signing_key(payload, request_auth).await)
        }
        (Method::GET, "/api/authorization/capabilities") => {
            json_result(crate::application::authorization_capabilities().await)
        }
        (Method::POST, "/api/authorization/check") => {
            let payload = parse_json(req).await?;
            json_result(crate::application::check_authorization(payload, request_auth).await)
        }
        (Method::POST, "/api/authorization/batch-check") => {
            let payload = parse_json::<AuthorizationBatchCheckRequest>(req).await?;
            json_result(crate::application::batch_check_authorization(payload, request_auth).await)
        }
        (Method::GET, "/api/organizations") => {
            json_result(crate::application::list_organizations(request_auth).await)
        }
        (Method::POST, "/api/organizations") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            let payload = parse_json::<OrganizationCreateRequest>(req).await?;
            json_result(crate::application::create_organization(payload, request_auth).await)
        }
        (Method::PATCH, "/api/organizations/current") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            let payload = parse_json::<OrganizationUpdateRequest>(req).await?;
            json_result(crate::application::update_organization(payload, request_auth).await)
        }
        (Method::POST, "/api/organizations/select") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            let payload = parse_json::<OrganizationSelectRequest>(req).await?;
            json_result(crate::application::select_organization(payload, request_auth).await)
        }
        (Method::GET, "/api/organizations/members") => json_result(
            crate::application::list_members(
                required_query(&uri, "organization_id")?,
                request_auth,
            )
            .await,
        ),
        (Method::POST, "/api/organizations/invitations") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            let payload = parse_json::<InvitationCreateRequest>(req).await?;
            json_result(crate::application::invite_member(payload, request_auth).await)
        }
        (Method::GET, "/api/organizations/invitations") => json_result(
            crate::application::list_invitations(
                required_query(&uri, "organization_id")?,
                request_auth,
            )
            .await,
        ),
        (Method::POST, "/api/organizations/invitations/accept") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            let payload = parse_json::<InvitationAcceptRequest>(req).await?;
            json_result(crate::application::accept_invitation(payload, request_auth).await)
        }
        (Method::PATCH, "/api/organizations/members/role") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            let payload = parse_json::<MembershipRoleRequest>(req).await?;
            json_result(crate::application::assign_role(payload, request_auth).await)
        }
        (Method::DELETE, "/api/organizations/members") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            let payload = parse_json::<MembershipRemoveRequest>(req).await?;
            json_result(crate::application::remove_member(payload, request_auth).await)
        }
        (Method::GET, "/api/organizations/roles") => json_result(
            crate::application::list_roles(required_query(&uri, "organization_id")?, request_auth)
                .await,
        ),
        (Method::PUT, "/api/organizations/roles") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            let payload = parse_json::<RoleUpsertRequest>(req).await?;
            json_result(crate::application::upsert_role(payload, request_auth).await)
        }
        (Method::GET, "/api/organizations/permissions") => json_result(
            crate::application::list_permissions(
                required_query(&uri, "organization_id")?,
                request_auth,
            )
            .await,
        ),
        (Method::GET, "/api/admin/users") => {
            json_result(crate::application::list_admin_users(request_auth).await)
        }
        (Method::PATCH, "/api/admin/users/status") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            let payload = parse_json::<AdminUserStatusRequest>(req).await?;
            json_result(crate::application::set_admin_user_status(payload, request_auth).await)
        }
        (Method::GET, "/api/admin/providers") => {
            json_result(crate::application::admin_list_providers(request_auth).await)
        }
        (Method::PUT, "/api/admin/providers") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            let payload = parse_json::<AdminProviderRequest>(req).await?;
            json_result(
                crate::application::admin_save_provider(
                    payload.provider_id,
                    payload.enabled,
                    request_auth,
                )
                .await,
            )
        }
        (Method::GET, "/api/admin/policy-versions") => {
            json_result(crate::application::list_policy_versions(request_auth).await)
        }
        (Method::POST, "/api/admin/policy-versions") => {
            validate_csrf_if_cookie_authenticated(&req, &request_auth).await?;
            let payload = parse_json::<PolicyPublishRequest>(req).await?;
            json_result(crate::application::publish_policy(payload, request_auth).await)
        }
        (Method::GET, "/api/admin/health") => {
            json_result(crate::application::get_health(request_auth).await)
        }
        (Method::GET, "/api/audit/events") => {
            let organization_id = query_value(&uri, "organization_id");
            let after_cursor = optional_u64_query(&uri, "after_cursor")?.unwrap_or_default();
            let limit = optional_usize_query(&uri, "limit")?.unwrap_or(100);
            json_result(
                crate::application::list_audit_events(
                    organization_id,
                    after_cursor,
                    limit,
                    request_auth,
                )
                .await,
            )
        }
        (_, known_path) if known_rest_path(known_path) => validation_error_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "method is not allowed for this auth API route",
        ),
        _ => auth_error_response(&AuthStackError::not_found("unknown auth API route")),
    }
}

async fn parse_json<T: DeserializeOwned>(req: RestRequest) -> AuthStackResult<T> {
    let mut incoming = req.into_body();
    let mut body = Vec::new();
    while let Some(frame) = incoming.frame().await {
        let frame = frame.map_err(|error| {
            AuthStackError::transport(format!("failed to read request body: {error:?}"))
        })?;
        let Ok(data) = frame.into_data() else {
            continue;
        };
        if body.len().saturating_add(data.len()) > MAX_REST_BODY_BYTES {
            return Err(AuthStackError::validation(
                "JSON body exceeds the 256 KiB limit",
            ));
        }
        body.extend_from_slice(&data);
    }

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
    let stream = futures::stream::once(async move {
        Ok::<_, std::io::Error>(http_body::Frame::data(Bytes::new()))
    });
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
    let mut response = json_response(error.http_status(), &error.response_body())?;
    if let AuthStackError::RateLimited {
        retry_after_seconds,
    } = error
    {
        let value = http::HeaderValue::from_str(&retry_after_seconds.to_string())
            .map_err(|header_error| AuthStackError::transport(header_error.to_string()))?;
        response
            .headers_mut()
            .insert(http::header::RETRY_AFTER, value);
    }
    Ok(response)
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

fn query_value(uri: &http::Uri, key: &str) -> Option<String> {
    form_urlencoded::parse(uri.query()?.as_bytes())
        .find_map(|(candidate_key, value)| (candidate_key == key).then(|| value.into_owned()))
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

fn optional_u64_query(uri: &http::Uri, key: &str) -> AuthStackResult<Option<u64>> {
    query_value(uri, key)
        .map(|value| {
            value.parse::<u64>().map_err(|error| {
                AuthStackError::validation(format!("{key} must be a non-negative integer: {error}"))
            })
        })
        .transpose()
}

fn required_query(uri: &http::Uri, key: &str) -> AuthStackResult<String> {
    query_value(uri, key)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AuthStackError::validation(format!("{key} is required")))
}

fn session_id_from_request(req: &RestRequest) -> Option<String> {
    req.headers()
        .get(http::header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(session_id_from_cookie_header)
}

fn cookie_session_id_from_request(req: &RestRequest) -> Option<String> {
    req.headers()
        .get(http::header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(session_id_from_cookie_header)
}

fn access_token_from_request(req: &RestRequest) -> Option<String> {
    req.headers()
        .get(http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(bearer_token)
}

fn request_auth_from_request(req: &RestRequest) -> crate::application::RequestAuth {
    if let Some(context) = req
        .extensions()
        .get::<wasi_auth::context::VerifiedRequestContext>()
    {
        return crate::application::RequestAuth::from_verified(context.clone());
    }
    crate::application::RequestAuth::from_parts(
        session_id_from_request(req),
        access_token_from_request(req),
        header_value(req, "x-request-id"),
    )
}

async fn validate_csrf_if_cookie_authenticated(
    req: &RestRequest,
    auth: &crate::application::RequestAuth,
) -> AuthStackResult<()> {
    if auth.access_token.is_some() {
        return Ok(());
    }
    let cookie_session_id = cookie_session_id_from_request(req);
    if cookie_session_id.is_none() {
        return Ok(());
    }
    crate::application::validate_csrf_token_for_session(
        cookie_session_id,
        header_value(req, "x-csrf-token"),
    )
    .await
}

fn bearer_token(value: &str) -> Option<String> {
    value
        .trim()
        .strip_prefix("Bearer ")
        .and_then(non_empty_string)
}

fn header_value(req: &RestRequest, name: &str) -> Option<String> {
    req.headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(non_empty_string)
}

fn session_id_from_cookie_header(cookie_header: &str) -> Option<String> {
    cookie_header.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        if matches!(name, "__Host-session" | "wasi_auth_dev_session") {
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
    path.starts_with("/api/organizations")
        || path.starts_with("/api/admin/")
        || path.starts_with("/api/audit/")
        || matches!(
            path,
            "/api/auth/capabilities"
                | "/api/auth/providers"
                | "/api/auth/csrf"
                | "/api/auth/password/register"
                | "/api/auth/password/login"
                | "/api/auth/email/verify"
                | "/api/auth/email/verify/resend"
                | "/api/auth/password/reset/start"
                | "/api/auth/password/reset/complete"
                | "/api/auth/passkeys/register/options"
                | "/api/auth/passkeys/register/verify"
                | "/api/auth/passkeys/login/options"
                | "/api/auth/passkeys/login/verify"
                | "/api/auth/session"
                | "/api/auth/token/refresh"
                | "/api/auth/token/verify"
                | "/api/auth/password/change"
                | "/api/auth/sessions"
                | "/api/auth/sessions/revoke"
                | "/api/auth/logout"
                | "/api/auth/.well-known/jwks.json"
                | "/api/auth/dev/mail/latest"
                | "/api/auth/signing-keys"
                | "/api/auth/storage/status"
                | "/api/auth/storage/projections/run"
                | "/api/auth/signing-keys/rotate"
                | "/api/authorization/capabilities"
                | "/api/authorization/check"
                | "/api/authorization/batch-check"
        )
        || (path.starts_with("/api/auth/oauth/")
            && (path.ends_with("/start") || path.ends_with("/callback")))
        || (cfg!(feature = "mail-capture") && path == "/api/auth/dev/storage/rollback-probe")
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
    fn query_value_decodes_percent_encoded_email() {
        let uri = "/api/auth/dev/mail/latest?recipient=user%40example.test&kind=email-verification"
            .parse::<http::Uri>()
            .unwrap();

        assert_eq!(
            query_value(&uri, "recipient").as_deref(),
            Some("user@example.test")
        );
    }

    #[test]
    fn session_id_from_cookie_header_reads_auth_cookie() {
        assert_eq!(
            session_id_from_cookie_header("theme=light; wasi_auth_dev_session=session_1; other=1")
                .as_deref(),
            Some("session_1")
        );
    }
}
