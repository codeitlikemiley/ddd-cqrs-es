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

pub async fn get_current_session_for(session_id: Option<String>) -> AuthStackResult<SessionView> {
    let Some(session_id) = session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return crate::auth_product::get_session(None).await;
    };
    let session = crate::auth_product::get_session(Some(session_id)).await?;
    if !session.authenticated {
        return Ok(session);
    }
    let Some(user_id) = session.user_id.as_deref() else {
        return Ok(session);
    };
    // Restore first workspace as default when login left selected_organization_id null.
    crate::auth_product::ensure_default_organization(session_id, user_id).await
}

pub async fn csrf_token_for_session(
    session_id: Option<String>,
) -> AuthStackResult<CsrfTokenResponse> {
    require_authenticated_route_for(session_id.clone()).await?;
    let session_id = session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(AuthStackError::AuthRequired)?;
    Ok(CsrfTokenResponse {
        token: crate::store::csrf_token_for_session(session_id).await?,
    })
}

pub async fn validate_csrf_token_for_session(
    session_id: Option<String>,
    csrf_token: Option<String>,
) -> AuthStackResult<()> {
    let Some(session_id) = session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(AuthStackError::AuthRequired);
    };
    require_authenticated_route_for(Some(session_id.to_string())).await?;
    let expected = crate::store::csrf_token_for_session(session_id).await?;
    let Some(candidate) = csrf_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(AuthStackError::validation("x-csrf-token is required"));
    };
    if expected != candidate {
        return Err(AuthStackError::Forbidden);
    }
    Ok(())
}

pub async fn session_cookie_secure_enabled() -> bool {
    if let Some(value) = config_value("AUTH_COOKIE_SECURE")
        .await
        .filter(|value| !value.trim().is_empty())
    {
        return truthy(&value);
    }
    feature_enabled("AUTH_PRODUCTION_MODE", false).await
}

pub fn session_cookie_header_value(
    session_id: &str,
    max_age_seconds: Option<u64>,
    secure: bool,
) -> String {
    let cookie_name = if secure {
        "__Host-session"
    } else {
        "wasi_auth_dev_session"
    };
    let mut value = format!("{cookie_name}={session_id}; Path=/; HttpOnly; SameSite=Lax");
    if let Some(max_age_seconds) = max_age_seconds {
        value.push_str(&format!("; Max-Age={max_age_seconds}"));
    }
    if secure {
        value.push_str("; Secure");
    }
    value
}

pub fn expired_session_cookie_header_value(secure: bool) -> String {
    session_cookie_header_value("", Some(0), secure)
}

pub async fn require_authenticated_route_for(
    session_id: Option<String>,
) -> AuthStackResult<SessionView> {
    let session = get_current_session_for(session_id).await?;
    if session.authenticated {
        Ok(session)
    } else {
        Err(AuthStackError::AuthRequired)
    }
}

pub async fn require_authorized_route_for(
    permission: &str,
    session_id: Option<String>,
) -> AuthStackResult<SessionView> {
    let session = require_authenticated_route_for(session_id).await?;
    if session.permissions.iter().any(|value| value == permission)
        || system_administrator_may(permission, &session)
    {
        Ok(session)
    } else {
        Err(AuthStackError::Forbidden)
    }
}

pub async fn require_permission_for(
    permission: &str,
    auth: RequestAuth,
) -> AuthStackResult<SessionView> {
    if let Some(access_token) = auth
        .access_token
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        let verified = verify_access_token(TokenVerifyRequest {
            access_token: access_token.to_string(),
        })
        .await?;
        if verified.scopes.iter().any(|scope| scope == permission)
            || (verified.system_administrator
                && verified.assurance == "aal2"
                && is_system_administration_permission(permission))
        {
            return Ok(SessionView {
                authenticated: true,
                session_id: verified.session_id.clone(),
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
        return Err(AuthStackError::Forbidden);
    }

    require_authorized_route_for(permission, auth.session_id).await
}

pub async fn require_step_up_permission_for(
    permission: &str,
    auth: RequestAuth,
) -> AuthStackResult<SessionView> {
    let session = require_permission_for(permission, auth).await?;
    if session.assurance == "aal2" {
        Ok(session)
    } else {
        Err(AuthStackError::Forbidden)
    }
}
