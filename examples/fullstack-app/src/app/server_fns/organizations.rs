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
