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
    crate::auth_product::accept_invitation(context.session_id().as_str(), request.token.trim())
        .await
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
        options: crate::auth_product::organization_permission_options(),
    })
}

/// Workspace resolved by URL slug + active membership (not session tenant alone).
#[derive(Clone, Debug)]
pub struct ResolvedWorkspace {
    pub organization_id: String,
    pub organization: OrganizationSummary,
}

/// Resolve `/org/{slug}/…` for the authenticated principal.
///
/// Authorization is membership in the org identified by `slug`. Session
/// `tenant_id` is not used for the scope check and is not auto-selected.
pub async fn resolve_workspace_by_slug(
    auth: RequestAuth,
    slug: &str,
) -> AuthStackResult<ResolvedWorkspace> {
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    resolve_workspace_by_slug_with_context(&context, slug).await
}

pub(crate) async fn resolve_workspace_by_slug_with_context(
    context: &VerifiedAuthContext,
    slug: &str,
) -> AuthStackResult<ResolvedWorkspace> {
    let slug = slug.trim().to_ascii_lowercase();
    crate::store::validate_org_slug(&slug)?;
    let organization_id = crate::store::resolve_org_id_for_slug(&slug).await?;
    let mut organization = crate::auth_product::organization_for_session(
        context.session_id().as_str(),
        &organization_id,
    )
    .await?;
    if organization.slug.trim().is_empty() {
        organization.slug = slug;
        let _ = crate::store::register_org_slug(&organization_id, &organization.slug).await;
    } else {
        let _ = crate::store::register_org_slug(&organization_id, &organization.slug).await;
    }
    Ok(ResolvedWorkspace {
        organization_id,
        organization,
    })
}

fn built_in_role_display_name(role_id: &str) -> String {
    match role_id {
        "owner" => "Owner".to_owned(),
        "admin" => "Admin".to_owned(),
        "member" => "Member".to_owned(),
        "viewer" => "Viewer".to_owned(),
        other => other.to_owned(),
    }
}

fn has_capability(organization: &OrganizationSummary, permission: &str) -> bool {
    organization
        .permissions
        .iter()
        .any(|candidate| candidate == permission)
}

/// Settings bootstrap for a slug-scoped workspace (membership + capabilities).
pub async fn get_workspace_settings_context(
    slug: String,
    auth: RequestAuth,
) -> AuthStackResult<WorkspaceSettingsContext> {
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    let requires_step_up = !assurance_satisfies(context.assurance(), AssuranceRequirement::Aal2);
    let resolved = resolve_workspace_by_slug_with_context(&context, &slug).await?;
    let org = &resolved.organization;
    let session_id = context.session_id().as_str();

    let mut role_options = Vec::new();
    let mut role_name = built_in_role_display_name(&org.current_user_role);
    if has_capability(org, "role.view") {
        if let Ok(roles) =
            crate::auth_product::list_roles(session_id, &resolved.organization_id).await
        {
            if let Some(mine) = roles
                .roles
                .iter()
                .find(|role| role.role_id == org.current_user_role)
            {
                role_name = mine.name.clone();
            }
            role_options = roles
                .roles
                .into_iter()
                .filter(|role| role.role_id != "owner")
                .map(|role| WorkspaceRoleOption {
                    role_id: role.role_id,
                    name: role.name,
                    built_in: role.built_in,
                })
                .collect();
        }
    }

    let mut member_count = 0_u32;
    let mut pending_invitation_count = 0_u32;
    if has_capability(org, "member.view") {
        if let Ok(members) =
            crate::auth_product::list_memberships(session_id, &resolved.organization_id).await
        {
            member_count = members.memberships.len() as u32;
        }
        if let Ok(invitations) =
            crate::auth_product::list_invitations(session_id, &resolved.organization_id).await
        {
            pending_invitation_count = invitations
                .invitations
                .iter()
                .filter(|inv| inv.status == "pending")
                .count() as u32;
        }
    }

    Ok(WorkspaceSettingsContext {
        organization: WorkspaceSettingsOrganization {
            organization_id: org.organization_id.clone(),
            name: org.name.clone(),
            slug: org.slug.clone(),
            status: org.status.clone(),
            created_at_ms: org.created_at_ms,
        },
        membership: WorkspaceSettingsMembership {
            role_id: org.current_user_role.clone(),
            role_name,
            status: "active".to_owned(),
        },
        capabilities: org.permissions.clone(),
        role_options,
        member_count,
        pending_invitation_count,
        requires_step_up,
    })
}

pub async fn list_workspace_members(
    slug: String,
    auth: RequestAuth,
) -> AuthStackResult<MembershipListResponse> {
    let resolved = resolve_workspace_by_slug(auth.clone(), &slug).await?;
    list_members(resolved.organization_id, auth).await
}

pub async fn list_workspace_invitations(
    slug: String,
    auth: RequestAuth,
) -> AuthStackResult<InvitationListResponse> {
    let resolved = resolve_workspace_by_slug(auth.clone(), &slug).await?;
    list_invitations(resolved.organization_id, auth).await
}

pub async fn list_workspace_roles(
    slug: String,
    auth: RequestAuth,
) -> AuthStackResult<RoleListResponse> {
    let resolved = resolve_workspace_by_slug(auth.clone(), &slug).await?;
    list_roles(resolved.organization_id, auth).await
}

pub async fn list_workspace_audit(
    slug: String,
    after: Option<String>,
    limit: Option<u32>,
    auth: RequestAuth,
) -> AuthStackResult<AuditEventListResponse> {
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    let resolved = resolve_workspace_by_slug_with_context(&context, &slug).await?;
    require_organization_permission(
        &context,
        &resolved.organization_id,
        "audit.view",
        AssuranceRequirement::Aal1,
    )
    .await?;
    let after_cursor = after
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| AuthStackError::validation("after cursor is invalid"))
        })
        .transpose()?
        .unwrap_or(0);
    let limit = limit.unwrap_or(50).clamp(1, 100) as usize;
    // Membership + audit.view checked above; wasi-auth also scopes by org id.
    crate::auth_product::list_audit_events(
        context.session_id().as_str(),
        Some(resolved.organization_id.as_str()),
        after_cursor,
        limit,
    )
    .await
}

pub async fn update_workspace_name(
    slug: String,
    name: String,
    auth: RequestAuth,
) -> AuthStackResult<OrganizationSummary> {
    validate_display_name("organization name", &name, 120)?;
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    let resolved = resolve_workspace_by_slug_with_context(&context, &slug).await?;
    let mut organization = crate::auth_product::update_organization(
        context.session_id().as_str(),
        &resolved.organization_id,
        name.trim(),
    )
    .await?;
    // Slug is immutable on rename; preserve resolved slug for the UI.
    if organization.slug.trim().is_empty() {
        organization.slug = resolved.organization.slug;
    }
    Ok(organization)
}

pub async fn assign_workspace_member_role(
    slug: String,
    user_id: String,
    role_id: String,
    auth: RequestAuth,
) -> AuthStackResult<MembershipSummary> {
    validate_identifier("user_id", &user_id)?;
    validate_identifier("role_id", &role_id)?;
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    let resolved = resolve_workspace_by_slug_with_context(&context, &slug).await?;
    crate::auth_product::assign_role(
        context.session_id().as_str(),
        &resolved.organization_id,
        &user_id,
        &role_id,
    )
    .await
}

pub async fn remove_workspace_member(
    slug: String,
    user_id: String,
    auth: RequestAuth,
) -> AuthStackResult<AcceptedResponse> {
    validate_identifier("user_id", &user_id)?;
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    let resolved = resolve_workspace_by_slug_with_context(&context, &slug).await?;
    crate::auth_product::remove_member(
        context.session_id().as_str(),
        &resolved.organization_id,
        &user_id,
    )
    .await?;
    Ok(AcceptedResponse { accepted: true })
}

pub async fn invite_workspace_member(
    slug: String,
    email: String,
    role_id: String,
    auth: RequestAuth,
) -> AuthStackResult<InvitationSummary> {
    validate_required_email(&email)?;
    validate_identifier("role_id", &role_id)?;
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    let resolved = resolve_workspace_by_slug_with_context(&context, &slug).await?;
    crate::auth_product::create_invitation(
        context.session_id().as_str(),
        &resolved.organization_id,
        &email,
        &role_id,
    )
    .await
}

pub async fn revoke_workspace_invitation(
    slug: String,
    invitation_id: String,
    auth: RequestAuth,
) -> AuthStackResult<InvitationSummary> {
    validate_identifier("invitation_id", &invitation_id)?;
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    let resolved = resolve_workspace_by_slug_with_context(&context, &slug).await?;
    crate::auth_product::revoke_invitation(
        context.session_id().as_str(),
        &resolved.organization_id,
        &invitation_id,
    )
    .await
}

pub async fn resend_workspace_invitation(
    slug: String,
    invitation_id: String,
    auth: RequestAuth,
) -> AuthStackResult<InvitationSummary> {
    validate_identifier("invitation_id", &invitation_id)?;
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    let resolved = resolve_workspace_by_slug_with_context(&context, &slug).await?;
    crate::auth_product::resend_invitation(
        context.session_id().as_str(),
        &resolved.organization_id,
        &invitation_id,
    )
    .await
}

pub async fn upsert_workspace_role(
    slug: String,
    role_id: String,
    name: String,
    permissions: Vec<String>,
    auth: RequestAuth,
) -> AuthStackResult<RoleSummary> {
    validate_identifier("role_id", &role_id)?;
    validate_display_name("role name", &name, 80)?;
    if permissions.len() > 100 {
        return Err(AuthStackError::validation(
            "role permission list is too large",
        ));
    }
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    let resolved = resolve_workspace_by_slug_with_context(&context, &slug).await?;
    crate::auth_product::upsert_role(
        context.session_id().as_str(),
        &resolved.organization_id,
        &role_id,
        name.trim(),
        permissions,
    )
    .await
}

pub async fn delete_workspace_role(
    slug: String,
    role_id: String,
    auth: RequestAuth,
) -> AuthStackResult<AcceptedResponse> {
    validate_identifier("role_id", &role_id)?;
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    let resolved = resolve_workspace_by_slug_with_context(&context, &slug).await?;
    crate::auth_product::delete_role(
        context.session_id().as_str(),
        &resolved.organization_id,
        &role_id,
    )
    .await?;
    Ok(AcceptedResponse { accepted: true })
}

/// Transfer workspace ownership to another active member (owner + AAL2).
pub async fn transfer_workspace_ownership(
    slug: String,
    target_user_id: String,
    auth: RequestAuth,
) -> AuthStackResult<MembershipSummary> {
    validate_identifier("target_user_id", &target_user_id)?;
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    let resolved = resolve_workspace_by_slug_with_context(&context, &slug).await?;
    crate::auth_product::transfer_ownership(
        context.session_id().as_str(),
        &resolved.organization_id,
        &target_user_id,
    )
    .await
}

/// Leave the workspace (self-remove). Fails if last owner.
pub async fn leave_workspace(slug: String, auth: RequestAuth) -> AuthStackResult<AcceptedResponse> {
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    let resolved = resolve_workspace_by_slug_with_context(&context, &slug).await?;
    crate::auth_product::leave_organization(
        context.session_id().as_str(),
        &resolved.organization_id,
    )
    .await?;
    Ok(AcceptedResponse { accepted: true })
}

/// Soft-deactivate (archive) the workspace. Owner + AAL2.
pub async fn deactivate_workspace(
    slug: String,
    auth: RequestAuth,
) -> AuthStackResult<OrganizationSummary> {
    let (context, _) = verified_context_and_permissions(auth, true).await?;
    let resolved = resolve_workspace_by_slug_with_context(&context, &slug).await?;
    let mut organization = crate::auth_product::archive_organization(
        context.session_id().as_str(),
        &resolved.organization_id,
    )
    .await?;
    if organization.slug.trim().is_empty() {
        organization.slug = resolved.organization.slug;
    }
    Ok(organization)
}

pub async fn list_workspace_permissions(
    slug: String,
    auth: RequestAuth,
) -> AuthStackResult<PermissionCatalogResponse> {
    let resolved = resolve_workspace_by_slug(auth.clone(), &slug).await?;
    list_permissions(resolved.organization_id, auth).await
}
