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

impl AdminService for AdminGrpcService {
    async fn list_users(
        &self,
        request: Request<admin_proto::ListUsersRequest>,
    ) -> Result<Response<admin_proto::UserListResponse>, Status> {
        let response = crate::application::list_admin_users(request_auth(&request))
            .await
            .map_err(|error| status_from_app_error("Admin.ListUsers", error))?;
        Ok(Response::new(response.into()))
    }

    async fn set_user_disabled(
        &self,
        request: Request<admin_proto::SetUserDisabledRequest>,
    ) -> Result<Response<admin_proto::User>, Status> {
        let auth = request_auth(&request);
        let request = request.into_inner();
        let response = crate::application::set_admin_user_status(
            crate::contracts::AdminUserStatusRequest {
                user_id: request.user_id,
                disabled: request.disabled,
            },
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("Admin.SetUserDisabled", error))?;
        Ok(Response::new(response.into()))
    }

    async fn list_providers(
        &self,
        request: Request<admin_proto::ListProvidersRequest>,
    ) -> Result<Response<admin_proto::ProviderListResponse>, Status> {
        let providers = crate::application::admin_list_providers(request_auth(&request))
            .await
            .map_err(|error| status_from_app_error("Admin.ListProviders", error))?;
        Ok(Response::new(admin_proto::ProviderListResponse {
            providers: providers.into_iter().map(Into::into).collect(),
        }))
    }

    async fn save_provider(
        &self,
        request: Request<admin_proto::SaveProviderRequest>,
    ) -> Result<Response<admin_proto::Provider>, Status> {
        let auth = request_auth(&request);
        let request = request.into_inner();
        let provider =
            crate::application::admin_save_provider(request.provider_id, request.enabled, auth)
                .await
                .map_err(|error| status_from_app_error("Admin.SaveProvider", error))?;
        Ok(Response::new(provider.into()))
    }

    async fn list_signing_keys(
        &self,
        request: Request<admin_proto::ListSigningKeysRequest>,
    ) -> Result<Response<admin_proto::SigningKeyListResponse>, Status> {
        let response = crate::application::list_signing_keys(request_auth(&request))
            .await
            .map_err(|error| status_from_app_error("Admin.ListSigningKeys", error))?;
        Ok(Response::new(response.into()))
    }

    async fn rotate_signing_key(
        &self,
        request: Request<admin_proto::RotateSigningKeyRequest>,
    ) -> Result<Response<admin_proto::SigningKeyRotateResponse>, Status> {
        let auth = request_auth(&request);
        let request = request.into_inner();
        let response = crate::application::rotate_signing_key(
            crate::contracts::SigningKeyRotateRequest {
                kid: request.kid,
                retire_previous: Some(request.retire_previous),
            },
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("Admin.RotateSigningKey", error))?;
        Ok(Response::new(response.into()))
    }

    async fn list_policy_versions(
        &self,
        request: Request<admin_proto::ListPolicyVersionsRequest>,
    ) -> Result<Response<admin_proto::PolicyVersionListResponse>, Status> {
        let response = crate::application::list_policy_versions(request_auth(&request))
            .await
            .map_err(|error| status_from_app_error("Admin.ListPolicyVersions", error))?;
        Ok(Response::new(response.into()))
    }

    async fn publish_policy(
        &self,
        request: Request<admin_proto::PublishPolicyRequest>,
    ) -> Result<Response<admin_proto::PolicyVersion>, Status> {
        let auth = request_auth(&request);
        let request = request.into_inner();
        let response = crate::application::publish_policy(
            crate::contracts::PolicyPublishRequest {
                policy_text: request.policy_text,
                schema_text: request.schema_text,
            },
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("Admin.PublishPolicy", error))?;
        Ok(Response::new(response.into()))
    }

    async fn get_health(
        &self,
        request: Request<admin_proto::GetHealthRequest>,
    ) -> Result<Response<admin_proto::HealthResponse>, Status> {
        let response = crate::application::get_health(request_auth(&request))
            .await
            .map_err(|error| status_from_app_error("Admin.GetHealth", error))?;
        Ok(Response::new(response.into()))
    }
}
