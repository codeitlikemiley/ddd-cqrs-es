use std::collections::BTreeMap;

use spin_sdk::http::IntoResponse;
use tonic::{Request, Response, Status};

pub mod auth_proto {
    tonic::include_proto!("auth.v1");
}

pub mod authz_proto {
    tonic::include_proto!("authz.v1");
}

use auth_proto::auth_service_server::{AuthService, AuthServiceServer};
use authz_proto::authz_service_server::{AuthzService, AuthzServiceServer};

struct AuthGrpcService;
struct AuthzGrpcService;

pub fn is_grpc_request(req: &spin_sdk::http::Request) -> bool {
    if req.uri().path().starts_with("/auth.v1.AuthService/")
        || req.uri().path().starts_with("/authz.v1.AuthzService/")
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
    let response = if path.starts_with("/authz.v1.AuthzService/") {
        spin_sdk::http::grpc::serve(AuthzServiceServer::new(AuthzGrpcService), req).await
    } else {
        spin_sdk::http::grpc::serve(AuthServiceServer::new(AuthGrpcService), req).await
    };
    response.into_response()
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
        let request = request.into_inner();
        let response =
            crate::application::start_passkey_registration(crate::contracts::PasskeyStartRequest {
                email: empty_to_option(request.email),
                redirect_url: empty_to_option(request.redirect_url),
            })
            .await
            .map_err(|error| status_from_app_error("StartPasskeyRegistration", error))?;
        Ok(Response::new(response.into()))
    }

    async fn verify_passkey_registration(
        &self,
        request: Request<auth_proto::PasskeyVerifyRequest>,
    ) -> Result<Response<auth_proto::LoginCompletionResponse>, Status> {
        let request = request.into_inner();
        let response = crate::application::verify_passkey_registration(
            crate::contracts::PasskeyVerifyRequest {
                challenge_id: request.challenge_id,
                credential_json: request.credential_json,
                redirect_url: empty_to_option(request.redirect_url),
            },
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

    async fn list_signing_keys(
        &self,
        request: Request<auth_proto::ListSigningKeysRequest>,
    ) -> Result<Response<auth_proto::SigningKeyListResponse>, Status> {
        let response =
            crate::application::list_signing_keys(empty_to_option(request.into_inner().admin_token))
                .await
                .map_err(|error| status_from_app_error("ListSigningKeys", error))?;
        Ok(Response::new(response.into()))
    }

    async fn rotate_signing_key(
        &self,
        request: Request<auth_proto::SigningKeyRotateRequest>,
    ) -> Result<Response<auth_proto::SigningKeyRotateResponse>, Status> {
        let request = request.into_inner();
        let response = crate::application::rotate_signing_key(
            crate::contracts::SigningKeyRotateRequest {
                admin_token: empty_to_option(request.admin_token),
                kid: request.kid,
                retire_previous: Some(request.retire_previous),
            },
        )
        .await
        .map_err(|error| status_from_app_error("RotateSigningKey", error))?;
        Ok(Response::new(response.into()))
    }
}

#[tonic::async_trait]
impl AuthzService for AuthzGrpcService {
    async fn check(
        &self,
        request: Request<authz_proto::CheckRequest>,
    ) -> Result<Response<authz_proto::CheckResponse>, Status> {
        let response = crate::application::check_authorization(request.into_inner().into())
            .await
            .map_err(|error| status_from_app_error("Check", error))?;
        Ok(Response::new(response.into()))
    }

    async fn batch_check(
        &self,
        request: Request<authz_proto::BatchCheckRequest>,
    ) -> Result<Response<authz_proto::BatchCheckResponse>, Status> {
        let response = crate::application::batch_check_authorization(
            crate::contracts::AuthzBatchCheckRequest {
                checks: request
                    .into_inner()
                    .checks
                    .into_iter()
                    .map(Into::into)
                    .collect(),
            },
        )
        .await
        .map_err(|error| status_from_app_error("BatchCheck", error))?;
        Ok(Response::new(response.into()))
    }

    async fn list_objects(
        &self,
        request: Request<authz_proto::ListObjectsRequest>,
    ) -> Result<Response<authz_proto::ListObjectsResponse>, Status> {
        let response = crate::application::list_authorized_objects(request.into_inner().into())
            .await
            .map_err(|error| status_from_app_error("ListObjects", error))?;
        Ok(Response::new(response.into()))
    }

    async fn expand(
        &self,
        request: Request<authz_proto::ExpandRequest>,
    ) -> Result<Response<authz_proto::ExpandResponse>, Status> {
        let response = crate::application::expand_authorization(request.into_inner().into())
            .await
            .map_err(|error| status_from_app_error("Expand", error))?;
        Ok(Response::new(response.into()))
    }

    async fn write_authorization_model(
        &self,
        request: Request<authz_proto::WriteAuthorizationModelRequest>,
    ) -> Result<Response<authz_proto::WriteAuthorizationModelResponse>, Status> {
        let request = request.into_inner();
        let response = crate::application::write_authorization_model(
            crate::contracts::AuthzModelWriteRequest {
                model_id: request.model_id,
                schema_json: request.schema_json,
            },
        )
        .await
        .map_err(|error| status_from_app_error("WriteAuthorizationModel", error))?;
        Ok(Response::new(response.into()))
    }

    async fn activate_authorization_model(
        &self,
        request: Request<authz_proto::ActivateAuthorizationModelRequest>,
    ) -> Result<Response<authz_proto::WriteAuthorizationModelResponse>, Status> {
        let response =
            crate::application::activate_authorization_model(request.into_inner().model_id)
                .await
                .map_err(|error| status_from_app_error("ActivateAuthorizationModel", error))?;
        Ok(Response::new(response.into()))
    }

    async fn read_authorization_model(
        &self,
        request: Request<authz_proto::ReadAuthorizationModelRequest>,
    ) -> Result<Response<authz_proto::ReadAuthorizationModelResponse>, Status> {
        let model_id = request.into_inner().model_id;
        let response = crate::application::read_authorization_model(model_id)
            .await
            .map_err(|error| status_from_app_error("ReadAuthorizationModel", error))?;
        Ok(Response::new(response.into()))
    }

    async fn write_relationship_tuples(
        &self,
        request: Request<authz_proto::RelationshipTupleWriteRequest>,
    ) -> Result<Response<authz_proto::RelationshipTupleWriteResponse>, Status> {
        let response = crate::application::write_relationship_tuples(
            crate::contracts::RelationshipTupleWriteRequest {
                tuples_json: request.into_inner().tuples_json,
            },
        )
        .await
        .map_err(|error| status_from_app_error("WriteRelationshipTuples", error))?;
        Ok(Response::new(response.into()))
    }

    async fn delete_relationship_tuples(
        &self,
        request: Request<authz_proto::RelationshipTupleWriteRequest>,
    ) -> Result<Response<authz_proto::RelationshipTupleWriteResponse>, Status> {
        let response = crate::application::delete_relationship_tuples(
            crate::contracts::RelationshipTupleWriteRequest {
                tuples_json: request.into_inner().tuples_json,
            },
        )
        .await
        .map_err(|error| status_from_app_error("DeleteRelationshipTuples", error))?;
        Ok(Response::new(response.into()))
    }

    async fn read_relationship_tuples(
        &self,
        request: Request<authz_proto::ReadRelationshipTuplesRequest>,
    ) -> Result<Response<authz_proto::ReadRelationshipTuplesResponse>, Status> {
        let request = request.into_inner();
        let tuples_json = crate::application::read_relationship_tuples(
            request.tenant,
            request.object,
            request.relation,
        )
        .await
        .map_err(|error| status_from_app_error("ReadRelationshipTuples", error))?;
        Ok(Response::new(authz_proto::ReadRelationshipTuplesResponse {
            tuples_json,
        }))
    }
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
            reset_url: value.reset_url.unwrap_or_default(),
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

impl From<ddd_auth::JwksDocument> for auth_proto::JwksDocument {
    fn from(value: ddd_auth::JwksDocument) -> Self {
        Self {
            keys: value.keys.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ddd_auth::JwksKey> for auth_proto::JwksKey {
    fn from(value: ddd_auth::JwksKey) -> Self {
        Self {
            kid: value.kid,
            kty: value.kty,
            alg: value.alg,
            r#use: value.use_,
            public_parameters: value.public_parameters.into_iter().collect(),
        }
    }
}

impl From<crate::contracts::SigningKeyListResponse> for auth_proto::SigningKeyListResponse {
    fn from(value: crate::contracts::SigningKeyListResponse) -> Self {
        Self {
            keys: value.keys.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::SigningKeyRotateResponse> for auth_proto::SigningKeyRotateResponse {
    fn from(value: crate::contracts::SigningKeyRotateResponse) -> Self {
        Self {
            active_kid: value.active_kid,
            previous_kid: value.previous_kid.unwrap_or_default(),
            retired_previous: value.retired_previous,
            keys: value.keys.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::SigningKeySummary> for auth_proto::SigningKeySummary {
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

impl From<authz_proto::CheckRequest> for crate::contracts::AuthzCheckRequest {
    fn from(value: authz_proto::CheckRequest) -> Self {
        Self {
            tenant: value.tenant,
            subject: value.subject,
            object: value.object,
            relation: value.relation,
            context: value.context.into_iter().collect(),
        }
    }
}

impl From<crate::contracts::AuthzCheckResponse> for authz_proto::CheckResponse {
    fn from(value: crate::contracts::AuthzCheckResponse) -> Self {
        Self {
            allowed: value.allowed,
            reason: value.reason,
            model_id: value.model_id,
        }
    }
}

impl From<crate::contracts::AuthzBatchCheckResponse> for authz_proto::BatchCheckResponse {
    fn from(value: crate::contracts::AuthzBatchCheckResponse) -> Self {
        Self {
            results: value.results.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<authz_proto::ListObjectsRequest> for crate::contracts::AuthzListObjectsRequest {
    fn from(value: authz_proto::ListObjectsRequest) -> Self {
        Self {
            tenant: value.tenant,
            subject: value.subject,
            relation: value.relation,
            object_type: value.object_type,
            context: BTreeMap::new(),
        }
    }
}

impl From<crate::contracts::AuthzListObjectsResponse> for authz_proto::ListObjectsResponse {
    fn from(value: crate::contracts::AuthzListObjectsResponse) -> Self {
        Self {
            objects: value.objects,
        }
    }
}

impl From<authz_proto::ExpandRequest> for crate::contracts::AuthzExpandRequest {
    fn from(value: authz_proto::ExpandRequest) -> Self {
        Self {
            tenant: value.tenant,
            object: value.object,
            relation: value.relation,
            context: BTreeMap::new(),
        }
    }
}

impl From<crate::contracts::AuthzExpandResponse> for authz_proto::ExpandResponse {
    fn from(value: crate::contracts::AuthzExpandResponse) -> Self {
        Self {
            graph_json: value.graph_json,
        }
    }
}

impl From<crate::contracts::AuthzModelReadResponse>
    for authz_proto::ReadAuthorizationModelResponse
{
    fn from(value: crate::contracts::AuthzModelReadResponse) -> Self {
        Self {
            model_id: value.model_id,
            schema_json: value.schema_json,
            active: value.active,
        }
    }
}

impl From<crate::contracts::AuthzModelWriteResponse>
    for authz_proto::WriteAuthorizationModelResponse
{
    fn from(value: crate::contracts::AuthzModelWriteResponse) -> Self {
        Self {
            model_id: value.model_id,
            active: value.active,
        }
    }
}

impl From<crate::contracts::RelationshipTupleWriteResponse>
    for authz_proto::RelationshipTupleWriteResponse
{
    fn from(value: crate::contracts::RelationshipTupleWriteResponse) -> Self {
        Self {
            accepted: value.accepted,
        }
    }
}
