#![allow(unused_imports)]
#![allow(dead_code)]

use std::collections::VecDeque;
use std::pin::Pin;

use futures::Stream;
use tonic::{Request, Response, Status};
use wasi_auth::authentication::jwt;

use super::admin_proto::admin_service_server::{AdminService, AdminServiceServer};
use super::audit_proto::audit_service_server::{AuditService, AuditServiceServer};
use super::auth_proto::auth_service_server::{AuthService, AuthServiceServer};
use super::authorization_proto::authorization_service_server::{
    AuthorizationService, AuthorizationServiceServer,
};
use super::organization_proto::organization_service_server::{
    OrganizationService, OrganizationServiceServer,
};
use super::*;

#[tonic::async_trait]

impl OrganizationService for OrganizationGrpcService {
    async fn list_organizations(
        &self,
        request: Request<organization_proto::ListOrganizationsRequest>,
    ) -> Result<Response<organization_proto::OrganizationListResponse>, Status> {
        let response = crate::application::list_organizations(request_auth(&request))
            .await
            .map_err(|error| status_from_app_error("Organization.ListOrganizations", error))?;
        Ok(Response::new(response.into()))
    }

    async fn create_organization(
        &self,
        request: Request<organization_proto::CreateOrganizationRequest>,
    ) -> Result<Response<organization_proto::Organization>, Status> {
        let auth = request_auth(&request);
        let response = crate::application::create_organization(
            crate::contracts::OrganizationCreateRequest {
                name: request.into_inner().name,
                slug: String::new(),
            },
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("Organization.CreateOrganization", error))?;
        Ok(Response::new(response.into()))
    }

    async fn update_organization(
        &self,
        request: Request<organization_proto::UpdateOrganizationRequest>,
    ) -> Result<Response<organization_proto::Organization>, Status> {
        let auth = request_auth(&request);
        let request = request.into_inner();
        let response = crate::application::update_organization(
            crate::contracts::OrganizationUpdateRequest {
                organization_id: request.organization_id,
                name: request.name,
            },
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("Organization.UpdateOrganization", error))?;
        Ok(Response::new(response.into()))
    }

    async fn select_organization(
        &self,
        request: Request<organization_proto::SelectOrganizationRequest>,
    ) -> Result<Response<organization_proto::SessionView>, Status> {
        let auth = request_auth(&request);
        let response = crate::application::select_organization(
            crate::contracts::OrganizationSelectRequest {
                organization_id: request.into_inner().organization_id,
            },
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("Organization.SelectOrganization", error))?;
        Ok(Response::new(response.into()))
    }

    async fn list_members(
        &self,
        request: Request<organization_proto::ListMembersRequest>,
    ) -> Result<Response<organization_proto::MembershipListResponse>, Status> {
        let auth = request_auth(&request);
        let response = crate::application::list_members(request.into_inner().organization_id, auth)
            .await
            .map_err(|error| status_from_app_error("Organization.ListMembers", error))?;
        Ok(Response::new(response.into()))
    }

    async fn invite_member(
        &self,
        request: Request<organization_proto::InviteMemberRequest>,
    ) -> Result<Response<organization_proto::Invitation>, Status> {
        let auth = request_auth(&request);
        let request = request.into_inner();
        let response = crate::application::invite_member(
            crate::contracts::InvitationCreateRequest {
                organization_id: request.organization_id,
                email: request.email,
                role_id: request.role_id,
            },
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("Organization.InviteMember", error))?;
        Ok(Response::new(response.into()))
    }

    async fn list_invitations(
        &self,
        request: Request<organization_proto::ListInvitationsRequest>,
    ) -> Result<Response<organization_proto::InvitationListResponse>, Status> {
        let auth = request_auth(&request);
        let response =
            crate::application::list_invitations(request.into_inner().organization_id, auth)
                .await
                .map_err(|error| status_from_app_error("Organization.ListInvitations", error))?;
        Ok(Response::new(response.into()))
    }

    async fn accept_invitation(
        &self,
        request: Request<organization_proto::AcceptInvitationRequest>,
    ) -> Result<Response<organization_proto::Organization>, Status> {
        let auth = request_auth(&request);
        let response = crate::application::accept_invitation(
            crate::contracts::InvitationAcceptRequest {
                token: request.into_inner().token,
            },
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("Organization.AcceptInvitation", error))?;
        Ok(Response::new(response.into()))
    }

    async fn assign_role(
        &self,
        request: Request<organization_proto::AssignRoleRequest>,
    ) -> Result<Response<organization_proto::Membership>, Status> {
        let auth = request_auth(&request);
        let request = request.into_inner();
        let response = crate::application::assign_role(
            crate::contracts::MembershipRoleRequest {
                organization_id: request.organization_id,
                user_id: request.user_id,
                role_id: request.role_id,
            },
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("Organization.AssignRole", error))?;
        Ok(Response::new(response.into()))
    }

    async fn remove_member(
        &self,
        request: Request<organization_proto::RemoveMemberRequest>,
    ) -> Result<Response<organization_proto::AcceptedResponse>, Status> {
        let auth = request_auth(&request);
        let request = request.into_inner();
        let response = crate::application::remove_member(
            crate::contracts::MembershipRemoveRequest {
                organization_id: request.organization_id,
                user_id: request.user_id,
            },
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("Organization.RemoveMember", error))?;
        Ok(Response::new(organization_proto::AcceptedResponse {
            accepted: response.accepted,
        }))
    }

    async fn list_roles(
        &self,
        request: Request<organization_proto::ListRolesRequest>,
    ) -> Result<Response<organization_proto::RoleListResponse>, Status> {
        let auth = request_auth(&request);
        let response = crate::application::list_roles(request.into_inner().organization_id, auth)
            .await
            .map_err(|error| status_from_app_error("Organization.ListRoles", error))?;
        Ok(Response::new(response.into()))
    }

    async fn upsert_role(
        &self,
        request: Request<organization_proto::UpsertRoleRequest>,
    ) -> Result<Response<organization_proto::Role>, Status> {
        let auth = request_auth(&request);
        let request = request.into_inner();
        let response = crate::application::upsert_role(
            crate::contracts::RoleUpsertRequest {
                organization_id: request.organization_id,
                role_id: request.role_id,
                name: request.name,
                permissions: request.permissions,
            },
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("Organization.UpsertRole", error))?;
        Ok(Response::new(response.into()))
    }

    async fn list_permissions(
        &self,
        request: Request<organization_proto::ListPermissionsRequest>,
    ) -> Result<Response<organization_proto::PermissionListResponse>, Status> {
        let auth = request_auth(&request);
        let response =
            crate::application::list_permissions(request.into_inner().organization_id, auth)
                .await
                .map_err(|error| status_from_app_error("Organization.ListPermissions", error))?;
        Ok(Response::new(organization_proto::PermissionListResponse {
            permissions: response.permissions,
        }))
    }
}
