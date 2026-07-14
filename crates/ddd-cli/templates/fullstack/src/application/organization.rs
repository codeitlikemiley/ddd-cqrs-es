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


pub async fn list_organizations(auth: RequestAuth) -> AuthStackResult<OrganizationListResponse> {
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    crate::auth_product::list_organizations(context.principal().user_id().as_str()).await
}

pub async fn create_organization(
    request: OrganizationCreateRequest,
    auth: RequestAuth,
) -> AuthStackResult<OrganizationSummary> {
    validate_display_name("organization name", &request.name, 120)?;
    let mut slug = request.slug.trim().to_ascii_lowercase();
    if slug.is_empty() {
        slug = crate::store::suggest_org_slug(request.name.trim());
    }
    crate::store::validate_org_slug(&slug)?;
    // Fail fast if taken.
    if crate::store::resolve_org_id_for_slug(&slug).await.is_ok() {
        return Err(AuthStackError::validation(format!(
            "workspace URL “{slug}” is already taken"
        )));
    }
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    let summary = crate::auth_product::create_organization(
        request.name.trim(),
        &slug,
        context.session_id().as_str(),
    )
    .await?;
    // Auto-select the new workspace on the session.
    let _ = crate::auth_product::select_organization(
        context.session_id().as_str(),
        &summary.organization_id,
    )
    .await;
    Ok(summary)
}

pub async fn update_organization(
    request: OrganizationUpdateRequest,
    auth: RequestAuth,
) -> AuthStackResult<OrganizationSummary> {
    validate_identifier("organization_id", &request.organization_id)?;
    validate_display_name("organization name", &request.name, 120)?;
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    enforce_organization_scope(&context, &request.organization_id).await?;
    let organization = crate::auth_product::update_organization(
        context.session_id().as_str(),
        &request.organization_id,
        request.name.trim(),
    )
    .await?;
    Ok(organization)
}

pub async fn select_organization(
    request: OrganizationSelectRequest,
    auth: RequestAuth,
) -> AuthStackResult<SessionView> {
    validate_identifier("organization_id", &request.organization_id)?;
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    match crate::auth_product::select_organization(
        context.session_id().as_str(),
        &request.organization_id,
    )
    .await
    {
        Ok(session) => Ok(session),
        Err(AuthStackError::Forbidden) => Err(AuthStackError::validation(
            "cannot select this workspace — you may not be an active member, or the session expired. Sign in again and retry.",
        )),
        Err(error) => Err(error),
    }
}

pub async fn list_members(
    organization_id: String,
    auth: RequestAuth,
) -> AuthStackResult<MembershipListResponse> {
    validate_identifier("organization_id", &organization_id)?;
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    enforce_organization_scope(&context, &organization_id).await?;
    crate::auth_product::list_memberships(context.session_id().as_str(), &organization_id).await
}

pub async fn invite_member(
    request: InvitationCreateRequest,
    auth: RequestAuth,
) -> AuthStackResult<InvitationSummary> {
    validate_identifier("organization_id", &request.organization_id)?;
    validate_required_email(&request.email)?;
    validate_identifier("role_id", &request.role_id)?;
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    enforce_organization_scope(&context, &request.organization_id).await?;
    let invitation = crate::auth_product::create_invitation(
        context.session_id().as_str(),
        &request.organization_id,
        &request.email,
        &request.role_id,
    )
    .await?;
    Ok(invitation)
}

pub async fn list_invitations(
    organization_id: String,
    auth: RequestAuth,
) -> AuthStackResult<InvitationListResponse> {
    validate_identifier("organization_id", &organization_id)?;
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    enforce_organization_scope(&context, &organization_id).await?;
    crate::auth_product::list_invitations(context.session_id().as_str(), &organization_id).await
}

pub async fn accept_invitation(
    request: InvitationAcceptRequest,
    auth: RequestAuth,
) -> AuthStackResult<OrganizationSummary> {
    if request.token.trim().is_empty() {
        return Err(AuthStackError::validation("invitation token is required"));
    }
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    crate::auth_product::accept_invitation(context.session_id().as_str(), request.token.trim()).await
}

pub async fn assign_role(
    request: MembershipRoleRequest,
    auth: RequestAuth,
) -> AuthStackResult<MembershipSummary> {
    validate_identifier("organization_id", &request.organization_id)?;
    validate_identifier("user_id", &request.user_id)?;
    validate_identifier("role_id", &request.role_id)?;
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    enforce_organization_scope(&context, &request.organization_id).await?;
    let membership = crate::auth_product::assign_role(
        context.session_id().as_str(),
        &request.organization_id,
        &request.user_id,
        &request.role_id,
    )
    .await?;
    Ok(membership)
}

pub async fn remove_member(
    request: MembershipRemoveRequest,
    auth: RequestAuth,
) -> AuthStackResult<AcceptedResponse> {
    validate_identifier("organization_id", &request.organization_id)?;
    validate_identifier("user_id", &request.user_id)?;
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    enforce_organization_scope(&context, &request.organization_id).await?;
    crate::auth_product::remove_member(
        context.session_id().as_str(),
        &request.organization_id,
        &request.user_id,
    )
    .await?;
    Ok(AcceptedResponse { accepted: true })
}

pub async fn list_roles(
    organization_id: String,
    auth: RequestAuth,
) -> AuthStackResult<RoleListResponse> {
    validate_identifier("organization_id", &organization_id)?;
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    enforce_organization_scope(&context, &organization_id).await?;
    crate::auth_product::list_roles(context.session_id().as_str(), &organization_id).await
}

pub async fn upsert_role(
    request: RoleUpsertRequest,
    auth: RequestAuth,
) -> AuthStackResult<RoleSummary> {
    validate_identifier("organization_id", &request.organization_id)?;
    validate_identifier("role_id", &request.role_id)?;
    validate_display_name("role name", &request.name, 80)?;
    if request.permissions.len() > 100 {
        return Err(AuthStackError::validation(
            "role permission list is too large",
        ));
    }
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    enforce_organization_scope(&context, &request.organization_id).await?;
    let role = crate::auth_product::upsert_role(
        context.session_id().as_str(),
        &request.organization_id,
        &request.role_id,
        request.name.trim(),
        request.permissions,
    )
    .await?;
    Ok(role)
}

pub async fn list_permissions(
    organization_id: String,
    auth: RequestAuth,
) -> AuthStackResult<PermissionCatalogResponse> {
    validate_identifier("organization_id", &organization_id)?;
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    let organization = crate::auth_product::organization_for_session(
        context.session_id().as_str(),
        &organization_id,
    )
    .await?;
    if !organization
        .permissions
        .iter()
        .any(|permission| permission == "role.view")
    {
        return Err(AuthStackError::Forbidden);
    }
    Ok(PermissionCatalogResponse {
        permissions: crate::auth_product::organization_permission_catalog(),
    })
}
