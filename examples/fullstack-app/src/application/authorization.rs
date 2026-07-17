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

pub async fn authorization_capabilities() -> AuthStackResult<AuthorizationCapabilitiesResponse> {
    let cedar = cedar_provider().await?;
    let capabilities = Authorizer::new(&cedar).capabilities();
    let spicedb = crate::store::direct_spicedb_enabled().await;
    Ok(AuthorizationCapabilitiesResponse {
        provider: if spicedb {
            "embedded-cedar+direct-spicedb"
        } else {
            "embedded-cedar"
        }
        .to_string(),
        batch_check: capabilities.batch_check,
        list_resources: capabilities.list_resources,
        consistency_tokens: spicedb || capabilities.consistency_tokens,
        max_batch_checks: MAX_BATCH_CHECKS as u32,
    })
}

pub async fn check_authorization(
    request: AuthorizationCheckRequest,
    auth: RequestAuth,
) -> AuthStackResult<AuthorizationCheckResponse> {
    let (context, permissions) = verified_context_and_permissions(auth, false).await?;
    let access_request = authorization_access_request(request, context, &permissions)?;
    let cedar = cedar_provider().await?;
    let cedar_decision = Authorizer::new(&cedar)
        .check(&access_request)
        .await
        .map_err(map_cedar_error)?;
    if !cedar_decision.is_allowed()
        || !crate::store::direct_spicedb_enabled().await
        || (access_request
            .context()
            .principal()
            .is_system_administrator()
            && access_request.context().assurance() == AuthenticationAssurance::Aal2)
    {
        return Ok(authorization_response(cedar_decision, None));
    }
    if let Some(organization_id) = access_request.context().organization_id() {
        let (decision, resource_revision) = crate::store::check_direct_spicedb_membership(
            access_request.context().clone(),
            organization_id.as_str(),
        )
        .await?;
        return Ok(authorization_response(decision, resource_revision));
    }
    Ok(authorization_response(cedar_decision, None))
}

pub async fn batch_check_authorization(
    request: AuthorizationBatchCheckRequest,
    auth: RequestAuth,
) -> AuthStackResult<AuthorizationBatchCheckResponse> {
    if request.checks.len() > MAX_BATCH_CHECKS {
        return Err(AuthStackError::validation(format!(
            "authorization batch exceeds the maximum of {MAX_BATCH_CHECKS}"
        )));
    }
    let (context, permissions) = verified_context_and_permissions(auth, false).await?;
    let requests = request
        .checks
        .into_iter()
        .map(|request| authorization_access_request(request, context.clone(), &permissions))
        .collect::<AuthStackResult<Vec<_>>>()?;
    let cedar = cedar_provider().await?;
    let decisions = Authorizer::new(&cedar)
        .batch_check(&requests)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "embedded Cedar batch failed closed");
            AuthStackError::Forbidden
        })?;
    let spicedb = crate::store::direct_spicedb_enabled().await;
    let mut results = Vec::with_capacity(decisions.len());
    for (access_request, cedar_decision) in requests.iter().zip(decisions) {
        if cedar_decision.is_allowed()
            && spicedb
            && !(access_request
                .context()
                .principal()
                .is_system_administrator()
                && access_request.context().assurance() == AuthenticationAssurance::Aal2)
            && let Some(organization_id) = access_request.context().organization_id()
        {
            let (decision, resource_revision) = crate::store::check_direct_spicedb_membership(
                access_request.context().clone(),
                organization_id.as_str(),
            )
            .await?;
            results.push(authorization_response(decision, resource_revision));
        } else {
            results.push(authorization_response(cedar_decision, None));
        }
    }
    Ok(AuthorizationBatchCheckResponse { results })
}

pub(crate) fn authorization_access_request(
    request: AuthorizationCheckRequest,
    context: VerifiedAuthContext,
    permissions: &[String],
) -> AuthStackResult<AccessRequest> {
    let requested_action = ActionName::new(request.action.clone())
        .map_err(|_| AuthStackError::validation("action is invalid"))?;
    let requested_resource_type = ResourceType::new(request.resource_type.clone())
        .map_err(|_| AuthStackError::validation("resource_type is invalid"))?;
    let organization_id = request
        .organization_id
        .map(OrganizationId::new)
        .transpose()
        .map_err(|_| AuthStackError::validation("organization_id is invalid"))?
        .or_else(|| context.organization_id().cloned());
    let resource_id = format!(
        "{}:{}",
        requested_resource_type.as_str(),
        request.resource_id
    );
    let resource = Resource::new(
        ResourceType::new("ApplicationResource")
            .map_err(|_| AuthStackError::configuration("embedded Cedar resource is invalid"))?,
        resource_id,
        organization_id,
    )
    .map_err(|_| AuthStackError::validation("resource_id is invalid"))?;
    let mut effective_permissions = permissions.to_vec();
    if context.principal().is_system_administrator()
        && context.assurance() == AuthenticationAssurance::Aal2
    {
        effective_permissions.push(requested_action.as_str().to_owned());
    }
    let authorization = AuthorizationSnapshot::new(effective_permissions, [], None, None)
        .map_err(|_| AuthStackError::configuration("authorization snapshot is invalid"))?;
    let access_request = AccessRequest::new(
        context,
        ActionName::new("authorization.check")
            .map_err(|_| AuthStackError::configuration("embedded Cedar action is invalid"))?,
        resource,
    )
    .map_err(|_| AuthStackError::Forbidden)?
    .with_authorization_snapshot(authorization)
    .with_attribute("requested_action", requested_action.as_str())
    .and_then(|request| {
        request.with_attribute("requested_resource_type", requested_resource_type.as_str())
    })
    .map_err(|_| AuthStackError::validation("authorization context is invalid"))?;
    Ok(access_request)
}

pub(crate) fn authorization_response(
    decision: wasi_auth::authorization::Decision,
    resource_revision: Option<u64>,
) -> AuthorizationCheckResponse {
    AuthorizationCheckResponse {
        allowed: decision.is_allowed(),
        reason: decision.reason().to_string(),
        policy_revision: decision.policy_revision().as_str().to_string(),
        consistency_token: decision.consistency_token().map(ToOwned::to_owned),
        resource_revision,
    }
}

pub(crate) async fn cedar_provider() -> AuthStackResult<CedarProvider> {
    if let Some(bundle) = crate::auth_product::active_policy_bundle().await? {
        let entities = serde_json::to_string(&bundle.entities)
            .map_err(|_| AuthStackError::configuration("active Cedar entities are invalid"))?;
        return CedarProvider::new_validated(
            &bundle.cedar_policy,
            &bundle.cedar_schema,
            &entities,
            bundle.policy_revision,
        )
        .map_err(map_cedar_error);
    }
    embedded_cedar_provider().cloned()
}

pub(crate) fn embedded_cedar_provider() -> AuthStackResult<&'static CedarProvider> {
    static PROVIDER: OnceLock<Result<CedarProvider, CedarError>> = OnceLock::new();
    match PROVIDER.get_or_init(|| {
        CedarProvider::new_prevalidated(
            DEFAULT_APPLICATION_POLICY,
            "[]",
            DEFAULT_APPLICATION_POLICY_REVISION,
        )
    }) {
        Ok(provider) => Ok(provider),
        Err(error) => Err(map_cedar_error(*error)),
    }
}

pub(crate) fn map_cedar_error(error: CedarError) -> AuthStackError {
    tracing::error!(error = %error, "embedded Cedar failed closed");
    AuthStackError::Forbidden
}
