use std::collections::VecDeque;
use std::pin::Pin;

use futures::Stream;
use tonic::{Request, Response, Status};
use wasi_auth::authentication::jwt;

pub mod auth_proto {
    tonic::include_proto!("auth.v1");
}

pub mod authorization_proto {
    tonic::include_proto!("authorization.v1");
}

pub mod organization_proto {
    tonic::include_proto!("organization.v1");
}

pub mod admin_proto {
    tonic::include_proto!("admin.v1");
}

pub mod audit_proto {
    tonic::include_proto!("audit.v1");
}

use admin_proto::admin_service_server::{AdminService, AdminServiceServer};
use audit_proto::audit_service_server::{AuditService, AuditServiceServer};
use auth_proto::auth_service_server::{AuthService, AuthServiceServer};
use authorization_proto::authorization_service_server::{
    AuthorizationService, AuthorizationServiceServer,
};
use organization_proto::organization_service_server::{
    OrganizationService, OrganizationServiceServer,
};

struct AuthGrpcService;
struct AuthorizationGrpcService;
struct OrganizationGrpcService;
struct AdminGrpcService;
struct AuditGrpcService;

const MAX_GRPC_MESSAGE_BYTES: usize = 256 * 1024;
const AUDIT_STREAM_WINDOW_NANOS: u64 = 5 * 60 * 1_000_000_000;

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

#[tonic::async_trait]
impl AuthService for AuthGrpcService {
    async fn get_capabilities(
        &self,
        _request: Request<auth_proto::GetCapabilitiesRequest>,
    ) -> Result<Response<auth_proto::AuthCapabilitiesResponse>, Status> {
        let capabilities = crate::application::auth_capabilities()
            .await
            .map_err(|error| status_from_app_error("GetCapabilities", error))?;
        Ok(Response::new(capabilities.into()))
    }

    async fn list_providers(
        &self,
        _request: Request<auth_proto::ListProvidersRequest>,
    ) -> Result<Response<auth_proto::ListProvidersResponse>, Status> {
        let providers = crate::application::list_auth_providers()
            .await
            .map_err(|error| status_from_app_error("ListProviders", error))?
            .into_iter()
            .map(Into::into)
            .collect();
        Ok(Response::new(auth_proto::ListProvidersResponse {
            providers,
        }))
    }

    async fn register_password(
        &self,
        request: Request<auth_proto::EmailPasswordRegisterRequest>,
    ) -> Result<Response<auth_proto::LoginCompletionResponse>, Status> {
        let request = request.into_inner();
        let response = crate::application::register_email_password(
            crate::contracts::EmailPasswordRegisterRequest {
                email: request.email,
                password: request.password,
                redirect_url: empty_to_option(request.redirect_url),
            },
        )
        .await
        .map_err(|error| status_from_app_error("RegisterPassword", error))?;
        Ok(Response::new(response.into()))
    }

    async fn login_password(
        &self,
        request: Request<auth_proto::EmailPasswordLoginRequest>,
    ) -> Result<Response<auth_proto::LoginCompletionResponse>, Status> {
        let request = request.into_inner();
        let response =
            crate::application::login_email_password(crate::contracts::EmailPasswordLoginRequest {
                email: request.email,
                password: request.password,
                redirect_url: empty_to_option(request.redirect_url),
            })
            .await
            .map_err(|error| status_from_app_error("LoginPassword", error))?;
        Ok(Response::new(response.into()))
    }

    async fn complete_email_verification(
        &self,
        request: Request<auth_proto::EmailVerificationCompleteRequest>,
    ) -> Result<Response<auth_proto::LoginCompletionResponse>, Status> {
        let request = request.into_inner();
        let response = crate::application::complete_email_verification(
            crate::contracts::EmailVerificationCompleteRequest {
                token: request.token,
                redirect_url: empty_to_option(request.redirect_url),
            },
        )
        .await
        .map_err(|error| status_from_app_error("CompleteEmailVerification", error))?;
        Ok(Response::new(response.into()))
    }

    async fn resend_email_verification(
        &self,
        request: Request<auth_proto::EmailVerificationResendRequest>,
    ) -> Result<Response<auth_proto::AcceptedResponse>, Status> {
        let request = request.into_inner();
        let response = crate::application::resend_email_verification(
            crate::contracts::EmailVerificationResendRequest {
                email: request.email,
                redirect_url: empty_to_option(request.redirect_url),
            },
        )
        .await
        .map_err(|error| status_from_app_error("ResendEmailVerification", error))?;
        Ok(Response::new(auth_proto::AcceptedResponse {
            accepted: response.accepted,
        }))
    }

    async fn start_password_reset(
        &self,
        request: Request<auth_proto::PasswordResetStartRequest>,
    ) -> Result<Response<auth_proto::PasswordResetStartResponse>, Status> {
        let request = request.into_inner();
        let response =
            crate::application::start_password_reset(crate::contracts::PasswordResetStartRequest {
                email: request.email,
                redirect_url: empty_to_option(request.redirect_url),
            })
            .await
            .map_err(|error| status_from_app_error("StartPasswordReset", error))?;
        Ok(Response::new(response.into()))
    }

    async fn complete_password_reset(
        &self,
        request: Request<auth_proto::PasswordResetCompleteRequest>,
    ) -> Result<Response<auth_proto::LoginCompletionResponse>, Status> {
        let request = request.into_inner();
        let response = crate::application::complete_password_reset(
            crate::contracts::PasswordResetCompleteRequest {
                token: request.token,
                password: request.password,
                redirect_url: empty_to_option(request.redirect_url),
            },
        )
        .await
        .map_err(|error| status_from_app_error("CompletePasswordReset", error))?;
        Ok(Response::new(response.into()))
    }

    async fn start_o_auth_login(
        &self,
        request: Request<auth_proto::StartOAuthLoginRequest>,
    ) -> Result<Response<auth_proto::OAuthStartResponse>, Status> {
        let request = request.into_inner();
        let response = crate::application::start_oauth_login(
            request.provider_id,
            empty_to_option(request.redirect_url),
        )
        .await
        .map_err(|error| status_from_app_error("StartOAuthLogin", error))?;
        Ok(Response::new(response.into()))
    }

    async fn complete_o_auth_callback(
        &self,
        request: Request<auth_proto::CompleteOAuthCallbackRequest>,
    ) -> Result<Response<auth_proto::LoginCompletionResponse>, Status> {
        let request = request.into_inner();
        let response =
            crate::application::complete_oauth_callback(crate::contracts::OAuthCallbackRequest {
                provider_id: request.provider_id,
                code: empty_to_option(request.code),
                state: empty_to_option(request.state),
                redirect_url: empty_to_option(request.redirect_url),
            })
            .await
            .map_err(|error| status_from_app_error("CompleteOAuthCallback", error))?;
        Ok(Response::new(response.into()))
    }

    async fn start_passkey_registration(
        &self,
        request: Request<auth_proto::PasskeyStartRequest>,
    ) -> Result<Response<auth_proto::PasskeyStartResponse>, Status> {
        let auth = request_auth(&request);
        let request = request.into_inner();
        let response = crate::application::start_passkey_registration(
            crate::contracts::PasskeyStartRequest {
                email: empty_to_option(request.email),
                redirect_url: empty_to_option(request.redirect_url),
            },
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("StartPasskeyRegistration", error))?;
        Ok(Response::new(response.into()))
    }

    async fn verify_passkey_registration(
        &self,
        request: Request<auth_proto::PasskeyVerifyRequest>,
    ) -> Result<Response<auth_proto::LoginCompletionResponse>, Status> {
        let auth = request_auth(&request);
        let request = request.into_inner();
        let response = crate::application::verify_passkey_registration(
            crate::contracts::PasskeyVerifyRequest {
                challenge_id: request.challenge_id,
                credential_json: request.credential_json,
                redirect_url: empty_to_option(request.redirect_url),
            },
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("VerifyPasskeyRegistration", error))?;
        Ok(Response::new(response.into()))
    }

    async fn start_passkey_login(
        &self,
        request: Request<auth_proto::PasskeyStartRequest>,
    ) -> Result<Response<auth_proto::PasskeyStartResponse>, Status> {
        let request = request.into_inner();
        let response =
            crate::application::start_passkey_login(crate::contracts::PasskeyStartRequest {
                email: empty_to_option(request.email),
                redirect_url: empty_to_option(request.redirect_url),
            })
            .await
            .map_err(|error| status_from_app_error("StartPasskeyLogin", error))?;
        Ok(Response::new(response.into()))
    }

    async fn verify_passkey_login(
        &self,
        request: Request<auth_proto::PasskeyVerifyRequest>,
    ) -> Result<Response<auth_proto::LoginCompletionResponse>, Status> {
        let request = request.into_inner();
        let response =
            crate::application::verify_passkey_login(crate::contracts::PasskeyVerifyRequest {
                challenge_id: request.challenge_id,
                credential_json: request.credential_json,
                redirect_url: empty_to_option(request.redirect_url),
            })
            .await
            .map_err(|error| status_from_app_error("VerifyPasskeyLogin", error))?;
        Ok(Response::new(response.into()))
    }

    async fn get_session(
        &self,
        request: Request<auth_proto::GetSessionRequest>,
    ) -> Result<Response<auth_proto::SessionView>, Status> {
        let session = crate::application::get_current_session_for(empty_to_option(
            request.into_inner().session_id,
        ))
        .await
        .map_err(|error| status_from_app_error("GetSession", error))?;
        Ok(Response::new(session.into()))
    }

    async fn refresh_token(
        &self,
        request: Request<auth_proto::RefreshTokenRequest>,
    ) -> Result<Response<auth_proto::TokenRefreshResponse>, Status> {
        let request = request.into_inner();
        let response = crate::application::refresh_token_for(
            empty_to_option(request.session_id),
            empty_to_option(request.refresh_token),
        )
        .await
        .map_err(|error| status_from_app_error("RefreshToken", error))?;
        Ok(Response::new(response.into()))
    }

    async fn verify_token(
        &self,
        request: Request<auth_proto::TokenVerifyRequest>,
    ) -> Result<Response<auth_proto::TokenVerifyResponse>, Status> {
        let response =
            crate::application::verify_access_token(crate::contracts::TokenVerifyRequest {
                access_token: request.into_inner().access_token,
            })
            .await
            .map_err(|error| status_from_app_error("VerifyToken", error))?;
        Ok(Response::new(response.into()))
    }

    async fn change_password(
        &self,
        request: Request<auth_proto::ChangePasswordRequest>,
    ) -> Result<Response<auth_proto::AcceptedResponse>, Status> {
        let auth = request_auth(&request);
        let request = request.into_inner();
        let response = crate::application::change_password(
            crate::contracts::PasswordChangeRequest {
                current_password: request.current_password,
                new_password: request.new_password,
            },
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("ChangePassword", error))?;
        Ok(Response::new(auth_proto::AcceptedResponse {
            accepted: response.accepted,
        }))
    }

    async fn list_sessions(
        &self,
        request: Request<auth_proto::ListSessionsRequest>,
    ) -> Result<Response<auth_proto::SessionListResponse>, Status> {
        let response = crate::application::list_sessions(request_auth(&request))
            .await
            .map_err(|error| status_from_app_error("ListSessions", error))?;
        Ok(Response::new(response.into()))
    }

    async fn revoke_session(
        &self,
        request: Request<auth_proto::RevokeSessionRequest>,
    ) -> Result<Response<auth_proto::AcceptedResponse>, Status> {
        let auth = request_auth(&request);
        let response = crate::application::revoke_account_session(
            crate::contracts::SessionRevokeRequest {
                session_id: request.into_inner().session_id,
            },
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("RevokeSession", error))?;
        Ok(Response::new(auth_proto::AcceptedResponse {
            accepted: response.accepted,
        }))
    }

    async fn get_mfa_status(
        &self,
        request: Request<auth_proto::GetMfaStatusRequest>,
    ) -> Result<Response<auth_proto::MfaStatusResponse>, Status> {
        let response = crate::application::mfa_status(request_auth(&request))
            .await
            .map_err(|error| status_from_app_error("GetMfaStatus", error))?;
        Ok(Response::new(auth_proto::MfaStatusResponse {
            totp_enrolled: response.totp_enrolled,
            recovery_codes_remaining: response.recovery_codes_remaining,
            assurance: response.assurance,
        }))
    }

    async fn start_totp_enrollment(
        &self,
        request: Request<auth_proto::StartTotpEnrollmentRequest>,
    ) -> Result<Response<auth_proto::MfaEnrollStartResponse>, Status> {
        let response = crate::application::start_totp_enrollment(request_auth(&request))
            .await
            .map_err(|error| status_from_app_error("StartTotpEnrollment", error))?;
        Ok(Response::new(auth_proto::MfaEnrollStartResponse {
            credential_id: response.credential_id,
            secret_base32: response.secret_base32,
            provisioning_uri: response.provisioning_uri,
        }))
    }

    async fn confirm_totp_enrollment(
        &self,
        request: Request<auth_proto::MfaCodeRequest>,
    ) -> Result<Response<auth_proto::MfaEnrollConfirmResponse>, Status> {
        let auth = request_auth(&request);
        let response = crate::application::confirm_totp_enrollment(
            crate::contracts::MfaCodeRequest {
                code: request.into_inner().code,
            },
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("ConfirmTotpEnrollment", error))?;
        Ok(Response::new(auth_proto::MfaEnrollConfirmResponse {
            recovery_codes: response.recovery_codes,
            assurance: response.assurance,
        }))
    }

    async fn verify_totp_step_up(
        &self,
        request: Request<auth_proto::MfaCodeRequest>,
    ) -> Result<Response<auth_proto::SessionView>, Status> {
        let auth = request_auth(&request);
        let response = crate::application::verify_totp_step_up(
            crate::contracts::MfaCodeRequest {
                code: request.into_inner().code,
            },
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("VerifyTotpStepUp", error))?;
        Ok(Response::new(response.into()))
    }

    async fn verify_recovery_code(
        &self,
        request: Request<auth_proto::MfaCodeRequest>,
    ) -> Result<Response<auth_proto::SessionView>, Status> {
        let auth = request_auth(&request);
        let response = crate::application::use_recovery_code_for_step_up(
            crate::contracts::MfaCodeRequest {
                code: request.into_inner().code,
            },
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("VerifyRecoveryCode", error))?;
        Ok(Response::new(response.into()))
    }

    async fn logout(
        &self,
        request: Request<auth_proto::LogoutRequest>,
    ) -> Result<Response<auth_proto::LogoutResponse>, Status> {
        let response =
            crate::application::logout_session(empty_to_option(request.into_inner().session_id))
                .await
                .map_err(|error| status_from_app_error("Logout", error))?;
        Ok(Response::new(response.into()))
    }

    async fn get_jwks(
        &self,
        _request: Request<auth_proto::GetJwksRequest>,
    ) -> Result<Response<auth_proto::JwksDocument>, Status> {
        let response = crate::application::get_jwks()
            .await
            .map_err(|error| status_from_app_error("GetJwks", error))?;
        Ok(Response::new(response.into()))
    }
}

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

#[tonic::async_trait]
impl AuditService for AuditGrpcService {
    async fn list_audit_events(
        &self,
        request: Request<audit_proto::ListAuditEventsRequest>,
    ) -> Result<Response<audit_proto::AuditEventListResponse>, Status> {
        let auth = request_auth(&request);
        let request = request.into_inner();
        let response = crate::application::list_audit_events(
            empty_to_option(request.organization_id),
            request.after_cursor,
            usize::try_from(request.limit.clamp(1, 100)).unwrap_or(100),
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("Audit.ListAuditEvents", error))?;
        Ok(Response::new(response.into()))
    }

    type WatchAuditEventsStream =
        Pin<Box<dyn Stream<Item = Result<audit_proto::AuditEvent, Status>> + Send>>;

    async fn watch_audit_events(
        &self,
        request: Request<audit_proto::WatchAuditEventsRequest>,
    ) -> Result<Response<Self::WatchAuditEventsStream>, Status> {
        let auth = request_auth(&request);
        let request = request.into_inner();
        crate::application::list_audit_events(
            empty_to_option(request.organization_id.clone()),
            request.after_cursor,
            1,
            auth.clone(),
        )
        .await
        .map_err(|error| status_from_app_error("Audit.WatchAuditEvents.authorize", error))?;
        let state = AuditWatchState {
            organization_id: empty_to_option(request.organization_id),
            cursor: request.after_cursor,
            buffered: VecDeque::new(),
            auth,
            started_at: wasip3::clocks::monotonic_clock::now(),
            terminated: false,
        };
        let stream = futures::stream::unfold(state, |mut state| async move {
            if state.terminated
                || wasip3::clocks::monotonic_clock::now().saturating_sub(state.started_at)
                    >= AUDIT_STREAM_WINDOW_NANOS
            {
                return None;
            }
            loop {
                if let Some(event) = state.buffered.pop_front() {
                    if let Err(error) = crate::application::list_audit_events(
                        state.organization_id.clone(),
                        state.cursor,
                        1,
                        state.auth.for_revalidation(),
                    )
                    .await
                    {
                        state.terminated = true;
                        return Some((
                            Err(status_from_app_error(
                                "Audit.WatchAuditEvents.reauthorize",
                                error,
                            )),
                            state,
                        ));
                    }
                    state.cursor = event.sequence;
                    return Some((Ok(event.into()), state));
                }
                match crate::application::list_audit_events(
                    state.organization_id.clone(),
                    state.cursor,
                    100,
                    state.auth.for_revalidation(),
                )
                .await
                {
                    Ok(response) if response.events.is_empty() => {
                        wasip3::clocks::monotonic_clock::wait_for(250_000_000).await;
                        if wasip3::clocks::monotonic_clock::now().saturating_sub(state.started_at)
                            >= AUDIT_STREAM_WINDOW_NANOS
                        {
                            return None;
                        }
                    }
                    Ok(response) => state.buffered.extend(response.events),
                    Err(error) => {
                        state.terminated = true;
                        return Some((
                            Err(status_from_app_error("Audit.WatchAuditEvents.poll", error)),
                            state,
                        ));
                    }
                }
            }
        });
        Ok(Response::new(Box::pin(stream)))
    }
}

struct AuditWatchState {
    organization_id: Option<String>,
    cursor: u64,
    buffered: VecDeque<crate::contracts::AuditEventSummary>,
    auth: crate::application::RequestAuth,
    started_at: u64,
    terminated: bool,
}

fn status_from_app_error(operation: &'static str, error: crate::error::AuthStackError) -> Status {
    if error.is_client_error() {
        tracing::warn!(
            operation,
            error = %error,
            error_code = error.public_code(),
            grpc_code = ?error.grpc_code(),
            "auth gRPC request rejected"
        );
    } else {
        tracing::error!(
            operation,
            error = %error,
            error_code = error.public_code(),
            grpc_code = ?error.grpc_code(),
            "auth gRPC request failed"
        );
    }
    error.grpc_status()
}

fn empty_to_option(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

fn request_auth<T>(request: &Request<T>) -> crate::application::RequestAuth {
    if let Some(context) = request
        .extensions()
        .get::<wasi_auth::context::VerifiedRequestContext>()
    {
        return crate::application::RequestAuth::from_verified(context.clone());
    }
    let metadata = request.metadata();
    crate::application::RequestAuth::from_parts(
        None,
        metadata_text(metadata, "authorization").and_then(|value| bearer_token(&value)),
        metadata_text(metadata, "x-request-id"),
    )
}

fn metadata_text(metadata: &tonic::metadata::MetadataMap, name: &str) -> Option<String> {
    metadata
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn bearer_token(value: &str) -> Option<String> {
    value
        .trim()
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

impl From<crate::contracts::AuthProviderSummary> for auth_proto::AuthProvider {
    fn from(value: crate::contracts::AuthProviderSummary) -> Self {
        Self {
            provider_id: value.provider_id,
            display_name: value.display_name,
            login_url: value.login_url,
            enabled: value.enabled,
        }
    }
}

impl From<crate::contracts::AuthCapabilities> for auth_proto::AuthCapabilitiesResponse {
    fn from(value: crate::contracts::AuthCapabilities) -> Self {
        Self {
            password_enabled: value.password_enabled,
            oauth_enabled: value.oauth_enabled,
            passkeys_enabled: value.passkeys_enabled,
            providers: value.providers.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::OAuthStartResponse> for auth_proto::OAuthStartResponse {
    fn from(value: crate::contracts::OAuthStartResponse) -> Self {
        Self {
            provider_id: value.provider_id,
            authorization_url: value.authorization_url,
            state: value.state,
        }
    }
}

impl From<crate::contracts::LoginCompletionResponse> for auth_proto::LoginCompletionResponse {
    fn from(value: crate::contracts::LoginCompletionResponse) -> Self {
        Self {
            authenticated: value.authenticated,
            redirect_url: value.redirect_url,
            session_id: value.session_id.unwrap_or_default(),
            access_token: value.access_token.unwrap_or_default(),
            refresh_token: value.refresh_token.unwrap_or_default(),
            expires_in_seconds: value.expires_in_seconds,
        }
    }
}

impl From<crate::contracts::PasswordResetStartResponse> for auth_proto::PasswordResetStartResponse {
    fn from(value: crate::contracts::PasswordResetStartResponse) -> Self {
        Self {
            accepted: value.accepted,
            expires_in_seconds: value.expires_in_seconds,
        }
    }
}

impl From<crate::contracts::PasskeyStartResponse> for auth_proto::PasskeyStartResponse {
    fn from(value: crate::contracts::PasskeyStartResponse) -> Self {
        Self {
            challenge_id: value.challenge_id,
            public_key_options_json: value.public_key_options_json,
            redirect_url: value.redirect_url,
        }
    }
}

impl From<crate::contracts::SessionView> for auth_proto::SessionView {
    fn from(value: crate::contracts::SessionView) -> Self {
        Self {
            authenticated: value.authenticated,
            tenant_id: value.tenant_id.unwrap_or_default(),
            user_id: value.user_id.unwrap_or_default(),
            primary_email: value.primary_email.unwrap_or_default(),
            expires_at: value.expires_at.unwrap_or_default(),
            permissions: value.permissions,
            assurance: value.assurance,
            system_administrator: value.system_administrator,
            issued_at_unix_seconds: value.issued_at_unix_seconds.unwrap_or_default(),
            expires_at_unix_seconds: value.expires_at_unix_seconds.unwrap_or_default(),
            session_id: value.session_id.unwrap_or_default(),
        }
    }
}

impl From<crate::contracts::TokenRefreshResponse> for auth_proto::TokenRefreshResponse {
    fn from(value: crate::contracts::TokenRefreshResponse) -> Self {
        Self {
            access_token: value.access_token.unwrap_or_default(),
            refresh_token: value.refresh_token.unwrap_or_default(),
            expires_in_seconds: value.expires_in_seconds,
        }
    }
}

impl From<crate::contracts::TokenVerifyResponse> for auth_proto::TokenVerifyResponse {
    fn from(value: crate::contracts::TokenVerifyResponse) -> Self {
        Self {
            active: value.active,
            subject: value.subject,
            tenant_id: value.tenant_id.unwrap_or_default(),
            session_id: value.session_id.unwrap_or_default(),
            expires_at: value.expires_at,
            scopes: value.scopes,
            assurance: value.assurance,
            system_administrator: value.system_administrator,
            issued_at_unix_seconds: value.issued_at_unix_seconds,
        }
    }
}

impl From<crate::contracts::AccountSessionSummary> for auth_proto::AccountSession {
    fn from(value: crate::contracts::AccountSessionSummary) -> Self {
        Self {
            session_id: value.session_id,
            organization_id: value.organization_id.unwrap_or_default(),
            assurance: value.assurance,
            issued_at_ms: value.issued_at_ms,
            expires_at_ms: value.expires_at_ms,
            current: value.current,
        }
    }
}

impl From<crate::contracts::AccountSessionListResponse> for auth_proto::SessionListResponse {
    fn from(value: crate::contracts::AccountSessionListResponse) -> Self {
        Self {
            sessions: value.sessions.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::LogoutResponse> for auth_proto::LogoutResponse {
    fn from(value: crate::contracts::LogoutResponse) -> Self {
        Self {
            redirect_url: value.redirect_url,
        }
    }
}

impl From<jwt::JwksDocument> for auth_proto::JwksDocument {
    fn from(value: jwt::JwksDocument) -> Self {
        Self {
            keys: value.keys.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<jwt::JwksKey> for auth_proto::JwksKey {
    fn from(value: jwt::JwksKey) -> Self {
        Self {
            kid: value.kid,
            kty: value.kty,
            alg: value.alg,
            r#use: value.use_,
            public_parameters: value.public_parameters.into_iter().collect(),
        }
    }
}

impl From<crate::contracts::SigningKeyListResponse> for admin_proto::SigningKeyListResponse {
    fn from(value: crate::contracts::SigningKeyListResponse) -> Self {
        Self {
            keys: value.keys.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::SigningKeyRotateResponse> for admin_proto::SigningKeyRotateResponse {
    fn from(value: crate::contracts::SigningKeyRotateResponse) -> Self {
        Self {
            active_kid: value.active_kid,
            previous_kid: value.previous_kid.unwrap_or_default(),
            retired_previous: value.retired_previous,
            keys: value.keys.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::SigningKeySummary> for admin_proto::SigningKey {
    fn from(value: crate::contracts::SigningKeySummary) -> Self {
        Self {
            kid: value.kid,
            alg: value.alg,
            status: value.status,
            active: value.active,
            source: value.source,
            created_at_ms: value.created_at_ms.unwrap_or_default(),
            activated_at_ms: value.activated_at_ms.unwrap_or_default(),
            retired_at_ms: value.retired_at_ms.unwrap_or_default(),
            revoked_at_ms: value.revoked_at_ms.unwrap_or_default(),
        }
    }
}

impl From<crate::contracts::AuthProviderSummary> for admin_proto::Provider {
    fn from(value: crate::contracts::AuthProviderSummary) -> Self {
        Self {
            provider_id: value.provider_id,
            display_name: value.display_name,
            login_url: value.login_url,
            enabled: value.enabled,
        }
    }
}

impl From<crate::contracts::OrganizationSummary> for organization_proto::Organization {
    fn from(value: crate::contracts::OrganizationSummary) -> Self {
        Self {
            organization_id: value.organization_id,
            name: value.name,
            status: value.status,
            current_user_role: value.current_user_role,
            permissions: value.permissions,
            created_at_ms: value.created_at_ms,
        }
    }
}

impl From<crate::contracts::OrganizationListResponse>
    for organization_proto::OrganizationListResponse
{
    fn from(value: crate::contracts::OrganizationListResponse) -> Self {
        Self {
            organizations: value.organizations.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::SessionView> for organization_proto::SessionView {
    fn from(value: crate::contracts::SessionView) -> Self {
        Self {
            authenticated: value.authenticated,
            organization_id: value.tenant_id.unwrap_or_default(),
            user_id: value.user_id.unwrap_or_default(),
            primary_email: value.primary_email.unwrap_or_default(),
            permissions: value.permissions,
            assurance: value.assurance,
            system_administrator: value.system_administrator,
            session_id: value.session_id.unwrap_or_default(),
            issued_at_unix_seconds: value.issued_at_unix_seconds.unwrap_or_default(),
            expires_at_unix_seconds: value.expires_at_unix_seconds.unwrap_or_default(),
        }
    }
}

impl From<crate::contracts::MembershipSummary> for organization_proto::Membership {
    fn from(value: crate::contracts::MembershipSummary) -> Self {
        Self {
            organization_id: value.organization_id,
            user_id: value.user_id,
            primary_email: value.primary_email,
            role_id: value.role_id,
            status: value.status,
            joined_at_ms: value.joined_at_ms,
        }
    }
}

impl From<crate::contracts::MembershipListResponse> for organization_proto::MembershipListResponse {
    fn from(value: crate::contracts::MembershipListResponse) -> Self {
        Self {
            memberships: value.memberships.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::InvitationSummary> for organization_proto::Invitation {
    fn from(value: crate::contracts::InvitationSummary) -> Self {
        Self {
            invitation_id: value.invitation_id,
            organization_id: value.organization_id,
            email: value.email,
            role_id: value.role_id,
            status: value.status,
            expires_at_ms: value.expires_at_ms,
        }
    }
}

impl From<crate::contracts::InvitationListResponse> for organization_proto::InvitationListResponse {
    fn from(value: crate::contracts::InvitationListResponse) -> Self {
        Self {
            invitations: value.invitations.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::RoleSummary> for organization_proto::Role {
    fn from(value: crate::contracts::RoleSummary) -> Self {
        Self {
            organization_id: value.organization_id,
            role_id: value.role_id,
            name: value.name,
            built_in: value.built_in,
            permissions: value.permissions,
        }
    }
}

impl From<crate::contracts::RoleListResponse> for organization_proto::RoleListResponse {
    fn from(value: crate::contracts::RoleListResponse) -> Self {
        Self {
            roles: value.roles.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::AdminUserSummary> for admin_proto::User {
    fn from(value: crate::contracts::AdminUserSummary) -> Self {
        Self {
            user_id: value.user_id,
            primary_email: value.primary_email,
            disabled: value.disabled,
            email_verified: value.email_verified,
            created_at_ms: value.created_at_ms,
        }
    }
}

impl From<crate::contracts::AdminUserListResponse> for admin_proto::UserListResponse {
    fn from(value: crate::contracts::AdminUserListResponse) -> Self {
        Self {
            users: value.users.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::PolicyVersionSummary> for admin_proto::PolicyVersion {
    fn from(value: crate::contracts::PolicyVersionSummary) -> Self {
        Self {
            version_id: value.version_id,
            status: value.status,
            policy_hash: value.policy_hash,
            published_by: value.published_by,
            created_at_ms: value.created_at_ms,
        }
    }
}

impl From<crate::contracts::PolicyVersionListResponse> for admin_proto::PolicyVersionListResponse {
    fn from(value: crate::contracts::PolicyVersionListResponse) -> Self {
        Self {
            versions: value.versions.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::HealthStatusResponse> for admin_proto::HealthResponse {
    fn from(value: crate::contracts::HealthStatusResponse) -> Self {
        Self {
            status: value.status,
            storage_backend: value.storage_backend,
            mail_transport: value.mail_transport,
            authorization_provider: value.authorization_provider,
            production_mode: value.production_mode,
        }
    }
}

impl From<crate::contracts::AuditEventSummary> for audit_proto::AuditEvent {
    fn from(value: crate::contracts::AuditEventSummary) -> Self {
        Self {
            sequence: value.sequence,
            organization_id: value.organization_id.unwrap_or_default(),
            actor_user_id: value.actor_user_id,
            action: value.action,
            target_type: value.target_type,
            target_id: value.target_id,
            outcome: value.outcome,
            recorded_at_ms: value.recorded_at_ms,
        }
    }
}

impl From<crate::contracts::AuditEventListResponse> for audit_proto::AuditEventListResponse {
    fn from(value: crate::contracts::AuditEventListResponse) -> Self {
        Self {
            events: value.events.into_iter().map(Into::into).collect(),
            next_cursor: value.next_cursor,
        }
    }
}

impl From<authorization_proto::CheckRequest> for crate::contracts::AuthorizationCheckRequest {
    fn from(value: authorization_proto::CheckRequest) -> Self {
        Self {
            action: value.action,
            resource_type: value.resource_type,
            resource_id: value.resource_id,
            organization_id: empty_to_option(value.organization_id),
        }
    }
}

impl From<crate::contracts::AuthorizationCheckResponse> for authorization_proto::CheckResponse {
    fn from(value: crate::contracts::AuthorizationCheckResponse) -> Self {
        Self {
            allowed: value.allowed,
            reason: value.reason,
            policy_revision: value.policy_revision,
            consistency_token: value.consistency_token.unwrap_or_default(),
            resource_revision: value.resource_revision,
        }
    }
}

impl From<crate::contracts::AuthorizationBatchCheckResponse>
    for authorization_proto::BatchCheckResponse
{
    fn from(value: crate::contracts::AuthorizationBatchCheckResponse) -> Self {
        Self {
            results: value.results.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::AuthorizationCapabilitiesResponse>
    for authorization_proto::CapabilitiesResponse
{
    fn from(value: crate::contracts::AuthorizationCapabilitiesResponse) -> Self {
        Self {
            provider: value.provider,
            batch_check: value.batch_check,
            list_resources: value.list_resources,
            consistency_tokens: value.consistency_tokens,
            max_batch_checks: value.max_batch_checks,
        }
    }
}
