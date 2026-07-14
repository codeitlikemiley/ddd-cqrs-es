#![allow(unused_imports)]
#![allow(dead_code)]

use std::sync::OnceLock;


use wasi_auth::authentication::jwt::JwksDocument;
use wasi_auth::authentication::Clock;
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


pub async fn list_signing_keys(auth: RequestAuth) -> AuthStackResult<SigningKeyListResponse> {
    require_step_up_permission_for("system.signing-key.manage", auth).await?;
    crate::auth_product::list_signing_keys().await
}

pub async fn rotate_signing_key(
    request: SigningKeyRotateRequest,
    auth: RequestAuth,
) -> AuthStackResult<SigningKeyRotateResponse> {
    let actor = require_step_up_permission_for("system.signing-key.manage", auth).await?;
    validate_signing_key_id(&request.kid)?;
    crate::auth_product::rotate_signing_key(
        actor
            .session_id
            .as_deref()
            .ok_or(AuthStackError::AuthRequired)?,
        &request.kid,
        request.retire_previous.unwrap_or(true),
    )
    .await
}

pub async fn storage_status(auth: RequestAuth) -> AuthStackResult<StorageStatusResponse> {
    require_step_up_permission_for("auth:storage:admin", auth).await?;
    crate::store::storage_status().await
}

pub async fn run_storage_projections(
    auth: RequestAuth,
    batch_limit: Option<usize>,
) -> AuthStackResult<Vec<StorageProjectionRunResponse>> {
    require_step_up_permission_for("auth:storage:admin", auth).await?;
    crate::store::catch_up_storage_projections(batch_limit).await
}

#[cfg(feature = "mail-capture")]
pub async fn verify_storage_atomic_rollback() -> AuthStackResult<serde_json::Value> {
    crate::store::verify_atomic_rollback_probe().await
}

pub async fn list_admin_users(auth: RequestAuth) -> AuthStackResult<AdminUserListResponse> {
    let session = require_step_up_permission_for("system.user.manage", auth).await?;
    crate::auth_product::list_admin_users(
        session.session_id.as_deref().ok_or(AuthStackError::AuthRequired)?,
    )
    .await
}

pub async fn set_admin_user_status(
    request: AdminUserStatusRequest,
    auth: RequestAuth,
) -> AuthStackResult<AdminUserSummary> {
    validate_identifier("user_id", &request.user_id)?;
    let actor = require_step_up_permission_for("system.user.manage", auth).await?;
    crate::auth_product::set_user_disabled(
        actor.session_id.as_deref().ok_or(AuthStackError::AuthRequired)?,
        &request.user_id,
        request.disabled,
    )
    .await
}

pub async fn admin_list_providers(auth: RequestAuth) -> AuthStackResult<Vec<AuthProviderSummary>> {
    require_step_up_permission_for("system.provider.manage", auth).await?;
    crate::auth_product::list_oauth_providers().await
}

pub async fn admin_save_provider(
    provider_id: String,
    enabled: bool,
    auth: RequestAuth,
) -> AuthStackResult<AuthProviderSummary> {
    validate_provider_id(&provider_id)?;
    let actor = require_step_up_permission_for("system.provider.manage", auth).await?;
    crate::auth_product::save_oauth_provider(
        actor
            .session_id
            .as_deref()
            .ok_or(AuthStackError::AuthRequired)?,
        &provider_id,
        enabled,
    )
    .await
}

pub async fn list_policy_versions(auth: RequestAuth) -> AuthStackResult<PolicyVersionListResponse> {
    require_step_up_permission_for("system.policy.manage", auth).await?;
    crate::auth_product::list_policy_versions().await
}

pub async fn publish_policy(
    request: PolicyPublishRequest,
    auth: RequestAuth,
) -> AuthStackResult<PolicyVersionSummary> {
    if request.policy_text.len() > 1024 * 1024 || request.schema_text.len() > 1024 * 1024 {
        return Err(AuthStackError::validation("policy bundle is too large"));
    }
    CedarProvider::new_validated(
        &request.policy_text,
        &request.schema_text,
        "[]",
        "candidate",
    )
    .map_err(map_cedar_error)?;
    let actor = require_step_up_permission_for("system.policy.manage", auth).await?;
    let version = crate::auth_product::publish_policy_version(
        actor
            .session_id
            .as_deref()
            .ok_or(AuthStackError::AuthRequired)?,
        &request.policy_text,
        &request.schema_text,
    )
    .await?;
    Ok(version)
}

pub async fn get_health(auth: RequestAuth) -> AuthStackResult<HealthStatusResponse> {
    require_step_up_permission_for("system.health.read", auth).await?;
    crate::store::health_status().await
}

pub async fn list_audit_events(
    organization_id: Option<String>,
    after_cursor: u64,
    limit: usize,
    auth: RequestAuth,
) -> AuthStackResult<AuditEventListResponse> {
    let (context, permissions) = verified_context_and_permissions(auth, false).await?;
    let is_system_admin = context.principal().is_system_administrator()
        && context.assurance() == AuthenticationAssurance::Aal2;
    let organization_id = organization_id
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            context
                .organization_id()
                .map(|value| value.as_str().to_owned())
        })
        .filter(|value| value != "tenant:default");
    if !is_system_admin {
        let organization_id = organization_id
            .as_deref()
            .ok_or(AuthStackError::Forbidden)?;
        if context.organization_id().map(|value| value.as_str()) != Some(organization_id)
            || !permissions
                .iter()
                .any(|permission| permission == "audit.view")
        {
            return Err(AuthStackError::Forbidden);
        }
    }
    crate::auth_product::list_audit_events(
        context.session_id().as_str(),
        organization_id.as_deref(),
        after_cursor,
        limit.clamp(1, 100),
    )
    .await
}

pub async fn save_redirect_allowlist(
    redirects_json: String,
    auth: RequestAuth,
) -> AuthStackResult<bool> {
    let redirects: Vec<String> = serde_json::from_str(&redirects_json)
        .map_err(|error| AuthStackError::validation(format!("invalid redirects_json: {error}")))?;
    for redirect in &redirects {
        if !redirect.starts_with('/') || redirect.starts_with("//") {
            return Err(AuthStackError::validation(
                "redirect allowlist entries must be local paths",
            ));
        }
    }
    let actor = require_step_up_permission_for("system.provider.manage", auth).await?;
    crate::auth_product::replace_oauth_redirects(
        actor
            .session_id
            .as_deref()
            .ok_or(AuthStackError::AuthRequired)?,
        &redirects,
    )
    .await?;
    Ok(true)
}
