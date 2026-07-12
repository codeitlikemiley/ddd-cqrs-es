//! Thin Spin runtime adapter for product workflows owned by `wasi-auth`.

use std::{
    collections::VecDeque,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD}};
use sha2::{Digest, Sha256};
use wasi_auth::{
    authentication::jwt::JwksDocument,
    authentication::mfa::TotpConfig,
    authentication::passkeys::Attachment as PasskeyAttachment,
    authentication::{Clock, RandomSource},
    context::{AuthenticationAssurance, RequestId, SessionId, UserId},
    http::{AuthenticatedSession, TrustedContextCodec},
    postgres::{
        flows::FlowSealingKey,
        management::{
            AdminUserRecord, AuditEventRecord, InvitationRecord, InvitationService,
            ManagementError, MembershipRecord, OrganizationManagementService, RoleRecord,
            UpsertRoleRequest as ProductUpsertRoleRequest, ORGANIZATION_PERMISSION_CATALOG,
        },
        mfa::{MfaKeyMaterial, MfaService, MfaServiceError},
        oauth::{
            OAuthFlowService, OAuthProviderService, OAuthProviderServiceError,
            OAuthServiceConfig, OAuthServiceError, PendingOAuthFlow, VerifiedOAuthIdentity,
        },
        passkeys::{
            PasskeyConfigurationError, PasskeyService, PasskeyServiceConfig, PasskeyServiceError,
        },
        policy::{
            ActivePolicyBundle, PolicyBundleLoadError, PolicyBundleRecord, PolicyBundleService,
            PolicyBundleServiceError,
        },
        PostgresAuthStore, PostgresStoreError,
        organizations::{
            CreateOrganizationRequest, OrganizationError, OrganizationRecord, OrganizationService,
        },
        rate_limits::{RateLimitError, RateLimitService},
        sessions::{SessionService, SessionServiceError},
        signing::{SigningKeyRecord, SigningKeyService, SigningKeyServiceError},
        spin::{SpinPostgresError, SpinPostgresTransport},
        tokens::{
            AccessTokenVerifier, JwtKeyRing, RefreshSealingKey, TokenService, TokenServiceConfig,
            TokenServiceError, VerifiedAccessToken,
        },
    },
    postgres::workflows::{
        Argon2Policy, EmailVerificationError, EmailVerificationRequest,
        EmailVerificationResendRequest as ProductEmailVerificationResendRequest,
        EmailVerificationService, OutboxSealingKey,
        PasswordChangeRequest as ProductPasswordChangeRequest,
        PasswordLoginError, PasswordLoginRequest, PasswordLoginService,
        PasswordRegistrationError, PasswordRegistrationRequest, PasswordRegistrationService,
        PasswordResetCompleteRequest as ProductPasswordResetCompleteRequest, PasswordResetError,
        PasswordResetService, PasswordResetStartRequest as ProductPasswordResetStartRequest,
    },
};
#[cfg(feature = "mail-capture")]
use wasi_auth::mail::{CaptureMailer, EmailKind, Recipient};
#[cfg(any(feature = "mail-capture", all(feature = "mail-http", runtime_spin)))]
use wasi_auth::postgres::outbox::{MailOutboxWorker, PublicBaseUrl};
#[cfg(all(feature = "mail-http", runtime_spin))]
use wasi_auth::mail::{
    HttpMailBearerToken, HttpMailEndpoint, HttpMailTransport, HttpMailer,
};

use crate::{
    contracts::{
        AccountSessionListResponse, AccountSessionSummary, AdminUserListResponse,
        AdminUserSummary, AuditEventListResponse, AuditEventSummary, AuthProviderSummary,
        CapturedMailResponse,
        EmailPasswordLoginRequest, EmailPasswordRegisterRequest, EmailVerificationCompleteRequest,
        InvitationListResponse, InvitationSummary, LoginCompletionResponse, LogoutResponse,
        MembershipListResponse, MembershipSummary, MfaEnrollConfirmResponse,
        MfaEnrollStartResponse, MfaStatusResponse, OrganizationListResponse, OrganizationSummary,
        PasskeyStartResponse,
        PasswordResetCompleteRequest, PasswordResetStartRequest, PasswordResetStartResponse,
        PolicyVersionListResponse, PolicyVersionSummary, RoleListResponse, RoleSummary,
        SessionView, SigningKeyListResponse, SigningKeyRotateResponse, SigningKeySummary,
        TokenRefreshResponse, TokenVerifyResponse,
    },
    error::{AuthStackError, AuthStackResult},
};

const DEVELOPMENT_OUTBOX_KEY: &[u8] = b"fullstack-development-outbox-key";
const VERIFIED_TOKEN_CACHE_CAPACITY: usize = 256;

#[cfg(feature = "mail-capture")]
static CAPTURE_MAILER: OnceLock<CaptureMailer> = OnceLock::new();
static TOKEN_VERIFIER: OnceLock<RuntimeTokenVerifier> = OnceLock::new();
static TRUSTED_CONTEXT_CODEC: OnceLock<TrustedContextCodec> = OnceLock::new();
type VerifiedTokenCache = Mutex<VecDeque<([u8; 32], VerifiedAccessToken)>>;
static VERIFIED_TOKEN_CACHE: OnceLock<VerifiedTokenCache> = OnceLock::new();

pub use wasi_auth::postgres::oauth::PendingOAuthFlow as ProductPendingOAuthFlow;

pub struct OAuthStartValues {
    pub state: String,
    pub nonce: String,
    pub pkce_challenge: String,
}

#[cfg(all(feature = "mail-http", runtime_spin))]
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Spin outbound HTTP transport failed")]
struct SpinMailTransportError;

#[cfg(all(feature = "mail-http", runtime_spin))]
#[derive(Clone, Copy, Debug)]
struct SpinMailTransport;

#[cfg(all(feature = "mail-http", runtime_spin))]
impl HttpMailTransport for SpinMailTransport {
    type Error = SpinMailTransportError;

    async fn send(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Vec<u8>>, Self::Error> {
        use bytes::Bytes;
        use http_body_util::BodyExt as _;
        use spin_sdk::http::{FullBody, send};

        const MAX_RESPONSE_BYTES: usize = 16 * 1_024;
        let (parts, body) = request.into_parts();
        let request = http::Request::from_parts(parts, FullBody::new(Bytes::from(body)));
        let response = send(request).await.map_err(|_| SpinMailTransportError)?;
        if response
            .headers()
            .get(http::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok())
            .is_some_and(|length| length > MAX_RESPONSE_BYTES)
        {
            return Err(SpinMailTransportError);
        }
        let (parts, body) = response.into_parts();
        let body = body
            .collect()
            .await
            .map_err(|_| SpinMailTransportError)?
            .to_bytes();
        if body.len() > MAX_RESPONSE_BYTES {
            return Err(SpinMailTransportError);
        }
        Ok(http::Response::from_parts(parts, body.to_vec()))
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RuntimeClock;

impl Clock for RuntimeClock {
    fn now_unix_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("host cryptographic randomness is unavailable")]
pub(crate) struct RuntimeRandomError;

#[derive(Clone, Copy, Debug)]
pub(crate) struct RuntimeRandom;

impl RandomSource for RuntimeRandom {
    type Error = RuntimeRandomError;

    fn fill_bytes(&self, destination: &mut [u8]) -> Result<(), Self::Error> {
        #[cfg(target_arch = "wasm32")]
        {
            let bytes = wasip3::random::random::get_random_bytes(destination.len() as u64);
            if bytes.len() != destination.len() {
                return Err(RuntimeRandomError);
            }
            destination.copy_from_slice(&bytes);
            Ok(())
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            getrandom::getrandom(destination).map_err(|_| RuntimeRandomError)
        }
    }
}

pub async fn list_oauth_providers() -> AuthStackResult<Vec<AuthProviderSummary>> {
    oauth_provider_service()
        .await?
        .list()
        .await
        .map_err(map_oauth_provider_error)
        .map(|providers| {
            providers
                .into_iter()
                .map(|provider| AuthProviderSummary {
                    login_url: format!("/api/auth/oauth/{}/start", provider.provider_id),
                    provider_id: provider.provider_id,
                    display_name: provider.display_name,
                    enabled: provider.enabled,
                })
                .collect()
        })
}

pub async fn find_oauth_provider(
    provider_id: &str,
) -> AuthStackResult<Option<AuthProviderSummary>> {
    oauth_provider_service()
        .await?
        .get(provider_id)
        .await
        .map_err(map_oauth_provider_error)
        .map(|provider| {
            provider.map(|provider| AuthProviderSummary {
                login_url: format!("/api/auth/oauth/{}/start", provider.provider_id),
                provider_id: provider.provider_id,
                display_name: provider.display_name,
                enabled: provider.enabled,
            })
        })
}

pub async fn save_oauth_provider(
    session_id: &str,
    provider_id: &str,
    enabled: bool,
) -> AuthStackResult<AuthProviderSummary> {
    let session_id = bounded_session_id(session_id)?;
    let display_name = provider_id
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + characters.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ");
    oauth_provider_service()
        .await?
        .save(
            &session_id,
            provider_id,
            &display_name,
            enabled,
            &request_id("oauth-provider-save")?,
        )
        .await
        .map_err(map_oauth_provider_error)
        .map(|provider| AuthProviderSummary {
            login_url: format!("/api/auth/oauth/{}/start", provider.provider_id),
            provider_id: provider.provider_id,
            display_name: provider.display_name,
            enabled: provider.enabled,
        })
}

pub async fn replace_oauth_redirects(
    session_id: &str,
    redirects: &[String],
) -> AuthStackResult<Vec<String>> {
    let session_id = bounded_session_id(session_id)?;
    oauth_provider_service()
        .await?
        .replace_redirects(
            &session_id,
            redirects,
            &request_id("oauth-redirects-replace")?,
        )
        .await
        .map_err(map_oauth_provider_error)
}

pub async fn list_policy_versions() -> AuthStackResult<PolicyVersionListResponse> {
    let versions = policy_bundle_service()
        .await?
        .list(100)
        .await
        .map_err(map_policy_error)?
        .into_iter()
        .map(policy_version_summary)
        .collect();
    Ok(PolicyVersionListResponse { versions })
}

pub async fn list_signing_keys() -> AuthStackResult<SigningKeyListResponse> {
    let mut key_ring = configured_jwt_key_ring().await?;
    synchronize_signing_keys(&mut key_ring).await?;
    let keys = signing_key_service()
        .await?
        .list()
        .await
        .map_err(map_signing_key_error)?
        .into_iter()
        .map(signing_key_summary)
        .collect();
    Ok(SigningKeyListResponse { keys })
}

pub async fn rotate_signing_key(
    session_id: &str,
    key_id: &str,
    retire_previous: bool,
) -> AuthStackResult<SigningKeyRotateResponse> {
    let session_id = bounded_session_id(session_id)?;
    let mut key_ring = configured_jwt_key_ring().await?;
    synchronize_signing_keys(&mut key_ring).await?;
    let rotation = signing_key_service()
        .await?
        .rotate(
            &session_id,
            key_id,
            retire_previous,
            &request_id("signing-key-rotate")?,
        )
        .await
        .map_err(map_signing_key_error)?;
    let keys = signing_key_service()
        .await?
        .list()
        .await
        .map_err(map_signing_key_error)?
        .into_iter()
        .map(signing_key_summary)
        .collect();
    Ok(SigningKeyRotateResponse {
        active_kid: rotation.key.key_id,
        previous_kid: rotation.previous_key_id,
        retired_previous: retire_previous,
        keys,
    })
}

fn signing_key_summary(record: SigningKeyRecord) -> SigningKeySummary {
    SigningKeySummary {
        kid: record.key_id,
        alg: record.algorithm,
        active: record.status == "active",
        status: record.status,
        source: record.secret_reference,
        created_at_ms: Some(record.created_at_ms),
        activated_at_ms: record.activated_at_ms,
        retired_at_ms: record.retired_at_ms,
        revoked_at_ms: record.revoked_at_ms,
    }
}

pub async fn publish_policy_version(
    session_id: &str,
    policy_text: &str,
    schema_text: &str,
) -> AuthStackResult<PolicyVersionSummary> {
    let session_id = bounded_session_id(session_id)?;
    policy_bundle_service()
        .await?
        .publish(
            &session_id,
            schema_text,
            policy_text,
            serde_json::json!([]),
            &request_id("policy-publish")?,
        )
        .await
        .map(policy_version_summary)
        .map_err(map_policy_error)
}

pub async fn active_policy_bundle() -> AuthStackResult<Option<ActivePolicyBundle>> {
    store()
        .await?
        .load_active_policy_bundle()
        .await
        .map_err(map_policy_load_error)
}

fn policy_version_summary(record: PolicyBundleRecord) -> PolicyVersionSummary {
    PolicyVersionSummary {
        version_id: record.policy_revision,
        status: record.status,
        policy_hash: record.checksum_hex,
        published_by: record.created_by,
        created_at_ms: record.created_at_ms,
    }
}

pub async fn start_oauth_flow(
    provider_id: &str,
    redirect_path: &str,
) -> AuthStackResult<OAuthStartValues> {
    let (state, nonce, pkce_challenge) = oauth_flow_service()
        .await?
        .start(provider_id, redirect_path)
        .await
        .map_err(map_oauth_error)?
        .into_parts();
    Ok(OAuthStartValues {
        state,
        nonce,
        pkce_challenge,
    })
}

pub async fn load_oauth_callback(
    provider_id: &str,
    state: &str,
) -> AuthStackResult<PendingOAuthFlow> {
    oauth_flow_service()
        .await?
        .load_callback(provider_id, state)
        .await
        .map_err(map_oauth_error)
}

pub async fn complete_oauth_identity(
    pending: PendingOAuthFlow,
    identity: VerifiedOAuthIdentity,
) -> AuthStackResult<LoginCompletionResponse> {
    let completion = oauth_flow_service()
        .await?
        .complete(pending, identity, &request_id("oauth-complete")?)
        .await
        .map_err(map_oauth_error)?;
    let (access_token, refresh_token, expires_in_seconds) =
        issue_tokens(&completion.session_id).await?;
    Ok(LoginCompletionResponse {
        authenticated: true,
        redirect_url: completion.redirect_path,
        session_id: Some(completion.session_id.into_string()),
        access_token: Some(access_token),
        refresh_token: Some(refresh_token),
        expires_in_seconds,
    })
}

pub async fn start_passkey_login(
    email: &str,
    redirect_path: &str,
) -> AuthStackResult<PasskeyStartResponse> {
    let (challenge_id, public_key_options_json, redirect_url) = passkey_service()
        .await?
        .start_authentication(email, redirect_path)
        .await
        .map_err(map_passkey_error)?
        .into_parts();
    Ok(PasskeyStartResponse {
        challenge_id,
        public_key_options_json,
        redirect_url,
    })
}

pub async fn start_passkey_registration(
    session_id: &str,
    redirect_path: &str,
) -> AuthStackResult<PasskeyStartResponse> {
    let session_id = bounded_session_id(session_id)?;
    let (challenge_id, public_key_options_json, redirect_url) = passkey_service()
        .await?
        .start_registration(
            &session_id,
            &request_id("passkey-registration-start")?,
            redirect_path,
        )
        .await
        .map_err(map_passkey_error)?
        .into_parts();
    Ok(PasskeyStartResponse {
        challenge_id,
        public_key_options_json,
        redirect_url,
    })
}

pub async fn finish_passkey_login(
    challenge_id: &str,
    credential_json: &str,
) -> AuthStackResult<LoginCompletionResponse> {
    let completion = passkey_service()
        .await?
        .finish_authentication(
            challenge_id,
            credential_json,
            &request_id("passkey-login-finish")?,
        )
        .await
        .map_err(map_passkey_error)?;
    passkey_login_response(completion).await
}

pub async fn finish_passkey_registration(
    session_id: &str,
    challenge_id: &str,
    credential_json: &str,
) -> AuthStackResult<LoginCompletionResponse> {
    let session_id = bounded_session_id(session_id)?;
    let completion = passkey_service()
        .await?
        .finish_registration(
            &session_id,
            challenge_id,
            credential_json,
            &request_id("passkey-registration-finish")?,
            "Primary passkey",
        )
        .await
        .map_err(map_passkey_error)?;
    passkey_login_response(completion).await
}

async fn passkey_login_response(
    completion: wasi_auth::postgres::passkeys::PasskeyCompletion,
) -> AuthStackResult<LoginCompletionResponse> {
    let (access_token, refresh_token, expires_in_seconds) =
        issue_tokens(&completion.session_id).await?;
    Ok(LoginCompletionResponse {
        authenticated: true,
        redirect_url: completion.redirect_path,
        session_id: Some(completion.session_id.into_string()),
        access_token: Some(access_token),
        refresh_token: Some(refresh_token),
        expires_in_seconds,
    })
}

pub async fn register_email_password(
    request: &EmailPasswordRegisterRequest,
    redirect_uri: &str,
) -> AuthStackResult<LoginCompletionResponse> {
    let email_key = email_key(&request.email);
    let service = PasswordRegistrationService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
        argon2_policy().await?,
        outbox_key().await?,
    );
    service
        .register(PasswordRegistrationRequest::new(
            format!("register:{email_key}"),
            format!("anonymous:{email_key}"),
            request_id("register")?,
            request.email.clone(),
            request.password.clone(),
            redirect_uri,
        ))
        .await
        .map_err(map_registration_error)?;
    dispatch_pending_mail().await?;
    Ok(LoginCompletionResponse {
        authenticated: false,
        redirect_url: "/verify-email".to_owned(),
        session_id: None,
        access_token: None,
        refresh_token: None,
        expires_in_seconds: 0,
    })
}

pub async fn enforce_account_rate_limit(
    scope: &str,
    subject: &str,
    maximum_attempts: u64,
    window_seconds: u64,
) -> AuthStackResult<()> {
    let decision = RateLimitService::new(store().await?, RuntimeClock)
        .check(scope, subject, maximum_attempts, window_seconds)
        .await
        .map_err(map_rate_limit_error)?;
    if decision.allowed {
        Ok(())
    } else {
        Err(AuthStackError::RateLimited {
            retry_after_seconds: decision.retry_after_seconds,
        })
    }
}

pub async fn dispatch_pending_mail() -> AuthStackResult<usize> {
    let transport = runtime_config_value("AUTH_MAIL_TRANSPORT")
        .await
        .unwrap_or_else(|| "capture".to_owned())
        .trim()
        .to_ascii_lowercase();
    match transport.as_str() {
        "capture" => {
            #[cfg(feature = "mail-capture")]
            {
                let public_base_url = runtime_config_value("AUTH_PUBLIC_BASE_URL")
                    .await
                    .unwrap_or_else(|| "http://127.0.0.1:3008".to_owned());
                let worker = MailOutboxWorker::new(
                    store().await?,
                    RuntimeClock,
                    RuntimeRandom,
                    outbox_key().await?,
                    PublicBaseUrl::new(&public_base_url).map_err(|_| {
                        AuthStackError::configuration("AUTH_PUBLIC_BASE_URL is invalid")
                    })?,
                );
                let report = worker
                    .dispatch(CAPTURE_MAILER.get_or_init(CaptureMailer::default), 25)
                    .await
                    .map_err(|_| AuthStackError::store("mail outbox dispatch failed"))?;
                Ok(report.delivered)
            }
            #[cfg(not(feature = "mail-capture"))]
            {
                Err(AuthStackError::configuration(
                    "capture mail requires the mail-capture feature",
                ))
            }
        }
        "http" => {
            #[cfg(all(feature = "mail-http", runtime_spin))]
            {
                let endpoint = runtime_config_value("AUTH_MAIL_HTTP_URL")
                    .await
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        AuthStackError::configuration("AUTH_MAIL_HTTP_URL is required")
                    })?;
                let token = runtime_config_value("AUTH_MAIL_HTTP_TOKEN")
                    .await
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        AuthStackError::configuration("AUTH_MAIL_HTTP_TOKEN is required")
                    })?;
                let public_base_url = runtime_config_value("AUTH_PUBLIC_BASE_URL")
                    .await
                    .ok_or_else(|| {
                        AuthStackError::configuration("AUTH_PUBLIC_BASE_URL is required")
                    })?;
                let mailer = HttpMailer::new(
                    HttpMailEndpoint::new(&endpoint).map_err(|_| {
                        AuthStackError::configuration("AUTH_MAIL_HTTP_URL is invalid")
                    })?,
                    HttpMailBearerToken::new(token).map_err(|_| {
                        AuthStackError::configuration("AUTH_MAIL_HTTP_TOKEN is invalid")
                    })?,
                    SpinMailTransport,
                );
                let worker = MailOutboxWorker::new(
                    store().await?,
                    RuntimeClock,
                    RuntimeRandom,
                    outbox_key().await?,
                    PublicBaseUrl::new(&public_base_url).map_err(|_| {
                        AuthStackError::configuration("AUTH_PUBLIC_BASE_URL is invalid")
                    })?,
                );
                let report = worker
                    .dispatch(&mailer, 25)
                    .await
                    .map_err(|_| AuthStackError::store("mail outbox dispatch failed"))?;
                Ok(report.delivered)
            }
            #[cfg(not(all(feature = "mail-http", runtime_spin)))]
            {
                Err(AuthStackError::configuration(
                    "HTTP mail requires the mail-http feature on Spin",
                ))
            }
        }
        "smtp" => Err(AuthStackError::configuration(
            "SMTP delivery runs in the external native outbox worker",
        )),
        _ => Err(AuthStackError::configuration(
            "AUTH_MAIL_TRANSPORT must be capture, http, or smtp",
        )),
    }
}

pub async fn latest_captured_mail(
    recipient: &str,
    message_kind: &str,
) -> AuthStackResult<CapturedMailResponse> {
    if config_bool("AUTH_PRODUCTION_MODE", false).await
        || !config_bool("AUTH_DEV_TOOLS", false).await
    {
        return Err(AuthStackError::Forbidden);
    }
    #[cfg(feature = "mail-capture")]
    {
        dispatch_pending_mail().await?;
        let expected_kind = match message_kind {
            "email-verification" => EmailKind::Verification,
            "password-reset" => EmailKind::PasswordReset,
            "invitation" => EmailKind::Invitation,
            _ => return Err(AuthStackError::validation("message_kind is invalid")),
        };
        let recipient = Recipient::new(recipient.to_owned())
            .map_err(|_| AuthStackError::validation("recipient is invalid"))?;
        let public_base_url = runtime_config_value("AUTH_PUBLIC_BASE_URL")
            .await
            .unwrap_or_else(|| "http://127.0.0.1:3008".to_owned());
        let worker = MailOutboxWorker::new(
            store().await?,
            RuntimeClock,
            RuntimeRandom,
            outbox_key().await?,
            PublicBaseUrl::new(&public_base_url)
                .map_err(|_| AuthStackError::configuration("AUTH_PUBLIC_BASE_URL is invalid"))?,
        );
        let captured = worker
            .latest_delivered_for_development(&recipient, expected_kind)
            .await
            .map_err(|_| AuthStackError::store("captured mail is unavailable"))?
            .ok_or_else(|| AuthStackError::not_found("captured mail was not found"))?;
        Ok(CapturedMailResponse {
            message_kind: message_kind.to_owned(),
            recipient: captured.recipient().as_str().to_owned(),
            subject: captured.subject().to_owned(),
            body_text: captured.text_body().to_owned(),
        })
    }
    #[cfg(not(feature = "mail-capture"))]
    {
        let _ = (recipient, message_kind);
        Err(AuthStackError::configuration(
            "captured mail requires the mail-capture feature",
        ))
    }
}

pub async fn login_email_password(
    request: &EmailPasswordLoginRequest,
    redirect_uri: &str,
) -> AuthStackResult<LoginCompletionResponse> {
    let service = PasswordLoginService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
        argon2_policy().await?,
    )
    .with_session_ttl_seconds(session_ttl_seconds().await?)
    .map_err(map_login_error)?;
    let receipt = service
        .login(PasswordLoginRequest::new(
            request.email.clone(),
            request.password.clone(),
            request_id("login")?,
            redirect_uri,
        ))
        .await
        .map_err(map_login_error)?;
    let session_id = receipt.session_id;
    let (access_token, refresh_token, expires_in_seconds) = issue_tokens(&session_id).await?;
    Ok(LoginCompletionResponse {
        authenticated: true,
        redirect_url: receipt.redirect_uri,
        session_id: Some(session_id.into_string()),
        access_token: Some(access_token),
        refresh_token: Some(refresh_token),
        expires_in_seconds,
    })
}

pub async fn complete_email_verification(
    request: &EmailVerificationCompleteRequest,
    redirect_uri: &str,
) -> AuthStackResult<LoginCompletionResponse> {
    let service = EmailVerificationService::new(store().await?, RuntimeClock, RuntimeRandom)
        .with_session_ttl_seconds(session_ttl_seconds().await?)
        .map_err(map_verification_error)?;
    let receipt = service
        .verify(EmailVerificationRequest::new(
            request.token.clone(),
            request_id("verify-email")?,
            redirect_uri,
        ))
        .await
        .map_err(map_verification_error)?;
    let session_id = receipt.session_id;
    let (access_token, refresh_token, expires_in_seconds) = issue_tokens(&session_id).await?;
    Ok(LoginCompletionResponse {
        authenticated: true,
        redirect_url: receipt.redirect_uri,
        session_id: Some(session_id.into_string()),
        access_token: Some(access_token),
        refresh_token: Some(refresh_token),
        expires_in_seconds,
    })
}

pub async fn resend_email_verification(
    email: &str,
    redirect_uri: &str,
) -> AuthStackResult<()> {
    PasswordRegistrationService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
        argon2_policy().await?,
        outbox_key().await?,
    )
    .resend_verification(ProductEmailVerificationResendRequest::new(
        email,
        request_id("verification-resend")?,
        redirect_uri,
    ))
    .await
    .map_err(map_registration_error)?;
    dispatch_pending_mail().await?;
    Ok(())
}

pub async fn start_password_reset(
    request: &PasswordResetStartRequest,
    redirect_uri: &str,
) -> AuthStackResult<PasswordResetStartResponse> {
    let service = PasswordResetService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
        argon2_policy().await?,
        outbox_key().await?,
    )
    .with_session_ttl_seconds(session_ttl_seconds().await?)
    .map_err(map_password_reset_error)?;
    let receipt = service
        .start(ProductPasswordResetStartRequest::new(
            request.email.clone(),
            redirect_uri,
            request_id("password-reset-start")?,
        ))
        .await
        .map_err(map_password_reset_error)?;
    dispatch_pending_mail().await?;
    Ok(PasswordResetStartResponse {
        accepted: receipt.accepted,
        expires_in_seconds: receipt.expires_in_seconds,
    })
}

pub async fn complete_password_reset(
    request: &PasswordResetCompleteRequest,
    redirect_uri: &str,
) -> AuthStackResult<LoginCompletionResponse> {
    let service = PasswordResetService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
        argon2_policy().await?,
        outbox_key().await?,
    )
    .with_session_ttl_seconds(session_ttl_seconds().await?)
    .map_err(map_password_reset_error)?;
    let receipt = service
        .complete(ProductPasswordResetCompleteRequest::new(
            request.token.clone(),
            request.password.clone(),
            request_id("password-reset-complete")?,
            redirect_uri,
        ))
        .await
        .map_err(map_password_reset_error)?;
    let session_id = receipt.session_id;
    let (access_token, refresh_token, expires_in_seconds) = issue_tokens(&session_id).await?;
    Ok(LoginCompletionResponse {
        authenticated: true,
        redirect_url: receipt.redirect_uri,
        session_id: Some(session_id.into_string()),
        access_token: Some(access_token),
        refresh_token: Some(refresh_token),
        expires_in_seconds,
    })
}

pub async fn refresh_tokens(
    session_id: Option<&str>,
    refresh_token: &str,
) -> AuthStackResult<TokenRefreshResponse> {
    let session_id = session_id.map(bounded_session_id).transpose()?;
    let pair = token_service().await?
        .refresh(
            session_id.as_ref(),
            refresh_token,
            &request_id("refresh-token")?,
        )
        .await
        .map_err(map_token_error)?;
    let (access_token, refresh_token, expires_in_seconds) = pair.into_parts();
    Ok(TokenRefreshResponse {
        access_token: Some(access_token),
        refresh_token: Some(refresh_token),
        expires_in_seconds,
    })
}

pub async fn verify_access_token(token: &str) -> AuthStackResult<TokenVerifyResponse> {
    let cached = cached_verified_token(token);
    let verified = match token_verification_service()
        .await?
        .verify_cached(
            token,
            &request_id("verify-access-token")?,
            cached.as_ref(),
        )
        .await
    {
        Ok(verified) => verified,
        Err(error) => {
            remove_cached_verified_token(token);
            return Err(map_token_error(error));
        }
    };
    cache_verified_token(token, verified.clone());
    Ok(TokenVerifyResponse {
        active: true,
        subject: verified.user_id,
        tenant_id: verified.organization_id,
        session_id: Some(verified.session_id),
        expires_at: verified.expires_at_seconds,
        scopes: verified.permissions,
        role_ids: verified.role_ids,
        policy_revision: verified.policy_revision,
        assurance: verified.assurance,
        system_administrator: verified.system_administrator,
        issued_at_unix_seconds: verified.issued_at_seconds,
    })
}

pub async fn get_jwks() -> AuthStackResult<JwksDocument> {
    Ok(token_service().await?.jwks())
}

pub async fn change_password(
    user_id: &str,
    session_id: &str,
    current_password: &str,
    new_password: &str,
) -> AuthStackResult<()> {
    let user_id = UserId::new(user_id.to_owned()).map_err(|_| AuthStackError::AuthRequired)?;
    let session_id = SessionId::new(session_id.to_owned())
        .map_err(|_| AuthStackError::AuthRequired)?;
    PasswordLoginService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
        argon2_policy().await?,
    )
    .change_password(ProductPasswordChangeRequest::new(
        user_id,
        session_id,
        current_password,
        new_password,
        request_id("password-change")?,
    ))
    .await
    .map_err(map_login_error)
}

pub async fn mfa_status(session_id: &str) -> AuthStackResult<MfaStatusResponse> {
    let session_id = bounded_session_id(session_id)?;
    let status = mfa_service().await?
        .status(&session_id)
        .await
        .map_err(map_mfa_error)?;
    Ok(MfaStatusResponse {
        totp_enrolled: status.totp_enrolled,
        recovery_codes_remaining: status.recovery_codes_remaining,
        assurance: status.assurance,
    })
}

pub async fn start_totp_enrollment(
    session_id: &str,
) -> AuthStackResult<MfaEnrollStartResponse> {
    let session_id = bounded_session_id(session_id)?;
    let session = get_session(Some(session_id.as_str())).await?;
    let user_id = session.user_id.ok_or(AuthStackError::AuthRequired)?;
    let enrollment = mfa_service().await?
        .start(&session_id, &request_id("mfa-start")?)
        .await
        .map_err(map_mfa_error)?;
    let (provisioning_uri, secret_base32) = enrollment.into_parts();
    Ok(MfaEnrollStartResponse {
        credential_id: format!("totp:{user_id}"),
        secret_base32,
        provisioning_uri,
    })
}

pub async fn confirm_totp_enrollment(
    session_id: &str,
    code: &str,
) -> AuthStackResult<MfaEnrollConfirmResponse> {
    let session_id = bounded_session_id(session_id)?;
    let confirmation = mfa_service().await?
        .confirm(&session_id, code, &request_id("mfa-confirm")?)
        .await
        .map_err(map_mfa_error)?;
    Ok(MfaEnrollConfirmResponse {
        recovery_codes: confirmation.into_recovery_codes(),
        assurance: "aal2".to_owned(),
    })
}

pub async fn verify_totp_step_up(
    session_id: &str,
    code: &str,
) -> AuthStackResult<SessionView> {
    let session_id = bounded_session_id(session_id)?;
    mfa_service().await?
        .verify_step_up(&session_id, code, &request_id("mfa-step-up")?)
        .await
        .map_err(map_mfa_error)?;
    get_session(Some(session_id.as_str())).await
}

pub async fn use_recovery_code(
    session_id: &str,
    code: &str,
) -> AuthStackResult<SessionView> {
    let session_id = bounded_session_id(session_id)?;
    mfa_service().await?
        .use_recovery_code(&session_id, code, &request_id("mfa-recovery")?)
        .await
        .map_err(map_mfa_error)?;
    get_session(Some(session_id.as_str())).await
}

pub async fn authenticated_session_from_cookie(
    session_id: &str,
) -> AuthStackResult<AuthenticatedSession> {
    let session = load_session(session_id).await?;
    let context = session.context();
    Ok(AuthenticatedSession {
        principal: context.auth().principal().clone(),
        organization_id: context.auth().organization_id().cloned(),
        session_id: context.auth().session_id().clone(),
        assurance: context.auth().assurance(),
        issued_at_unix_seconds: context.auth().issued_at_unix_seconds(),
        expires_at_unix_seconds: context.auth().expires_at_unix_seconds(),
        decision_id: context.auth().decision_id().cloned(),
        policy_revision: context.auth().policy_revision().cloned(),
        authorization: context.authorization().clone(),
    })
}

pub async fn get_session(session_id: Option<&str>) -> AuthStackResult<SessionView> {
    let Some(session_id) = session_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(unauthenticated_session());
    };
    let session = match load_session(session_id).await {
        Ok(session) => session,
        Err(AuthStackError::AuthRequired) => return Ok(unauthenticated_session()),
        Err(error) => return Err(error),
    };
    let context = session.context();
    let assurance = match context.auth().assurance() {
        AuthenticationAssurance::Aal1 => "aal1",
        AuthenticationAssurance::Aal2 => "aal2",
        _ => "aal1",
    };
    Ok(SessionView {
        authenticated: true,
        session_id: Some(context.auth().session_id().as_str().to_owned()),
        tenant_id: context
            .auth()
            .organization_id()
            .map(|organization| organization.as_str().to_owned()),
        user_id: Some(context.auth().principal().user_id().as_str().to_owned()),
        primary_email: Some(session.primary_email().to_owned()),
        expires_at: Some(
            context
                .auth()
                .expires_at_unix_seconds()
                .saturating_mul(1_000)
                .to_string(),
        ),
        permissions: context
            .authorization()
            .permissions()
            .map(ToOwned::to_owned)
            .collect(),
        assurance: assurance.to_owned(),
        system_administrator: context.auth().principal().is_system_administrator(),
        issued_at_unix_seconds: Some(context.auth().issued_at_unix_seconds()),
        expires_at_unix_seconds: Some(context.auth().expires_at_unix_seconds()),
    })
}

pub async fn list_organizations(user_id: &str) -> AuthStackResult<OrganizationListResponse> {
    let user_id = UserId::new(user_id.to_owned()).map_err(|_| AuthStackError::AuthRequired)?;
    let organizations = OrganizationService::new(store().await?, RuntimeClock, RuntimeRandom)
        .list(&user_id)
        .await
        .map_err(map_organization_error)?;
    Ok(OrganizationListResponse {
        organizations: organizations
            .into_iter()
            .map(organization_summary)
            .collect(),
    })
}

pub async fn create_organization(
    name: &str,
    session_id: &str,
) -> AuthStackResult<OrganizationSummary> {
    let session_id = SessionId::new(session_id.to_owned())
        .map_err(|_| AuthStackError::AuthRequired)?;
    let name_key = URL_SAFE_NO_PAD.encode(Sha256::digest(name.trim().as_bytes()));
    let organization = OrganizationService::new(store().await?, RuntimeClock, RuntimeRandom)
        .create(CreateOrganizationRequest {
            idempotency_key: format!("create-organization:{}:{name_key}", session_id.as_str()),
            session_id,
            name: name.to_owned(),
            request_id: request_id("create-organization")?,
        })
        .await
        .map_err(map_organization_error)?;
    Ok(organization_summary(organization))
}

pub async fn select_organization(
    session_id: &str,
    organization_id: &str,
) -> AuthStackResult<SessionView> {
    let session_id = SessionId::new(session_id.to_owned())
        .map_err(|_| AuthStackError::AuthRequired)?;
    OrganizationService::new(store().await?, RuntimeClock, RuntimeRandom)
        .select(
            &session_id,
            organization_id,
            &request_id("select-organization")?,
        )
        .await
        .map_err(map_organization_error)?;
    get_session(Some(session_id.as_str())).await
}

pub async fn organization_for_session(
    session_id: &str,
    organization_id: &str,
) -> AuthStackResult<OrganizationSummary> {
    let session_id = bounded_session_id(session_id)?;
    management_service().await?
        .organization(&session_id, organization_id)
        .await
        .map(organization_summary)
        .map_err(map_management_error)
}

pub async fn update_organization(
    session_id: &str,
    organization_id: &str,
    name: &str,
) -> AuthStackResult<OrganizationSummary> {
    let session_id = bounded_session_id(session_id)?;
    management_service().await?
        .update_organization(
            &session_id,
            organization_id,
            name,
            &request_id("update-organization")?,
        )
        .await
        .map(organization_summary)
        .map_err(map_management_error)
}

pub async fn list_memberships(
    session_id: &str,
    organization_id: &str,
) -> AuthStackResult<MembershipListResponse> {
    let session_id = bounded_session_id(session_id)?;
    let memberships = management_service().await?
        .list_memberships(&session_id, organization_id)
        .await
        .map_err(map_management_error)?;
    Ok(MembershipListResponse {
        memberships: memberships.into_iter().map(membership_summary).collect(),
    })
}

pub async fn create_invitation(
    session_id: &str,
    organization_id: &str,
    email: &str,
    role_id: &str,
) -> AuthStackResult<InvitationSummary> {
    let session_id = bounded_session_id(session_id)?;
    let invitation = InvitationService::new(
        store().await?, RuntimeClock, RuntimeRandom, outbox_key().await?,
    )
    .create(
        &session_id,
        organization_id,
        email,
        role_id,
        &request_id("create-invitation")?,
    )
    .await
    .map_err(map_management_error)?;
    dispatch_pending_mail().await?;
    Ok(invitation_summary(invitation))
}

pub async fn list_invitations(
    session_id: &str,
    organization_id: &str,
) -> AuthStackResult<InvitationListResponse> {
    let session_id = bounded_session_id(session_id)?;
    let invitations = management_service().await?
        .list_invitations(&session_id, organization_id)
        .await
        .map_err(map_management_error)?;
    Ok(InvitationListResponse {
        invitations: invitations.into_iter().map(invitation_summary).collect(),
    })
}

pub async fn accept_invitation(
    session_id: &str,
    token: &str,
) -> AuthStackResult<OrganizationSummary> {
    let session_id = bounded_session_id(session_id)?;
    InvitationService::new(
        store().await?, RuntimeClock, RuntimeRandom, outbox_key().await?,
    )
    .accept(&session_id, token, &request_id("accept-invitation")?)
    .await
    .map(organization_summary)
    .map_err(map_management_error)
}

pub async fn list_roles(
    session_id: &str,
    organization_id: &str,
) -> AuthStackResult<RoleListResponse> {
    let session_id = bounded_session_id(session_id)?;
    let roles = management_service().await?
        .list_roles(&session_id, organization_id)
        .await
        .map_err(map_management_error)?;
    Ok(RoleListResponse {
        roles: roles.into_iter().map(role_summary).collect(),
    })
}

pub async fn upsert_role(
    session_id: &str,
    organization_id: &str,
    role_id: &str,
    name: &str,
    permissions: Vec<String>,
) -> AuthStackResult<RoleSummary> {
    let session_id = bounded_session_id(session_id)?;
    management_service().await?
        .upsert_role(ProductUpsertRoleRequest {
            session_id,
            organization_id: organization_id.to_owned(),
            role_id: role_id.to_owned(),
            name: name.to_owned(),
            permissions,
            request_id: request_id("upsert-role")?,
        })
        .await
        .map(role_summary)
        .map_err(map_management_error)
}

pub async fn assign_role(
    session_id: &str,
    organization_id: &str,
    user_id: &str,
    role_id: &str,
) -> AuthStackResult<MembershipSummary> {
    let session_id = bounded_session_id(session_id)?;
    management_service().await?
        .assign_role(
            &session_id,
            organization_id,
            user_id,
            role_id,
            &request_id("assign-role")?,
        )
        .await
        .map(membership_summary)
        .map_err(map_management_error)
}

pub async fn remove_member(
    session_id: &str,
    organization_id: &str,
    user_id: &str,
) -> AuthStackResult<()> {
    let session_id = bounded_session_id(session_id)?;
    management_service().await?
        .remove_member(
            &session_id,
            organization_id,
            user_id,
            &request_id("remove-member")?,
        )
        .await
        .map_err(map_management_error)
}

pub fn organization_permission_catalog() -> Vec<String> {
    ORGANIZATION_PERMISSION_CATALOG
        .iter()
        .map(|permission| (*permission).to_owned())
        .collect()
}

pub async fn list_admin_users(session_id: &str) -> AuthStackResult<AdminUserListResponse> {
    let session_id = bounded_session_id(session_id)?;
    let users = management_service().await?
        .list_admin_users(&session_id)
        .await
        .map_err(map_management_error)?;
    Ok(AdminUserListResponse {
        users: users.into_iter().map(admin_user_summary).collect(),
    })
}

pub async fn set_user_disabled(
    session_id: &str,
    user_id: &str,
    disabled: bool,
) -> AuthStackResult<AdminUserSummary> {
    let session_id = bounded_session_id(session_id)?;
    management_service().await?
        .set_user_disabled(
            &session_id,
            user_id,
            disabled,
            &request_id("set-user-disabled")?,
        )
        .await
        .map(admin_user_summary)
        .map_err(map_management_error)
}

pub async fn list_audit_events(
    session_id: &str,
    organization_id: Option<&str>,
    after_cursor: u64,
    limit: usize,
) -> AuthStackResult<AuditEventListResponse> {
    let session_id = bounded_session_id(session_id)?;
    let page = management_service().await?
        .list_audit_events(&session_id, organization_id, after_cursor, limit)
        .await
        .map_err(map_management_error)?;
    Ok(AuditEventListResponse {
        events: page.events.into_iter().map(audit_event_summary).collect(),
        next_cursor: page.next_cursor,
    })
}

pub async fn list_user_sessions(
    user_id: &str,
    current_session_id: &str,
) -> AuthStackResult<AccountSessionListResponse> {
    let user_id = UserId::new(user_id.to_owned()).map_err(|_| AuthStackError::AuthRequired)?;
    let sessions = SessionService::new(store().await?, RuntimeClock, RuntimeRandom)
        .list(&user_id)
        .await
        .map_err(map_session_service_error)?;
    Ok(AccountSessionListResponse {
        sessions: sessions
            .into_iter()
            .map(|session| AccountSessionSummary {
                current: session.session_id.as_str() == current_session_id,
                session_id: session.session_id.into_string(),
                organization_id: session.organization_id,
                assurance: session.assurance,
                issued_at_ms: session.created_at_ms,
                expires_at_ms: session.expires_at_ms,
            })
            .collect(),
    })
}

pub async fn revoke_user_session(
    target_session_id: &str,
    actor_session_id: &str,
) -> AuthStackResult<()> {
    let target = SessionId::new(target_session_id.to_owned())
        .map_err(|_| AuthStackError::validation("session_id is invalid"))?;
    let actor = SessionId::new(actor_session_id.to_owned())
        .map_err(|_| AuthStackError::AuthRequired)?;
    SessionService::new(store().await?, RuntimeClock, RuntimeRandom)
        .revoke(&target, &actor, &request_id("revoke-session")?)
        .await
        .map_err(map_session_service_error)
}

pub async fn logout_session(session_id: Option<&str>) -> AuthStackResult<LogoutResponse> {
    if let Some(session_id) = session_id.map(str::trim).filter(|value| !value.is_empty()) {
        revoke_user_session(session_id, session_id).await?;
    }
    Ok(LogoutResponse {
        redirect_url: "/".to_owned(),
    })
}

fn organization_summary(record: OrganizationRecord) -> OrganizationSummary {
    OrganizationSummary {
        organization_id: record.organization_id,
        name: record.name,
        status: record.status,
        current_user_role: record.role_id,
        permissions: record.permissions,
        created_at_ms: record.created_at_ms,
    }
}

fn membership_summary(record: MembershipRecord) -> MembershipSummary {
    MembershipSummary {
        organization_id: record.organization_id,
        user_id: record.user_id,
        primary_email: record.primary_email,
        role_id: record.role_id,
        status: record.status,
        joined_at_ms: record.joined_at_ms,
    }
}

fn invitation_summary(record: InvitationRecord) -> InvitationSummary {
    InvitationSummary {
        invitation_id: record.invitation_id,
        organization_id: record.organization_id,
        email: record.email,
        role_id: record.role_id,
        status: record.status,
        expires_at_ms: record.expires_at_ms,
    }
}

fn role_summary(record: RoleRecord) -> RoleSummary {
    RoleSummary {
        organization_id: record.organization_id,
        role_id: record.role_id,
        name: record.name,
        built_in: record.built_in,
        permissions: record.permissions,
    }
}

fn admin_user_summary(record: AdminUserRecord) -> AdminUserSummary {
    AdminUserSummary {
        user_id: record.user_id,
        primary_email: record.primary_email,
        disabled: record.status == "disabled",
        email_verified: record.status != "pending_verification",
        created_at_ms: record.created_at_ms,
    }
}

fn audit_event_summary(record: AuditEventRecord) -> AuditEventSummary {
    AuditEventSummary {
        sequence: record.sequence,
        organization_id: record.organization_id,
        actor_user_id: record.actor_user_id,
        action: record.action,
        target_type: record.resource_type,
        target_id: record.resource_id,
        outcome: record.outcome,
        recorded_at_ms: record.occurred_at_ms,
    }
}

async fn management_service(
) -> AuthStackResult<OrganizationManagementService<SpinPostgresTransport, RuntimeClock, RuntimeRandom>> {
    Ok(OrganizationManagementService::new(
        store().await?, RuntimeClock, RuntimeRandom,
    ))
}

fn bounded_session_id(session_id: &str) -> AuthStackResult<SessionId> {
    SessionId::new(session_id.to_owned()).map_err(|_| AuthStackError::AuthRequired)
}

async fn load_session(
    session_id: &str,
) -> AuthStackResult<wasi_auth::postgres::VerifiedSession> {
    let session_id = SessionId::new(session_id.to_owned())
        .map_err(|_| AuthStackError::AuthRequired)?;
    store()
        .await?
        .load_verified_session(
            &session_id,
            request_id("session")?,
            RuntimeClock.now_unix_seconds(),
        )
        .await
        .map_err(map_session_store_error)
}

fn unauthenticated_session() -> SessionView {
    SessionView {
        authenticated: false,
        session_id: None,
        tenant_id: None,
        user_id: None,
        primary_email: None,
        expires_at: None,
        permissions: Vec::new(),
        assurance: "none".to_owned(),
        system_administrator: false,
        issued_at_unix_seconds: None,
        expires_at_unix_seconds: None,
    }
}

pub(crate) async fn store() -> AuthStackResult<PostgresAuthStore<SpinPostgresTransport>> {
    let database_url = runtime_config_value("DATABASE_URL")
        .await
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AuthStackError::configuration("DATABASE_URL is required"))?;
    let transport = SpinPostgresTransport::new(database_url)
        .map_err(|_| AuthStackError::configuration("DATABASE_URL is invalid"))?;
    Ok(PostgresAuthStore::new(transport))
}

pub(crate) async fn outbox_key() -> AuthStackResult<OutboxSealingKey> {
    let (key_version, key) = vault_key_material().await?;
    OutboxSealingKey::new(key_version, key)
        .map_err(|_| AuthStackError::configuration("outbox key configuration is invalid"))
}

async fn flow_sealing_key() -> AuthStackResult<FlowSealingKey> {
    let (version, key) = vault_key_material().await?;
    FlowSealingKey::new(format!("flow:{version}"), key)
        .map_err(|_| AuthStackError::configuration("flow sealing key is invalid"))
}

async fn oauth_flow_service(
) -> AuthStackResult<OAuthFlowService<SpinPostgresTransport, RuntimeClock, RuntimeRandom>> {
    let config = OAuthServiceConfig::new(
        config_u64("AUTH_OAUTH_STATE_TTL_SECONDS", 10 * 60).await?,
        session_ttl_seconds().await?,
    )
    .map_err(|_| AuthStackError::configuration("OAuth lifetime configuration is invalid"))?;
    Ok(OAuthFlowService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
        flow_sealing_key().await?,
        config,
    ))
}

async fn oauth_provider_service(
) -> AuthStackResult<OAuthProviderService<SpinPostgresTransport, RuntimeClock, RuntimeRandom>> {
    Ok(OAuthProviderService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
    ))
}

async fn policy_bundle_service(
) -> AuthStackResult<PolicyBundleService<SpinPostgresTransport, RuntimeClock, RuntimeRandom>> {
    Ok(PolicyBundleService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
    ))
}

async fn signing_key_service(
) -> AuthStackResult<SigningKeyService<SpinPostgresTransport, RuntimeClock, RuntimeRandom>> {
    Ok(SigningKeyService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
    ))
}

async fn passkey_service(
) -> AuthStackResult<PasskeyService<SpinPostgresTransport, RuntimeClock, RuntimeRandom>> {
    let attachment = match runtime_config_value("AUTH_PASSKEY_AUTHENTICATOR_ATTACHMENT")
        .await
        .unwrap_or_else(|| "platform".to_owned())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "platform" => PasskeyAttachment::Platform,
        "cross-platform" | "cross_platform" | "roaming" => PasskeyAttachment::CrossPlatform,
        "any" | "none" | "unspecified" => PasskeyAttachment::Any,
        _ => {
            return Err(AuthStackError::configuration(
                "AUTH_PASSKEY_AUTHENTICATOR_ATTACHMENT must be platform, cross-platform, or any",
            ));
        }
    };
    let rp_id = runtime_config_value("AUTH_PASSKEY_RP_ID")
        .await
        .unwrap_or_else(|| "localhost".to_owned());
    let rp_name = runtime_config_value("AUTH_PASSKEY_RP_NAME")
        .await
        .unwrap_or_else(|| "fullstack-app".to_owned());
    let origin = runtime_config_value("AUTH_PASSKEY_ORIGIN")
        .await
        .unwrap_or_else(|| "http://localhost:3008".to_owned());
    let config = PasskeyServiceConfig::new(
        &rp_id,
        &rp_name,
        &origin,
        config_bool("AUTH_PRODUCTION_MODE", false).await,
        attachment,
        config_bool("AUTH_PASSKEY_REQUIRE_USER_VERIFICATION", true).await,
        config_bool("AUTH_PASSKEY_REQUIRE_USER_HANDLE", false).await,
        config_u64("AUTH_PASSKEY_CHALLENGE_TTL_SECONDS", 5 * 60).await?,
        session_ttl_seconds().await?,
    )
    .map_err(|PasskeyConfigurationError::Invalid| {
        AuthStackError::configuration("passkey relying-party configuration is invalid")
    })?;
    Ok(PasskeyService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
        flow_sealing_key().await?,
        config,
    ))
}

async fn vault_key_material() -> AuthStackResult<(String, [u8; 32])> {
    let configured = runtime_config_value("AUTH_VAULT_KEY_BASE64")
        .await
        .filter(|value| !value.trim().is_empty());
    let production = config_bool("AUTH_PRODUCTION_MODE", false).await;
    let key: [u8; 32] = match configured {
        Some(encoded) => STANDARD
            .decode(encoded.trim())
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| {
                AuthStackError::configuration("AUTH_VAULT_KEY_BASE64 must decode to 32 bytes")
            })?,
        None if production => {
            return Err(AuthStackError::configuration(
                "AUTH_VAULT_KEY_BASE64 is required in production",
            ));
        }
        None => Sha256::digest(DEVELOPMENT_OUTBOX_KEY).into(),
    };
    let key_version = runtime_config_value("AUTH_VAULT_KEY_VERSION")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "development-v1".to_owned());
    Ok((key_version, key))
}

type RuntimeTokenService = TokenService<SpinPostgresTransport, RuntimeClock, RuntimeRandom>;
type RuntimeTokenVerifier = AccessTokenVerifier<SpinPostgresTransport, RuntimeClock>;

async fn token_service() -> AuthStackResult<RuntimeTokenService> {
    let mut key_ring = configured_jwt_key_ring().await?;
    synchronize_signing_keys(&mut key_ring).await?;
    configured_token_service(key_ring).await
}

async fn token_verification_service() -> AuthStackResult<&'static RuntimeTokenVerifier> {
    if let Some(verifier) = TOKEN_VERIFIER.get() {
        return Ok(verifier);
    }
    let issuer = runtime_config_value("AUTH_JWT_ISSUER")
        .await
        .unwrap_or_else(|| "http://127.0.0.1:3008".to_owned());
    let audience = runtime_config_value("AUTH_JWT_AUDIENCE")
        .await
        .unwrap_or_else(|| "fullstack-app".to_owned());
    let verifier = AccessTokenVerifier::new(
        store().await?,
        RuntimeClock,
        configured_jwt_key_ring().await?,
        issuer,
        audience,
    )
    .map_err(|_| AuthStackError::configuration("token verification configuration is invalid"))?;
    let _ = TOKEN_VERIFIER.set(verifier);
    TOKEN_VERIFIER.get().ok_or_else(|| {
        AuthStackError::configuration("token verification service could not be initialized")
    })
}

pub async fn trusted_context_codec() -> AuthStackResult<Option<&'static TrustedContextCodec>> {
    if let Some(codec) = TRUSTED_CONTEXT_CODEC.get() {
        return Ok(Some(codec));
    }
    let Some(encoded_key) = runtime_config_value("AUTH_TRUSTED_INGRESS_KEY_BASE64")
        .await
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(None);
    };
    let key: [u8; 32] = STANDARD
        .decode(encoded_key.trim())
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| {
            AuthStackError::configuration(
                "AUTH_TRUSTED_INGRESS_KEY_BASE64 must decode to 32 bytes",
            )
        })?;
    let audience = runtime_config_value("AUTH_TRUSTED_INGRESS_AUDIENCE")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "fullstack-app".to_owned());
    let max_age = config_u64("AUTH_TRUSTED_INGRESS_MAX_AGE_SECONDS", 5).await?;
    let codec = TrustedContextCodec::new(audience, key)
        .and_then(|codec| codec.with_max_age_seconds(max_age))
        .map_err(|_| AuthStackError::configuration("trusted ingress configuration is invalid"))?;
    let _ = TRUSTED_CONTEXT_CODEC.set(codec);
    Ok(TRUSTED_CONTEXT_CODEC.get())
}

pub async fn trusted_ingress_required() -> bool {
    config_bool("AUTH_PRODUCTION_MODE", false).await
        || config_bool("AUTH_REQUIRE_TRUSTED_INGRESS", false).await
}

fn verified_token_cache() -> &'static VerifiedTokenCache {
    VERIFIED_TOKEN_CACHE.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn token_cache_key(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

fn cached_verified_token(token: &str) -> Option<VerifiedAccessToken> {
    let key = token_cache_key(token);
    let mut cache = verified_token_cache().lock().ok()?;
    let index = cache.iter().position(|(candidate, _)| candidate == &key)?;
    let entry = cache.remove(index)?;
    if entry.1.expires_at_seconds <= RuntimeClock.now_unix_seconds() {
        return None;
    }
    let verified = entry.1.clone();
    cache.push_back(entry);
    Some(verified)
}

fn cache_verified_token(token: &str, verified: VerifiedAccessToken) {
    let key = token_cache_key(token);
    let Ok(mut cache) = verified_token_cache().lock() else {
        return;
    };
    if let Some(index) = cache.iter().position(|(candidate, _)| candidate == &key) {
        cache.remove(index);
    }
    cache.push_back((key, verified));
    while cache.len() > VERIFIED_TOKEN_CACHE_CAPACITY {
        cache.pop_front();
    }
}

fn remove_cached_verified_token(token: &str) {
    let key = token_cache_key(token);
    let Ok(mut cache) = verified_token_cache().lock() else {
        return;
    };
    if let Some(index) = cache.iter().position(|(candidate, _)| candidate == &key) {
        cache.remove(index);
    }
}

async fn configured_token_service(key_ring: JwtKeyRing) -> AuthStackResult<RuntimeTokenService> {
    let (version, key) = vault_key_material().await?;
    let sealing_key = RefreshSealingKey::new(format!("refresh:{version}"), key)
        .map_err(|_| AuthStackError::configuration("refresh sealing key is invalid"))?;
    let issuer = runtime_config_value("AUTH_JWT_ISSUER")
        .await
        .unwrap_or_else(|| "http://127.0.0.1:3008".to_owned());
    let audience = runtime_config_value("AUTH_JWT_AUDIENCE")
        .await
        .unwrap_or_else(|| "fullstack-app".to_owned());
    let config = TokenServiceConfig::new(
        issuer,
        audience,
        config_u64("AUTH_ACCESS_TOKEN_TTL_SECONDS", 15 * 60).await?,
        config_u64("AUTH_REFRESH_TOKEN_TTL_SECONDS", 30 * 24 * 60 * 60).await?,
        session_ttl_seconds().await?,
    )
    .map_err(|_| AuthStackError::configuration("token lifetime configuration is invalid"))?;
    Ok(TokenService::new(
        store().await?, RuntimeClock, RuntimeRandom, key_ring, sealing_key, config,
    ))
}

async fn configured_jwt_key_ring() -> AuthStackResult<JwtKeyRing> {
    let production = config_bool("AUTH_PRODUCTION_MODE", false).await;
    Ok(match runtime_config_value("AUTH_JWT_KEY_RING_JSON")
        .await
        .filter(|value| !value.trim().is_empty())
    {
        Some(value) => JwtKeyRing::from_json(&value, production)
            .map_err(|_| AuthStackError::configuration("AUTH_JWT_KEY_RING_JSON is invalid"))?,
        None if production => {
            return Err(AuthStackError::configuration(
                "AUTH_JWT_KEY_RING_JSON with an active ES256 key is required in production",
            ));
        }
        None => {
            let kid = runtime_config_value("AUTH_JWT_KID")
                .await
                .unwrap_or_else(|| "fullstack-app-dev-hs256".to_owned());
            let secret = runtime_config_value("AUTH_JWT_SECRET")
                .await
                .unwrap_or_else(|| "dev-fullstack-app-secret-change-me".to_owned());
            JwtKeyRing::development_hs256(kid, secret.into_bytes())
                .map_err(|_| AuthStackError::configuration("development JWT key is invalid"))?
        }
    })
}

async fn synchronize_signing_keys(key_ring: &mut JwtKeyRing) -> AuthStackResult<()> {
    let service = signing_key_service().await?;
    let configured = key_ring.descriptors();
    let mut metadata = service.list().await.map_err(map_signing_key_error)?;
    if configured
        .iter()
        .any(|descriptor| !metadata.iter().any(|record| record.key_id == descriptor.kid))
    {
        let version = runtime_config_value("AUTH_JWT_KEY_VERSION")
            .await
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "runtime-v1".to_owned());
        metadata = service
            .synchronize(key_ring, &version)
            .await
            .map_err(map_signing_key_error)?;
    }
    let statuses = metadata
        .into_iter()
        .map(|record| (record.key_id, record.status))
        .collect::<Vec<_>>();
    key_ring
        .apply_statuses(&statuses)
        .map_err(|_| AuthStackError::configuration("signing-key lifecycle is invalid"))
}

async fn mfa_service(
) -> AuthStackResult<MfaService<SpinPostgresTransport, RuntimeClock, RuntimeRandom>> {
    let (version, encryption_key) = vault_key_material().await?;
    let production = config_bool("AUTH_PRODUCTION_MODE", false).await;
    let recovery_pepper = match runtime_config_value("AUTH_RECOVERY_CODE_PEPPER_BASE64")
        .await
        .filter(|value| !value.trim().is_empty())
    {
        Some(value) => STANDARD
            .decode(value.trim())
            .map_err(|_| AuthStackError::configuration("recovery-code pepper is invalid"))?,
        None if production => {
            return Err(AuthStackError::configuration(
                "AUTH_RECOVERY_CODE_PEPPER_BASE64 is required in production",
            ));
        }
        None => Sha256::digest(b"fullstack-development-recovery-pepper").to_vec(),
    };
    let keys = MfaKeyMaterial::new(format!("mfa:{version}"), encryption_key, recovery_pepper)
        .map_err(|_| AuthStackError::configuration("MFA key configuration is invalid"))?;
    let issuer = runtime_config_value("AUTH_MFA_ISSUER")
        .await
        .unwrap_or_else(|| "fullstack-app".to_owned());
    MfaService::new(
        store().await?, RuntimeClock, RuntimeRandom, keys, TotpConfig::default(), issuer,
    )
    .map_err(|_| AuthStackError::configuration("MFA service configuration is invalid"))
}

async fn issue_tokens(session_id: &SessionId) -> AuthStackResult<(String, String, u64)> {
    token_service().await?
        .issue(session_id, &request_id("issue-token")?)
        .await
        .map(|pair| pair.into_parts())
        .map_err(map_token_error)
}

async fn argon2_policy() -> AuthStackResult<Argon2Policy> {
    let memory = config_u32("AUTH_PASSWORD_ARGON2_MEMORY_KIB", 19_456).await?;
    let iterations = config_u32("AUTH_PASSWORD_ARGON2_ITERATIONS", 2).await?;
    let parallelism = config_u32("AUTH_PASSWORD_ARGON2_PARALLELISM", 1).await?;
    Argon2Policy::new(memory, iterations, parallelism)
        .map_err(|_| AuthStackError::configuration("Argon2id policy is invalid"))
}

async fn session_ttl_seconds() -> AuthStackResult<u64> {
    let value = runtime_config_value("AUTH_SESSION_TTL_SECONDS")
        .await
        .filter(|value| !value.trim().is_empty());
    value.map_or(Ok(3_600), |value| {
        value.trim().parse::<u64>().map_err(|_| {
            AuthStackError::configuration("AUTH_SESSION_TTL_SECONDS must be an integer")
        })
    })
}

async fn config_u32(name: &str, default: u32) -> AuthStackResult<u32> {
    runtime_config_value(name)
        .await
        .filter(|value| !value.trim().is_empty())
        .map_or(Ok(default), |value| {
            value
                .trim()
                .parse::<u32>()
                .map_err(|_| AuthStackError::configuration(format!("{name} must be an integer")))
        })
}

async fn config_u64(name: &str, default: u64) -> AuthStackResult<u64> {
    runtime_config_value(name)
        .await
        .filter(|value| !value.trim().is_empty())
        .map_or(Ok(default), |value| {
            value
                .trim()
                .parse::<u64>()
                .map_err(|_| AuthStackError::configuration(format!("{name} must be an integer")))
        })
}

async fn config_bool(name: &str, default: bool) -> bool {
    runtime_config_value(name)
        .await
        .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}

async fn runtime_config_value(name: &str) -> Option<String> {
    #[cfg(all(runtime_spin, not(test)))]
    {
        let variable_name = name.to_ascii_lowercase();
        if let Ok(value) = spin_sdk::variables::get(&variable_name).await
            && !value.is_empty()
        {
            return Some(value);
        }
    }
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn email_key(email: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(email.trim().to_ascii_lowercase().as_bytes()))
}

fn request_id(prefix: &str) -> AuthStackResult<RequestId> {
    let mut bytes = [0_u8; 18];
    RuntimeRandom
        .fill_bytes(&mut bytes)
        .map_err(|_| AuthStackError::store("cryptographic randomness is unavailable"))?;
    RequestId::new(format!("{prefix}-{}", URL_SAFE_NO_PAD.encode(bytes)))
        .map_err(|_| AuthStackError::store("failed to construct request identifier"))
}

fn map_registration_error(
    error: PasswordRegistrationError<SpinPostgresError>,
) -> AuthStackError {
    match error {
        PasswordRegistrationError::InvalidRequest => {
            AuthStackError::validation("registration request is invalid")
        }
        PasswordRegistrationError::InvalidConfiguration => {
            AuthStackError::configuration("password registration is not configured")
        }
        PasswordRegistrationError::Store(PostgresStoreError::EmailAlreadyExists) => {
            AuthStackError::conflict("An account already exists for this email address")
        }
        PasswordRegistrationError::Store(PostgresStoreError::IdempotencyConflict) => {
            AuthStackError::conflict("registration request conflicts with an earlier attempt")
        }
        PasswordRegistrationError::RandomnessUnavailable
        | PasswordRegistrationError::Crypto
        | PasswordRegistrationError::Store(_) => {
            AuthStackError::store("password registration failed")
        }
        _ => AuthStackError::store("password registration failed"),
    }
}

fn map_login_error(error: PasswordLoginError<SpinPostgresError>) -> AuthStackError {
    match error {
        PasswordLoginError::InvalidRequest => AuthStackError::validation("login request is invalid"),
        PasswordLoginError::InvalidCredentials => AuthStackError::InvalidCredentials,
        PasswordLoginError::InvalidConfiguration => {
            AuthStackError::configuration("password login is not configured")
        }
        PasswordLoginError::RandomnessUnavailable
        | PasswordLoginError::Crypto
        | PasswordLoginError::Transport(_)
        | PasswordLoginError::Row(_)
        | PasswordLoginError::Context => AuthStackError::store("password login failed"),
        _ => AuthStackError::store("password login failed"),
    }
}

fn map_verification_error(error: EmailVerificationError<SpinPostgresError>) -> AuthStackError {
    match error {
        EmailVerificationError::InvalidRequest => {
            AuthStackError::validation("verification request is invalid")
        }
        EmailVerificationError::InvalidToken => AuthStackError::InvalidToken,
        EmailVerificationError::InvalidConfiguration => {
            AuthStackError::configuration("email verification is not configured")
        }
        EmailVerificationError::RandomnessUnavailable
        | EmailVerificationError::Transport(_)
        | EmailVerificationError::Row(_)
        | EmailVerificationError::Context => AuthStackError::store("email verification failed"),
        _ => AuthStackError::store("email verification failed"),
    }
}

fn map_session_store_error(error: PostgresStoreError<SpinPostgresError>) -> AuthStackError {
    match error {
        PostgresStoreError::Unauthenticated => AuthStackError::AuthRequired,
        _ => AuthStackError::store("session verification failed"),
    }
}

fn map_organization_error(error: OrganizationError<SpinPostgresError>) -> AuthStackError {
    match error {
        OrganizationError::InvalidRequest => {
            AuthStackError::validation("organization request is invalid")
        }
        OrganizationError::Unauthenticated => AuthStackError::Forbidden,
        OrganizationError::IdempotencyConflict => {
            AuthStackError::conflict("organization request conflicts with an earlier attempt")
        }
        OrganizationError::RandomnessUnavailable
        | OrganizationError::Transport(_)
        | OrganizationError::Row(_)
        | OrganizationError::InvalidRow
        | OrganizationError::UnexpectedOutcome => {
            AuthStackError::store("organization operation failed")
        }
        _ => AuthStackError::store("organization operation failed"),
    }
}

fn map_password_reset_error(error: PasswordResetError<SpinPostgresError>) -> AuthStackError {
    match error {
        PasswordResetError::InvalidRequest => {
            AuthStackError::validation("password reset request is invalid")
        }
        PasswordResetError::InvalidToken => AuthStackError::InvalidToken,
        PasswordResetError::InvalidConfiguration => {
            AuthStackError::configuration("password reset is not configured")
        }
        PasswordResetError::RandomnessUnavailable
        | PasswordResetError::Crypto
        | PasswordResetError::Transport(_)
        | PasswordResetError::Row(_)
        | PasswordResetError::Context => AuthStackError::store("password reset failed"),
        _ => AuthStackError::store("password reset failed"),
    }
}

fn map_session_service_error(error: SessionServiceError<SpinPostgresError>) -> AuthStackError {
    match error {
        SessionServiceError::NotAuthorized => AuthStackError::Forbidden,
        SessionServiceError::RandomnessUnavailable
        | SessionServiceError::Transport(_)
        | SessionServiceError::Row(_)
        | SessionServiceError::InvalidRow => AuthStackError::store("session operation failed"),
        _ => AuthStackError::store("session operation failed"),
    }
}

fn map_rate_limit_error(error: RateLimitError<SpinPostgresError>) -> AuthStackError {
    match error {
        RateLimitError::InvalidRequest => {
            AuthStackError::validation("rate-limit request is invalid")
        }
        RateLimitError::Transport(_) | RateLimitError::Row(_) | RateLimitError::InvalidRow => {
            AuthStackError::store("rate-limit operation failed")
        }
        _ => AuthStackError::store("rate-limit operation failed"),
    }
}

fn map_management_error(error: ManagementError<SpinPostgresError>) -> AuthStackError {
    match error {
        ManagementError::InvalidRequest => AuthStackError::validation("management request is invalid"),
        ManagementError::NotAuthorized => AuthStackError::Forbidden,
        ManagementError::ProtectedInvariant => AuthStackError::conflict(
            "operation would violate an ownership or account invariant",
        ),
        ManagementError::RestrictedPermission => {
            AuthStackError::validation("custom role contains a restricted permission")
        }
        ManagementError::InvalidToken => AuthStackError::InvalidToken,
        ManagementError::RandomnessUnavailable
        | ManagementError::Crypto
        | ManagementError::Transport(_)
        | ManagementError::Row(_)
        | ManagementError::InvalidRow => AuthStackError::store("management operation failed"),
        _ => AuthStackError::store("management operation failed"),
    }
}

fn map_token_error(error: TokenServiceError<SpinPostgresError>) -> AuthStackError {
    match error {
        TokenServiceError::InvalidSession => AuthStackError::AuthRequired,
        TokenServiceError::InvalidToken | TokenServiceError::ReuseDetected => {
            AuthStackError::InvalidToken
        }
        TokenServiceError::RandomnessUnavailable
        | TokenServiceError::Crypto
        | TokenServiceError::Transport(_)
        | TokenServiceError::Row(_) => AuthStackError::store("token operation failed"),
        _ => AuthStackError::store("token operation failed"),
    }
}

fn map_mfa_error(error: MfaServiceError<SpinPostgresError>) -> AuthStackError {
    match error {
        MfaServiceError::InvalidSession => AuthStackError::AuthRequired,
        MfaServiceError::AlreadyEnrolled => AuthStackError::conflict("TOTP is already enrolled"),
        MfaServiceError::InvalidFactor | MfaServiceError::InvalidCode => {
            AuthStackError::InvalidCredentials
        }
        MfaServiceError::RandomnessUnavailable
        | MfaServiceError::Crypto
        | MfaServiceError::Transport(_)
        | MfaServiceError::Row(_)
        | MfaServiceError::InvalidRow => AuthStackError::store("MFA operation failed"),
        _ => AuthStackError::store("MFA operation failed"),
    }
}

fn map_oauth_error(error: OAuthServiceError<SpinPostgresError>) -> AuthStackError {
    match error {
        OAuthServiceError::InvalidInput | OAuthServiceError::InvalidIdentity => {
            AuthStackError::validation("OAuth request is invalid")
        }
        OAuthServiceError::InvalidFlow => AuthStackError::InvalidToken,
        OAuthServiceError::AccountUnavailable => AuthStackError::AuthRequired,
        OAuthServiceError::Flow(_) => AuthStackError::InvalidToken,
        OAuthServiceError::RandomnessUnavailable
        | OAuthServiceError::Serialization
        | OAuthServiceError::Transport(_)
        | OAuthServiceError::Row(_)
        | OAuthServiceError::InvalidRow => AuthStackError::store("OAuth operation failed"),
        _ => AuthStackError::store("OAuth operation failed"),
    }
}

fn map_oauth_provider_error(
    error: OAuthProviderServiceError<SpinPostgresError>,
) -> AuthStackError {
    match error {
        OAuthProviderServiceError::InvalidInput => {
            AuthStackError::validation("OAuth provider input is invalid")
        }
        OAuthProviderServiceError::InvalidAdminSession => AuthStackError::Forbidden,
        OAuthProviderServiceError::RandomnessUnavailable
        | OAuthProviderServiceError::Transport(_)
        | OAuthProviderServiceError::Row(_)
        | OAuthProviderServiceError::InvalidRow => {
            AuthStackError::store("OAuth provider operation failed")
        }
        _ => AuthStackError::store("OAuth provider operation failed"),
    }
}

fn map_passkey_error(error: PasskeyServiceError<SpinPostgresError>) -> AuthStackError {
    match error {
        PasskeyServiceError::InvalidInput | PasskeyServiceError::InvalidResponse => {
            AuthStackError::validation("passkey request is invalid")
        }
        PasskeyServiceError::InvalidFlow => AuthStackError::InvalidToken,
        PasskeyServiceError::InvalidSession => AuthStackError::AuthRequired,
        PasskeyServiceError::InvalidCredentials => AuthStackError::InvalidCredentials,
        PasskeyServiceError::CredentialConflict | PasskeyServiceError::CounterConflict => {
            AuthStackError::conflict("passkey credential changed or is already registered")
        }
        PasskeyServiceError::Flow(_) => AuthStackError::InvalidToken,
        PasskeyServiceError::RandomnessUnavailable
        | PasskeyServiceError::Serialization
        | PasskeyServiceError::Transport(_)
        | PasskeyServiceError::Row(_)
        | PasskeyServiceError::InvalidRow => AuthStackError::store("passkey operation failed"),
        _ => AuthStackError::store("passkey operation failed"),
    }
}

fn map_policy_error(error: PolicyBundleServiceError<SpinPostgresError>) -> AuthStackError {
    match error {
        PolicyBundleServiceError::InvalidInput => {
            AuthStackError::validation("policy bundle is invalid")
        }
        PolicyBundleServiceError::InvalidAdminSession => AuthStackError::Forbidden,
        PolicyBundleServiceError::RandomnessUnavailable
        | PolicyBundleServiceError::Transport(_)
        | PolicyBundleServiceError::Row(_)
        | PolicyBundleServiceError::InvalidRow => {
            AuthStackError::store("policy publication failed")
        }
        _ => AuthStackError::store("policy publication failed"),
    }
}

fn map_policy_load_error(error: PolicyBundleLoadError<SpinPostgresError>) -> AuthStackError {
    tracing::error!(error = %error, "active Cedar policy load failed closed");
    AuthStackError::store("active policy load failed")
}

fn map_signing_key_error(error: SigningKeyServiceError<SpinPostgresError>) -> AuthStackError {
    match error {
        SigningKeyServiceError::InvalidInput => {
            AuthStackError::validation("signing-key input is invalid")
        }
        SigningKeyServiceError::InvalidAdminOrKey => AuthStackError::Forbidden,
        SigningKeyServiceError::InvalidLifecycle => {
            AuthStackError::configuration("signing-key lifecycle is invalid")
        }
        SigningKeyServiceError::RandomnessUnavailable
        | SigningKeyServiceError::Transport(_)
        | SigningKeyServiceError::Row(_)
        | SigningKeyServiceError::InvalidRow => {
            AuthStackError::store("signing-key operation failed")
        }
        _ => AuthStackError::store("signing-key operation failed"),
    }
}
