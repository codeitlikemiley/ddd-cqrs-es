#![allow(unused_imports)]
#![allow(dead_code)]

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

use crate::application::common::{
    ApplicationClock, ApplicationCredentialAuthenticator, config_value,
};
use crate::error::{AuthStackError, AuthStackResult};

/// Canonical browser-facing origin fallback when Spin variables are unset.
/// Prefer `AUTH_PUBLIC_BASE_URL` / Makefile `listen` in real runs.
/// Prefer `localhost` (not `127.0.0.1`) so WebAuthn rpId/origin and session
/// cookies share a browser-valid hostname. Spin can still bind `127.0.0.1`.
pub const DEFAULT_PUBLIC_BASE_URL: &str = "http://localhost:3008";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BrowserOrigin {
    scheme: String,
    host: String,
    port: Option<u16>,
}

impl BrowserOrigin {
    fn effective_port(&self) -> u16 {
        self.port.unwrap_or_else(|| match self.scheme.as_str() {
            "https" => 443,
            _ => 80,
        })
    }
}

pub async fn authenticate_ingress<B>(
    request: &http::Request<B>,
) -> AuthStackResult<Option<VerifiedRequestContext>> {
    let public_base_url = public_base_url().await;
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

pub async fn trusted_context_from_request<B>(
    request: &http::Request<B>,
) -> AuthStackResult<Option<VerifiedRequestContext>> {
    let mut envelopes = request
        .headers()
        .get_all(&wasi_auth::http::AUTH_CONTEXT_HEADER)
        .iter();
    let Some(envelope) = envelopes.next() else {
        return Ok(None);
    };
    if envelopes.next().is_some() {
        return Err(AuthStackError::InvalidCredentials);
    }
    let envelope = envelope
        .to_str()
        .map_err(|_| AuthStackError::InvalidCredentials)?;
    let mut request_ids = request
        .headers()
        .get_all(&wasi_auth::http::REQUEST_ID_HEADER)
        .iter();
    let request_id = request_ids
        .next()
        .ok_or(AuthStackError::InvalidCredentials)?
        .to_str()
        .map_err(|_| AuthStackError::InvalidCredentials)?;
    if request_ids.next().is_some() {
        return Err(AuthStackError::InvalidCredentials);
    }
    let codec = crate::auth_product::trusted_context_codec()
        .await?
        .ok_or_else(|| {
            AuthStackError::configuration(
                "trusted context was supplied without AUTH_TRUSTED_INGRESS_KEY_BASE64",
            )
        })?;
    codec
        .open(
            envelope,
            request.method(),
            request.uri().path(),
            request_id,
            ApplicationClock.now_unix_seconds(),
        )
        .map(Some)
        .map_err(|error| {
            tracing::warn!(error = %error, "signed trusted context was rejected");
            AuthStackError::InvalidCredentials
        })
}

pub async fn validate_browser_origin(headers: &http::HeaderMap) -> AuthStackResult<()> {
    let allowed = public_base_url().await;
    let origin = headers
        .get(http::header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AuthStackError::validation("browser mutation origin is required"))?;
    // Browsers send Origin as scheme://host[:port]. AUTH_PUBLIC_BASE_URL is the
    // configured public origin. Treat loopback host aliases as equivalent so
    // developers can use either http://localhost:3008 or http://127.0.0.1:3008
    // without a cryptic server-function deserialization failure.
    if !browser_origins_match(origin, &allowed) {
        return Err(AuthStackError::validation(
            "browser mutation origin is not allowed",
        ));
    }
    Ok(())
}

pub async fn public_base_url() -> String {
    config_value("AUTH_PUBLIC_BASE_URL")
        .await
        .unwrap_or_else(|| DEFAULT_PUBLIC_BASE_URL.to_owned())
}

/// Compare a browser `Origin` header to the configured public base URL.
///
/// Exact string match wins first. Loopback hosts (`localhost`, `127.0.0.1`,
/// `::1`) are interchangeable when scheme and port match, which mirrors how
/// local fullstack demos are actually opened in a browser.
pub(crate) fn browser_origins_match(request_origin: &str, allowed_base_url: &str) -> bool {
    let request_origin = request_origin.trim();
    let allowed_base_url = allowed_base_url.trim().trim_end_matches('/');
    if request_origin == allowed_base_url {
        return true;
    }

    let Some(request) = parse_browser_origin(request_origin) else {
        return false;
    };
    let Some(allowed) = parse_browser_origin(allowed_base_url) else {
        return false;
    };

    if !request.scheme.eq_ignore_ascii_case(&allowed.scheme) {
        return false;
    }
    if request.effective_port() != allowed.effective_port() {
        return false;
    }
    if request.host.eq_ignore_ascii_case(&allowed.host) {
        return true;
    }
    is_loopback_host(&request.host) && is_loopback_host(&allowed.host)
}

pub(crate) fn parse_browser_origin(value: &str) -> Option<BrowserOrigin> {
    let uri = value.parse::<http::Uri>().ok()?;
    let scheme = uri.scheme_str()?.to_owned();
    let authority = uri.authority()?;
    Some(BrowserOrigin {
        scheme,
        host: authority
            .host()
            .trim_matches(|c| c == '[' || c == ']')
            .to_owned(),
        port: authority.port_u16(),
    })
}

pub(crate) fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
}
