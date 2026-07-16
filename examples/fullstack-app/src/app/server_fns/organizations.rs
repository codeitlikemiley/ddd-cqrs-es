#![allow(unused_imports)]

use super::common::*;
use crate::contracts::*;
use leptos::prelude::*;
use server_fn::ServerFnError;
use server_fn::codec::Json;

#[server(prefix = "/api/ui")]
pub async fn list_organizations() -> Result<OrganizationListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_organizations(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn create_organization(
    name: String,
    slug: String,
) -> Result<crate::contracts::OrganizationSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::create_organization(
            OrganizationCreateRequest { name, slug },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (name, slug);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn select_organization(organization_id: String) -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::select_organization(
            crate::contracts::OrganizationSelectRequest { organization_id },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = organization_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_current_organization_members() -> Result<MembershipListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::list_members(organization_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn list_current_organization_invitations() -> Result<InvitationListResponse, ServerFnError>
{
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::list_invitations(organization_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn invite_current_organization_member(
    email: String,
    role_id: String,
) -> Result<crate::contracts::InvitationSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::invite_member(
            InvitationCreateRequest {
                organization_id,
                email,
                role_id,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, role_id);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn accept_organization_invitation(
    token: String,
) -> Result<OrganizationSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::accept_invitation(
            InvitationAcceptRequest { token },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = token;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_current_organization_roles() -> Result<RoleListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::list_roles(organization_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn upsert_current_organization_role(
    role_id: String,
    name: String,
    permissions: Vec<String>,
) -> Result<crate::contracts::RoleSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::upsert_role(
            RoleUpsertRequest {
                organization_id,
                role_id,
                name,
                permissions,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (role_id, name, permissions);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_current_organization_audit() -> Result<AuditEventListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let organization_id = current_organization_id().await?;
        crate::application::list_audit_events(
            Some(organization_id),
            0,
            100,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

// --- Slug-scoped workspace settings (PR3) ------------------------------------

#[server(prefix = "/api/ui")]
pub async fn get_workspace_settings_context(
    slug: String,
) -> Result<WorkspaceSettingsContext, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::get_workspace_settings_context(slug, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = slug;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_workspace_members(slug: String) -> Result<MembershipListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_workspace_members(slug, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = slug;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_workspace_invitations(
    slug: String,
) -> Result<InvitationListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_workspace_invitations(slug, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = slug;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_workspace_roles(slug: String) -> Result<RoleListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_workspace_roles(slug, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = slug;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_workspace_audit(
    slug: String,
    after: Option<String>,
    limit: Option<u32>,
) -> Result<AuditEventListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_workspace_audit(slug, after, limit, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (slug, after, limit);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn update_workspace_name(
    slug: String,
    name: String,
) -> Result<OrganizationSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::update_workspace_name(slug, name, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (slug, name);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn assign_workspace_member_role(
    slug: String,
    user_id: String,
    role_id: String,
) -> Result<crate::contracts::MembershipSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::assign_workspace_member_role(
            slug,
            user_id,
            role_id,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (slug, user_id, role_id);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn remove_workspace_member(
    slug: String,
    user_id: String,
) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::remove_workspace_member(slug, user_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (slug, user_id);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn invite_workspace_member(
    slug: String,
    email: String,
    role_id: String,
) -> Result<crate::contracts::InvitationSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::invite_workspace_member(
            slug,
            email,
            role_id,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (slug, email, role_id);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn revoke_workspace_invitation(
    slug: String,
    invitation_id: String,
) -> Result<crate::contracts::InvitationSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::revoke_workspace_invitation(
            slug,
            invitation_id,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (slug, invitation_id);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn resend_workspace_invitation(
    slug: String,
    invitation_id: String,
) -> Result<crate::contracts::InvitationSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::resend_workspace_invitation(
            slug,
            invitation_id,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (slug, invitation_id);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn upsert_workspace_role(
    slug: String,
    role_id: String,
    name: String,
    permissions: Vec<String>,
) -> Result<crate::contracts::RoleSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::upsert_workspace_role(
            slug,
            role_id,
            name,
            permissions,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (slug, role_id, name, permissions);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn delete_workspace_role(
    slug: String,
    role_id: String,
) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::delete_workspace_role(slug, role_id, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (slug, role_id);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn transfer_workspace_ownership(
    slug: String,
    target_user_id: String,
) -> Result<crate::contracts::MembershipSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::transfer_workspace_ownership(
            slug,
            target_user_id,
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (slug, target_user_id);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn leave_workspace(slug: String) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::leave_workspace(slug, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = slug;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn deactivate_workspace(
    slug: String,
) -> Result<crate::contracts::OrganizationSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::deactivate_workspace(slug, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = slug;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_workspace_permissions(
    slug: String,
) -> Result<crate::contracts::PermissionCatalogResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_workspace_permissions(slug, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = slug;
        unreachable!()
    }
}
