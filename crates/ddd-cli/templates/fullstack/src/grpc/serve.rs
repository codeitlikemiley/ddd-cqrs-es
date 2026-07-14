#![allow(unused_imports)]
#![allow(dead_code)]

use tonic::Request;
use super::admin_proto::admin_service_server::AdminServiceServer;
use super::audit_proto::audit_service_server::AuditServiceServer;
use super::auth_proto::auth_service_server::AuthServiceServer;
use super::authorization_proto::authorization_service_server::AuthorizationServiceServer;
use super::organization_proto::organization_service_server::OrganizationServiceServer;
use super::*;

pub(crate) struct AuthGrpcService;
pub(crate) struct AuthorizationGrpcService;
pub(crate) struct OrganizationGrpcService;
pub(crate) struct AdminGrpcService;
pub(crate) struct AuditGrpcService;

pub(crate) const MAX_GRPC_MESSAGE_BYTES: usize = 256 * 1024;
pub(crate) const AUDIT_STREAM_WINDOW_NANOS: u64 = 5 * 60 * 1_000_000_000;

pub fn is_grpc_request(req: &spin_sdk::http::Request) -> bool {
    if req.uri().path().starts_with("/auth.v1.AuthService/")
        || req
            .uri()
            .path()
            .starts_with("/authorization.v1.AuthorizationService/")
        || req
            .uri()
            .path()
            .starts_with("/organization.v1.OrganizationService/")
        || req.uri().path().starts_with("/admin.v1.AdminService/")
        || req.uri().path().starts_with("/audit.v1.AuditService/")
    {
        return true;
    }

    req.headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/grpc"))
}

pub async fn serve(
    req: spin_sdk::http::Request,
) -> Result<wasip3::http::types::Response, wasip3::http::types::ErrorCode> {
    let path = req.uri().path().to_string();
    let response = if path.starts_with("/authorization.v1.AuthorizationService/") {
        spin_sdk::http::grpc::serve(
            AuthorizationServiceServer::new(AuthorizationGrpcService)
                .max_decoding_message_size(MAX_GRPC_MESSAGE_BYTES)
                .max_encoding_message_size(MAX_GRPC_MESSAGE_BYTES),
            req,
        )
        .await
    } else if path.starts_with("/organization.v1.OrganizationService/") {
        spin_sdk::http::grpc::serve(
            OrganizationServiceServer::new(OrganizationGrpcService)
                .max_decoding_message_size(MAX_GRPC_MESSAGE_BYTES)
                .max_encoding_message_size(MAX_GRPC_MESSAGE_BYTES),
            req,
        )
        .await
    } else if path.starts_with("/admin.v1.AdminService/") {
        spin_sdk::http::grpc::serve(
            AdminServiceServer::new(AdminGrpcService)
                .max_decoding_message_size(MAX_GRPC_MESSAGE_BYTES)
                .max_encoding_message_size(MAX_GRPC_MESSAGE_BYTES),
            req,
        )
        .await
    } else if path.starts_with("/audit.v1.AuditService/") {
        spin_sdk::http::grpc::serve(
            AuditServiceServer::new(AuditGrpcService)
                .max_decoding_message_size(MAX_GRPC_MESSAGE_BYTES)
                .max_encoding_message_size(MAX_GRPC_MESSAGE_BYTES),
            req,
        )
        .await
    } else {
        spin_sdk::http::grpc::serve(
            AuthServiceServer::new(AuthGrpcService)
                .max_decoding_message_size(MAX_GRPC_MESSAGE_BYTES)
                .max_encoding_message_size(MAX_GRPC_MESSAGE_BYTES),
            req,
        )
        .await
    };
    let mut response = if path == "/audit.v1.AuditService/WatchAuditEvents" {
        wasi_auth::spin_grpc::normalize_trailers_only_response(response)
    } else {
        wasi_auth::spin_grpc::normalize_trailers_only_response_awaiting_first_frame(response).await
    };
    wasi_auth::http::apply_response_security(
        &mut response,
        wasi_auth::http::ResponseSecurityPolicy::Sensitive,
        crate::application::session_cookie_secure_enabled().await,
    );
    wasi_auth::spin_grpc::into_final_wasi_response(response)
}

