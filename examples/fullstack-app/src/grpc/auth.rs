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

