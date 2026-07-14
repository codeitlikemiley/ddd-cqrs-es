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

impl AuthorizationService for AuthorizationGrpcService {
    async fn check(
        &self,
        request: Request<authorization_proto::CheckRequest>,
    ) -> Result<Response<authorization_proto::CheckResponse>, Status> {
        let auth = request_auth(&request);
        let response = crate::application::check_authorization(request.into_inner().into(), auth)
            .await
            .map_err(|error| status_from_app_error("Authorization.Check", error))?;
        Ok(Response::new(response.into()))
    }

    async fn batch_check(
        &self,
        request: Request<authorization_proto::BatchCheckRequest>,
    ) -> Result<Response<authorization_proto::BatchCheckResponse>, Status> {
        let auth = request_auth(&request);
        let request = crate::contracts::AuthorizationBatchCheckRequest {
            checks: request
                .into_inner()
                .checks
                .into_iter()
                .map(Into::into)
                .collect(),
        };
        let response = crate::application::batch_check_authorization(request, auth)
            .await
            .map_err(|error| status_from_app_error("Authorization.BatchCheck", error))?;
        Ok(Response::new(response.into()))
    }

    async fn get_capabilities(
        &self,
        _request: Request<authorization_proto::GetCapabilitiesRequest>,
    ) -> Result<Response<authorization_proto::CapabilitiesResponse>, Status> {
        let response = crate::application::authorization_capabilities()
            .await
            .map_err(|error| status_from_app_error("Authorization.GetCapabilities", error))?;
        Ok(Response::new(response.into()))
    }
}

