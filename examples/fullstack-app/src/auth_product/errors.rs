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

pub(crate) fn map_registration_error(
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

pub(crate) fn map_login_error(error: PasswordLoginError<SpinPostgresError>) -> AuthStackError {
    match error {
        PasswordLoginError::InvalidRequest => {
            AuthStackError::validation("login request is invalid")
        }
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

pub(crate) fn map_verification_error(
    error: EmailVerificationError<SpinPostgresError>,
) -> AuthStackError {
    match error {
        EmailVerificationError::InvalidRequest => {
            AuthStackError::validation("verification request is invalid")
        }
        EmailVerificationError::InvalidToken => {
            AuthStackError::validation("verification token is invalid or expired")
        }
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

pub(crate) fn map_session_store_error(
    error: PostgresStoreError<SpinPostgresError>,
) -> AuthStackError {
    match error {
        PostgresStoreError::Unauthenticated => AuthStackError::AuthRequired,
        _ => AuthStackError::store("session verification failed"),
    }
}

pub(crate) fn map_organization_error(
    error: OrganizationError<SpinPostgresError>,
) -> AuthStackError {
    match error {
        OrganizationError::InvalidRequest => {
            AuthStackError::validation("organization request is invalid")
        }
        OrganizationError::Unauthenticated => AuthStackError::Forbidden,
        OrganizationError::IdempotencyConflict => {
            AuthStackError::conflict("organization request conflicts with an earlier attempt")
        }
        OrganizationError::SlugConflict => AuthStackError::conflict(
            "That workspace URL is already taken. Choose a different slug.",
        ),
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

pub(crate) fn map_password_reset_error(
    error: PasswordResetError<SpinPostgresError>,
) -> AuthStackError {
    match error {
        PasswordResetError::InvalidRequest => {
            AuthStackError::validation("password reset request is invalid")
        }
        PasswordResetError::InvalidToken => {
            AuthStackError::validation("password reset token is invalid or expired")
        }
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

pub(crate) fn map_session_service_error(
    error: SessionServiceError<SpinPostgresError>,
) -> AuthStackError {
    match error {
        SessionServiceError::NotAuthorized => AuthStackError::Forbidden,
        SessionServiceError::RandomnessUnavailable
        | SessionServiceError::Transport(_)
        | SessionServiceError::Row(_)
        | SessionServiceError::InvalidRow => AuthStackError::store("session operation failed"),
        _ => AuthStackError::store("session operation failed"),
    }
}

pub(crate) fn map_rate_limit_error(error: RateLimitError<SpinPostgresError>) -> AuthStackError {
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

pub(crate) fn map_management_error(error: ManagementError<SpinPostgresError>) -> AuthStackError {
    match error {
        ManagementError::InvalidRequest => {
            AuthStackError::validation("management request is invalid")
        }
        ManagementError::NotAuthorized => AuthStackError::Forbidden,
        ManagementError::ProtectedInvariant => {
            AuthStackError::conflict("operation would violate an ownership or account invariant")
        }
        ManagementError::RoleInUse {
            member_count,
            invitation_count,
        } => AuthStackError::conflict(format!(
            "cannot delete role while {member_count} active member(s) and {invitation_count} pending invitation(s) still use it"
        )),
        ManagementError::RestrictedPermission => {
            AuthStackError::validation("custom role contains a restricted permission")
        }
        ManagementError::InvalidToken => {
            AuthStackError::validation("invitation token is invalid or expired")
        }
        ManagementError::RandomnessUnavailable
        | ManagementError::Crypto
        | ManagementError::Transport(_)
        | ManagementError::Row(_)
        | ManagementError::InvalidRow => AuthStackError::store("management operation failed"),
        _ => AuthStackError::store("management operation failed"),
    }
}

pub(crate) fn map_token_error(error: TokenServiceError<SpinPostgresError>) -> AuthStackError {
    match error {
        TokenServiceError::InvalidSession => AuthStackError::AuthRequired,
        TokenServiceError::ExpiredToken => AuthStackError::SessionExpired,
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

pub(crate) fn map_mfa_error(error: MfaServiceError<SpinPostgresError>) -> AuthStackError {
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

pub(crate) fn map_oauth_error(error: OAuthServiceError<SpinPostgresError>) -> AuthStackError {
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

pub(crate) fn map_oauth_provider_error(
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

pub(crate) fn map_passkey_error(error: PasskeyServiceError<SpinPostgresError>) -> AuthStackError {
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

pub(crate) fn map_policy_error(
    error: PolicyBundleServiceError<SpinPostgresError>,
) -> AuthStackError {
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

pub(crate) fn map_policy_load_error(
    error: PolicyBundleLoadError<SpinPostgresError>,
) -> AuthStackError {
    tracing::error!(error = %error, "active Cedar policy load failed closed");
    AuthStackError::store("active policy load failed")
}

pub(crate) fn map_signing_key_error(
    error: SigningKeyServiceError<SpinPostgresError>,
) -> AuthStackError {
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
