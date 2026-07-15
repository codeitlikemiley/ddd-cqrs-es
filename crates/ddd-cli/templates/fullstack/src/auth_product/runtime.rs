//! Thin Spin runtime adapter for product workflows owned by `wasi-auth`.
#![allow(unused_imports)]
#![allow(dead_code)]

use std::{
    collections::VecDeque,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use sha2::{Digest, Sha256};
#[cfg(feature = "mail-capture")]
use wasi_auth::mail::{EmailKind, Recipient};
#[cfg(feature = "mail-capture")]
use wasi_auth::postgres::outbox::{MailOutboxWorker, PublicBaseUrl};
use wasi_auth::{
    authentication::jwt::JwksDocument,
    authentication::mfa::TotpConfig,
    authentication::passkeys::Attachment as PasskeyAttachment,
    authentication::{Clock, RandomSource},
    context::{AuthenticationAssurance, RequestId, SessionId, UserId},
    http::{AuthenticatedSession, TrustedContextCodec},
    postgres::workflows::{
        Argon2Policy, EmailVerificationError, EmailVerificationRequest,
        EmailVerificationResendRequest as ProductEmailVerificationResendRequest,
        EmailVerificationService, OutboxSealingKey,
        PasswordChangeRequest as ProductPasswordChangeRequest, PasswordLoginError,
        PasswordLoginRequest, PasswordLoginService, PasswordRegistrationError,
        PasswordRegistrationRequest, PasswordRegistrationService,
        PasswordResetCompleteRequest as ProductPasswordResetCompleteRequest, PasswordResetError,
        PasswordResetService, PasswordResetStartRequest as ProductPasswordResetStartRequest,
    },
    postgres::{
        PostgresAuthStore, PostgresStoreError,
        flows::FlowSealingKey,
        management::{
            AdminUserRecord, AuditEventRecord, InvitationRecord, InvitationService,
            ManagementError, MembershipRecord, ORGANIZATION_PERMISSION_CATALOG,
            OrganizationManagementService, RoleRecord,
            UpsertRoleRequest as ProductUpsertRoleRequest,
        },
        mfa::{MfaKeyMaterial, MfaService, MfaServiceError},
        oauth::{
            OAuthFlowService, OAuthProviderService, OAuthProviderServiceError, OAuthServiceConfig,
            OAuthServiceError, PendingOAuthFlow, VerifiedOAuthIdentity,
        },
        organizations::{
            CreateOrganizationRequest, OrganizationError, OrganizationRecord, OrganizationService,
        },
        passkeys::{
            PasskeyConfigurationError, PasskeyService, PasskeyServiceConfig, PasskeyServiceError,
        },
        policy::{
            ActivePolicyBundle, PolicyBundleLoadError, PolicyBundleRecord, PolicyBundleService,
            PolicyBundleServiceError,
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
};

use crate::{
    contracts::{
        AccountSessionListResponse, AccountSessionSummary, AdminUserListResponse, AdminUserSummary,
        AuditEventListResponse, AuditEventSummary, AuthProviderSummary, CapturedMailResponse,
        EmailPasswordLoginRequest, EmailPasswordRegisterRequest, EmailVerificationCompleteRequest,
        InvitationListResponse, InvitationSummary, LoginCompletionResponse, LogoutResponse,
        MembershipListResponse, MembershipSummary, MfaEnrollConfirmResponse,
        MfaEnrollStartResponse, MfaStatusResponse, OrganizationListResponse, OrganizationSummary,
        PasskeyStartResponse, PasswordResetCompleteRequest, PasswordResetStartRequest,
        PasswordResetStartResponse, PolicyVersionListResponse, PolicyVersionSummary,
        RoleListResponse, RoleSummary, SessionView, SigningKeyListResponse,
        SigningKeyRotateResponse, SigningKeySummary, TokenRefreshResponse, TokenVerifyResponse,
    },
    error::{AuthStackError, AuthStackResult},
};

use super::*;

pub(crate) type RuntimeTokenService =
    TokenService<SpinPostgresTransport, RuntimeClock, RuntimeRandom>;
pub(crate) type RuntimeTokenVerifier = AccessTokenVerifier<SpinPostgresTransport, RuntimeClock>;

pub(crate) const DEVELOPMENT_OUTBOX_KEY: &[u8] = b"fullstack-development-outbox-key";
pub(crate) const VERIFIED_TOKEN_CACHE_CAPACITY: usize = 256;

pub(crate) static TOKEN_VERIFIER: OnceLock<RuntimeTokenVerifier> = OnceLock::new();
pub(crate) static TRUSTED_CONTEXT_CODEC: OnceLock<TrustedContextCodec> = OnceLock::new();
pub(crate) type VerifiedTokenCache = Mutex<VecDeque<([u8; 32], VerifiedAccessToken)>>;
pub(crate) static VERIFIED_TOKEN_CACHE: OnceLock<VerifiedTokenCache> = OnceLock::new();

pub use wasi_auth::postgres::oauth::PendingOAuthFlow as ProductPendingOAuthFlow;

pub struct OAuthStartValues {
    pub state: String,
    pub nonce: String,
    pub pkce_challenge: String,
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
